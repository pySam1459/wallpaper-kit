use super::{
    CloudMotion, CloudMotionReport, ImageComposition, ImageLayer, InspectionError, PreciseBlur,
    RawScene,
};
use crate::assets::package::{Package, asset_key};
use crate::{Size, TextureDecoder, TexturePage, TextureRegion};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

/// Explicitly incomplete static image preparation, with visible omissions.
pub struct ImageSnapshot {
    /// Checked ordered draws for `Renderer::render_images`.
    pub composition: Arc<ImageComposition>,
    /// Drawn object IDs in authored scene order, not selection argument order.
    pub object_ids: Vec<u64>,
    /// Omitted scene behavior and objects. Never a full compatibility claim.
    pub omissions: Vec<String>,
}

pub(crate) struct ImagePlan {
    pub canvas: Size,
    pub layers: Vec<LayerDescription>,
    pub object_ids: Vec<u64>,
    pub draw_order: Vec<u64>,
    pub(super) particles: Vec<super::RawObject>,
    pub(super) ray_objects: Vec<super::RawObject>,
    pub(super) text_objects: Vec<super::RawObject>,
    pub(super) hdr: Option<super::hdr::HdrBloom>,
    pub omissions: Vec<String>,
}

pub(crate) struct LayerDescription {
    pub asset: String,
    pub sprite: bool,
    center: [f32; 2],
    size: [f32; 2],
    angle: f32,
    opacity: f32,
    brightness: f32,
    pub(super) effect: Option<EffectDescription>,
}

pub(super) enum EffectDescription {
    Precise(PreciseBlur),
    Cloud(CloudMotionReport),
    Flow(super::flow::FlowProfile),
}

impl LayerDescription {
    pub fn layer(
        &self,
        region: Arc<TextureRegion>,
        cloud: Option<&Arc<CloudMotion>>,
        time: Duration,
    ) -> Result<ImageLayer, InspectionError> {
        let layer = ImageLayer::new(
            region,
            self.center,
            self.size,
            self.angle,
            self.opacity,
            self.brightness,
        )
        .map_err(|error| InspectionError::invalid(&self.asset, error.to_string()))?;
        Ok(match &self.effect {
            Some(EffectDescription::Precise(blur)) => layer.with_precise_blur(*blur),
            Some(EffectDescription::Cloud(_)) => {
                layer.with_cloud_motion(cloud.expect("cloud inputs prepared").clone(), time)
            }
            Some(EffectDescription::Flow(_)) => layer,
            None => layer,
        })
    }
    pub(super) fn cloud_profile(&self) -> Option<&CloudMotionReport> {
        match &self.effect {
            Some(EffectDescription::Cloud(profile)) => Some(profile),
            _ => None,
        }
    }
}

pub(super) fn json(package: &Package, asset: &str) -> Result<Value, InspectionError> {
    let mut reader = package
        .reader(asset)?
        .ok_or_else(|| InspectionError::invalid(asset, "missing JSON asset"))?;
    let bytes = reader.bytes(reader.length, 4 * 1024 * 1024)?;
    serde_json::from_slice(&bytes).map_err(|error| reader.invalid(error.to_string()))
}

pub(super) fn vector<const N: usize>(
    value: Option<&Value>,
    default: [f32; N],
    asset: &str,
) -> Result<[f32; N], InspectionError> {
    let Some(value) = value else {
        return Ok(default);
    };
    let text = value
        .as_str()
        .ok_or_else(|| InspectionError::invalid(asset, "expected a vector string"))?;
    let values: Vec<f32> = text
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|_| InspectionError::invalid(asset, "invalid vector number"))?;
    if values.len() != N || values.iter().any(|value| !value.is_finite()) {
        return Err(InspectionError::invalid(
            asset,
            "invalid vector length or nonfinite component",
        ));
    }
    Ok(values.try_into().expect("component count checked"))
}

pub(super) fn scalar(
    value: Option<&Value>,
    default: f32,
    asset: &str,
) -> Result<f32, InspectionError> {
    match value {
        Some(value) => value
            .as_f64()
            .map(|v| v as f32)
            .filter(|v| v.is_finite())
            .ok_or_else(|| InspectionError::invalid(asset, "expected a finite scalar")),
        None => Ok(default),
    }
}

pub(super) fn visible(value: Option<&Value>, asset: &str) -> Result<bool, InspectionError> {
    let Some(value) = value else {
        return Ok(true);
    };
    let value = if let Some(fields) = value.as_object() {
        if fields.keys().any(|key| key != "value" && key != "user")
            || !fields.get("user").is_some_and(Value::is_string)
        {
            return Err(InspectionError::invalid(
                asset,
                "unsupported visibility binding",
            ));
        }
        fields
            .get("value")
            .ok_or_else(|| InspectionError::invalid(asset, "missing visibility default"))?
    } else {
        value
    };
    value.as_bool().ok_or_else(|| {
        InspectionError::invalid(asset, "visible must be Boolean or a user-property default")
    })
}

pub(crate) fn plan(
    decoder: &TextureDecoder,
    selection: Option<&[u64]>,
    settings: super::PlaybackSettings,
) -> Result<ImagePlan, InspectionError> {
    let selected: Option<BTreeSet<_>> = selection.map(|ids| ids.iter().copied().collect());
    if let (Some(ids), Some(selected)) = (selection, &selected)
        && (selected.is_empty() || selected.len() != ids.len() || selected.len() > 32)
    {
        return Err(InspectionError::invalid(
            "scene.json",
            "select 1..=32 distinct image objects",
        ));
    }
    let raw: RawScene = serde_json::from_value(json(&decoder.package, "scene.json")?)
        .map_err(|error| InspectionError::invalid("scene.json", error.to_string()))?;
    if !matches!(raw.version, 1 | 5) || raw.objects.len() > 4096 {
        return Err(InspectionError::invalid(
            "scene.json",
            "unsupported scene version or object count",
        ));
    }
    let canvas = Size::new(
        raw.general.orthogonalprojection.width,
        raw.general.orthogonalprojection.height,
    )
    .map_err(|error| InspectionError::invalid("scene.json", error.to_string()))?;
    let hdr = if selection.is_none() && settings.visual_effects {
        super::hdr::HdrBloom::load(&raw.general.properties)?
    } else {
        None
    };
    let mut seen = BTreeSet::new();
    let mut layers = Vec::new();
    let mut object_ids = Vec::new();
    let draw_order = raw.objects.iter().map(|object| object.id).collect();
    let mut particles = Vec::new();
    let mut ray_objects = Vec::new();
    let mut text_objects = Vec::new();
    let mut omissions=vec!["General scene camera, color and other postprocessing settings are omitted; the diagnostic canvas fills the output over BLANK_RGBA.".into()];
    if hdr.is_some() {
        omissions.push("HDR bloom uses a bounded floating-point filter pyramid; exact Wallpaper Engine bloom filtering is unverified.".into());
    }
    for object in raw.objects {
        if !seen.insert(object.id) {
            return Err(InspectionError::invalid(
                "scene.json",
                format!("duplicate object {}", object.id),
            ));
        }
        if selected
            .as_ref()
            .is_some_and(|ids| !ids.contains(&object.id))
        {
            omissions.push(format!(
                "Object {} ({}) is not selected.",
                object.id, object.name
            ));
            continue;
        }
        let context = format!("scene.json object {}", object.id);
        let discovered_model = if selected.is_none() {
            if !visible(object.properties.get("visible"), &context)? {
                omissions.push(format!("Object {} is invisible.", object.id));
                continue;
            }
            let selectors = [
                object.image.is_some(),
                object.particle.is_some(),
                object.shape.is_some(),
                object.sound.is_some(),
            ];
            if selectors.iter().filter(|&&present| present).count() > 1 {
                return Err(InspectionError::invalid(
                    &context,
                    "ambiguous object content types",
                ));
            }
            let Some(model) = &object.image else {
                if object.properties.contains_key("text") {
                    text_objects.push(object);
                    continue;
                }
                if object.shape.as_deref() == Some("quad")
                    && object.effects.iter().any(|e| {
                        e.file
                            .eq_ignore_ascii_case("effects/lightshafts/effect.json")
                    })
                    && settings.visual_effects
                {
                    ray_objects.push(object);
                    continue;
                }
                if object.particle.is_some() {
                    if !settings.visual_effects || settings.reduce_flashing {
                        omissions.push(format!(
                            "Object {}: particles omitted by consumer {} settings.",
                            object.id,
                            if settings.reduce_flashing {
                                "reduced-flashing"
                            } else {
                                "visual-effects"
                            }
                        ));
                    } else {
                        particles.push(object);
                    }
                    continue;
                }
                omissions.push(format!(
                    "Object {} ({}): non-image content is omitted.",
                    object.id, object.name
                ));
                continue;
            };
            let model_json = json(&decoder.package, &asset_key(model)?)?;
            if model_json
                .get("projectlayer")
                .is_some_and(|value| value != true && value != false)
            {
                return Err(InspectionError::invalid(
                    model,
                    "projectlayer must be Boolean",
                ));
            }
            if model_json.get("projectlayer") == Some(&Value::Bool(true)) {
                omissions.push(format!(
                    "Object {} ({}): composition layer and its effects are omitted.",
                    object.id, object.name
                ));
                continue;
            }
            Some(model_json)
        } else {
            None
        };
        let model = object
            .image
            .as_ref()
            .filter(|_| {
                object.particle.is_none() && object.shape.is_none() && object.sound.is_none()
            })
            .ok_or_else(|| {
                InspectionError::invalid(&context, "selected object must be an image")
            })?;
        for key in object.properties.keys() {
            if ![
                "origin",
                "size",
                "scale",
                "angles",
                "alpha",
                "brightness",
                "visible",
                "parallaxDepth",
                "disablepropagation",
                "locktransforms",
                "castshadow",
            ]
            .contains(&key.as_str())
            {
                if raw.version == 1
                    && [
                        "alignment",
                        "color",
                        "colorBlendMode",
                        "copybackground",
                        "ledsource",
                        "locktransforms",
                        "perspective",
                        "solid",
                    ]
                    .contains(&key.as_str())
                {
                    continue;
                }
                return Err(InspectionError::invalid(
                    &context,
                    format!("unsupported selected image property {key}"),
                ));
            }
        }
        if raw.version == 1
            && (object
                .properties
                .get("alignment")
                .is_some_and(|v| v != "center")
                || vector(object.properties.get("color"), [1.; 3], &context)? != [1.; 3]
                || object
                    .properties
                    .get("colorBlendMode")
                    .is_some_and(|v| v != 0)
                || object
                    .properties
                    .get("perspective")
                    .is_some_and(|v| v != false))
        {
            return Err(InspectionError::invalid(
                &context,
                "unsupported legacy image appearance or perspective",
            ));
        }
        for flag in ["disablepropagation", "locktransforms", "castshadow"] {
            if object
                .properties
                .get(flag)
                .is_some_and(|value| !value.is_boolean())
            {
                return Err(InspectionError::invalid(
                    &context,
                    format!("{flag} must be Boolean"),
                ));
            }
        }
        if object.properties.get("castshadow") == Some(&Value::Bool(true)) {
            return Err(InspectionError::invalid(
                &context,
                "image shadow casting is unsupported",
            ));
        }
        if !visible(object.properties.get("visible"), &context)? {
            omissions.push(format!("Object {} is invisible.", object.id));
            continue;
        }
        if object
            .properties
            .get("disablepropagation")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return Err(InspectionError::invalid(
                &context,
                "disablepropagation is unsupported",
            ));
        }
        let mut applied_effect = None;
        for effect in &object.effects {
            if !settings.visual_effects {
                omissions.push(format!(
                    "Object {}: effect {} disabled by consumer settings.",
                    object.id, effect.file
                ));
                continue;
            }
            let visibility = effect.properties.get("visible");
            if visibility.is_some_and(|v| !v.is_boolean()) {
                return Err(InspectionError::invalid(
                    &context,
                    "effect visible must be Boolean",
                ));
            }
            if visibility == Some(&Value::Bool(false)) {
                continue;
            }
            let key = asset_key(&effect.file)?;
            if key.eq_ignore_ascii_case("effects/blurprecise/effect.json")
                || key.eq_ignore_ascii_case("effects/cloudmotion/effect.json")
                || key.eq_ignore_ascii_case("effects/waterflow/effect.json")
            {
                if raw
                    .general
                    .properties
                    .get("hdr")
                    .is_some_and(|value| value != false)
                {
                    return Err(InspectionError::invalid(
                        &context,
                        "supported image effects require SDR scene settings",
                    ));
                }
                if applied_effect.is_some() {
                    return Err(InspectionError::invalid(
                        &context,
                        "multiple supported image effects on one layer are unsupported",
                    ));
                }
                applied_effect = Some(
                    if key.eq_ignore_ascii_case("effects/blurprecise/effect.json") {
                        EffectDescription::Precise(super::precise::load(&decoder.package, effect)?)
                    } else if key.eq_ignore_ascii_case("effects/waterflow/effect.json") {
                        EffectDescription::Flow(super::flow::load(decoder, effect)?)
                    } else {
                        EffectDescription::Cloud(super::cloud::load(decoder, effect)?)
                    },
                );
            } else {
                omissions.push(format!(
                    "Object {}: effect {} is omitted.",
                    object.id, effect.file
                ));
            }
        }
        if object.properties.contains_key("parallaxDepth") {
            omissions.push(format!("Object {}: parallax is omitted.", object.id));
        }
        let origin = vector(object.properties.get("origin"), [0.; 3], &context)?;
        let scale = vector(object.properties.get("scale"), [1.; 3], &context)?;
        let angles = vector(object.properties.get("angles"), [0.; 3], &context)?;
        if origin[2] != 0. || angles[0] != 0. || angles[1] != 0. {
            return Err(InspectionError::invalid(
                &context,
                "only 2D origin/rotation are supported",
            ));
        }
        let size = vector(object.properties.get("size"), [0.; 2], &context)?;
        if size.iter().chain(&scale[..2]).any(|value| *value <= 0.) {
            return Err(InspectionError::invalid(
                &context,
                "size and XY scale must be positive",
            ));
        }
        let opacity = scalar(object.properties.get("alpha"), 1., &context)?;
        let brightness = scalar(object.properties.get("brightness"), 1., &context)?;
        let center = [origin[0], origin[1]];
        let displayed_size = [size[0] * scale[0], size[1] * scale[1]];
        ImageLayer::validate_geometry(center, displayed_size, angles[2], opacity, brightness)
            .map_err(|error| InspectionError::invalid(&context, error.to_string()))?;
        let model = asset_key(model)?;
        let model_json = match discovered_model {
            Some(model) => model,
            None => json(&decoder.package, &model)?,
        };
        let model_fields = model_json
            .as_object()
            .ok_or_else(|| InspectionError::invalid(&model, "expected image model object"))?;
        if model_fields
            .keys()
            .any(|key| !["autosize", "material", "cropoffset"].contains(&key.as_str()))
        {
            return Err(InspectionError::invalid(
                &model,
                "unsupported image model field",
            ));
        }
        if model_json
            .get("autosize")
            .is_some_and(|value| value != true)
        {
            return Err(InspectionError::invalid(
                &model,
                "only autosized image models are supported",
            ));
        }
        // Static import cropping already adjusts the authored object origin
        // and size. cropoffset describes removed space, not another transform.
        let offset = vector(model_json.get("cropoffset"), [0.; 2], &model)?;
        let material = asset_key(
            model_json["material"]
                .as_str()
                .ok_or_else(|| InspectionError::invalid(&model, "missing material path"))?,
        )?;
        let material_json = json(&decoder.package, &material)?;
        if material_json
            .as_object()
            .is_none_or(|fields| fields.keys().any(|key| key != "passes"))
        {
            return Err(InspectionError::invalid(
                &material,
                "unsupported material root field",
            ));
        }
        let passes = material_json["passes"]
            .as_array()
            .filter(|passes| passes.len() == 1)
            .ok_or_else(|| InspectionError::invalid(&material, "one material pass required"))?;
        let pass = &passes[0];
        let fields = pass
            .as_object()
            .ok_or_else(|| InspectionError::invalid(&material, "expected material pass object"))?;
        if fields.keys().any(|key| {
            ![
                "shader",
                "textures",
                "blending",
                "combos",
                "cullmode",
                "depthtest",
                "depthwrite",
            ]
            .contains(&key.as_str())
        }) || !(pass["shader"] == "genericimage4"
            || (raw.version == 1 && pass["shader"] == "genericimage2"))
            || pass["blending"] != "translucent"
            || pass["cullmode"] != "nocull"
            || pass["depthtest"] != "disabled"
            || pass["depthwrite"] != "disabled"
        {
            return Err(InspectionError::invalid(
                &material,
                "unsupported image material pass/state",
            ));
        }
        let combos = pass["combos"]
            .as_object()
            .ok_or_else(|| InspectionError::invalid(&material, "expected combos object"))?;
        let legacy = raw.version == 1 && pass["shader"] == "genericimage2";
        if combos.iter().any(|(key, value)| {
            !(key == "SPRITESHEET" && value.as_u64() == Some(1)
                || legacy && key == "VERSION" && value.as_u64() == Some(2))
        }) {
            return Err(InspectionError::invalid(
                &material,
                "unsupported material combination",
            ));
        }
        let sprite = combos.contains_key("SPRITESHEET");
        if sprite && offset != [0.; 2] {
            return Err(InspectionError::invalid(
                &model,
                "animated model cropoffset semantics are not yet verified",
            ));
        }
        let textures = pass["textures"]
            .as_array()
            .filter(|values| values.len() == 1)
            .ok_or_else(|| InspectionError::invalid(&material, "one source texture required"))?;
        let reference = textures[0]
            .as_str()
            .ok_or_else(|| InspectionError::invalid(&material, "expected texture name"))?;
        let asset = asset_key(&format!("materials/{reference}.tex"))?.to_ascii_lowercase();
        layers.push(LayerDescription {
            asset,
            sprite,
            center,
            size: displayed_size,
            angle: angles[2],
            opacity,
            brightness,
            effect: applied_effect,
        });
        object_ids.push(object.id);
    }
    if selected.as_ref().is_some_and(|ids| !ids.is_subset(&seen)) {
        return Err(InspectionError::invalid(
            "scene.json",
            "selected object ID is missing",
        ));
    }
    if layers.is_empty() || layers.len() > 32 {
        return Err(InspectionError::invalid(
            "scene.json",
            "image plan requires 1..=32 visible image layers",
        ));
    }
    Ok(ImagePlan {
        canvas,
        layers,
        object_ids,
        draw_order,
        particles,
        ray_objects,
        text_objects,
        hdr,
        omissions,
    })
}

pub(crate) fn prepare(
    decoder: &TextureDecoder,
    selection: &[u64],
    frame_index: u32,
) -> Result<ImageSnapshot, InspectionError> {
    let plan = plan(decoder, Some(selection), super::PlaybackSettings::default())?;
    let mut pages: BTreeMap<(String, u32), Arc<TexturePage>> = BTreeMap::new();
    let mut bytes = 0;
    let mut layers = Vec::new();
    for description in &plan.layers {
        let asset = &description.asset;
        let (page_index, region_origin, region_extent) = if description.sprite {
            let frames = decoder.sprite_frames(asset)?;
            let frame = frames.get(frame_index as usize).ok_or_else(|| {
                InspectionError::invalid(asset, "sprite frame index out of range")
            })?;
            (frame.page_index(), frame.origin(), Some(frame.extent()))
        } else {
            (0, [0, 0], None)
        };
        let page = page(decoder, asset, page_index, &mut pages, &mut bytes)?;
        let region = Arc::new(
            TextureRegion::new(
                page.clone(),
                region_origin,
                region_extent.unwrap_or(page.extent()),
            )
            .map_err(|error| InspectionError::invalid(asset, error.to_string()))?,
        );
        let cloud = description
            .cloud_profile()
            .map(|profile| profile.prepare(decoder, &mut pages, &mut bytes))
            .transpose()?;
        let mut layer = description.layer(region, cloud.as_ref(), Duration::ZERO)?;
        if let Some(EffectDescription::Flow(profile)) = &description.effect {
            layer = layer.with_flow(
                profile.prepare(decoder, &mut pages, &mut bytes)?,
                Duration::ZERO,
            );
        }
        layers.push(layer);
    }
    let composition = Arc::new(
        ImageComposition::new(plan.canvas, layers)
            .map_err(|error| InspectionError::invalid("scene.json", error.to_string()))?,
    );
    Ok(ImageSnapshot {
        composition,
        object_ids: plan.object_ids,
        omissions: plan.omissions,
    })
}

pub(super) fn page(
    decoder: &TextureDecoder,
    asset: &str,
    index: u32,
    pages: &mut BTreeMap<(String, u32), Arc<TexturePage>>,
    bytes: &mut u64,
) -> Result<Arc<TexturePage>, InspectionError> {
    let key = (asset.to_owned(), index);
    if !pages.contains_key(&key) {
        let page = Arc::new(decoder.decode_page_admitted(
            asset,
            index,
            super::layers::MAX_SOURCE_BYTES - *bytes,
        )?);
        *bytes += page.data().len() as u64;
        pages.insert(key.clone(), page);
    }
    Ok(pages[&key].clone())
}
