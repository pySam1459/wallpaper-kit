//! Generated composition with an independent inverse-transform pixel oracle.

use crate::texture_checks;
use scene_renderer::{
    CloudMotion, CloudMotionSettings, GaussianBlur, ImageComposition, ImageLayer, Pixels,
    PreciseBlur, Size, TextureDecoder, TextureFormat, TextureRegion,
};
use std::{sync::Arc, time::Duration};

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
        let mask_page = Arc::new(decoder.decode_page("r8-raw", 0)?);
        let mask = Arc::new(TextureRegion::new(mask_page.clone(), [0, 0], [3, 2])?);
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
            (
                "rgba-raw",
                [0, 0],
                [2, 2],
                [27.137, 4.717],
                [10.337, 7.713],
                90.,
                0.8,
                1.1,
            ),
            (
                "rgba-raw",
                [0, 0],
                [2, 2],
                [23.917, 17.137],
                [8.517, 9.317],
                0.,
                0.7,
                1.2,
            ),
        ] {
            let case = cases
                .iter()
                .find(|case| case.name == name)
                .expect("fixture exists");
            let page = if name == "r8-raw" {
                mask_page.clone()
            } else if let Some(page) = &shared
                && name == "rgba-raw"
            {
                Arc::clone(page)
            } else {
                Arc::new(decoder.decode_page(name, 0)?)
            };
            if name == "rgba-raw" {
                shared = Some(page.clone());
            }
            let layer = ImageLayer::new(
                Arc::new(TextureRegion::new(page, origin, extent)?),
                center,
                size,
                angle,
                opacity,
                brightness,
            )?;
            layers.push(match layers.len() {
                0 => layer.with_blur(GaussianBlur::new([0.8, 1.4], Some(mask.clone()))?),
                2 => layer.with_blur(GaussianBlur::new([0.4, 0.], None)?),
                4 => layer.with_precise_blur(PreciseBlur::new([0.32, 2.])?),
                5 => layer.with_cloud_motion(
                    Arc::new(CloudMotion::new(
                        CloudMotionSettings::new(0.08, 0.4, [2., 0.5], 0.02)?,
                        Arc::new(TextureRegion::new(mask_page.clone(), [1, 0], [2, 1])?),
                        Some(mask.clone()),
                    )),
                    Duration::from_millis(1337),
                ),
                _ => layer,
            });
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
        // Direct two-dimensional f64 convolution, independent of the GPU's
        // separable RGBA16F intermediate and shader implementation.
        let filtered: Vec<_> = self
            .composition
            .layers()
            .iter()
            .zip(&self.rgba)
            .map(|(layer, rgba)| match layer.precise_blur() {
                Some(blur) => precise_filter(rgba, layer.region().extent(), blur.scale()),
                None => filter(
                    rgba,
                    layer.region().extent(),
                    layer.blur().map_or([0., 0.], GaussianBlur::sigma),
                ),
            })
            .collect();
        for y in 0..pixels.size.height() {
            for x in 0..pixels.size.width() {
                let point = [
                    (f64::from(x) + 0.5) * 32. / f64::from(pixels.size.width()),
                    24. - (f64::from(y) + 0.5) * 24. / f64::from(pixels.size.height()),
                ];
                let mut expected = [16_u8, 24, 32, 255];
                for ((layer, rgba), filtered) in self
                    .composition
                    .layers()
                    .iter()
                    .zip(&self.rgba)
                    .zip(&filtered)
                {
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
                    let mut source = filtered[texel / 4];
                    if let Some((motion, time)) = layer.cloud_motion() {
                        let settings = motion.settings();
                        let [grain, horizontal] = settings.granularity().map(f64::from);
                        let noise_uv = [
                            u * grain * horizontal
                                + (time.as_secs_f64() * f64::from(settings.speed())) % 1.,
                            v * grain,
                        ];
                        let noise = bilinear(
                            &region_rgba(motion.noise()),
                            motion.noise().extent(),
                            noise_uv,
                            true,
                        )[0] / 255.
                            * 2.
                            - 1.;
                        let mask = motion.mask().map_or(1., |mask| {
                            bilinear(&region_rgba(mask), mask.extent(), [u, v], false)[0] / 255.
                        });
                        let amount = noise * mask.powi(2) * f64::from(settings.amount());
                        let angle = f64::from(settings.direction_radians());
                        source = bilinear(
                            rgba,
                            extent,
                            [u - angle.sin() * amount, v + angle.cos() * amount],
                            false,
                        );
                        source[3] /= 255.;
                        for channel in 0..3 {
                            source[channel] *= source[3];
                        }
                    }
                    if let Some(mask) = layer.blur().and_then(GaussianBlur::mask) {
                        let [width, height] = mask.extent();
                        let [left, top] = mask.origin();
                        let mx = left + (u * f64::from(width)) as u32;
                        let my = top + (v * f64::from(height)) as u32;
                        let amount = f64::from(
                            mask.page().data()[(my * mask.page().extent()[0] + mx) as usize],
                        ) / 255.;
                        let original_alpha = f64::from(rgba[texel + 3]) / 255.;
                        let original = [
                            f64::from(rgba[texel]) * original_alpha,
                            f64::from(rgba[texel + 1]) * original_alpha,
                            f64::from(rgba[texel + 2]) * original_alpha,
                            original_alpha,
                        ];
                        for channel in 0..4 {
                            source[channel] =
                                original[channel] * (1. - amount) + source[channel] * amount;
                        }
                    }
                    let alpha = source[3] * f64::from(layer.opacity());
                    for channel in 0..3 {
                        let color = if source[3] == 0. {
                            0.
                        } else {
                            (source[channel] / source[3] * f64::from(layer.brightness())).min(255.)
                        };
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

fn region_rgba(region: &TextureRegion) -> Vec<u8> {
    let page = region.page();
    let rgba = match page.format() {
        TextureFormat::Rgba8 => page.data().to_vec(),
        TextureFormat::R8 => page.data().iter().flat_map(|&v| [v, v, v, 255]).collect(),
        TextureFormat::Bc3 => {
            assert_eq!(page.data(), texture_checks::bc3());
            texture_checks::cases()
                .into_iter()
                .find(|case| case.name == "bc3-raw")
                .expect("BC3 fixture")
                .rgba
        }
    };
    texture_checks::crop(&rgba, page.extent(), region.origin(), region.extent())
}

fn bilinear(rgba: &[u8], extent: [u32; 2], uv: [f64; 2], repeat: bool) -> [f64; 4] {
    let position = [
        uv[0] * f64::from(extent[0]) - 0.5,
        uv[1] * f64::from(extent[1]) - 0.5,
    ];
    let base = position.map(|p| p.floor() as i64);
    let fraction = [
        position[0] - position[0].floor(),
        position[1] - position[1].floor(),
    ];
    let mut result = [0.; 4];
    for dy in 0..2 {
        for dx in 0..2 {
            let coordinate = [base[0] + dx, base[1] + dy];
            let point = std::array::from_fn::<_, 2, _>(|axis| {
                if repeat {
                    coordinate[axis].rem_euclid(i64::from(extent[axis]))
                } else {
                    coordinate[axis].clamp(0, i64::from(extent[axis]) - 1)
                }
            });
            let weight = (if dx == 0 {
                1. - fraction[0]
            } else {
                fraction[0]
            }) * (if dy == 0 {
                1. - fraction[1]
            } else {
                fraction[1]
            });
            let index = ((point[1] as u32 * extent[0] + point[0] as u32) * 4) as usize;
            for channel in 0..4 {
                result[channel] += f64::from(rgba[index + channel]) * weight;
            }
        }
    }
    result
}

fn precise_filter(rgba: &[u8], extent: [u32; 2], scale: [f32; 2]) -> Vec<[f64; 4]> {
    // Expand each paired bilinear sample into integer weighted taps. The CPU
    // oracle then convolves byte arrays; it never samples a GPU intermediate.
    // Coefficients: MIT glsl-fast-gaussian-blur, see THIRD-PARTY-NOTICES.md.
    let kernels = scale.map(|scale| {
        let mut taps = std::collections::BTreeMap::from([(0_i32, 0.1964825501511404)]);
        for (distance, weight) in [
            (1.411764705882353, 0.2969069646728344),
            (3.2941176470588234, 0.09447039785044732),
            (5.176470588235294, 0.010381362401148057),
        ] {
            for sign in [-1., 1.] {
                let offset = sign * distance * f64::from(scale);
                let left = offset.floor() as i32;
                let fraction = offset - f64::from(left);
                *taps.entry(left).or_default() += weight * (1. - fraction);
                *taps.entry(left + 1).or_default() += weight * fraction;
            }
        }
        taps
    });
    let mut horizontal = vec![[0.; 4]; (extent[0] * extent[1]) as usize];
    for y in 0..extent[1] {
        for x in 0..extent[0] {
            let color = &mut horizontal[(y * extent[0] + x) as usize];
            for (&offset, &weight) in &kernels[0] {
                let sx = (x as i32 + offset).clamp(0, extent[0] as i32 - 1) as u32;
                let pixel = ((y * extent[0] + sx) * 4) as usize;
                for channel in 0..4 {
                    color[channel] += f64::from(rgba[pixel + channel]) * weight;
                }
            }
            for value in color {
                *value = value.round();
            }
        }
    }
    let mut result = vec![[0.; 4]; horizontal.len()];
    for y in 0..extent[1] {
        for x in 0..extent[0] {
            let color = &mut result[(y * extent[0] + x) as usize];
            for (&offset, &weight) in &kernels[1] {
                let sy = (y as i32 + offset).clamp(0, extent[1] as i32 - 1) as u32;
                let source = horizontal[(sy * extent[0] + x) as usize];
                for channel in 0..4 {
                    color[channel] += source[channel] * weight;
                }
            }
            color[3] /= 255.;
            for channel in 0..3 {
                color[channel] *= color[3];
            }
        }
    }
    result
}

fn filter(rgba: &[u8], extent: [u32; 2], sigma: [f32; 2]) -> Vec<[f64; 4]> {
    let weights = sigma.map(|sigma| {
        let radius = (3. * f64::from(sigma)).ceil() as i32;
        let mut values: Vec<_> = (-radius..=radius)
            .map(|offset| {
                let weight = if sigma == 0. {
                    1.
                } else {
                    (-0.5 * (f64::from(offset) / f64::from(sigma)).powi(2)).exp()
                };
                (offset, weight)
            })
            .collect();
        let total: f64 = values.iter().map(|(_, weight)| weight).sum();
        for (_, weight) in &mut values {
            *weight /= total;
        }
        values
    });
    let mut result = Vec::new();
    for y in 0..extent[1] as i32 {
        for x in 0..extent[0] as i32 {
            let mut color = [0.; 4];
            for &(dy, wy) in &weights[1] {
                for &(dx, wx) in &weights[0] {
                    let sx = (x + dx).clamp(0, extent[0] as i32 - 1) as usize;
                    let sy = (y + dy).clamp(0, extent[1] as i32 - 1) as usize;
                    let offset = (sy * extent[0] as usize + sx) * 4;
                    let alpha = f64::from(rgba[offset + 3]) / 255.;
                    for channel in 0..3 {
                        color[channel] += f64::from(rgba[offset + channel]) * alpha * wx * wy;
                    }
                    color[3] += alpha * wx * wy;
                }
            }
            result.push(color);
        }
    }
    result
}
