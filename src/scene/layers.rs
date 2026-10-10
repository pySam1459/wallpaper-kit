use crate::{Error, Size, TexturePage, TextureRegion};
use std::sync::Arc;

pub(crate) const MAX_SOURCE_BYTES: u64 = 256 * 1024 * 1024;

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
/// excludes output/consumer textures, driver overhead and caller-owned content.
#[derive(Debug)]
pub struct ImageComposition {
    canvas: Size,
    layers: Vec<ImageLayer>,
    pages: Vec<Arc<TexturePage>>,
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
        for layer in &layers {
            let page = layer.region.page();
            if !pages.iter().any(|existing| Arc::ptr_eq(existing, page)) {
                bytes += page.data().len() as u64;
                if bytes > MAX_SOURCE_BYTES {
                    return Err(Error::ImageCompositionLimit);
                }
                pages.push(page.clone());
            }
        }
        Ok(Self {
            canvas,
            layers,
            pages,
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
}
