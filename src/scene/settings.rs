use serde::Serialize;

/// Sampling for ordinary image layers. Output resolution is set with [`crate::Size`].
/// Cloud effects and particles retain their own bilinear sampling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum ImageQuality {
    /// Point sampling: preserves texel edges and existing renderer behavior.
    #[default]
    Nearest,
    /// Bilinear sampling: smoother image scaling, clamped inside each atlas crop.
    /// This does not add mipmaps, HDR, or recover missing source detail.
    Linear,
}

/// Consumer policy applied during scene preparation and playback.
///
/// Defaults preserve existing playback behavior. Opening remains paused; call
/// [`super::ImagePlayback::play`] to animate when `animation` is enabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PlaybackSettings {
    /// Sampling for ordinary images, including the output of image blur effects.
    pub image_quality: ImageQuality,
    /// Allow the clock to advance. False holds time zero; explicit seeks still work.
    pub animation: bool,
    /// Prepare image effects and particles. False skips their assets entirely.
    pub visual_effects: bool,
    /// Omit all particle effects, including fireworks and star flicker.
    /// Image effects and sprite imagery remain enabled. This is not automatic
    /// flash detection or a guarantee about arbitrary content.
    pub reduce_flashing: bool,
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            image_quality: ImageQuality::Nearest,
            animation: true,
            visual_effects: true,
            reduce_flashing: false,
        }
    }
}
