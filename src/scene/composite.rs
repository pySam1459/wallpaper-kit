//! Accumulated-scene effect points, distinct from source-image layers.
use super::{
    ImageLayer, InspectionError, PlaybackSettings, RawObject, layers::LayerEffect, snapshot,
};
use crate::{Size, TextureDecoder};
use std::time::Duration;

pub(super) struct Description {
    pub id: u64,
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub angle: f32,
    pub opacity: f32,
    pub brightness: f32,
    pub capture: Option<Size>,
    pub effects: Vec<snapshot::EffectDescription>,
}

#[derive(Debug)]
pub(crate) struct Point {
    pub after: usize,
    pub coordinate_scale: [f32; 2],
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub angle: f32,
    pub opacity: f32,
    pub brightness: f32,
    pub capture: Option<Size>,
    pub effects: Vec<LayerEffect>,
}

impl Description {
    pub fn point(
        &self,
        after: usize,
        scale: [f32; 2],
        effects: &[LayerEffect],
        time: Duration,
    ) -> Point {
        Point {
            after,
            coordinate_scale: scale,
            center: [self.center[0] * scale[0], self.center[1] * scale[1]],
            size: [self.size[0] * scale[0], self.size[1] * scale[1]],
            angle: self.angle,
            opacity: self.opacity,
            brightness: self.brightness,
            capture: self.capture,
            effects: effects.iter().map(|effect| effect.at(time)).collect(),
        }
    }
}

impl Point {
    pub fn capture_size(&self, output: Size) -> Size {
        self.capture.unwrap_or(output)
    }
    pub fn active(&self) -> bool {
        !self.effects.is_empty() || self.brightness != 1.
    }
}

pub(super) fn describe(
    decoder: &TextureDecoder,
    object: &RawObject,
    full: bool,
    canvas: [u32; 2],
    settings: PlaybackSettings,
    hdr: bool,
    omissions: &mut Vec<String>,
) -> Result<Description, InspectionError> {
    let tolerant = settings.allow_unsupported_features;
    let context = format!("scene.json composition {}", object.id);
    let p = &object.properties;
    for key in p.keys() {
        if ![
            "origin",
            "size",
            "scale",
            "angles",
            "alpha",
            "brightness",
            "visible",
            "color",
            "colorBlendMode",
            "alignment",
            "copybackground",
            "perspective",
            "config",
            "disablepropagation",
            "parent",
            "parallaxDepth",
            "locktransforms",
            "castshadow",
            "clampuvs",
            "solid",
            "ledsource",
        ]
        .contains(&key.as_str())
        {
            if !tolerant {
                return Err(InspectionError::invalid(
                    &context,
                    format!("unsupported composition property {key}"),
                ));
            }
            omissions.push(format!(
                "Object {}: unsupported composition property {key} is omitted.",
                object.id
            ));
        }
    }
    for (key, expected) in [
        ("copybackground", serde_json::json!(true)),
        ("perspective", serde_json::json!(false)),
        ("color", serde_json::json!("1 1 1")),
        ("colorBlendMode", serde_json::json!(0)),
        ("alignment", serde_json::json!("center")),
    ] {
        let valid = if key == "color" {
            snapshot::vector(p.get(key), [1.; 3], &context)? == [1.; 3]
        } else {
            p.get(key).is_none_or(|value| value == &expected)
        };
        if !valid {
            return Err(InspectionError::invalid(
                &context,
                format!("unsupported composition {key}"),
            ));
        }
    }
    if p.get("config").is_some_and(|config| {
        config.as_object().is_none_or(|map| {
            map.len() != 1 || map.get("passthrough") != Some(&serde_json::Value::Bool(true))
        })
    }) || p.get("disablepropagation").is_some_and(|v| v != false)
        || p.get("parent").is_some()
    {
        return Err(InspectionError::invalid(
            &context,
            "unsupported composition propagation/parent profile",
        ));
    }
    if p.get("castshadow").is_some_and(|value| value != false) {
        return Err(InspectionError::invalid(
            &context,
            "composition shadows are unsupported",
        ));
    }
    if p.contains_key("parallaxDepth") {
        omissions.push(format!("Object {}: parallax is omitted.", object.id));
    }
    let scale = snapshot::vector(p.get("scale"), [1.; 3], &context)?;
    let angles = snapshot::vector(p.get("angles"), [0.; 3], &context)?;
    let origin = snapshot::vector(
        p.get("origin"),
        [canvas[0] as f32 / 2., canvas[1] as f32 / 2., 0.],
        &context,
    )?;
    if full && (scale[..2] != [1.; 2] || angles[2] != 0.) {
        return Err(InspectionError::invalid(
            &context,
            "transformed full-screen composition is unsupported",
        ));
    }
    if origin[2] != 0. || angles[0] != 0. || angles[1] != 0. || scale.iter().any(|v| *v <= 0.) {
        return Err(InspectionError::invalid(
            &context,
            "only 2D composition transforms are supported",
        ));
    }
    let authored_size = if full {
        canvas.map(|v| v as f32)
    } else {
        snapshot::vector(p.get("size"), [0.; 2], &context)?
    };
    let center = if full {
        [canvas[0] as f32 / 2., canvas[1] as f32 / 2.]
    } else {
        [origin[0], origin[1]]
    };
    let size = if full {
        authored_size
    } else {
        [authored_size[0] * scale[0], authored_size[1] * scale[1]]
    };
    let opacity = snapshot::scalar(p.get("alpha"), 1., &context)?;
    let brightness = snapshot::scalar(p.get("brightness"), 1., &context)?;
    ImageLayer::validate_geometry(center, size, angles[2], opacity, brightness)
        .map_err(|e| InspectionError::invalid(&context, e.to_string()))?;
    let capture = if full {
        None
    } else {
        Some(
            Size::new(
                authored_size[0].ceil() as u32,
                authored_size[1].ceil() as u32,
            )
            .map_err(|e| InspectionError::invalid(&context, e.to_string()))?,
        )
    };
    let effects = snapshot::effects(decoder, object, settings, hdr, tolerant, omissions)?;
    Ok(Description {
        id: object.id,
        center,
        size,
        angle: if full { 0. } else { angles[2] },
        opacity,
        brightness,
        capture,
        effects,
    })
}
