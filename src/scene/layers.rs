use crate::{Error, Size, TexturePage, TextureRegion};
use std::sync::Arc;

pub(crate) const MAX_SOURCE_BYTES: u64 = 256 * 1024 * 1024;

/// Separable Gaussian blur in source-crop texels, before layer composition.
///
/// The normalized kernel reaches three standard deviations, clamping samples
/// to the crop edge. Filtering and mask mixing use premultiplied RGBA code
/// values to avoid transparent-color fringes. This operation does not establish
/// compatibility with Wallpaper Engine's precise-blur parameter conventions.
#[derive(Debug)]
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

#[derive(Debug)]
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

/// A cropped image placed on a two-dimensional orthographic canvas.
///
/// The region retains its shared source page. All values are checked at
/// construction; rendering uses point sampling and straight-alpha SDR blending.
#[derive(Debug)]
pub struct ImageLayer {
    region: Arc<TextureRegion>,
    center: [f32; 2],
    size: [f32; 2],
    angle: f32,
    opacity: f32,
    brightness: f32,
    blur: Option<LayerBlur>,
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
            angle: angle.rem_euclid(360.),
            opacity,
            brightness,
            blur: None,
        })
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

    /// Apply one checked blur before placing and blending this image. The
    /// source and mask join the composition's existing unique-page budget.
    pub fn with_blur(mut self, blur: GaussianBlur) -> Self {
        self.blur = Some(LayerBlur::Gaussian(blur));
        self
    }
    /// Optional premultiplied Gaussian blur. Precise blur uses a separate accessor.
    pub fn blur(&self) -> Option<&GaussianBlur> {
        match self.blur.as_ref() {
            Some(LayerBlur::Gaussian(blur)) => Some(blur),
            _ => None,
        }
    }
    /// Replace any existing blur with the checked unmasked precise operation.
    pub fn with_precise_blur(mut self, blur: PreciseBlur) -> Self {
        self.blur = Some(LayerBlur::Precise(blur));
        self
    }
    /// Optional fixed-kernel straight-RGBA blur, before placement and appearance.
    pub fn precise_blur(&self) -> Option<&PreciseBlur> {
        match self.blur.as_ref() {
            Some(LayerBlur::Precise(blur)) => Some(blur),
            _ => None,
        }
    }
    pub(crate) fn filter(&self) -> Option<&LayerBlur> {
        self.blur.as_ref()
    }

    pub(crate) fn axes(&self) -> [[f32; 2]; 2] {
        let (sin, cos) = match self.angle {
            0. => (0., 1.),
            90. => (1., 0.),
            180. => (0., -1.),
            270. => (-1., 0.),
            _ => self.angle.to_radians().sin_cos(),
        };
        [
            [self.size[0] * cos, self.size[0] * sin],
            [self.size[1] * sin, -self.size[1] * cos],
        ]
    }
}

/// Checked ordered image draws, independent of a GPU device or playback timer.
///
/// The canvas fills the output, without preserving aspect ratio. At most 32
/// layers and 256 MiB of unique source-page payloads are admitted. The budget
/// includes masks but excludes output/consumer textures, driver overhead and
/// caller-owned content. Blur uses one additional RGBA16F target capped at
/// 128 MiB; maximum crop width/height across blurred layers determine its size.
#[derive(Debug)]
pub struct ImageComposition {
    canvas: Size,
    layers: Vec<ImageLayer>,
    pages: Vec<Arc<TexturePage>>,
    blur_extent: Option<Size>,
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
        let mut pages = Vec::new();
        let mut bytes = 0;
        for page in layers.iter().flat_map(|layer| {
            std::iter::once(layer.region.page()).chain(
                layer
                    .filter()
                    .and_then(LayerBlur::mask)
                    .map(|mask| mask.page()),
            )
        }) {
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
            if layer.filter().is_some_and(LayerBlur::active) {
                let source = layer.region.extent();
                extent = [extent[0].max(source[0]), extent[1].max(source[1])];
            }
        }
        // One reusable RGBA16F horizontal target; Size's 64 MiB RGBA8 cap
        // bounds this target to 128 MiB. Final vertical filtering draws directly
        // into the output. No target exists for compositions without blur.
        let blur_extent = if extent[0] == 0 {
            None
        } else {
            Some(Size::new(extent[0], extent[1]).map_err(|_| Error::InvalidBlur)?)
        };
        Ok(Self {
            canvas,
            layers,
            pages,
            blur_extent,
        })
    }
    /// Logical canvas dimensions, independent of output resolution.
    pub fn canvas(&self) -> Size {
        self.canvas
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
