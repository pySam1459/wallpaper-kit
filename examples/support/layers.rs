//! Generated composition with an independent inverse-transform pixel oracle.

use crate::texture_checks;
use scene_renderer::{ImageComposition, ImageLayer, Pixels, Size, TextureDecoder, TextureRegion};
use std::sync::Arc;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub struct Fixture {
    pub composition: Arc<ImageComposition>,
    rgba: Vec<Vec<u8>>,
}

impl Fixture {
    pub fn new(decoder: &TextureDecoder) -> Result<Self> {
        let cases = texture_checks::cases();
        let mut layers = Vec::new();
        let mut rgba = Vec::new();
        let mut shared = None;
        for (name, origin, extent, center, size, angle, opacity, brightness) in [
            (
                "rgba-raw",
                [0, 0],
                [2, 2],
                [12.237, 13.317],
                [20.713, 17.123],
                0.,
                1.,
                1.,
            ),
            (
                "r8-raw",
                [1, 0],
                [2, 1],
                [15.337, 11.713],
                [13.733, 8.117],
                90.,
                0.45,
                1.,
            ),
            (
                "bc3-raw",
                [1, 1],
                [6, 6],
                [20.337, 13.137],
                [19.713, 14.317],
                180.,
                0.6,
                1.,
            ),
            (
                "rgba-raw",
                [1, 1],
                [1, 1],
                [1.717, 2.317],
                [11.717, 9.117],
                0.,
                0.7,
                2.,
            ),
        ] {
            let case = cases
                .iter()
                .find(|case| case.name == name)
                .expect("fixture exists");
            let page = if let Some(page) = &shared
                && name == "rgba-raw"
            {
                Arc::clone(page)
            } else {
                Arc::new(decoder.decode_page(name, 0)?)
            };
            if name == "rgba-raw" {
                shared = Some(page.clone());
            }
            layers.push(ImageLayer::new(
                Arc::new(TextureRegion::new(page, origin, extent)?),
                center,
                size,
                angle,
                opacity,
                brightness,
            )?);
            rgba.push(texture_checks::crop(
                &case.rgba,
                case.extent,
                origin,
                extent,
            ));
        }
        Ok(Self {
            composition: Arc::new(ImageComposition::new(Size::new(32, 24)?, layers)?),
            rgba,
        })
    }

    pub fn check(&self, pixels: &Pixels) -> Result<()> {
        for y in 0..pixels.size.height() {
            for x in 0..pixels.size.width() {
                let point = [
                    (f64::from(x) + 0.5) * 32. / f64::from(pixels.size.width()),
                    24. - (f64::from(y) + 0.5) * 24. / f64::from(pixels.size.height()),
                ];
                let mut expected = [16_u8, 24, 32, 255];
                for (layer, rgba) in self.composition.layers().iter().zip(&self.rgba) {
                    let angle = f64::from(layer.angle_degrees()).to_radians();
                    let (sin, cos) = angle.sin_cos();
                    let dx = point[0] - f64::from(layer.center()[0]);
                    let dy = point[1] - f64::from(layer.center()[1]);
                    let u = (cos * dx + sin * dy) / f64::from(layer.size()[0]) + 0.5;
                    let v = 0.5 - (-sin * dx + cos * dy) / f64::from(layer.size()[1]);
                    if !(0. ..1.).contains(&u) || !(0. ..1.).contains(&v) {
                        continue;
                    }
                    let extent = layer.region().extent();
                    let texel = ((v * f64::from(extent[1])) as usize * extent[0] as usize
                        + (u * f64::from(extent[0])) as usize)
                        * 4;
                    let source = &rgba[texel..texel + 4];
                    let alpha = f64::from(source[3]) / 255. * f64::from(layer.opacity());
                    for channel in 0..3 {
                        let color =
                            (f64::from(source[channel]) * f64::from(layer.brightness())).min(255.);
                        expected[channel] = (color * alpha
                            + f64::from(expected[channel]) * (1. - alpha))
                            .round() as u8;
                    }
                }
                let offset = (y as usize * pixels.size.width() as usize + x as usize) * 4;
                let actual = &pixels.rgba[offset..offset + 4];
                if actual[3] != 255
                    || (0..3).any(|channel| actual[channel].abs_diff(expected[channel]) > 2)
                {
                    return Err(format!(
                        "composition pixel ({x},{y}): expected {expected:?}, got {actual:?}"
                    )
                    .into());
                }
            }
        }
        Ok(())
    }
}
