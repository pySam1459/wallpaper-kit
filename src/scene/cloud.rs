//! Checked cloud-motion profile and its prepared texture inputs.
use super::{InspectionError, RawEffect, snapshot};
use crate::{Error, TextureDecoder, TexturePage, TextureRegion};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

/// Source-normalized cloud displacement and repeating noise-coordinate motion.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct CloudMotionSettings {
    amount: f32,
    direction_radians: f32,
    granularity: [f32; 2],
    speed: f32,
}

impl CloudMotionSettings {
    /// Amount must be 0..=0.2, granularity `[overall, horizontal]` must be
    /// `[0.5..=4, 0.1..=1]`, and speed 0..=0.1 noise periods/second.
    /// Direction must be finite and is normalized to 0..2π. Zero speed freezes
    /// noise motion. Returns [`Error::InvalidCloudMotion`] for invalid values.
    pub fn new(
        amount: f32,
        direction_radians: f32,
        granularity: [f32; 2],
        speed: f32,
    ) -> Result<Self, Error> {
        if [
            amount,
            direction_radians,
            granularity[0],
            granularity[1],
            speed,
        ]
        .iter()
        .any(|v| !v.is_finite())
            || !(0. ..=0.2).contains(&amount)
            || !(0.5..=4.).contains(&granularity[0])
            || !(0.1..=1.).contains(&granularity[1])
            || !(0. ..=0.1).contains(&speed)
        {
            return Err(Error::InvalidCloudMotion);
        }
        Ok(Self {
            amount,
            direction_radians: direction_radians.rem_euclid(std::f32::consts::TAU),
            granularity,
            speed,
        })
    }
    /// Maximum signed displacement as a fraction of source width/height.
    pub fn amount(self) -> f32 {
        self.amount
    }
    /// Displacement direction. Zero samples downward; π/2 samples leftward.
    pub fn direction_radians(self) -> f32 {
        self.direction_radians
    }
    /// Overall noise frequency and its horizontal multiplier.
    pub fn granularity(self) -> [f32; 2] {
        self.granularity
    }
    /// Positive horizontal noise-coordinate advance in periods/second.
    pub fn speed(self) -> f32 {
        self.speed
    }
    pub(crate) fn phase(self, time: Duration) -> f32 {
        (time.as_secs_f64() * f64::from(self.speed)).rem_euclid(1.) as f32
    }
}

/// Prepared cloud motion, retaining immutable noise and optional mask crops.
/// Noise's red channel is sampled bilinearly with repeat inside its crop.
/// The mask is bilinear and crop-clamped; its squared red value scales the
/// displacement. Source sampling is bilinear, crop-clamped, straight RGBA.
#[derive(Debug)]
pub struct CloudMotion {
    settings: CloudMotionSettings,
    noise: Arc<TextureRegion>,
    mask: Option<Arc<TextureRegion>>,
}

impl CloudMotion {
    /// Retain checked inputs without decoding, uploading or allocating pixels.
    pub fn new(
        settings: CloudMotionSettings,
        noise: Arc<TextureRegion>,
        mask: Option<Arc<TextureRegion>>,
    ) -> Self {
        Self {
            settings,
            noise,
            mask,
        }
    }
    /// Checked authored settings, independent of the sample clock.
    pub fn settings(&self) -> CloudMotionSettings {
        self.settings
    }
    /// Repeating red-channel noise, including custom crops.
    pub fn noise(&self) -> &Arc<TextureRegion> {
        &self.noise
    }
    /// Optional stationary mask in normalized source coordinates.
    pub fn mask(&self) -> Option<&Arc<TextureRegion>> {
        self.mask.as_ref()
    }
}

/// Applied cloud profile and logical input identities from scene preparation.
#[derive(Debug, Clone, Serialize)]
pub struct CloudMotionReport {
    /// Checked effect settings.
    pub settings: CloudMotionSettings,
    /// Static noise TEX, defaulting to the engine's `util/perlin_256`.
    pub noise_asset: String,
    /// Optional authored stationary mask TEX.
    pub mask_asset: Option<String>,
}

fn invalid(asset: &str) -> InspectionError {
    InspectionError::invalid(
        asset,
        "unsupported cloud-motion profile, parameters or bindings",
    )
}
fn fields(value: &Value, allowed: &[&str], asset: &str) -> Result<(), InspectionError> {
    if value
        .as_object()
        .is_none_or(|map| map.keys().any(|k| !allowed.contains(&k.as_str())))
    {
        return Err(invalid(asset));
    }
    Ok(())
}
fn combinations(value: Option<&Value>, mask: bool, asset: &str) -> Result<(), InspectionError> {
    if let Some(value) = value {
        fields(value, &["MASK"], asset)?;
        if value
            .get("MASK")
            .is_some_and(|v| v.as_u64() != Some(u64::from(mask)))
        {
            return Err(invalid(asset));
        }
    }
    Ok(())
}
fn parameters(effect: &RawEffect) -> Result<CloudMotionReport, InspectionError> {
    if effect
        .properties
        .keys()
        .any(|k| !["id", "name", "visible"].contains(&k.as_str()))
        || effect.passes.len() > 1
    {
        return Err(invalid(&effect.file));
    }
    let mut values = [0.1, std::f32::consts::FRAC_PI_2, 2., 0.5, 0.02];
    let mut noise = "util/perlin_256".to_owned();
    let mut mask = None;
    if let Some(pass) = effect.passes.first() {
        fields(
            pass,
            &["id", "combos", "constantshadervalues", "textures"],
            &effect.file,
        )?;
        if let Some(constants) = pass.get("constantshadervalues") {
            let names = [
                "ui_editor_properties_amount",
                "ui_editor_properties_direction",
                "ui_editor_properties_granularity",
                "ui_editor_properties_granularity_horizontal",
                "ui_editor_properties_speed",
            ];
            fields(constants, &names, &effect.file)?;
            for (i, name) in names.iter().enumerate() {
                if let Some(value) = constants.get(name) {
                    values[i] = value.as_f64().ok_or_else(|| invalid(&effect.file))? as f32;
                }
            }
        }
        if let Some(textures) = pass.get("textures") {
            let textures = textures
                .as_array()
                .filter(|v| v.len() <= 3)
                .ok_or_else(|| invalid(&effect.file))?;
            if textures.first().is_some_and(|v| !v.is_null()) {
                return Err(invalid(&effect.file));
            }
            for (index, value) in textures.iter().enumerate().skip(1) {
                if !value.is_null() {
                    let name = value
                        .as_str()
                        .ok_or_else(|| invalid(&effect.file))?
                        .to_owned();
                    if index == 1 {
                        mask = Some(name);
                    } else {
                        noise = name;
                    }
                }
            }
        }
        combinations(pass.get("combos"), mask.is_some(), &effect.file)?;
    }
    let settings =
        CloudMotionSettings::new(values[0], values[1], [values[2], values[3]], values[4])
            .map_err(|_| invalid(&effect.file))?;
    let texture = |name: &str| {
        crate::assets::package::asset_key(&format!("materials/{name}.tex"))
            .map(|s| s.to_ascii_lowercase())
    };
    Ok(CloudMotionReport {
        settings,
        noise_asset: texture(&noise)?,
        mask_asset: mask.as_deref().map(texture).transpose()?,
    })
}

pub(super) fn load(
    decoder: &TextureDecoder,
    effect: &RawEffect,
) -> Result<CloudMotionReport, InspectionError> {
    let profile = parameters(effect)?;
    let package = &decoder.package;
    let definition = snapshot::json(package, &effect.file)?;
    fields(
        &definition,
        &[
            "version",
            "replacementkey",
            "passes",
            "dependencies",
            "editable",
            "group",
            "name",
            "description",
            "preview",
            "performance",
        ],
        &effect.file,
    )?;
    if definition["version"] != 2 || definition["replacementkey"] != "cloudmotion" {
        return Err(invalid(&effect.file));
    }
    let passes = definition["passes"]
        .as_array()
        .filter(|p| p.len() == 1)
        .ok_or_else(|| invalid(&effect.file))?;
    fields(&passes[0], &["material"], &effect.file)?;
    let material = passes[0]["material"]
        .as_str()
        .ok_or_else(|| invalid(&effect.file))?;
    let data = snapshot::json(package, material)?;
    fields(&data, &["passes"], material)?;
    let passes = data["passes"]
        .as_array()
        .filter(|p| p.len() == 1)
        .ok_or_else(|| invalid(material))?;
    let pass = &passes[0];
    fields(
        pass,
        &[
            "shader",
            "blending",
            "depthtest",
            "depthwrite",
            "cullmode",
            "combos",
        ],
        material,
    )?;
    if pass["shader"] != "effects/cloudmotion"
        || pass["blending"] != "normal"
        || pass["depthtest"] != "disabled"
        || pass["depthwrite"] != "disabled"
        || pass["cullmode"] != "nocull"
    {
        return Err(invalid(material));
    }
    combinations(pass.get("combos"), profile.mask_asset.is_some(), material)?;
    // Identity checks only. Proprietary code is never compiled or translated.
    for (asset, expected) in [
        (
            "shaders/effects/cloudmotion.vert",
            "108e4ebd4198fda3842ee8d25b5fa3301df429c2c9bda87c2638b1c366a9116f",
        ),
        (
            "shaders/effects/cloudmotion.frag",
            "9481749c00175d40cd7b5a0b9d85c65cdbd484967ed6f479a4d81924c493050f",
        ),
        (
            "shaders/common.h",
            "acf56edaeca256e6d40b4fc29ababe1f9742197300c418589137911bc8f82ed1",
        ),
    ] {
        let mut reader = package.reader(asset)?.ok_or_else(|| invalid(asset))?;
        if reader.length > 64 * 1024 || reader.hash()? != expected {
            return Err(invalid(asset));
        }
    }
    for asset in std::iter::once(&profile.noise_asset).chain(profile.mask_asset.as_ref()) {
        let mut reader = package.reader(asset)?.ok_or_else(|| invalid(asset))?;
        let metadata = crate::assets::texture::inspect(&mut reader)?;
        if metadata.frames != 0
            || metadata.pages.len() != 1
            || metadata.extent != metadata.pages[0].extent
        {
            return Err(invalid(asset));
        }
    }
    Ok(profile)
}

impl CloudMotionReport {
    pub(super) fn prepare(
        &self,
        decoder: &TextureDecoder,
        pages: &mut BTreeMap<(String, u32), Arc<TexturePage>>,
        bytes: &mut u64,
    ) -> Result<Arc<CloudMotion>, InspectionError> {
        let mut region = |asset: &str| {
            let page = snapshot::page(decoder, asset, 0, pages, bytes)?;
            let extent = page.extent();
            TextureRegion::new(page, [0, 0], extent)
                .map(Arc::new)
                .map_err(|e| InspectionError::invalid(asset, e.to_string()))
        };
        let noise = region(&self.noise_asset)?;
        let mask = self.mask_asset.as_deref().map(region).transpose()?;
        Ok(Arc::new(CloudMotion::new(self.settings, noise, mask)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn effect(passes: Value) -> RawEffect {
        serde_json::from_value(json!({"file":"effects/cloudmotion/effect.json","passes":passes}))
            .unwrap()
    }
    #[test]
    fn authored_settings_and_unsupported_overrides_are_checked_before_io() {
        let defaults = parameters(&effect(json!([]))).unwrap();
        assert_eq!(defaults.settings.granularity(), [2., 0.5]);
        assert_eq!(defaults.noise_asset, "materials/util/perlin_256.tex");
        let profile = parameters(&effect(json!([{"textures":[null,"mask","custom"],"combos":{"MASK":1},"constantshadervalues":{"ui_editor_properties_amount":0.02,"ui_editor_properties_speed":0.025}}]))).unwrap();
        assert_eq!(profile.settings.amount(), 0.02);
        assert_eq!(profile.mask_asset.as_deref(), Some("materials/mask.tex"));
        for passes in [
            json!([{}, {}]),
            json!([{"textures":["source"]}]),
            json!([{"textures":[null,"../mask"]}]),
            json!([{"combos":{"MASK":1}}]),
            json!([{"combos":{"OTHER":0}}]),
            json!([{"constantshadervalues":{"unknown":1}}]),
            json!([{"constantshadervalues":{"ui_editor_properties_amount":0.3}}]),
            json!([{"constantshadervalues":{"ui_editor_properties_speed":"0.02"}}]),
        ] {
            assert!(parameters(&effect(passes)).is_err());
        }
    }
}
