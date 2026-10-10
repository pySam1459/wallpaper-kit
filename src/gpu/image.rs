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
    blur_horizontal: ID3D11PixelShader,
    blur_vertical: ID3D11PixelShader,
    blur_constants: ID3D11Buffer,
    blur: Option<BlurTarget>,
    blend: ID3D11BlendState,
    textures: Vec<Uploaded>,
    pub(super) uploads: u64,
}

struct Uploaded {
    source: Weak<TexturePage>,
    view: ID3D11ShaderResourceView,
    bytes: u64,
}

struct BlurTarget {
    size: Size,
    target: ID3D11RenderTargetView,
    source: ID3D11ShaderResourceView,
}

impl BlurTarget {
    fn new(device: &ID3D11Device, size: Size) -> Result<Self, Error> {
        let (mut texture, mut target, mut source) = (None, None, None);
        // SAFETY: checked dimensions, default GPU-only texture, and owned COM
        // outputs. Both views retain the resource after this local ref drops.
        unsafe {
            device.CreateTexture2D(
                &D3D11_TEXTURE2D_DESC {
                    Width: size.width(),
                    Height: size.height(),
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_R16G16B16A16_FLOAT,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
                    ..Default::default()
                },
                None,
                Some(&mut texture),
            )?;
            let texture = texture.expect("blur texture output");
            device.CreateRenderTargetView(&texture, None, Some(&mut target))?;
            device.CreateShaderResourceView(&texture, None, Some(&mut source))?;
        }
        Ok(Self {
            size,
            target: target.expect("blur target output"),
            source: source.expect("blur source output"),
        })
    }
}

impl ImageRenderer {
    pub(super) fn new(device: &ID3D11Device) -> Result<Self, Error> {
        let vertex_code = compile(s!("vertex_main"), s!("vs_5_0"))?;
        let rgba_code = compile(s!("rgba_main"), s!("ps_5_0"))?;
        let r8_code = compile(s!("r8_main"), s!("ps_5_0"))?;
        let layer_vertex_code = compile(s!("layer_vertex"), s!("vs_5_0"))?;
        let layer_pixel_code = compile(s!("layer_pixel"), s!("ps_5_0"))?;
        let blur_horizontal_code = compile(s!("blur_horizontal"), s!("ps_5_0"))?;
        let blur_vertical_code = compile(s!("blur_vertical"), s!("ps_5_0"))?;
        let (mut vertex, mut rgba, mut r8, mut dimensions) = (None, None, None, None);
        let (mut layer_vertex, mut layer_pixel, mut layer_constants, mut blend) =
            (None, None, None, None);
        let (mut blur_horizontal, mut blur_vertical, mut blur_constants) = (None, None, None);
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
            device.CreatePixelShader(&blur_horizontal_code, None, Some(&mut blur_horizontal))?;
            device.CreatePixelShader(&blur_vertical_code, None, Some(&mut blur_vertical))?;
            device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: 32,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut blur_constants),
            )?;
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
            blur_horizontal: blur_horizontal.expect("horizontal shader output"),
            blur_vertical: blur_vertical.expect("vertical shader output"),
            blur_constants: blur_constants.expect("blur buffer output"),
            blur: None,
            blend: blend.expect("blend output"),
            textures: Vec::new(),
            uploads: 0,
        })
    }

    pub(super) fn bytes(&self) -> u64 {
        self.textures.iter().map(|texture| texture.bytes).sum()
    }

    pub(super) fn effect_bytes(&self) -> u64 {
        self.blur.as_ref().map_or(0, |blur| {
            u64::from(blur.size.width()) * u64::from(blur.size.height()) * 8
        })
    }

    pub(super) fn release_texture(&mut self) {
        self.textures.clear();
        self.blur = None;
    }

    pub(super) fn prepare(
        &mut self,
        device: &ID3D11Device,
        pages: &[Arc<TexturePage>],
        blur_extent: Option<Size>,
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
        if self.blur.as_ref().map(|blur| blur.size) != blur_extent {
            // Previous submission completed. Release before replacement so two
            // scratch allocations cannot exceed the admitted scratch bound.
            self.blur = None;
            self.blur = blur_extent
                .map(|extent| BlurTarget::new(device, extent))
                .transpose()?;
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
        // shaders and source views are owned through submission. Initialized
        // scalar arrays match the HLSL b1 (64 bytes) and b2 (32 bytes) layouts.
        unsafe {
            context.IASetInputLayout(None);
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.VSSetConstantBuffers(1, Some(&[Some(self.layer_constants.clone())]));
            context.PSSetConstantBuffers(1, Some(&[Some(self.layer_constants.clone())]));
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
                let mut sources = [Some(texture.view.clone()), None, None];
                if let Some(blur) = layer.filter().filter(|blur| blur.active()) {
                    let scratch = self.blur.as_ref().expect("blur target admitted");
                    let mask = blur.mask();
                    let mask_origin = mask.map_or([0, 0], |mask| mask.origin());
                    let mask_extent = mask.map_or([1, 1], |mask| mask.extent());
                    let (amount, profile) = blur.parameters();
                    let constants = [
                        amount[0].to_bits(),
                        amount[1].to_bits(),
                        u32::from(mask.is_some()),
                        profile,
                        mask_origin[0],
                        mask_origin[1],
                        mask_extent[0],
                        mask_extent[1],
                    ];
                    context.UpdateSubresource(
                        &self.blur_constants,
                        0,
                        None,
                        constants.as_ptr().cast(),
                        0,
                        0,
                    );
                    context.PSSetConstantBuffers(2, Some(&[Some(self.blur_constants.clone())]));
                    context.VSSetShader(&self.vertex, None);
                    context.PSSetShader(&self.blur_horizontal, None);
                    context.OMSetRenderTargets(Some(&[Some(scratch.target.clone())]), None);
                    context.OMSetBlendState(None, None, u32::MAX);
                    context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                        Width: region.extent()[0] as f32,
                        Height: region.extent()[1] as f32,
                        MinDepth: 0.,
                        MaxDepth: 1.,
                        ..Default::default()
                    }]));
                    context.PSSetShaderResources(0, Some(&[Some(texture.view.clone())]));
                    context.Draw(3, 0);
                    context.OMSetRenderTargets(None, None);
                    sources[1] = Some(scratch.source.clone());
                    sources[2] = mask.map(|mask| {
                        self.textures
                            .iter()
                            .find(|texture| texture.source.ptr_eq(&Arc::downgrade(mask.page())))
                            .expect("mask source uploaded")
                            .view
                            .clone()
                    });
                    context.PSSetShader(&self.blur_vertical, None);
                } else {
                    context.PSSetShader(&self.layer_pixel, None);
                }
                context.VSSetShader(&self.layer_vertex, None);
                context.OMSetRenderTargets(Some(&[Some(output.view.clone())]), None);
                context.OMSetBlendState(&self.blend, None, u32::MAX);
                context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                    Width: output.size.width() as f32,
                    Height: output.size.height() as f32,
                    MinDepth: 0.,
                    MaxDepth: 1.,
                    ..Default::default()
                }]));
                context.PSSetShaderResources(0, Some(&sources));
                context.Draw(6, 0);
                // Unbind all inputs before the next layer reuses scratch as an
                // output, including when original and mask share a source.
                context.PSSetShaderResources(0, Some(&[None, None, None]));
            }
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
