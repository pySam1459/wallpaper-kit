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
    Foliage {
        direction: f32,
        ratio: f32,
        scale: f32,
        phase: f32,
        power: f32,
        strength: f32,
        speed: f32,
    },
    Ripple {
        animation_speed: f32,
        scroll_speed: f32,
        direction: f32,
        scale: f32,
        ratio: f32,
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
            Self::Foliage { .. } => "foliagesway",
            Self::Ripple { .. } => "waterripple",
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
    if !matches!(
        kind,
        "spin" | "waterwaves" | "shake" | "foliagesway" | "waterripple"
    ) || !definition["version"]
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
                if kind == "foliagesway" {
                    return !((k == "MODE" && v == 0) || (k == "MASK" && (v == 0 || v == 1)));
                }
                if kind == "waterripple" {
                    return !((["PERSPECTIVE", "SPECULAR"].contains(&k.as_str()) && v == 0)
                        || (k == "MASK" && (v == 0 || v == 1)));
                }
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
        "waterripple" => &[
            "animationspeed",
            "scrollspeed",
            "scrolldirection",
            "scale",
            "ratio",
            "ripplestrength",
        ],
        "foliagesway" => &[
            "speeduv",
            "strength",
            "phase",
            "power",
            "ratio",
            "scale",
            "scrolldirection",
            "cornerweights",
            "directionweights",
        ],
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
    let speed = if kind == "foliagesway" {
        scalar("speeduv", 5., -100., 100.)?
    } else {
        scalar("speed", 1., -100., 100.)?
    };
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
    } else if kind == "foliagesway" {
        // Corner controls are inactive in UV mode, but still validate their data.
        snapshot::vector::<4>(
            constants.and_then(|v| v.get("cornerweights")),
            [1., 1., 0., 0.],
            &effect.file,
        )?;
        snapshot::vector::<2>(
            constants.and_then(|v| v.get("directionweights")),
            [1., 0.2],
            &effect.file,
        )?;
        MotionProfile::Foliage {
            direction: scalar("scrolldirection", 0., -65536., 65536.)?,
            ratio: scalar("ratio", 0.3, 0.01, 10.)?,
            scale: scalar("scale", 0.05, 0., 1.)?,
            phase: scalar("phase", 0.5, 0., 2.)?,
            power: scalar("power", 1., 0.01, 2.)?,
            strength: scalar("strength", 0.4, 0., 1.)?,
            speed,
        }
    } else if kind == "waterripple" {
        MotionProfile::Ripple {
            animation_speed: scalar("animationspeed", 0.15, -10., 10.)?,
            scroll_speed: scalar("scrollspeed", 0., -10., 10.)?,
            direction: scalar("scrolldirection", 0., -65536., 65536.)?,
            scale: scalar("scale", 1., 0., 10.)?,
            ratio: scalar("ratio", 1., 0., 10.)?,
            strength: scalar("ripplestrength", 0.1, 0., 1.)?,
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
                    (1..=match kind {
                        "shake" => 4,
                        "foliagesway" | "waterripple" => 3,
                        _ => 2,
                    })
                        .contains(&v.len())
                        && v[0].is_null()
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
    let mask = if matches!(kind, "foliagesway" | "waterripple") && pass["combos"]["MASK"] == 0 {
        None
    } else {
        texture(1)?
    };
    if kind == "shake" && mask.is_none() {
        return Err(InspectionError::invalid(
            &effect.file,
            "Shake requires a painted direction mask",
        ));
    }
    Ok(MotionDescription {
        profile,
        mask,
        phase_mask: if kind == "foliagesway" {
            Some(texture(2)?.unwrap_or_else(|| "materials/util/noise.tex".into()))
        } else if kind == "waterripple" {
            Some(match texture(2)? {
                Some(normal) => normal,
                None => asset_key(&format!(
                    "materials/{}.tex",
                    material["passes"][0]["textures"][2]
                        .as_str()
                        .unwrap_or("effects/waterripplenormal")
                ))?,
            })
        } else {
            texture(2)?
        },
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
            MotionProfile::Foliage {
                direction,
                ratio,
                strength,
                speed,
                ..
            } => (
                3,
                speed,
                direction.cos(),
                direction.sin(),
                ratio,
                strength * strength * 0.005,
            ),
            MotionProfile::Ripple {
                direction,
                scale,
                strength,
                ..
            } => (
                4,
                1.,
                direction.cos(),
                direction.sin(),
                scale,
                strength * strength,
            ),
        };
        // Sway combines unrelated frequencies, so a single-cycle wrap would jump.
        let phase = time.as_secs_f64() * f64::from(speed);
        let phase = if kind >= 3 {
            phase
        } else {
            phase.rem_euclid(std::f64::consts::TAU)
        } as f32;
        let extent = self.mask.as_ref().map_or([1, 1], |mask| mask.extent());
        let mut constants = [
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
        ];
        if let MotionProfile::Foliage {
            scale,
            phase,
            power,
            ..
        } = self.profile
        {
            constants[3] = scale.to_bits();
            constants[12] = phase.to_bits();
            constants[13] = power.to_bits();
        }
        if let MotionProfile::Ripple {
            animation_speed,
            scroll_speed,
            ratio,
            ..
        } = self.profile
        {
            constants[3] = ratio.to_bits();
            constants[12] = (animation_speed * animation_speed).to_bits();
            constants[13] = (scroll_speed * scroll_speed).to_bits();
        }
        constants
    }
}
