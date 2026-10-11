//! Prepared ASCII glyph atlases and checked calendar/clock text profiles.
//! Arbitrary SceneScript is rejected; sampling performs no font or script I/O.
use super::{ImageLayer, InspectionError, PreciseBlur, RawObject, snapshot};
use crate::{TextureDecoder, TexturePage, TextureRegion};
use chrono::{Datelike, Local, Timelike};
use fontdue::{
    Font, FontSettings,
    layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Arc};

enum Content {
    Static(String),
    Clock {
        twenty_four: bool,
        seconds: bool,
        delimiter: String,
    },
    Calendar {
        weekday_only: bool,
        show_day: bool,
        full_day: bool,
        month: u8,
        delimiter: String,
        vertical: bool,
    },
}
struct Glyphs {
    font: Font,
    regions: BTreeMap<char, Arc<TextureRegion>>,
    pixels: f32,
    page: Arc<TexturePage>,
}
pub(super) struct TextLayer {
    pub id: u64,
    content: Content,
    glyphs: Arc<Glyphs>,
    origin: [f32; 2],
    scale: [f32; 2],
    angle: f32,
    brightness: f32,
    opacity: f32,
    blur: Option<PreciseBlur>,
}

fn content(value: &Value, asset: &str) -> Result<Content, InspectionError> {
    let invalid =
        || InspectionError::invalid(asset, "unsupported text value or SceneScript profile");
    if let Some(text) = value.as_str() {
        return checked_string(text, asset).map(Content::Static);
    }
    let Some(script) = value.get("script").and_then(Value::as_str) else {
        return Err(invalid());
    };
    if script.len() > 16384 {
        return Err(invalid());
    }
    let hash = format!("{:x}", Sha256::digest(script.as_bytes()));
    let props = &value["scriptproperties"];
    let boolean = |name, default| {
        props
            .get(name)
            .map_or(Ok(default), |v| v.as_bool().ok_or_else(invalid))
    };
    let string = |name, default: &str| {
        props.get(name).map_or(Ok(default.to_owned()), |v| {
            v.as_str()
                .ok_or_else(invalid)
                .and_then(|s| checked_string(s, asset))
        })
    };
    if hash == "ebf5e5f476ec0a0e35c9dd5c77a9691e23468d24b58a465488157c9e026e4912" {
        return Ok(Content::Clock {
            twenty_four: boolean("use24hFormat", true)?,
            seconds: boolean("showSeconds", false)?,
            delimiter: string("delimiter", ":")?,
        });
    }
    let weekday_only = match hash.as_str() {
        "dca4b368c3630dec922fd4015afec4962069827b2d45d18af83ff3ab80608f67" => true,
        "2bca0f3a950267440fe733611caee5481f7d60617b318a78ffd60235e29ce1bb" => false,
        _ => return Err(invalid()),
    };
    let month = match string("monthFormat", "1")?.as_str() {
        "1" => 1,
        "2" => 2,
        "3" => 3,
        _ => return Err(invalid()),
    };
    let full_day = match string("dayFormat", "1")?.as_str() {
        "1" => false,
        "2" => true,
        _ => return Err(invalid()),
    };
    Ok(Content::Calendar {
        weekday_only,
        show_day: boolean("showDay", weekday_only)?,
        full_day,
        month,
        delimiter: if boolean("useDelimiter", false)? {
            string("addDelimiter", "/")?
        } else {
            " ".into()
        },
        vertical: boolean("alignVertical", false)?,
    })
}
fn checked_string(text: &str, asset: &str) -> Result<String, InspectionError> {
    if text.len() > 256
        || text
            .chars()
            .any(|c| !(c.is_ascii_graphic() || c == ' ' || c == '\n'))
    {
        return Err(InspectionError::invalid(
            asset,
            "text profile requires at most 256 ASCII characters",
        ));
    }
    Ok(text.to_owned())
}
impl Content {
    fn value(&self, now: chrono::DateTime<Local>) -> String {
        match self {
            Self::Static(text) => text.clone(),
            Self::Clock {
                twenty_four,
                seconds,
                delimiter,
            } => {
                let hour = if *twenty_four {
                    now.hour()
                } else {
                    let h = now.hour() % 12;
                    if h == 0 { 12 } else { h }
                };
                let mut text = format!("-{hour:02}{delimiter}{:02}-", now.minute());
                if *seconds {
                    text += &format!("{delimiter}{:02}", now.second());
                }
                text
            }
            Self::Calendar {
                weekday_only,
                show_day,
                full_day,
                month,
                delimiter,
                vertical,
            } => {
                let day = [
                    "Sunday",
                    "Monday",
                    "Tuesday",
                    "Wednesday",
                    "Thursday",
                    "Friday",
                    "Saturday",
                ][now.weekday().num_days_from_sunday() as usize];
                let day = if *full_day { day } else { &day[..3] };
                if *weekday_only && *show_day {
                    return day
                        .to_uppercase()
                        .chars()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(" ");
                }
                let month = match month {
                    1 => now.month().to_string(),
                    2 => format!(
                        "{}{}{}",
                        if *weekday_only { "" } else { " " },
                        [
                            "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT",
                            "NOV", "DEC"
                        ][now.month0() as usize],
                        if *weekday_only { "" } else { " " }
                    ),
                    _ => [
                        "January",
                        "February",
                        "March",
                        "April",
                        "May",
                        "June",
                        "July",
                        "August",
                        "September",
                        "October",
                        "November",
                        "December",
                    ][now.month0() as usize]
                        .to_owned(),
                };
                let date = format!("{}{delimiter}{month}{delimiter}{}", now.day(), now.year());
                if *show_day {
                    format!("{day}{} {date}", if *vertical { "\n" } else { "" })
                } else {
                    date
                }
            }
        }
    }
}

fn transform(
    objects: &[RawObject],
    id: u64,
    active: &mut Vec<u64>,
) -> Result<([f32; 2], [f32; 2], f32), InspectionError> {
    let asset = format!("scene.json text {id}");
    if active.len() >= 16 || active.contains(&id) {
        return Err(InspectionError::invalid(
            &asset,
            "cyclic or excessive text parents",
        ));
    }
    let object = objects
        .iter()
        .find(|o| o.id == id)
        .ok_or_else(|| InspectionError::invalid(&asset, "text parent is missing"))?;
    active.push(id);
    let origin = snapshot::vector(object.properties.get("origin"), [0.; 3], &asset)?;
    let scale = snapshot::vector(object.properties.get("scale"), [1.; 3], &asset)?;
    let angles = snapshot::vector(object.properties.get("angles"), [0.; 3], &asset)?;
    if origin[2] != 0. || angles[..2] != [0., 0.] || scale.iter().any(|v| *v <= 0. || *v > 16.) {
        return Err(InspectionError::invalid(
            &asset,
            "unsupported text transform",
        ));
    }
    let mut result = ([origin[0], origin[1]], [scale[0], scale[1]], angles[2]);
    if let Some(parent) = object.properties.get("parent") {
        let parent = parent
            .as_u64()
            .ok_or_else(|| InspectionError::invalid(&asset, "invalid text parent"))?;
        let (p, s, a) = transform(objects, parent, active)?;
        if (s[0] - s[1]).abs() > 0.00001 && angles[2] != 0. {
            return Err(InspectionError::invalid(
                &asset,
                "sheared text transform is unsupported",
            ));
        }
        let (sin, cos) = a.to_radians().sin_cos();
        result = (
            [
                p[0] + origin[0] * s[0] * cos - origin[1] * s[1] * sin,
                p[1] + origin[0] * s[0] * sin + origin[1] * s[1] * cos,
            ],
            [scale[0] * s[0], scale[1] * s[1]],
            angles[2] + a,
        );
    }
    active.pop();
    Ok(result)
}

fn glyphs(
    decoder: &TextureDecoder,
    asset: &str,
    pixels: f32,
    bytes: &mut u64,
) -> Result<Arc<Glyphs>, InspectionError> {
    let mut reader = decoder
        .package
        .reader(asset)?
        .ok_or_else(|| InspectionError::invalid(asset, "text font is missing"))?;
    let data = reader.bytes(reader.length, 2 * 1024 * 1024)?;
    let font = Font::from_bytes(data, FontSettings::default())
        .map_err(|e| InspectionError::invalid(asset, e))?;
    let chars: Vec<_> = (32u8..127).map(char::from).collect();
    let metrics: Vec<_> = chars.iter().map(|c| font.metrics(*c, pixels)).collect();
    let width = metrics.iter().map(|m| m.width).max().unwrap_or(0) + 64;
    let height = metrics.iter().map(|m| m.height).max().unwrap_or(0) + 64;
    if width > 512 || height > 512 {
        return Err(InspectionError::invalid(
            asset,
            "text glyph exceeds preparation bounds",
        ));
    }
    let extent = [(width * 16) as u32, (height * 6) as u32];
    let length = u64::from(extent[0]) * u64::from(extent[1]) * 4;
    if length > super::layers::MAX_SOURCE_BYTES - *bytes {
        return Err(InspectionError::invalid(
            asset,
            "text atlas exceeds remaining scene source budget",
        ));
    }
    let mut data = vec![0; length as usize];
    for (index, c) in chars.iter().enumerate() {
        let (m, coverage) = font.rasterize(*c, pixels);
        for y in 0..m.height {
            for x in 0..m.width {
                let at = (((index / 16 * height + 32 + y) * extent[0] as usize)
                    + (index % 16 * width + 32 + x))
                    * 4;
                data[at..at + 4].copy_from_slice(&[255, 255, 255, coverage[y * m.width + x]]);
            }
        }
    }
    let page = Arc::new(TexturePage::rgba(extent, data)?);
    *bytes += length;
    let regions = chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let m = &metrics[i];
            TextureRegion::new(
                page.clone(),
                [(i % 16 * width) as u32, (i / 16 * height) as u32],
                [(m.width + 64) as u32, (m.height + 64) as u32],
            )
            .map(|r| (*c, Arc::new(r)))
            .map_err(|e| InspectionError::invalid(asset, e.to_string()))
        })
        .collect::<Result<_, _>>()?;
    Ok(Arc::new(Glyphs {
        font,
        regions,
        pixels,
        page,
    }))
}

pub(super) fn prepare(
    decoder: &TextureDecoder,
    objects: &[RawObject],
    vfx: bool,
    bytes: &mut u64,
) -> Result<Vec<TextLayer>, InspectionError> {
    let mut fonts = BTreeMap::new();
    let mut result = Vec::new();
    for object in objects {
        let asset = format!("scene.json text {}", object.id);
        let p = &object.properties;
        for (flag, expected) in [
            ("opaquebackground", false),
            ("blockalign", false),
            ("limitrows", false),
            ("limitwidth", false),
            ("limituseellipsis", false),
            ("castshadow", false),
        ] {
            if p.get(flag).is_some_and(|v| v != expected) {
                return Err(InspectionError::invalid(
                    &asset,
                    format!("unsupported text {flag}"),
                ));
            }
        }
        if p.get("horizontalalign").is_some_and(|v| v != "center")
            || p.get("verticalalign").is_some_and(|v| v != "center")
            || p.get("anchor").is_some_and(|v| v != "none")
        {
            return Err(InspectionError::invalid(
                &asset,
                "unsupported text alignment",
            ));
        }
        let font = p
            .get("font")
            .and_then(Value::as_str)
            .ok_or_else(|| InspectionError::invalid(&asset, "missing text font"))?;
        let pixels = snapshot::scalar(p.get("pointsize"), 32., &asset)? * 4.;
        if !(4. ..=256.).contains(&pixels) {
            return Err(InspectionError::invalid(
                &asset,
                "text point size is outside supported bounds",
            ));
        }
        let key = (font.to_owned(), pixels.to_bits());
        if !fonts.contains_key(&key) {
            fonts.insert(key.clone(), glyphs(decoder, font, pixels, bytes)?);
        }
        let (origin, scale, angle) = transform(objects, object.id, &mut Vec::new())?;
        let mut blur = None;
        if vfx {
            for effect in &object.effects {
                if !snapshot::visible(effect.properties.get("visible"), &asset)? {
                    continue;
                }
                if blur.is_some() {
                    return Err(InspectionError::invalid(
                        &asset,
                        "multiple text effects are unsupported",
                    ));
                }
                blur = Some(super::precise::load(&decoder.package, effect)?);
            }
        }
        result.push(TextLayer {
            id: object.id,
            content: content(&p["text"], &asset)?,
            glyphs: fonts[&key].clone(),
            origin,
            scale,
            angle,
            brightness: snapshot::scalar(p.get("brightness"), 1., &asset)?,
            opacity: snapshot::scalar(p.get("alpha"), 1., &asset)?,
            blur,
        });
    }
    Ok(result)
}

impl TextLayer {
    pub(super) fn page(&self) -> Arc<TexturePage> {
        self.glyphs.page.clone()
    }
    pub(super) fn append(
        &self,
        now: chrono::DateTime<Local>,
        layers: &mut Vec<ImageLayer>,
    ) -> Result<(), InspectionError> {
        let text = self.content.value(now);
        if text.len() > 256 {
            return Err(InspectionError::invalid(
                "text sample",
                "formatted text exceeds 256 characters",
            ));
        }
        let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
        layout.reset(&LayoutSettings::default());
        layout.append(
            &[&self.glyphs.font],
            &TextStyle::new(&text, self.glyphs.pixels, 0),
        );
        let width = layout
            .glyphs()
            .iter()
            .map(|g| g.x + g.width as f32)
            .fold(0., f32::max);
        let height = layout.height();
        let (sin, cos) = self.angle.to_radians().sin_cos();
        for g in layout.glyphs() {
            if g.width == 0 || g.height == 0 {
                continue;
            }
            let region = self
                .glyphs
                .regions
                .get(&g.parent)
                .ok_or_else(|| {
                    InspectionError::invalid("text sample", "character is outside prepared atlas")
                })?
                .clone();
            let local = [
                (g.x + g.width as f32 * 0.5 - width * 0.5) * self.scale[0],
                (height * 0.5 - g.y - g.height as f32 * 0.5) * self.scale[1],
            ];
            let center = [
                self.origin[0] + local[0] * cos - local[1] * sin,
                self.origin[1] + local[0] * sin + local[1] * cos,
            ];
            let extent = region.extent();
            let mut layer = ImageLayer::new(
                region,
                center,
                [
                    extent[0] as f32 * self.scale[0],
                    extent[1] as f32 * self.scale[1],
                ],
                self.angle,
                self.opacity,
                self.brightness,
            )
            .map_err(|e| InspectionError::invalid("text sample", e.to_string()))?
            .with_image_quality(super::ImageQuality::Linear);
            if let Some(blur) = self.blur {
                layer = layer.with_precise_blur(blur);
            }
            layers.push(layer);
        }
        Ok(())
    }
}
