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
    particle_pixel: ID3D11PixelShader,
    refract_pixel: ID3D11PixelShader,
    resolve_pixel: ID3D11PixelShader,
    bloom_down: ID3D11PixelShader,
    bloom_up: ID3D11PixelShader,
    scene_constants: ID3D11Buffer,
    scene: Option<BlurTarget>,
    background: Option<BlurTarget>,
    bloom: Vec<BlurTarget>,
    layer_constants: ID3D11Buffer,
    blur_horizontal: ID3D11PixelShader,
    blur_vertical: ID3D11PixelShader,
    blur_constants: ID3D11Buffer,
    cloud_pixel: ID3D11PixelShader,
    flow_pixel: ID3D11PixelShader,
    motion_pixel: ID3D11PixelShader,
    ray_vertex: ID3D11VertexShader,
    ray_pixel: ID3D11PixelShader,
    ray_constants: ID3D11Buffer,
    cloud_constants: ID3D11Buffer,
    blur: Option<BlurTarget>,
    chain: Vec<BlurTarget>,
    blend: ID3D11BlendState,
    additive: ID3D11BlendState,
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
    format: DXGI_FORMAT,
    texture: ID3D11Texture2D,
    target: ID3D11RenderTargetView,
    source: ID3D11ShaderResourceView,
}

impl BlurTarget {
    fn new(device: &ID3D11Device, size: Size) -> Result<Self, Error> {
        Self::with_format(device, size, DXGI_FORMAT_R16G16B16A16_FLOAT)
    }

    fn with_format(device: &ID3D11Device, size: Size, format: DXGI_FORMAT) -> Result<Self, Error> {
        let (mut texture, mut target, mut source) = (None, None, None);
        // SAFETY: checked dimensions, default GPU-only texture and owned COM
        // outputs. Texture and views remain owned through worker submission.
        unsafe {
            device.CreateTexture2D(
                &D3D11_TEXTURE2D_DESC {
                    Width: size.width(),
                    Height: size.height(),
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: format,
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
            Ok(Self {
                size,
                format,
                texture,
                target: target.expect("blur target output"),
                source: source.expect("blur source output"),
            })
        }
    }
}

impl ImageRenderer {
    pub(super) fn new(device: &ID3D11Device) -> Result<Self, Error> {
        let vertex_code = compile(s!("vertex_main"), s!("vs_5_0"))?;
        let rgba_code = compile(s!("rgba_main"), s!("ps_5_0"))?;
        let r8_code = compile(s!("r8_main"), s!("ps_5_0"))?;
        let layer_vertex_code = compile(s!("layer_vertex"), s!("vs_5_0"))?;
        let layer_pixel_code = compile(s!("layer_pixel"), s!("ps_5_0"))?;
        let particle_pixel_code = compile(s!("particle_pixel"), s!("ps_5_0"))?;
        let refract_code = compile(s!("refract_pixel"), s!("ps_5_0"))?;
        let resolve_code = compile(s!("resolve_pixel"), s!("ps_5_0"))?;
        let bloom_down_code = compile(s!("bloom_down"), s!("ps_5_0"))?;
        let bloom_up_code = compile(s!("bloom_up"), s!("ps_5_0"))?;
        let (
            mut refract_pixel,
            mut resolve_pixel,
            mut bloom_down,
            mut bloom_up,
            mut scene_constants,
        ) = (None, None, None, None, None);
        let blur_horizontal_code = compile(s!("blur_horizontal"), s!("ps_5_0"))?;
        let blur_vertical_code = compile(s!("blur_vertical"), s!("ps_5_0"))?;
        let cloud_pixel_code = compile(s!("cloud_pixel"), s!("ps_5_0"))?;
        let flow_pixel_code = compile(s!("flow_pixel"), s!("ps_5_0"))?;
        let motion_pixel_code = compile(s!("motion_pixel"), s!("ps_5_0"))?;
        let ray_vertex_code = compile(s!("ray_vertex"), s!("vs_5_0"))?;
        let ray_pixel_code = compile(s!("ray_pixel"), s!("ps_5_0"))?;
        let (mut ray_vertex, mut ray_pixel, mut ray_constants) = (None, None, None);
        let (mut vertex, mut rgba, mut r8, mut dimensions) = (None, None, None, None);
        let (mut layer_vertex, mut layer_pixel, mut layer_constants, mut blend) =
            (None, None, None, None);
        let (mut blur_horizontal, mut blur_vertical, mut blur_constants) = (None, None, None);
        let (mut cloud_pixel, mut cloud_constants) = (None, None);
        let mut flow_pixel = None;
        let mut motion_pixel = None;
        let (mut particle_pixel, mut additive) = (None, None);
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
            device.CreatePixelShader(&particle_pixel_code, None, Some(&mut particle_pixel))?;
            device.CreatePixelShader(&refract_code, None, Some(&mut refract_pixel))?;
            device.CreatePixelShader(&resolve_code, None, Some(&mut resolve_pixel))?;
            device.CreatePixelShader(&bloom_down_code, None, Some(&mut bloom_down))?;
            device.CreatePixelShader(&bloom_up_code, None, Some(&mut bloom_up))?;
            device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: 48,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut scene_constants),
            )?;
            device.CreatePixelShader(&blur_horizontal_code, None, Some(&mut blur_horizontal))?;
            device.CreatePixelShader(&blur_vertical_code, None, Some(&mut blur_vertical))?;
            device.CreatePixelShader(&cloud_pixel_code, None, Some(&mut cloud_pixel))?;
            device.CreatePixelShader(&flow_pixel_code, None, Some(&mut flow_pixel))?;
            device.CreatePixelShader(&motion_pixel_code, None, Some(&mut motion_pixel))?;
            device.CreateVertexShader(&ray_vertex_code, None, Some(&mut ray_vertex))?;
            device.CreatePixelShader(&ray_pixel_code, None, Some(&mut ray_pixel))?;
            device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: 128,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut ray_constants),
            )?;
            device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: 64,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut cloud_constants),
            )?;
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
                    ByteWidth: 160,
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
            blend_desc.RenderTarget[0].DestBlend = D3D11_BLEND_ONE;
            device.CreateBlendState(&blend_desc, Some(&mut additive))?;
        }
        Ok(Self {
            vertex: vertex.expect("vertex shader output"),
            rgba: rgba.expect("RGBA shader output"),
            r8: r8.expect("R8 shader output"),
            dimensions: dimensions.expect("dimension buffer output"),
            layer_vertex: layer_vertex.expect("layer vertex output"),
            layer_pixel: layer_pixel.expect("layer pixel output"),
            particle_pixel: particle_pixel.expect("particle pixel output"),
            refract_pixel: refract_pixel.expect("refraction shader output"),
            resolve_pixel: resolve_pixel.expect("resolve shader output"),
            bloom_down: bloom_down.expect("bloom down shader output"),
            bloom_up: bloom_up.expect("bloom up shader output"),
            scene_constants: scene_constants.expect("scene constants output"),
            scene: None,
            background: None,
            bloom: Vec::new(),
            layer_constants: layer_constants.expect("layer buffer output"),
            blur_horizontal: blur_horizontal.expect("horizontal shader output"),
            blur_vertical: blur_vertical.expect("vertical shader output"),
            blur_constants: blur_constants.expect("blur buffer output"),
            cloud_pixel: cloud_pixel.expect("cloud pixel output"),
            flow_pixel: flow_pixel.expect("flow pixel output"),
            motion_pixel: motion_pixel.expect("motion pixel output"),
            ray_vertex: ray_vertex.expect("ray vertex output"),
            ray_pixel: ray_pixel.expect("ray pixel output"),
            ray_constants: ray_constants.expect("ray constants output"),
            cloud_constants: cloud_constants.expect("cloud constants output"),
            blur: None,
            chain: Vec::new(),
            blend: blend.expect("blend output"),
            additive: additive.expect("additive blend output"),
            textures: Vec::new(),
            uploads: 0,
        })
    }

    pub(super) fn bytes(&self) -> u64 {
        self.textures.iter().map(|texture| texture.bytes).sum()
    }

    pub(super) fn effect_bytes(&self) -> u64 {
        self.blur
            .iter()
            .chain(&self.chain)
            .chain(self.scene.iter())
            .chain(self.background.iter())
            .chain(&self.bloom)
            .map(|target| {
                u64::from(target.size.width())
                    * u64::from(target.size.height())
                    * if target.format == DXGI_FORMAT_R16G16B16A16_FLOAT {
                        8
                    } else {
                        4
                    }
            })
            .sum()
    }

    pub(super) fn release_texture(&mut self) {
        self.textures.clear();
        self.blur = None;
        self.chain.clear();
        self.scene = None;
        self.background = None;
        self.bloom.clear();
    }

    pub(super) fn prepare(
        &mut self,
        device: &ID3D11Device,
        pages: &[Arc<TexturePage>],
        blur_extent: Option<Size>,
        output_size: Size,
        composition: Option<&crate::ImageComposition>,
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
        let hdr = composition.and_then(|composition| composition.hdr);
        let chain_extent = composition.and_then(|composition| composition.chain_extent);
        let chain_format = if hdr.is_some() {
            DXGI_FORMAT_R16G16B16A16_FLOAT
        } else {
            DXGI_FORMAT_R8G8B8A8_UNORM
        };
        if self
            .chain
            .first()
            .map(|target| (target.size, target.format))
            != chain_extent.map(|size| (size, chain_format))
        {
            self.chain.clear();
            if let Some(size) = chain_extent {
                for _ in 0..2 {
                    self.chain
                        .push(BlurTarget::with_format(device, size, chain_format)?);
                }
            }
        }
        let refract = composition.is_some_and(|composition| {
            composition
                .layers()
                .iter()
                .any(|layer| layer.refraction.is_some())
        });
        let scene_size = (hdr.is_some() || refract).then_some(output_size);
        let scene_format = if hdr.is_some() {
            DXGI_FORMAT_R16G16B16A16_FLOAT
        } else {
            DXGI_FORMAT_R8G8B8A8_UNORM
        };
        if self
            .scene
            .as_ref()
            .map(|target| (target.size, target.format))
            != scene_size.map(|size| (size, scene_format))
        {
            self.scene = None;
            self.background = None;
            self.bloom.clear();
            self.scene = scene_size
                .map(|size| BlurTarget::with_format(device, size, scene_format))
                .transpose()?;
        }
        let background_size = refract.then_some(output_size);
        if self.background.as_ref().map(|target| target.size) != background_size {
            self.background = None;
            self.background = background_size
                .map(|size| BlurTarget::with_format(device, size, scene_format))
                .transpose()?;
        }
        let mut sizes = Vec::new();
        if let Some(hdr) = hdr.filter(|hdr| hdr.strength > 0.) {
            let [mut width, mut height] = [output_size.width(), output_size.height()];
            for _ in 0..hdr.iterations {
                width = width.div_ceil(2);
                height = height.div_ceil(2);
                sizes.push(Size::new(width, height)?);
                if width == 1 && height == 1 {
                    break;
                }
            }
        }
        if !self
            .bloom
            .iter()
            .map(|target| target.size)
            .eq(sizes.iter().copied())
        {
            self.bloom.clear();
            for size in sizes {
                self.bloom.push(BlurTarget::new(device, size)?);
            }
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
            TextureFormat::Rg8 => (DXGI_FORMAT_R8G8_UNORM, width * 2),
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
        // scalar arrays match HLSL b1 (160 bytes), b2 (32) and b3 (64) layouts.
        unsafe {
            context.IASetInputLayout(None);
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.VSSetConstantBuffers(1, Some(&[Some(self.layer_constants.clone())]));
            context.PSSetConstantBuffers(1, Some(&[Some(self.layer_constants.clone())]));
            context.PSSetConstantBuffers(4, Some(&[Some(self.scene_constants.clone())]));
            if let Some(scene) = &self.scene {
                context.ClearRenderTargetView(&scene.target, &composition.background_color());
            }
            let target = self
                .scene
                .as_ref()
                .map_or(&output.view, |scene| &scene.target);
            for layer in composition.layers() {
                let region = layer.region();
                let source = Arc::downgrade(region.page());
                let texture = self
                    .textures
                    .iter()
                    .find(|texture| texture.source.ptr_eq(&source))
                    .expect("admitted source uploaded");
                for pass in 0..layer.effects.len().max(1) {
                    let effect = layer.effects.get(pass);
                    let final_pass = pass + 1 == layer.effects.len().max(1);
                    let input_view = if pass == 0 {
                        &texture.view
                    } else {
                        &self.chain[(pass - 1) % 2].source
                    };
                    let target = if final_pass {
                        target
                    } else {
                        &self.chain[pass % 2].target
                    };
                    let draw_size = if final_pass {
                        output.size
                    } else {
                        Size::new(region.extent()[0], region.extent()[1])
                            .expect("chain extent admitted")
                    };
                    let axes = layer.axes();
                    let canvas = composition.canvas();
                    let (color, additive) = layer.particle_color.unwrap_or(([1.; 3], false));
                    let uv = layer.particle_uv.unwrap_or([0., 0., 1., 0., 0., 1.]);
                    let (next_region, next_uv, frame_blend) = layer
                        .particle_next
                        .as_ref()
                        .map_or((region, uv, 0.), |(region, uv, blend)| {
                            (region, *uv, *blend)
                        });
                    let mut constants = [
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
                        match region.page().format() {
                            TextureFormat::R8 => 1,
                            TextureFormat::Rg8 if layer.particle_color.is_some() => 2,
                            _ => 0,
                        },
                        u32::from(layer.image_quality() == crate::ImageQuality::Linear),
                        color[0].to_bits(),
                        color[1].to_bits(),
                        color[2].to_bits(),
                        0,
                        uv[0].to_bits(),
                        uv[1].to_bits(),
                        uv[2].to_bits(),
                        uv[3].to_bits(),
                        uv[4].to_bits(),
                        uv[5].to_bits(),
                        0,
                        0,
                        next_uv[0].to_bits(),
                        next_uv[1].to_bits(),
                        next_uv[2].to_bits(),
                        next_uv[3].to_bits(),
                        next_uv[4].to_bits(),
                        next_uv[5].to_bits(),
                        frame_blend.to_bits(),
                        match next_region.page().format() {
                            TextureFormat::R8 => 1,
                            TextureFormat::Rg8 => 2,
                            _ => 0,
                        },
                        next_region.origin()[0],
                        next_region.origin()[1],
                        next_region.extent()[0],
                        next_region.extent()[1],
                    ];
                    if pass > 0 {
                        constants[8] = 0;
                        constants[9] = 0;
                        constants[14] = 0;
                    }
                    if !final_pass {
                        let [width, height] = region.extent().map(|v| v as f32);
                        constants[..8].copy_from_slice(
                            &[
                                width,
                                height,
                                width / 2.,
                                height / 2.,
                                width,
                                0.,
                                0.,
                                -height,
                            ]
                            .map(f32::to_bits),
                        );
                        constants[12] = 1_f32.to_bits();
                        constants[13] = 1_f32.to_bits();
                    }
                    context.UpdateSubresource(
                        &self.layer_constants,
                        0,
                        None,
                        constants.as_ptr().cast(),
                        0,
                        0,
                    );
                    let mut sources = [
                        Some(input_view.clone()),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        layer.particle_next.as_ref().map(|(region, _, _)| {
                            self.textures
                                .iter()
                                .find(|texture| {
                                    texture.source.ptr_eq(&Arc::downgrade(region.page()))
                                })
                                .expect("next particle page uploaded")
                                .view
                                .clone()
                        }),
                        None,
                    ];
                    if let Some((normal, amount)) = &layer.refraction {
                        let scene = self.scene.as_ref().expect("refraction scene admitted");
                        let background = self.background.as_ref().expect("background admitted");
                        context.OMSetRenderTargets(None, None);
                        context.CopyResource(&background.texture, &scene.texture);
                        self.scene_parameters(
                            context,
                            output.size,
                            normal.extent(),
                            *amount,
                            composition.hdr,
                            false,
                        );
                        sources[5] = Some(background.source.clone());
                        sources[6] = Some(
                            self.textures
                                .iter()
                                .find(|texture| {
                                    texture.source.ptr_eq(&Arc::downgrade(normal.page()))
                                })
                                .expect("normal uploaded")
                                .view
                                .clone(),
                        );
                        context.PSSetShader(&self.refract_pixel, None);
                    } else if let Some((rays, time)) = &layer.rays {
                        let noise = rays.noise.extent();
                        let gradient = rays.gradient.extent();
                        let h = rays.homography;
                        let constants = [
                            h[0].to_bits(),
                            h[1].to_bits(),
                            h[2].to_bits(),
                            h[3].to_bits(),
                            h[4].to_bits(),
                            h[5].to_bits(),
                            h[6].to_bits(),
                            h[7].to_bits(),
                            rays.start_color[0].to_bits(),
                            rays.start_color[1].to_bits(),
                            rays.start_color[2].to_bits(),
                            rays.exponent.to_bits(),
                            rays.end_color[0].to_bits(),
                            rays.end_color[1].to_bits(),
                            rays.end_color[2].to_bits(),
                            rays.intensity.to_bits(),
                            rays.feather[0].to_bits(),
                            rays.feather[1].to_bits(),
                            rays.radius.to_bits(),
                            rays.smoothness.to_bits(),
                            rays.scale[0].to_bits(),
                            rays.scale[1].to_bits(),
                            rays.noise_amount.to_bits(),
                            rays.noise_scale.to_bits(),
                            ((time.as_secs_f64() * f64::from(rays.speed)).rem_euclid(1.) as f32)
                                .to_bits(),
                            0,
                            noise[0],
                            noise[1],
                            gradient[0],
                            gradient[1],
                            0,
                            0,
                        ];
                        context.UpdateSubresource(
                            &self.ray_constants,
                            0,
                            None,
                            constants.as_ptr().cast(),
                            0,
                            0,
                        );
                        context.VSSetConstantBuffers(6, Some(&[Some(self.ray_constants.clone())]));
                        context.PSSetConstantBuffers(6, Some(&[Some(self.ray_constants.clone())]));
                        for (slot, region) in [(3, &rays.noise), (4, &rays.gradient)] {
                            sources[slot] = Some(
                                self.textures
                                    .iter()
                                    .find(|t| t.source.ptr_eq(&Arc::downgrade(region.page())))
                                    .expect("ray input uploaded")
                                    .view
                                    .clone(),
                            );
                        }
                        context.PSSetShader(&self.ray_pixel, None);
                    } else if let Some((motion, time)) =
                        effect.and_then(|effect| effect.image_motion())
                    {
                        let constants = motion.constants(time);
                        context.UpdateSubresource(
                            &self.cloud_constants,
                            0,
                            None,
                            constants.as_ptr().cast(),
                            0,
                            0,
                        );
                        context
                            .PSSetConstantBuffers(7, Some(&[Some(self.cloud_constants.clone())]));
                        for (slot, mask) in [
                            (4, &motion.mask),
                            (3, &motion.phase_mask),
                            (8, &motion.opacity_mask),
                        ] {
                            sources[slot] = mask.as_ref().map(|mask| {
                                self.textures
                                    .iter()
                                    .find(|texture| {
                                        texture.source.ptr_eq(&Arc::downgrade(mask.page()))
                                    })
                                    .expect("motion mask uploaded")
                                    .view
                                    .clone()
                            });
                        }
                        context.PSSetShader(&self.motion_pixel, None);
                    } else if let Some((motion, time)) =
                        effect.and_then(|effect| effect.flow_motion())
                    {
                        let p = &motion.profile;
                        let mask = motion.mask.extent();
                        let phase = motion.phase.extent();
                        let constants = [
                            p.strength.to_bits(),
                            p.feather.to_bits(),
                            ((time.as_secs_f64() * f64::from(p.speed) * 0.1).rem_euclid(1.) as f32)
                                .to_bits(),
                            p.phase_scale.to_bits(),
                            mask[0],
                            mask[1],
                            phase[0],
                            phase[1],
                            0,
                            0,
                            0,
                            0,
                            0,
                            0,
                            0,
                            0,
                        ];
                        context.UpdateSubresource(
                            &self.cloud_constants,
                            0,
                            None,
                            constants.as_ptr().cast(),
                            0,
                            0,
                        );
                        context
                            .PSSetConstantBuffers(5, Some(&[Some(self.cloud_constants.clone())]));
                        for (slot, region) in [(3, &motion.mask), (4, &motion.phase)] {
                            sources[slot] = Some(
                                self.textures
                                    .iter()
                                    .find(|texture| {
                                        texture.source.ptr_eq(&Arc::downgrade(region.page()))
                                    })
                                    .expect("flow input uploaded")
                                    .view
                                    .clone(),
                            );
                        }
                        context.PSSetShader(&self.flow_pixel, None);
                    } else if let Some((motion, time)) =
                        effect.and_then(|effect| effect.cloud_motion())
                    {
                        let settings = motion.settings();
                        let (sin, cos) = settings.direction_radians().sin_cos();
                        let [grain, horizontal] = settings.granularity();
                        let mask = motion.mask();
                        let noise_origin = motion.noise().origin();
                        let noise_extent = motion.noise().extent();
                        let mask_origin = mask.map_or([0, 0], |mask| mask.origin());
                        let mask_extent = mask.map_or([1, 1], |mask| mask.extent());
                        let constants = [
                            (-sin * settings.amount()).to_bits(),
                            (cos * settings.amount()).to_bits(),
                            (grain * horizontal).to_bits(),
                            grain.to_bits(),
                            settings.phase(time).to_bits(),
                            u32::from(mask.is_some()),
                            noise_origin[0],
                            noise_origin[1],
                            noise_extent[0],
                            noise_extent[1],
                            mask_origin[0],
                            mask_origin[1],
                            mask_extent[0],
                            mask_extent[1],
                            0,
                            0,
                        ];
                        context.UpdateSubresource(
                            &self.cloud_constants,
                            0,
                            None,
                            constants.as_ptr().cast(),
                            0,
                            0,
                        );
                        context
                            .PSSetConstantBuffers(3, Some(&[Some(self.cloud_constants.clone())]));
                        sources[3] = Some(
                            self.textures
                                .iter()
                                .find(|texture| {
                                    texture
                                        .source
                                        .ptr_eq(&Arc::downgrade(motion.noise().page()))
                                })
                                .expect("cloud noise uploaded")
                                .view
                                .clone(),
                        );
                        sources[4] = mask.map(|mask| {
                            self.textures
                                .iter()
                                .find(|texture| texture.source.ptr_eq(&Arc::downgrade(mask.page())))
                                .expect("cloud mask uploaded")
                                .view
                                .clone()
                        });
                        context.PSSetShader(&self.cloud_pixel, None);
                    } else if let Some(blur) = effect
                        .and_then(|effect| effect.filter())
                        .filter(|blur| blur.active())
                    {
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
                        context.PSSetShaderResources(0, Some(&[Some(input_view.clone())]));
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
                        context.PSSetShader(
                            if layer.particle_color.is_some() {
                                &self.particle_pixel
                            } else {
                                &self.layer_pixel
                            },
                            None,
                        );
                    }
                    context.VSSetShader(
                        if layer.rays.is_some() {
                            &self.ray_vertex
                        } else {
                            &self.layer_vertex
                        },
                        None,
                    );
                    context.OMSetRenderTargets(Some(&[Some(target.clone())]), None);
                    context.OMSetBlendState(
                        if !final_pass {
                            None
                        } else if additive {
                            Some(&self.additive)
                        } else {
                            Some(&self.blend)
                        },
                        None,
                        u32::MAX,
                    );
                    context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                        Width: draw_size.width() as f32,
                        Height: draw_size.height() as f32,
                        MinDepth: 0.,
                        MaxDepth: 1.,
                        ..Default::default()
                    }]));
                    context.PSSetShaderResources(0, Some(&sources));
                    context.Draw(6, 0);
                    // Unbind all inputs before the next layer reuses scratch as an
                    // output, including when original and mask share a source.
                    context.PSSetShaderResources(
                        0,
                        Some(&[None, None, None, None, None, None, None, None, None]),
                    );
                    context.OMSetRenderTargets(None, None);
                }
            }
            context.OMSetRenderTargets(None, None);
            context.OMSetBlendState(None, None, u32::MAX);
            if let Some(scene) = &self.scene {
                for (index, bloom) in self.bloom.iter().enumerate() {
                    let source = if index == 0 {
                        scene
                    } else {
                        &self.bloom[index - 1]
                    };
                    self.scene_parameters(
                        context,
                        bloom.size,
                        [source.size.width(), source.size.height()],
                        0.,
                        composition.hdr,
                        index == 0,
                    );
                    self.fullscreen(
                        context,
                        &bloom.target,
                        bloom.size,
                        &self.bloom_down,
                        &source.source,
                        None,
                        false,
                    );
                }
                for index in (1..self.bloom.len()).rev() {
                    let source = &self.bloom[index];
                    let destination = &self.bloom[index - 1];
                    self.scene_parameters(
                        context,
                        destination.size,
                        [source.size.width(), source.size.height()],
                        0.,
                        composition.hdr,
                        false,
                    );
                    self.fullscreen(
                        context,
                        &destination.target,
                        destination.size,
                        &self.bloom_up,
                        &source.source,
                        None,
                        true,
                    );
                }
                self.scene_parameters(
                    context,
                    output.size,
                    [output.size.width(), output.size.height()],
                    0.,
                    composition.hdr,
                    !self.bloom.is_empty(),
                );
                self.fullscreen(
                    context,
                    &output.view,
                    output.size,
                    &self.resolve_pixel,
                    &scene.source,
                    self.bloom.first().map(|target| &target.source),
                    false,
                );
            }
        }
    }

    fn scene_parameters(
        &self,
        context: &ID3D11DeviceContext,
        size: Size,
        source: [u32; 2],
        amount: f32,
        hdr: Option<crate::scene::hdr::HdrBloom>,
        first: bool,
    ) {
        let constants = [
            size.width(),
            size.height(),
            source[0],
            source[1],
            amount.to_bits(),
            hdr.map_or(0., |hdr| hdr.threshold).to_bits(),
            hdr.map_or(0., |hdr| hdr.feather).to_bits(),
            hdr.map_or(0., |hdr| hdr.scatter).to_bits(),
            hdr.map_or(0., |hdr| hdr.strength).to_bits(),
            u32::from(first),
            0,
            0,
        ];
        // SAFETY: initialized 48-byte array matches b4 and the worker owns context.
        unsafe {
            context.UpdateSubresource(
                &self.scene_constants,
                0,
                None,
                constants.as_ptr().cast(),
                0,
                0,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fullscreen(
        &self,
        context: &ID3D11DeviceContext,
        target: &ID3D11RenderTargetView,
        size: Size,
        pixel: &ID3D11PixelShader,
        source: &ID3D11ShaderResourceView,
        bloom: Option<&ID3D11ShaderResourceView>,
        additive: bool,
    ) {
        // SAFETY: all views are owned, nonaliasing, and retained for submission.
        unsafe {
            context.VSSetShader(&self.vertex, None);
            context.PSSetShader(pixel, None);
            context.OMSetRenderTargets(Some(&[Some(target.clone())]), None);
            context.OMSetBlendState(
                if additive { Some(&self.additive) } else { None },
                None,
                u32::MAX,
            );
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: size.width() as f32,
                Height: size.height() as f32,
                MinDepth: 0.,
                MaxDepth: 1.,
                ..Default::default()
            }]));
            context.PSSetShaderResources(5, Some(&[Some(source.clone()), bloom.cloned()]));
            context.Draw(3, 0);
            context.PSSetShaderResources(5, Some(&[None, None]));
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
