use super::*;
use windows::Win32::Graphics::Direct3D::Fxc::{D3DCOMPILE_ENABLE_STRICTNESS, D3DCompile};
use windows::core::{PCSTR, s};

pub(super) struct ImageRenderer {
    vertex: ID3D11VertexShader,
    rgba: ID3D11PixelShader,
    r8: ID3D11PixelShader,
    dimensions: ID3D11Buffer,
    layer_vertex: ID3D11VertexShader,
    layer_pixel: ID3D11PixelShader,
    layer_constants: ID3D11Buffer,
    blend: ID3D11BlendState,
    textures: Vec<Uploaded>,
    pub(super) uploads: u64,
}

struct Uploaded {
    source: Weak<TexturePage>,
    view: ID3D11ShaderResourceView,
    bytes: u64,
}

impl ImageRenderer {
    pub(super) fn new(device: &ID3D11Device) -> Result<Self, Error> {
        let vertex_code = compile(s!("vertex_main"), s!("vs_5_0"))?;
        let rgba_code = compile(s!("rgba_main"), s!("ps_5_0"))?;
        let r8_code = compile(s!("r8_main"), s!("ps_5_0"))?;
        let layer_vertex_code = compile(s!("layer_vertex"), s!("vs_5_0"))?;
        let layer_pixel_code = compile(s!("layer_pixel"), s!("ps_5_0"))?;
        let (mut vertex, mut rgba, mut r8, mut dimensions) = (None, None, None, None);
        let (mut layer_vertex, mut layer_pixel, mut layer_constants, mut blend) =
            (None, None, None, None);
        // SAFETY: live device and bytecode buffers; outputs are owned COM refs.
        unsafe {
            device.CreateVertexShader(&vertex_code, None, Some(&mut vertex))?;
            device.CreatePixelShader(&rgba_code, None, Some(&mut rgba))?;
            device.CreatePixelShader(&r8_code, None, Some(&mut r8))?;
            device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: 32,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut dimensions),
            )?;
            device.CreateVertexShader(&layer_vertex_code, None, Some(&mut layer_vertex))?;
            device.CreatePixelShader(&layer_pixel_code, None, Some(&mut layer_pixel))?;
            device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: 64,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut layer_constants),
            )?;
            let mut blend_desc = D3D11_BLEND_DESC::default();
            blend_desc.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: true.into(),
                SrcBlend: D3D11_BLEND_SRC_ALPHA,
                DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_ZERO,
                DestBlendAlpha: D3D11_BLEND_ONE,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
            };
            device.CreateBlendState(&blend_desc, Some(&mut blend))?;
        }
        Ok(Self {
            vertex: vertex.expect("vertex shader output"),
            rgba: rgba.expect("RGBA shader output"),
            r8: r8.expect("R8 shader output"),
            dimensions: dimensions.expect("dimension buffer output"),
            layer_vertex: layer_vertex.expect("layer vertex output"),
            layer_pixel: layer_pixel.expect("layer pixel output"),
            layer_constants: layer_constants.expect("layer buffer output"),
            blend: blend.expect("blend output"),
            textures: Vec::new(),
            uploads: 0,
        })
    }

    pub(super) fn bytes(&self) -> u64 {
        self.textures.iter().map(|texture| texture.bytes).sum()
    }

    pub(super) fn release_texture(&mut self) {
        self.textures.clear();
    }

    pub(super) fn prepare(
        &mut self,
        device: &ID3D11Device,
        pages: &[Arc<TexturePage>],
    ) -> Result<(), Error> {
        // Preceding queries completed and draw unbound sources. Evict obsolete
        // pages before uploading, keeping resident payloads within admission.
        self.textures.retain(|texture| {
            pages
                .iter()
                .any(|page| texture.source.ptr_eq(&Arc::downgrade(page)))
        });
        for page in pages {
            self.upload(device, page)?;
        }
        Ok(())
    }

    fn upload(&mut self, device: &ID3D11Device, page: &Arc<TexturePage>) -> Result<(), Error> {
        let source = Arc::downgrade(page);
        if self
            .textures
            .iter()
            .any(|texture| texture.source.ptr_eq(&source))
        {
            return Ok(());
        }
        let [width, height] = page.extent();
        let (format, pitch) = match page.format() {
            TextureFormat::Rgba8 => (DXGI_FORMAT_R8G8B8A8_UNORM, width * 4),
            TextureFormat::Bc3 => (DXGI_FORMAT_BC3_UNORM, width.div_ceil(4) * 16),
            TextureFormat::R8 => (DXGI_FORMAT_R8_UNORM, width),
        };
        let mut texture = None;
        let mut view = None;
        // SAFETY: TexturePage construction checked byte length and dimensions.
        // Initial data stays live through creation of the immutable resource.
        unsafe {
            device.CreateTexture2D(
                &D3D11_TEXTURE2D_DESC {
                    Width: width,
                    Height: height,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: format,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_IMMUTABLE,
                    BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                    ..Default::default()
                },
                Some(&D3D11_SUBRESOURCE_DATA {
                    pSysMem: page.data().as_ptr().cast(),
                    SysMemPitch: pitch,
                    SysMemSlicePitch: page.data().len() as u32,
                }),
                Some(&mut texture),
            )?;
            device.CreateShaderResourceView(
                &texture.expect("source texture output"),
                None,
                Some(&mut view),
            )?;
        }
        self.textures.push(Uploaded {
            source,
            view: view.expect("source view output"),
            bytes: page.data().len() as u64,
        });
        self.uploads += 1;
        Ok(())
    }

    pub(super) fn draw(
        &self,
        context: &ID3D11DeviceContext,
        output: &Output,
        format: TextureFormat,
        origin: [u32; 2],
        extent: [u32; 2],
    ) {
        let texture = self
            .textures
            .first()
            .expect("source prepared before drawing");
        let pixel = if format == TextureFormat::R8 {
            &self.r8
        } else {
            &self.rgba
        };
        // SAFETY: caller owns this context exclusively and holds output GPU
        // access. Shaders, views and constants remain owned through submission.
        unsafe {
            context.IASetInputLayout(None);
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.VSSetShader(&self.vertex, None);
            context.PSSetShader(pixel, None);
            let constants = [
                output.size.width(),
                output.size.height(),
                origin[0],
                origin[1],
                extent[0],
                extent[1],
                0,
                0,
            ];
            context.UpdateSubresource(&self.dimensions, 0, None, constants.as_ptr().cast(), 0, 0);
            context.PSSetConstantBuffers(0, Some(&[Some(self.dimensions.clone())]));
            context.PSSetShaderResources(0, Some(&[Some(texture.view.clone())]));
            context.OMSetRenderTargets(Some(&[Some(output.view.clone())]), None);
            context.OMSetBlendState(None, None, u32::MAX);
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: output.size.width() as f32,
                Height: output.size.height() as f32,
                MinDepth: 0.,
                MaxDepth: 1.,
                ..Default::default()
            }]));
            context.Draw(3, 0);
            context.PSSetShaderResources(0, Some(&[None]));
            context.OMSetRenderTargets(None, None);
        }
    }

    pub(super) fn draw_layers(
        &self,
        context: &ID3D11DeviceContext,
        output: &Output,
        composition: &crate::ImageComposition,
    ) {
        // SAFETY: exclusive worker context and output GPU key. All constants,
        // shaders and source views are owned through submission. Data is 64 bytes
        // of initialized scalar values matching the HLSL b1 buffer layout.
        unsafe {
            context.IASetInputLayout(None);
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.VSSetShader(&self.layer_vertex, None);
            context.PSSetShader(&self.layer_pixel, None);
            context.VSSetConstantBuffers(1, Some(&[Some(self.layer_constants.clone())]));
            context.PSSetConstantBuffers(1, Some(&[Some(self.layer_constants.clone())]));
            context.OMSetRenderTargets(Some(&[Some(output.view.clone())]), None);
            context.OMSetBlendState(&self.blend, None, u32::MAX);
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: output.size.width() as f32,
                Height: output.size.height() as f32,
                MinDepth: 0.,
                MaxDepth: 1.,
                ..Default::default()
            }]));
            for layer in composition.layers() {
                let region = layer.region();
                let source = Arc::downgrade(region.page());
                let texture = self
                    .textures
                    .iter()
                    .find(|texture| texture.source.ptr_eq(&source))
                    .expect("admitted source uploaded");
                let axes = layer.axes();
                let canvas = composition.canvas();
                let constants = [
                    (canvas.width() as f32).to_bits(),
                    (canvas.height() as f32).to_bits(),
                    layer.center()[0].to_bits(),
                    layer.center()[1].to_bits(),
                    axes[0][0].to_bits(),
                    axes[0][1].to_bits(),
                    axes[1][0].to_bits(),
                    axes[1][1].to_bits(),
                    region.origin()[0],
                    region.origin()[1],
                    region.extent()[0],
                    region.extent()[1],
                    layer.opacity().to_bits(),
                    layer.brightness().to_bits(),
                    u32::from(region.page().format() == TextureFormat::R8),
                    0,
                ];
                context.UpdateSubresource(
                    &self.layer_constants,
                    0,
                    None,
                    constants.as_ptr().cast(),
                    0,
                    0,
                );
                context.PSSetShaderResources(0, Some(&[Some(texture.view.clone())]));
                context.Draw(6, 0);
            }
            context.PSSetShaderResources(0, Some(&[None]));
            context.OMSetRenderTargets(None, None);
            context.OMSetBlendState(None, None, u32::MAX);
        }
    }
}

fn compile(entry: PCSTR, target: PCSTR) -> Result<Vec<u8>, Error> {
    let source = include_bytes!("shaders/texture.hlsl");
    let mut code = None;
    let mut diagnostics = None;
    // SAFETY: source buffer and output pointers stay live. Entry/target are
    // static NUL-terminated strings. No proprietary sources or includes used.
    unsafe {
        if let Err(error) = D3DCompile(
            source.as_ptr().cast(),
            source.len(),
            s!("texture.hlsl"),
            None,
            None,
            entry,
            target,
            D3DCOMPILE_ENABLE_STRICTNESS,
            0,
            &mut code,
            Some(&mut diagnostics),
        ) {
            let detail = diagnostics
                .map(|blob| {
                    String::from_utf8_lossy(std::slice::from_raw_parts(
                        blob.GetBufferPointer().cast(),
                        blob.GetBufferSize(),
                    ))
                    .into_owned()
                })
                .unwrap_or_else(|| error.to_string());
            return Err(Error::Worker(format!(
                "texture shader compilation failed: {detail}"
            )));
        }
        let code = code.expect("shader compilation output");
        Ok(
            std::slice::from_raw_parts(code.GetBufferPointer().cast(), code.GetBufferSize())
                .to_vec(),
        )
    }
}
