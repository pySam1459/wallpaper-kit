//! Independent masked UV animation from authored effect metadata.
use super::{InspectionError, RawEffect, snapshot};
use crate::{
    TextureDecoder, TextureFormat, TexturePage, TextureRegion, assets::package::asset_key,
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

#[derive(Debug, Clone)]
pub(crate) enum MotionProfile {
    Spin {
        center: [f32; 2],
        speed: f32,
    },
    Waves {
        direction: f32,
        scale: f32,
        strength: f32,
        speed: f32,
    },
    Shake {
        speed: f32,
        strength: f32,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct MotionDescription {
    pub profile: MotionProfile,
    mask: Option<String>,
    phase_mask: Option<String>,
    opacity_mask: Option<String>,
}

#[derive(Debug)]
pub(crate) struct ImageMotion {
    pub profile: MotionProfile,
    pub mask: Option<Arc<TextureRegion>>,
    pub phase_mask: Option<Arc<TextureRegion>>,
    pub opacity_mask: Option<Arc<TextureRegion>>,
}

impl MotionProfile {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Spin { .. } => "spin",
            Self::Waves { .. } => "waterwaves",
            Self::Shake { .. } => "shake",
        }
    }
}

pub(super) fn load(
    decoder: &TextureDecoder,
    effect: &RawEffect,
) -> Result<MotionDescription, InspectionError> {
    let invalid =
        || InspectionError::invalid(&effect.file, "unsupported masked image-motion profile");
    let definition = snapshot::json(&decoder.package, &effect.file)?;
    let kind = definition["replacementkey"].as_str().ok_or_else(invalid)?;
    if !matches!(kind, "spin" | "waterwaves" | "shake")
        || !definition["version"]
            .as_u64()
            .is_some_and(|v| (1..=2).contains(&v))
        || effect.passes.len() != 1
        || definition["passes"].as_array().is_none_or(|p| p.len() != 1)
    {
        return Err(invalid());
    }
    let material = snapshot::json(
        &decoder.package,
        definition["passes"][0]["material"]
            .as_str()
            .ok_or_else(invalid)?,
    )?;
    if material["passes"].as_array().is_none_or(|p| p.len() != 1)
        || material["passes"][0]["shader"] != format!("effects/{kind}")
    {
        return Err(invalid());
    }
    let pass = &effect.passes[0];
    if pass.as_object().is_none_or(|p| {
        p.keys()
            .any(|k| !["id", "textures", "combos", "constantshadervalues"].contains(&k.as_str()))
    }) || pass.get("combos").is_some_and(|v| {
        v.as_object().is_none_or(|p| {
            p.iter().any(|(k, v)| {
                v != 0
                    || (kind == "shake"
                        && !["AUDIOPROCESSING", "DIRECTION", "NOISE"].contains(&k.as_str()))
            })
        })
    }) {
        return Err(invalid());
    }
    let constants = pass.get("constantshadervalues");
    let allowed: &[&str] = match kind {
        "spin" => &["center", "speed"],
        "shake" => &["speed", "strength", "bounds", "friction"],
        _ => &["direction", "scale", "speed", "strength", "perspective"],
    };
    if constants.is_some_and(|v| {
        v.as_object()
            .is_none_or(|p| p.keys().any(|k| !allowed.contains(&k.as_str())))
    }) {
        return Err(invalid());
    }
    let scalar = |name, default, minimum, maximum| {
        let value = snapshot::scalar(constants.and_then(|v| v.get(name)), default, &effect.file)?;
        if !(minimum..=maximum).contains(&value) {
            return Err(invalid());
        }
        Ok(value)
    };
    let speed = scalar("speed", 1., -100., 100.)?;
    let profile = if kind == "spin" {
        let center = snapshot::vector(
            constants.and_then(|v| v.get("center")),
            [0.5; 2],
            &effect.file,
        )?;
        if center.iter().any(|v| !(0. ..=1.).contains(v)) {
            return Err(invalid());
        }
        MotionProfile::Spin { center, speed }
    } else if kind == "shake" {
        let bounds = snapshot::vector(
            constants.and_then(|v| v.get("bounds")),
            [0., 1.],
            &effect.file,
        )?;
        let friction = snapshot::vector(
            constants.and_then(|v| v.get("friction")),
            [1.; 2],
            &effect.file,
        )?;
        if bounds != [0., 1.] || friction != [1.; 2] {
            return Err(InspectionError::invalid(
                &effect.file,
                "Shake bounds/friction variant is unsupported",
            ));
        }
        MotionProfile::Shake {
            speed,
            strength: scalar("strength", 0.1, 0., 1.)?,
        }
    } else {
        scalar("perspective", 0., 0., 0.)?;
        MotionProfile::Waves {
            direction: scalar("direction", 0., -65536., 65536.)?,
            scale: scalar("scale", 1., 0., 100.)?,
            strength: scalar("strength", 0.1, 0., 1.)?,
            speed,
        }
    };
    let textures = match pass.get("textures") {
        None => None,
        Some(value) => {
            let textures = value
                .as_array()
                .filter(|v| {
                    (1..=if kind == "shake" { 4 } else { 2 }).contains(&v.len()) && v[0].is_null()
                })
                .ok_or_else(invalid)?;
            Some(textures)
        }
    };
    let texture = |slot: usize| {
        textures
            .and_then(|v| v.get(slot))
            .filter(|v| !v.is_null())
            .map(|v| {
                asset_key(&format!(
                    "materials/{}.tex",
                    v.as_str().ok_or_else(invalid)?
                ))
            })
            .transpose()
    };
    let mask = texture(1)?;
    if kind == "shake" && mask.is_none() {
        return Err(InspectionError::invalid(
            &effect.file,
            "Shake requires a painted direction mask",
        ));
    }
    Ok(MotionDescription {
        profile,
        mask,
        phase_mask: texture(2)?,
        opacity_mask: texture(3)?,
    })
}

impl MotionDescription {
    pub(super) fn prepare(
        &self,
        decoder: &TextureDecoder,
        pages: &mut BTreeMap<(String, u32), Arc<TexturePage>>,
        bytes: &mut u64,
    ) -> Result<Arc<ImageMotion>, InspectionError> {
        let mut prepare_mask = |asset: &Option<String>| {
            asset
                .as_ref()
                .map(|asset| {
                    let page = snapshot::page(decoder, asset, 0, pages, bytes)?;
                    let extent = page.extent();
                    TextureRegion::new(page, [0, 0], extent)
                        .map(Arc::new)
                        .map_err(|e| InspectionError::invalid(asset, e.to_string()))
                })
                .transpose()
        };
        let mask = prepare_mask(&self.mask)?;
        let phase_mask = prepare_mask(&self.phase_mask)?;
        let opacity_mask = prepare_mask(&self.opacity_mask)?;
        if matches!(self.profile, MotionProfile::Shake { .. })
            && mask
                .as_ref()
                .is_some_and(|m| m.page().format() == TextureFormat::R8)
        {
            return Err(InspectionError::invalid(
                self.mask.as_deref().unwrap_or("shake"),
                "Shake direction mask requires color channels",
            ));
        }
        Ok(Arc::new(ImageMotion {
            profile: self.profile.clone(),
            mask,
            phase_mask,
            opacity_mask,
        }))
    }
}

impl ImageMotion {
    pub fn constants(&self, time: Duration) -> [u32; 16] {
        let (kind, speed, a, b, c, d) = match self.profile {
            MotionProfile::Spin { center, speed } => (0, speed, center[0], center[1], 0., 0.),
            MotionProfile::Waves {
                direction,
                scale,
                strength,
                speed,
            } => (1, speed, direction.cos(), direction.sin(), scale, strength),
            MotionProfile::Shake { speed, strength } => (2, speed, strength * strength, 0., 0., 0.),
        };
        let phase =
            (time.as_secs_f64() * f64::from(speed)).rem_euclid(std::f64::consts::TAU) as f32;
        let extent = self.mask.as_ref().map_or([1, 1], |mask| mask.extent());
        [
            kind,
            u32::from(self.mask.is_some()),
            phase.to_bits(),
            u32::from(self.phase_mask.is_some()) | (u32::from(self.opacity_mask.is_some()) << 1),
            a.to_bits(),
            b.to_bits(),
            c.to_bits(),
            d.to_bits(),
            extent[0],
            extent[1],
            self.phase_mask.as_ref().map_or(1, |mask| mask.extent()[0]),
            self.phase_mask.as_ref().map_or(1, |mask| mask.extent()[1]),
            self.opacity_mask
                .as_ref()
                .map_or(1, |mask| mask.extent()[0]),
            self.opacity_mask
                .as_ref()
                .map_or(1, |mask| mask.extent()[1]),
            0,
            0,
        ]
    }
}
