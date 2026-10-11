//! Independent two-phase flow-map advection; no vendor shader translation.
use super::{InspectionError, RawEffect, snapshot};
use crate::{TextureDecoder, TexturePage, TextureRegion, assets::package::asset_key};
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FlowProfile {
    pub strength: f32,
    pub speed: f32,
    pub feather: f32,
    pub phase_scale: f32,
    pub mask_asset: String,
    pub phase_asset: String,
}

#[derive(Debug)]
pub(crate) struct FlowMotion {
    pub profile: FlowProfile,
    pub mask: Arc<TextureRegion>,
    pub phase: Arc<TextureRegion>,
}

pub(super) fn load(
    decoder: &TextureDecoder,
    effect: &RawEffect,
) -> Result<FlowProfile, InspectionError> {
    let invalid = || InspectionError::invalid(&effect.file, "unsupported water-flow profile");
    let definition = snapshot::json(&decoder.package, &effect.file)?;
    if definition["replacementkey"] != "waterflow"
        || definition["version"] != 1
        || effect.passes.len() != 1
    {
        return Err(invalid());
    }
    let passes = definition["passes"]
        .as_array()
        .filter(|p| p.len() == 1)
        .ok_or_else(invalid)?;
    let material = passes[0]["material"].as_str().ok_or_else(invalid)?;
    let material = snapshot::json(&decoder.package, material)?;
    if material["passes"].as_array().is_none_or(|p| p.len() != 1)
        || material["passes"][0]["shader"] != "effects/waterflow"
    {
        return Err(invalid());
    }
    let pass = &effect.passes[0];
    if pass.as_object().is_none_or(|m| {
        m.keys()
            .any(|k| !["id", "textures", "constantshadervalues", "combos"].contains(&k.as_str()))
    }) || pass
        .get("combos")
        .is_some_and(|v| v.as_object().is_none_or(|m| !m.is_empty()))
    {
        return Err(invalid());
    }
    let constants = pass.get("constantshadervalues");
    if constants.is_some_and(|v| {
        v.as_object().is_none_or(|m| {
            m.keys()
                .any(|k| !["strength", "speed", "feather", "phasescale"].contains(&k.as_str()))
        })
    }) {
        return Err(invalid());
    }
    let scalar = |name, default, maximum| {
        let value = snapshot::scalar(constants.and_then(|v| v.get(name)), default, &effect.file)?;
        if !(0. ..=maximum).contains(&value) {
            return Err(invalid());
        }
        Ok(value)
    };
    let textures = pass["textures"]
        .as_array()
        .filter(|v| v.len() == 3 && v[0].is_null())
        .ok_or_else(invalid)?;
    let asset = |index: usize| {
        asset_key(&format!(
            "materials/{}.tex",
            textures[index].as_str().ok_or_else(invalid)?
        ))
    };
    Ok(FlowProfile {
        strength: scalar("strength", 1., 2.)?,
        speed: scalar("speed", 1., 10.)?,
        feather: scalar("feather", 0.5, 1.)?,
        phase_scale: scalar("phasescale", 0.01, 1.)?,
        mask_asset: asset(1)?,
        phase_asset: asset(2)?,
    })
}

impl FlowProfile {
    pub(super) fn prepare(
        &self,
        decoder: &TextureDecoder,
        pages: &mut BTreeMap<(String, u32), Arc<TexturePage>>,
        bytes: &mut u64,
    ) -> Result<Arc<FlowMotion>, InspectionError> {
        let mut region = |asset: &str| {
            let page = snapshot::page(decoder, asset, 0, pages, bytes)?;
            let extent = page.extent();
            Ok(Arc::new(TextureRegion::new(page, [0, 0], extent).map_err(
                |e| InspectionError::invalid(asset, e.to_string()),
            )?))
        };
        Ok(Arc::new(FlowMotion {
            profile: self.clone(),
            mask: region(&self.mask_asset)?,
            phase: region(&self.phase_asset)?,
        }))
    }
}
