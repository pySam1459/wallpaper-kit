//! Checked unmasked default-kernel precise-blur profile, not shader translation.
use super::{InspectionError, PreciseBlur, RawEffect, snapshot};
use crate::assets::package::Package;
use serde_json::Value;

fn invalid(asset: &str) -> InspectionError {
    InspectionError::invalid(
        asset,
        "unsupported precise-blur profile, parameters or bindings",
    )
}

fn fields(value: &Value, allowed: &[&str], asset: &str) -> Result<(), InspectionError> {
    if value
        .as_object()
        .is_none_or(|map| map.keys().any(|key| !allowed.contains(&key.as_str())))
    {
        return Err(invalid(asset));
    }
    Ok(())
}

fn combos(value: Option<&Value>, vertical: usize, asset: &str) -> Result<(), InspectionError> {
    let Some(value) = value else { return Ok(()) };
    let map = value.as_object().ok_or_else(|| invalid(asset))?;
    for (key, value) in map {
        let expected = match key.as_str() {
            "VERTICAL" | "ENABLEMASK" => vertical as u64,
            "KERNEL" | "MASK" => 0,
            "BLURALPHA" => 1,
            _ => return Err(invalid(asset)),
        };
        if value.as_u64() != Some(expected) {
            return Err(invalid(asset));
        }
    }
    Ok(())
}

fn parameters(effect: &RawEffect) -> Result<PreciseBlur, InspectionError> {
    if effect
        .properties
        .keys()
        .any(|key| !["id", "name", "visible"].contains(&key.as_str()))
        || !matches!(effect.passes.len(), 0 | 2)
    {
        return Err(invalid(&effect.file));
    }
    let mut scale = [1.; 2];
    for (axis, pass) in effect.passes.iter().enumerate() {
        fields(
            pass,
            &["id", "combos", "constantshadervalues", "textures"],
            &effect.file,
        )?;
        combos(pass.get("combos"), axis, &effect.file)?;
        if let Some(textures) = pass.get("textures")
            && textures
                .as_array()
                .is_none_or(|values| values.len() > 3 || values.iter().any(|v| !v.is_null()))
        {
            return Err(invalid(&effect.file));
        }
        if let Some(constants) = pass.get("constantshadervalues") {
            fields(constants, &["scale"], &effect.file)?;
            let values = snapshot::vector(constants.get("scale"), [1.; 2], &effect.file)?;
            PreciseBlur::new(values).map_err(|_| invalid(&effect.file))?;
            scale[axis] = values[axis];
        }
    }
    PreciseBlur::new(scale).map_err(|_| invalid(&effect.file))
}

pub(super) fn load(package: &Package, effect: &RawEffect) -> Result<PreciseBlur, InspectionError> {
    let blur = parameters(effect)?;
    let definition = snapshot::json(package, &effect.file)?;
    fields(
        &definition,
        &[
            "version",
            "replacementkey",
            "passes",
            "fbos",
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
    if definition["version"] != 1 || definition["replacementkey"] != "blur_precise" {
        return Err(invalid(&effect.file));
    }
    let passes = definition["passes"]
        .as_array()
        .filter(|passes| passes.len() == 2)
        .ok_or_else(|| invalid(&effect.file))?;
    fields(&passes[0], &["material", "target"], &effect.file)?;
    fields(&passes[1], &["material", "bind"], &effect.file)?;
    let target = passes[0]["target"]
        .as_str()
        .filter(|s| s.starts_with("_rt_"))
        .ok_or_else(|| invalid(&effect.file))?;
    let bindings = passes[1]["bind"]
        .as_array()
        .filter(|b| b.len() == 2)
        .ok_or_else(|| invalid(&effect.file))?;
    for binding in bindings {
        fields(binding, &["name", "index"], &effect.file)?;
    }
    if !bindings
        .iter()
        .any(|b| b["index"] == 0 && b["name"] == target)
        || !bindings
            .iter()
            .any(|b| b["index"] == 1 && b["name"] == "previous")
    {
        return Err(invalid(&effect.file));
    }
    let fbos = definition["fbos"]
        .as_array()
        .filter(|f| f.len() == 1)
        .ok_or_else(|| invalid(&effect.file))?;
    fields(&fbos[0], &["name", "format"], &effect.file)?;
    if fbos[0]["name"] != target || fbos[0]["format"] != "rgba_backbuffer" {
        return Err(invalid(&effect.file));
    }
    for (axis, pass) in passes.iter().enumerate() {
        let material = pass["material"]
            .as_str()
            .ok_or_else(|| invalid(&effect.file))?;
        let json = snapshot::json(package, material)?;
        fields(&json, &["passes"], material)?;
        let material_pass = json["passes"]
            .as_array()
            .filter(|p| p.len() == 1)
            .ok_or_else(|| invalid(material))?;
        let material_pass = &material_pass[0];
        fields(
            material_pass,
            &[
                "shader",
                "combos",
                "blending",
                "depthtest",
                "depthwrite",
                "cullmode",
            ],
            material,
        )?;
        if material_pass["shader"] != "effects/blur_precise_gaussian"
            || material_pass["blending"] != "normal"
            || material_pass["depthtest"] != "disabled"
            || material_pass["depthwrite"] != "disabled"
            || material_pass["cullmode"] != "nocull"
        {
            return Err(invalid(material));
        }
        combos(material_pass.get("combos"), axis, material)?;
        // The vertical material must establish both inherited flags when the
        // instance does not supply them. Overrides cannot change this schedule.
        if axis == 1
            && (material_pass["combos"]["VERTICAL"] != 1
                || material_pass["combos"]["ENABLEMASK"] != 1)
        {
            return Err(invalid(material));
        }
    }
    // Identity checks diagnose custom/revised algorithms with familiar names.
    // No proprietary shader source is compiled or copied into our implementation.
    for (asset, expected) in [
        (
            "shaders/effects/blur_precise_gaussian.frag",
            "7d613ded8d61b1ca68b109d84a8f7f68d2901c65268af7e0ea7628adeb07b16f",
        ),
        (
            "shaders/effects/blur_precise_gaussian.vert",
            "4900e7a84bc5c1dc6e9d3b95a1c1862638a3ced4408a9bd17962be55149a10ec",
        ),
        (
            "shaders/common_blur.h",
            "b594e72584c3f9d2dff8ecb74cf71ac8e5b3e0ab7e57f9fd0dd1d964269cbc2e",
        ),
    ] {
        let mut reader = package.reader(asset)?.ok_or_else(|| invalid(asset))?;
        if reader.length > 64 * 1024 || reader.hash()? != expected {
            return Err(invalid(asset));
        }
    }
    Ok(blur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn effect(passes: Value) -> RawEffect {
        serde_json::from_value(
            json!({"file":"effects/blurprecise/effect.json","visible":true,"passes":passes}),
        )
        .unwrap()
    }
    #[test]
    fn defaults_and_axis_parameters_are_checked_without_decoding() {
        assert_eq!(parameters(&effect(json!([]))).unwrap().scale(), [1.; 2]);
        assert_eq!(parameters(&effect(json!([
            {"constantshadervalues":{"scale":"0.32 1.5"}},
            {"combos":{"VERTICAL":1,"ENABLEMASK":1,"KERNEL":0,"BLURALPHA":1},"constantshadervalues":{"scale":"2 0.7"},"textures":[null,null,null]}
        ]))).unwrap().scale(), [0.32, 0.7]);
        for passes in [
            json!([{}]),
            json!([{}, {}, {}]),
            json!([{"combos":{"KERNEL":1}},{}]),
            json!([{}, {"combos":{"VERTICAL":0}}]),
            json!([{}, {"textures":[null,null,"mask"]}]),
            json!([{"constantshadervalues":{"scale":"NaN 1"}},{}]),
            json!([{"constantshadervalues":{"scale":"2.1 1"}},{}]),
            json!([{"constantshadervalues":{"strength":1}},{}]),
            json!([{"script":"custom"},{}]),
        ] {
            assert!(parameters(&effect(passes)).is_err());
        }
    }
}
