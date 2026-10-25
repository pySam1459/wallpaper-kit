use crate::{CloudMotion, Error, Size, TexturePage, TextureRegion};
use std::{sync::Arc, time::Duration};

pub(crate) const MAX_SOURCE_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const MAX_IMAGE_EFFECTS: usize = 16;

/// Separable Gaussian blur in source-crop texels, before layer composition.
///
/// The normalized kernel reaches three standard deviations, clamping samples
/// to the crop edge. Filtering and mask mixing use premultiplied RGBA code
/// values to avoid transparent-color fringes. This operation does not establish
/// compatibility with Wallpaper Engine's precise-blur parameter conventions.
#[derive(Debug, Clone)]
pub struct GaussianBlur {
    sigma: [f32; 2],
    mask: Option<Arc<TextureRegion>>,
}

impl GaussianBlur {
    /// Standard deviations must be finite and 0..=8 texels per axis. Zero
    /// disables that axis. Returns [`Error::InvalidBlur`] otherwise. An optional
    /// mask's red channel mixes the original (zero) and blurred (one) image,
    /// sampled with normalized crop coordinates and point filtering. Its alpha
    /// is ignored; the region retains its source page.
    pub fn new(sigma: [f32; 2], mask: Option<Arc<TextureRegion>>) -> Result<Self, Error> {
        if sigma
            .iter()
            .any(|value| !value.is_finite() || !(0. ..=8.).contains(value))
        {
            return Err(Error::InvalidBlur);
        }
        Ok(Self { sigma, mask })
    }
    /// Standard deviations in cropped source texels, horizontal then vertical.
    pub fn sigma(&self) -> [f32; 2] {
        self.sigma
    }
    /// Optional normalized red-channel mask.
    pub fn mask(&self) -> Option<&Arc<TextureRegion>> {
        self.mask.as_ref()
    }
}

/// Fixed 13-texel Gaussian kernel with bilinear paired samples in straight RGBA.
/// Scale changes sampling distance per axis, not the kernel weights. This is
/// the unmasked SDR precise-blur profile; other kernels and masks are unsupported.
#[derive(Debug, Clone, Copy)]
pub struct PreciseBlur {
    scale: [f32; 2],
}

impl PreciseBlur {
    /// Finite sampling scales in 0..=2 source texels. Zero bypasses that axis.
    /// Returns [`Error::InvalidBlur`] otherwise. RGB under zero alpha participates
    /// in this filter. The horizontal result is quantized to eight-bit RGBA.
    pub fn new(scale: [f32; 2]) -> Result<Self, Error> {
        if scale
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=2.).contains(v))
        {
            return Err(Error::InvalidBlur);
        }
        Ok(Self { scale })
    }
    /// Horizontal and vertical sampling distances in source texels.
    pub fn scale(&self) -> [f32; 2] {
        self.scale
    }
}

#[derive(Debug, Clone)]
pub(crate) enum LayerBlur {
    Gaussian(GaussianBlur),
    Precise(PreciseBlur),
}

impl LayerBlur {
    pub(crate) fn parameters(&self) -> ([f32; 2], u32) {
        match self {
            Self::Gaussian(blur) => (blur.sigma(), 0),
            Self::Precise(blur) => (blur.scale(), 1),
        }
    }
    pub(crate) fn mask(&self) -> Option<&Arc<TextureRegion>> {
        match self {
            Self::Gaussian(blur) => blur.mask(),
            Self::Precise(_) => None,
        }
    }
    pub(crate) fn active(&self) -> bool {
        self.parameters().0 != [0.; 2]
    }
}

#[derive(Debug, Clone)]
pub(crate) enum LayerEffect {
    Blur(LayerBlur),
    Cloud {
        motion: Arc<CloudMotion>,
        time: Duration,
    },
    Flow {
        motion: Arc<super::flow::FlowMotion>,
        time: Duration,
    },
    Motion {
        motion: Arc<super::motion::ImageMotion>,
        time: Duration,
    },
}

impl LayerEffect {
    pub(crate) fn at(&self, time: Duration) -> Self {
        let mut effect = self.clone();
        match &mut effect {
            Self::Cloud { time: clock, .. }
            | Self::Flow { time: clock, .. }
            | Self::Motion { time: clock, .. } => *clock = time,
            Self::Blur(_) => {}
        }
        effect
    }
    pub(crate) fn filter(&self) -> Option<&LayerBlur> {
        match self {
            Self::Blur(blur) => Some(blur),
            _ => None,
        }
    }
    pub(crate) fn cloud_motion(&self) -> Option<(&Arc<CloudMotion>, Duration)> {
        match self {
            Self::Cloud { motion, time } => Some((motion, *time)),
            _ => None,
        }
    }
    pub(crate) fn flow_motion(&self) -> Option<(&Arc<super::flow::FlowMotion>, Duration)> {
        match self {
            Self::Flow { motion, time } => Some((motion, *time)),
            _ => None,
        }
    }
    pub(crate) fn image_motion(&self) -> Option<(&Arc<super::motion::ImageMotion>, Duration)> {
        match self {
            Self::Motion { motion, time } => Some((motion, *time)),
            _ => None,
        }
    }
    pub(crate) fn pages(&self) -> impl Iterator<Item = &Arc<TexturePage>> {
        let regions = match self {
            Self::Blur(blur) => [blur.mask(), None, None],
            Self::Cloud { motion, .. } => [Some(motion.noise()), motion.mask(), None],
            Self::Flow { motion, .. } => [Some(&motion.mask), Some(&motion.phase), None],
            Self::Motion { motion, .. } => [
                motion.mask.as_ref(),
                motion.phase_mask.as_ref(),
                motion.opacity_mask.as_ref(),
            ],
        };
        regions.into_iter().flatten().map(|region| region.page())
    }
}

/// A cropped image placed on a two-dimensional orthographic canvas.
///
/// The region retains its shared source page. All values are checked at
/// construction. Manually constructed images use point sampling and straight
/// alpha SDR blending. Prepared scene particles also use these descriptors,
/// with bilinear sampling, tint and their authored blend mode.
/// Scene discovery can attach a normal map for background refraction.
#[derive(Debug)]
pub struct ImageLayer {
    region: Arc<TextureRegion>,
    center: [f32; 2],
    size: [f32; 2],
    coordinate_scale: [f32; 2],
    angle: f32,
    opacity: f32,
    brightness: f32,
    pub(crate) effects: Vec<LayerEffect>,
    pub(crate) particle_color: Option<([f32; 3], bool)>,
    pub(crate) particle_uv: Option<[f32; 6]>,
    pub(crate) particle_next: Option<(Arc<TextureRegion>, [f32; 6], f32)>,
    pub(crate) rays: Option<(Arc<super::rays::Rays>, Duration)>,
    pub(crate) refraction: Option<(Arc<TextureRegion>, f32)>,
    quality: super::ImageQuality,
}

impl ImageLayer {
    /// Place a region using a centre in canvas units, with Y increasing upward.
    /// `size` is the positive displayed width/height. Rotation is in degrees,
    /// counterclockwise about the centre. Opacity must be 0..=1 and brightness
    /// 0..=16. Centres are bounded to +/-65,536, sizes to 65,536; nonfinite values
    /// fail with [`Error::InvalidImageLayer`]. Rotation is normalized to 0..360.
    pub fn new(
        region: Arc<TextureRegion>,
        center: [f32; 2],
        size: [f32; 2],
        angle: f32,
        opacity: f32,
        brightness: f32,
    ) -> Result<Self, Error> {
        Self::validate_geometry(center, size, angle, opacity, brightness)?;
        Ok(Self {
            region,
            center,
            size,
            coordinate_scale: [1.; 2],
            angle: angle.rem_euclid(360.),
            opacity,
            brightness,
            effects: Vec::new(),
            particle_color: None,
            particle_uv: None,
            particle_next: None,
            rays: None,
            refraction: None,
            quality: super::ImageQuality::Nearest,
        })
    }

    /// Choose ordinary image sampling. Cloud effects and particles always use
    /// bilinear sampling. Resolution remains a separate renderer setting.
    pub fn with_image_quality(mut self, quality: super::ImageQuality) -> Self {
        self.quality = quality;
        self
    }

    /// Requested ordinary image sampling.
    pub fn image_quality(&self) -> super::ImageQuality {
        self.quality
    }

    pub(super) fn validate_geometry(
        center: [f32; 2],
        size: [f32; 2],
        angle: f32,
        opacity: f32,
        brightness: f32,
    ) -> Result<(), Error> {
        if center.iter().any(|v| !v.is_finite() || v.abs() > 65_536.)
            || size
                .iter()
                .any(|v| !v.is_finite() || *v <= 0. || *v > 65_536.)
            || !angle.is_finite()
            || !opacity.is_finite()
            || !(0. ..=1.).contains(&opacity)
            || !brightness.is_finite()
            || !(0. ..=16.).contains(&brightness)
        {
            return Err(Error::InvalidImageLayer);
        }
        Ok(())
    }
    /// Cropped source, with rows increasing downward.
    pub fn region(&self) -> &Arc<TextureRegion> {
        &self.region
    }
    /// Centre in canvas units, with Y increasing upward.
    pub fn center(&self) -> [f32; 2] {
        self.center
    }
    /// Displayed width and height in canvas units.
    pub fn size(&self) -> [f32; 2] {
        self.size
    }
    /// Normalized counterclockwise rotation in degrees.
    pub fn angle_degrees(&self) -> f32 {
        self.angle
    }
    /// Alpha multiplier, from zero through one.
    pub fn opacity(&self) -> f32 {
        self.opacity
    }
    /// RGB code-value multiplier, from zero through sixteen.
    pub fn brightness(&self) -> f32 {
        self.brightness
    }

    pub(super) fn scale_coordinates(&mut self, scale: [f32; 2]) {
        for (axis, scale) in scale.into_iter().enumerate() {
            self.center[axis] *= scale;
            self.size[axis] *= scale;
            self.coordinate_scale[axis] *= scale;
        }
    }

    /// Replace any prior effect with checked Gaussian blur before blending. The
    /// source and mask join the composition's existing unique-page budget.
    pub fn with_blur(mut self, blur: GaussianBlur) -> Self {
        self.effects = vec![LayerEffect::Blur(LayerBlur::Gaussian(blur))];
        self
    }
    /// Optional premultiplied Gaussian blur. Precise blur uses a separate accessor.
    pub fn blur(&self) -> Option<&GaussianBlur> {
        match self.filter() {
            Some(LayerBlur::Gaussian(blur)) => Some(blur),
            _ => None,
        }
    }
    /// Replace any prior effect with the checked unmasked precise operation.
    pub fn with_precise_blur(mut self, blur: PreciseBlur) -> Self {
        self.effects = vec![LayerEffect::Blur(LayerBlur::Precise(blur))];
        self
    }
    /// Optional fixed-kernel straight-RGBA blur, before placement and appearance.
    pub fn precise_blur(&self) -> Option<&PreciseBlur> {
        match self.filter() {
            Some(LayerBlur::Precise(blur)) => Some(blur),
            _ => None,
        }
    }
    pub(crate) fn filter(&self) -> Option<&LayerBlur> {
        match self.effects.first() {
            Some(LayerEffect::Blur(blur)) => Some(blur),
            _ => None,
        }
    }
    /// Apply cloud motion at an explicit clock time, replacing any prior effect.
    /// Noise and mask crops join the existing unique-page budget. No I/O,
    /// decoding, timer or scratch target is created by this operation.
    pub fn with_cloud_motion(mut self, motion: Arc<CloudMotion>, time: Duration) -> Self {
        self.effects = vec![LayerEffect::Cloud { motion, time }];
        self
    }
    /// Prepared cloud inputs and the immutable time used for this draw.
    pub fn cloud_motion(&self) -> Option<(&Arc<CloudMotion>, Duration)> {
        match self.effects.first() {
            Some(LayerEffect::Cloud { motion, time }) => Some((motion, *time)),
            _ => None,
        }
    }

    pub(crate) fn axes(&self) -> [[f32; 2]; 2] {
        axes(self.size, self.angle, self.coordinate_scale)
    }
}

pub(crate) fn axes(size: [f32; 2], angle: f32, coordinate_scale: [f32; 2]) -> [[f32; 2]; 2] {
    let (sin, cos) = match angle {
        0. => (0., 1.),
        90. => (1., 0.),
        180. => (0., -1.),
        270. => (-1., 0.),
        _ => angle.to_radians().sin_cos(),
    };
    [
        [
            size[0] * cos,
            size[0] * sin * coordinate_scale[1] / coordinate_scale[0],
        ],
        [
            size[1] * sin * coordinate_scale[0] / coordinate_scale[1],
            -size[1] * cos,
        ],
    ]
}

/// Checked ordered image draws, independent of a GPU device or playback timer.
///
/// The canvas fills the output, without preserving aspect ratio. Construction
/// admits at most 32 images. Scene playback may add up to 4,096 particle quads.
/// Both paths admit 256 MiB of unique source-page payloads. The budget
/// includes masks but excludes output/consumer textures, driver overhead and
/// caller-owned content. Blur uses one additional RGBA16F target capped at
/// 128 MiB; maximum crop width/height across blurred layers determine its size.
/// Scene image chains admit up to 16 effects per image, using two reusable
/// crop-sized targets, capped together at 128 MiB for SDR or 256 MiB for HDR.
/// Scene composition points share those targets and may grow them to the
/// largest admitted composition capture. Adjustable points retain their authored
/// capture resolution; full-screen points capture the native output resolution.
/// At most 32 points add an output-sized scene target and background snapshot
/// in RGBA8 or RGBA16F. All effect target payloads are reported below; these are
/// separate from the source and three-output budgets.
/// Scene HDR/refraction additionally use output-sized RGBA16F targets and a
/// shrinking bloom pyramid, reported in [`crate::OutputStats::effect_texture_bytes`].
#[derive(Debug)]
pub struct ImageComposition {
    canvas: Size,
    pub(crate) background: [f32; 4],
    layers: Vec<ImageLayer>,
    pages: Vec<Arc<TexturePage>>,
    blur_extent: Option<Size>,
    pub(crate) chain_extent: Option<Size>,
    pub(crate) hdr: Option<super::hdr::HdrBloom>,
    pub(crate) compositions: Vec<super::composite::Point>,
}

impl ImageComposition {
    /// Draw `layers` in vector order, back to front, over [`crate::BLANK_RGBA`].
    /// Returns [`Error::ImageCompositionLimit`] for no layers, more than 32,
    /// or source bytes over 256 MiB. Repeated page Arcs count once. Construction
    /// allocates only descriptors; decoding and GPU upload are separate.
    pub fn new(canvas: Size, layers: Vec<ImageLayer>) -> Result<Self, Error> {
        if layers.is_empty() || layers.len() > 32 {
            return Err(Error::ImageCompositionLimit);
        }
        Self::from_layers(canvas, layers, &[], Vec::new())
    }

    pub(crate) fn scene(
        canvas: Size,
        layers: Vec<ImageLayer>,
        retained: &[Arc<TexturePage>],
        compositions: Vec<super::composite::Point>,
    ) -> Result<Self, Error> {
        if layers.len() > 4096 + 32
            || layers
                .iter()
                .filter(|layer| layer.particle_color.is_none())
                .count()
                > 2048
        {
            return Err(Error::ImageCompositionLimit);
        }
        Self::from_layers(canvas, layers, retained, compositions)
    }

    fn from_layers(
        canvas: Size,
        layers: Vec<ImageLayer>,
        retained: &[Arc<TexturePage>],
        compositions: Vec<super::composite::Point>,
    ) -> Result<Self, Error> {
        let mut pages = Vec::new();
        let mut bytes = 0;
        for page in layers
            .iter()
            .flat_map(|layer| {
                [
                    Some(layer.region.page()),
                    layer
                        .particle_next
                        .as_ref()
                        .map(|(region, _, _)| region.page()),
                    layer.refraction.as_ref().map(|(normal, _)| normal.page()),
                    layer.rays.as_ref().map(|(rays, _)| rays.noise.page()),
                    layer.rays.as_ref().map(|(rays, _)| rays.gradient.page()),
                ]
                .into_iter()
                .flatten()
                .chain(layer.effects.iter().flat_map(LayerEffect::pages))
            })
            .chain(
                compositions
                    .iter()
                    .flat_map(|point| point.effects.iter().flat_map(LayerEffect::pages)),
            )
            .chain(retained)
        {
            if !pages.iter().any(|existing| Arc::ptr_eq(existing, page)) {
                bytes += page.data().len() as u64;
                if bytes > MAX_SOURCE_BYTES {
                    return Err(Error::ImageCompositionLimit);
                }
                pages.push(page.clone());
            }
        }
        let mut extent = [0, 0];
        for layer in &layers {
            if layer
                .effects
                .iter()
                .any(|effect| effect.filter().is_some_and(LayerBlur::active))
            {
                let source = layer.region.extent();
                extent = [extent[0].max(source[0]), extent[1].max(source[1])];
            }
        }
        // One reusable RGBA16F horizontal target; Size's 64 MiB RGBA8 cap
        // bounds this target to 128 MiB. Vertical filtering draws into the
        // next chain target or output. No target exists without active blur.
        let blur_extent = if extent[0] == 0 {
            None
        } else {
            Some(Size::new(extent[0], extent[1]).map_err(|_| Error::InvalidBlur)?)
        };
        let mut chain = [0, 0];
        for layer in &layers {
            if layer.effects.len() > MAX_IMAGE_EFFECTS {
                return Err(Error::ImageCompositionLimit);
            }
            if layer.effects.len() > 1 {
                let source = layer.region.extent();
                chain = [chain[0].max(source[0]), chain[1].max(source[1])];
            }
        }
        // Two reusable source-sized targets, independent of layer/pass count.
        let chain_extent = if chain[0] == 0 {
            None
        } else {
            Some(Size::new(chain[0], chain[1]).map_err(|_| Error::ImageCompositionLimit)?)
        };
        Ok(Self {
            canvas,
            background: crate::BLANK_RGBA.map(|v| f32::from(v) / 255.),
            layers,
            pages,
            blur_extent,
            chain_extent,
            hdr: None,
            compositions,
        })
    }
    /// Logical canvas dimensions, independent of output resolution.
    pub fn canvas(&self) -> Size {
        self.canvas
    }
    /// Opaque SDR clear color in normalized RGBA code values. Manual
    /// compositions use [`crate::BLANK_RGBA`]; scene discovery applies the
    /// authored `general.clearcolor` when clearing is enabled.
    pub fn background_color(&self) -> [f32; 4] {
        self.background
    }
    /// Checked image draws in submission order.
    pub fn layers(&self) -> &[ImageLayer] {
        &self.layers
    }
    pub(crate) fn pages(&self) -> &[Arc<TexturePage>] {
        &self.pages
    }
    pub(crate) fn blur_extent(&self) -> Option<Size> {
        self.blur_extent
    }
}
