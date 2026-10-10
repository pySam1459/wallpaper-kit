use super::*;
use windows::Win32::Graphics::Direct3D::Fxc::{D3DCOMPILE_ENABLE_STRICTNESS, D3DCompile};
use windows::core::{PCSTR, s};

pub(super) struct ImageRenderer {
    vertex: ID3D11VertexShader,
    rgba: ID3D11PixelShader,
    r8: ID3D11PixelShader,
    dimensions: ID3D11Buffer,
    texture: Option<Uploaded>,
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
        let (mut vertex, mut rgba, mut r8, mut dimensions) = (None, None, None, None);
        // SAFETY: live device and bytecode buffers; outputs are owned COM refs.
        unsafe {
            device.CreateVertexShader(&vertex_code, None, Some(&mut vertex))?;
            device.CreatePixelShader(&rgba_code, None, Some(&mut rgba))?;
            device.CreatePixelShader(&r8_code, None, Some(&mut r8))?;
            device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: 16,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut dimensions),
            )?;
        }
        Ok(Self {
            vertex: vertex.expect("vertex shader output"),
            rgba: rgba.expect("RGBA shader output"),
            r8: r8.expect("R8 shader output"),
            dimensions: dimensions.expect("dimension buffer output"),
            texture: None,
            uploads: 0,
        })
    }

    pub(super) fn bytes(&self) -> u64 {
        self.texture.as_ref().map_or(0, |texture| texture.bytes)
    }

    pub(super) fn release_texture(&mut self) {
        self.texture = None;
    }

    pub(super) fn prepare(
        &mut self,
        device: &ID3D11Device,
        page: &Arc<TexturePage>,
    ) -> Result<(), Error> {
        let source = Arc::downgrade(page);
        if self
            .texture
            .as_ref()
            .is_some_and(|texture| texture.source.ptr_eq(&source))
        {
            return Ok(());
        }
        // The caller waits for the preceding render query before replacement;
        // draw unbinds the source. Retire it before allocating another page.
        self.texture = None;
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
        self.texture = Some(Uploaded {
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
    ) {
        let texture = self
            .texture
            .as_ref()
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
            let extent = [output.size.width(), output.size.height(), 0, 0];
            context.UpdateSubresource(&self.dimensions, 0, None, extent.as_ptr().cast(), 0, 0);
            context.PSSetConstantBuffers(0, Some(&[Some(self.dimensions.clone())]));
            context.PSSetShaderResources(0, Some(&[Some(texture.view.clone())]));
            context.OMSetRenderTargets(Some(&[Some(output.view.clone())]), None);
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
