use super::{InspectionError, snapshot};
use serde_json::Value;
use std::collections::BTreeMap;

// shortcut: independent bloom pyramid; calibrate filtering against stable
// engine material probes before claiming Wallpaper Engine fidelity.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HdrBloom {
    pub strength: f32,
    pub threshold: f32,
    pub feather: f32,
    pub scatter: f32,
    pub iterations: u32,
}

impl HdrBloom {
    pub(super) fn load(
        properties: &BTreeMap<String, Value>,
    ) -> Result<Option<Self>, InspectionError> {
        match properties.get("hdr") {
            None | Some(Value::Bool(false)) => return Ok(None),
            Some(Value::Bool(true)) => {}
            _ => {
                return Err(InspectionError::invalid(
                    "scene.json",
                    "hdr must be Boolean",
                ));
            }
        }
        let number = |name: &str, default: f32, low: f32, high: f32| {
            let value = snapshot::scalar(properties.get(name), default, "scene.json")?;
            if !(low..=high).contains(&value) {
                return Err(InspectionError::invalid(
                    "scene.json",
                    format!("{name} outside supported bounds"),
                ));
            }
            Ok(value)
        };
        let iterations = number("bloomhdriterations", 8., 1., 16.)?;
        if iterations.fract() != 0. {
            return Err(InspectionError::invalid(
                "scene.json",
                "bloomhdriterations must be an integer",
            ));
        }
        let strength = number("bloomhdrstrength", 1., 0., 16.)?;
        let bloom = match properties.get("bloom") {
            None | Some(Value::Bool(true)) => true,
            Some(Value::Bool(false)) => false,
            _ => {
                return Err(InspectionError::invalid(
                    "scene.json",
                    "bloom must be Boolean",
                ));
            }
        };
        Ok(Some(Self {
            // Keep float accumulation even when the authored glow is disabled.
            strength: if bloom { strength } else { 0. },
            threshold: number("bloomhdrthreshold", 1., 0., 16.)?,
            feather: number("bloomhdrfeather", 0.1, 0., 1.)?,
            scatter: number("bloomhdrscatter", 1., 0., 2.)?,
            iterations: iterations as u32,
        }))
    }
}
