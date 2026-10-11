//! Bounded, deterministic 2D particle profile. Asset work happens at opening;
//! analytic sampling does not replay the entire elapsed simulation.

use super::{ImageLayer, InspectionError, RawObject, snapshot};
use crate::{TextureDecoder, TexturePage, TextureRegion, assets::package::asset_key};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

const MAX_DRAWS: usize = 4096;
const MAX_WORK: usize = 32768;

pub(super) struct Particles {
    pub id: u64,
    root: System,
    origin: [f32; 2],
    scale: [f32; 2],
    angle: f32,
    appearance: Overrides,
}

#[derive(Clone, Copy)]
struct Overrides {
    count: f32,
    rate: f32,
    size: f32,
    speed: f32,
    lifetime: f32,
    alpha: f32,
    color: [f32; 3],
}

struct System {
    texture: Arc<TextureRegion>,
    frames: Vec<(Arc<TextureRegion>, f32, [f32; 6])>,
    sequence_multiplier: f32,
    rotation: Range,
    angular_velocity: Range,
    angular_force: f32,
    turbulence: Option<(Range, f32, f32)>,
    refraction: Option<(Arc<TextureRegion>, f32)>,
    additive: bool,
    brightness: f32,
    max_count: usize,
    start: f32,
    horizon: f32,
    emitters: Vec<Emitter>,
    lifetime: Range,
    size: Range,
    alpha: Range,
    color: [Range; 3],
    velocity: [Range; 3],
    movement: Option<([f32; 3], f32)>,
    operators: Vec<Operator>,
    renderer: Draw,
    children: Vec<(Event, System)>,
}

#[derive(Clone, Copy)]
struct Range {
    min: f32,
    max: f32,
    exponent: f32,
}
impl Range {
    fn fixed(value: f32) -> Self {
        Self {
            min: value,
            max: value,
            exponent: 1.,
        }
    }
    fn sample(self, rng: &mut Random) -> f32 {
        self.min + (self.max - self.min) * rng.unit().powf(self.exponent)
    }
    fn maximum(self) -> f32 {
        self.min.max(self.max)
    }
}

enum Shape {
    Box([f32; 3], [f32; 3]),
    Sphere {
        min: f32,
        max: f32,
        directions: [f32; 3],
        speed: Range,
    },
}
struct Emitter {
    shape: Shape,
    origin: [f32; 3],
    rate: f32,
    instantaneous: usize,
}
enum Operator {
    Fade(f32, f32),
    Size {
        start: f32,
        end: f32,
        from: f32,
        to: f32,
    },
    Oscillate {
        frequency: Range,
        scale: Range,
        phase: Range,
    },
}
enum Draw {
    Sprite,
    Trail { length: f32, min: f32, max: f32 },
    Rope { length: f32, segments: usize },
}
enum Event {
    Death,
    Follow,
    Independent,
}

struct Random(u64);
impl Random {
    fn unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        (((z ^ (z >> 31)) >> 40) as f32) / 16_777_216.
    }
}

fn number(
    value: Option<&Value>,
    default: f32,
    low: f32,
    high: f32,
    asset: &str,
) -> Result<f32, InspectionError> {
    let n = snapshot::scalar(value, default, asset)?;
    if !(low..=high).contains(&n) {
        return Err(InspectionError::invalid(
            asset,
            "particle number outside supported bounds",
        ));
    }
    Ok(n)
}
fn range(
    value: &Value,
    default: [f32; 2],
    low: f32,
    high: f32,
    asset: &str,
) -> Result<Range, InspectionError> {
    Ok(Range {
        min: number(value.get("min"), default[0], low, high, asset)?,
        max: number(value.get("max"), default[1], low, high, asset)?,
        exponent: number(value.get("exponent"), 1., 0.01, 32., asset)?,
    })
}
fn vector(
    value: Option<&Value>,
    default: [f32; 3],
    asset: &str,
) -> Result<[f32; 3], InspectionError> {
    let v = snapshot::vector(value, default, asset)?;
    if v.iter().any(|n| n.abs() > 16384.) {
        return Err(InspectionError::invalid(
            asset,
            "particle vector exceeds bounds",
        ));
    }
    Ok(v)
}
fn vector_range(
    value: &Value,
    default: [f32; 3],
    asset: &str,
) -> Result<[Range; 3], InspectionError> {
    let min = vector(value.get("min"), default, asset)?;
    let max = vector(value.get("max"), default, asset)?;
    let exponent = number(value.get("exponent"), 1., 0.01, 32., asset)?;
    Ok(std::array::from_fn(|i| Range {
        min: min[i],
        max: max[i],
        exponent,
    }))
}
fn components<'a>(
    value: &'a Value,
    field: &str,
    asset: &str,
) -> Result<&'a [Value], InspectionError> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(array)) if array.len() <= 32 => Ok(array),
        _ => Err(InspectionError::invalid(
            asset,
            format!("{field} requires at most 32 components"),
        )),
    }
}
fn fields(value: &Value, allowed: &[&str]) -> bool {
    value.as_object().is_some_and(|fields| {
        fields
            .keys()
            .all(|key| key == "id" || allowed.contains(&key.as_str()))
    })
}

pub(super) fn prepare(
    decoder: &TextureDecoder,
    objects: &[RawObject],
    pages: &mut BTreeMap<(String, u32), Arc<TexturePage>>,
    bytes: &mut u64,
    omissions: &mut Vec<String>,
) -> Result<Vec<Particles>, InspectionError> {
    if objects.len() > 32 {
        return Err(InspectionError::invalid(
            "scene.json",
            "at most 32 particle objects admitted",
        ));
    }
    let mut result = Vec::new();
    for object in objects {
        let key = asset_key(object.particle.as_ref().expect("particle selector"))?;
        let context = format!("scene.json object {}", object.id);
        if !object.effects.is_empty()
            || object.properties.keys().any(|key| {
                ![
                    "origin",
                    "scale",
                    "angles",
                    "visible",
                    "instanceoverride",
                    "locktransforms",
                    "parallaxDepth",
                ]
                .contains(&key.as_str())
            })
        {
            omissions.push(format!(
                "Object {}: unsupported particle layer properties/effects.",
                object.id
            ));
            continue;
        }
        let origin = vector(object.properties.get("origin"), [0.; 3], &context)?;
        let scale = vector(object.properties.get("scale"), [1.; 3], &context)?;
        let angle = vector(object.properties.get("angles"), [0.; 3], &context)?;
        if origin[2] != 0.
            || angle[0] != 0.
            || angle[1] != 0.
            || scale.iter().any(|v| *v == 0. || v.abs() > 16.)
        {
            return Err(InspectionError::invalid(
                &context,
                "only nonzero-scale 2D particle layers supported",
            ));
        }
        let overrides = object
            .properties
            .get("instanceoverride")
            .filter(|v| !v.is_null());
        if overrides.is_some_and(|v| {
            !fields(
                v,
                &[
                    "alpha", "colorn", "count", "rate", "size", "speed", "lifetime",
                ],
            )
        }) {
            omissions.push(format!(
                "Object {}: unsupported particle instance overrides.",
                object.id
            ));
            continue;
        }
        let get = |name: &str| overrides.and_then(|v| v.get(name));
        let color = vector(get("colorn"), [1.; 3], &context)?;
        if color.iter().any(|v| !(0. ..=1.).contains(v)) {
            return Err(InspectionError::invalid(
                &context,
                "particle tint outside 0..1",
            ));
        }
        let appearance = Overrides {
            count: number(get("count"), 1., 0., 16., &context)?,
            rate: number(get("rate"), 1., 0., 16., &context)?,
            size: number(get("size"), 1., 0., 16., &context)?,
            speed: number(get("speed"), 1., 0., 16., &context)?,
            lifetime: number(get("lifetime"), 1., 0.01, 16., &context)?,
            alpha: number(get("alpha"), 1., 0., 1., &context)?,
            color,
        };
        let mut active = Vec::new();
        let mut nodes = 0;
        if let Some(root) = load(
            decoder,
            &key,
            pages,
            bytes,
            omissions,
            &mut active,
            &mut nodes,
        )? {
            // Admission bounds the number of root births inspected at any seek.
            if root.emitters.iter().any(|e| {
                f64::from(e.rate * appearance.rate) * f64::from(root.horizon) > MAX_WORK as f64
            }) {
                return Err(InspectionError::invalid(
                    &key,
                    "particle history window exceeds sampling budget",
                ));
            }
            if object.properties.contains_key("parallaxDepth") {
                omissions.push(format!(
                    "Object {}: particle parallax is omitted.",
                    object.id
                ));
            }
            result.push(Particles {
                id: object.id,
                root,
                origin: [origin[0], origin[1]],
                scale: [scale[0], scale[1]],
                angle: angle[2],
                appearance,
            });
        }
    }
    if !result.is_empty() {
        omissions.push("Particles use deterministic library seeds and analytic gravity/drag; exact engine randomness and integration are not reproduced.".into());
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn load(
    decoder: &TextureDecoder,
    asset: &str,
    pages: &mut BTreeMap<(String, u32), Arc<TexturePage>>,
    bytes: &mut u64,
    omissions: &mut Vec<String>,
    active: &mut Vec<String>,
    nodes: &mut usize,
) -> Result<Option<System>, InspectionError> {
    if active.iter().any(|key| key == asset) || active.len() >= 4 || *nodes >= 32 {
        return Err(InspectionError::invalid(
            asset,
            "cyclic or excessive particle children",
        ));
    }
    *nodes += 1;
    active.push(asset.into());
    let result = load_inner(decoder, asset, pages, bytes, omissions, active, nodes);
    active.pop();
    result
}

#[allow(clippy::too_many_arguments)]
fn load_inner(
    decoder: &TextureDecoder,
    asset: &str,
    pages: &mut BTreeMap<(String, u32), Arc<TexturePage>>,
    bytes: &mut u64,
    omissions: &mut Vec<String>,
    active: &mut Vec<String>,
    nodes: &mut usize,
) -> Result<Option<System>, InspectionError> {
    let value = snapshot::json(&decoder.package, asset)?;
    let unsupported = |omissions: &mut Vec<String>, reason: &str| {
        omissions.push(format!("Particle {asset}: {reason} is omitted."));
        Ok(None)
    };
    if !fields(
        &value,
        &[
            "animationmode",
            "children",
            "controlpoint",
            "emitter",
            "flags",
            "initializer",
            "material",
            "maxcount",
            "operator",
            "renderer",
            "sequencemultiplier",
            "starttime",
        ],
    ) {
        return unsupported(omissions, "unsupported system field");
    }
    if value
        .get("flags")
        .is_some_and(|v| !v.is_null() && v != 0 && v != 1 && v != 4)
        || value
            .get("animationmode")
            .is_some_and(|v| !v.is_null() && v != 0)
    {
        return unsupported(omissions, "system flags or animation");
    }
    for cp in components(&value, "controlpoint", asset)? {
        if !fields(cp, &["flags", "offset"])
            || cp.get("flags").is_some_and(|v| v != 0)
            || vector(cp.get("offset"), [0.; 3], asset)? != [0.; 3]
        {
            return unsupported(omissions, "animated or offset control point");
        }
    }
    let max_count = number(value.get("maxcount"), 100., 1., 4096., asset)?;
    if max_count.fract() != 0. {
        return Err(InspectionError::invalid(
            asset,
            "maxcount must be an integer",
        ));
    }
    let start = number(value.get("starttime"), 0., 0., 120., asset)?;
    let material = asset_key(
        value["material"]
            .as_str()
            .ok_or_else(|| InspectionError::invalid(asset, "missing particle material"))?,
    )?;
    let material_json = snapshot::json(&decoder.package, &material)?;
    let passes = components(&material_json, "passes", &material)?;
    if passes.len() != 1 {
        return unsupported(omissions, "multiple material passes");
    }
    let pass = &passes[0];
    if !fields(
        pass,
        &[
            "shader",
            "blending",
            "combos",
            "constantshadervalues",
            "cullmode",
            "depthtest",
            "depthwrite",
            "textures",
        ],
    ) || pass["shader"] != "genericparticle"
        || !["additive", "translucent"]
            .iter()
            .any(|v| pass["blending"] == *v)
        || pass["cullmode"] != "nocull"
        || pass["depthtest"] != "disabled"
        || pass["depthwrite"] != "disabled"
    {
        return unsupported(omissions, "material state");
    }
    if pass.get("combos").is_some_and(|v| {
        !v.as_object()
            .is_some_and(|m| m.iter().all(|(k, v)| k == "REFRACT" && (v == 0 || v == 1)))
    }) {
        return unsupported(omissions, "material combination");
    }
    let constants = pass.get("constantshadervalues");
    if constants.is_some_and(|v| {
        !fields(
            v,
            &[
                "ui_editor_properties_overbright",
                "ui_editor_properties_refract_amount",
            ],
        )
    }) {
        return unsupported(omissions, "material constants");
    }
    let brightness = number(
        constants.and_then(|v| v.get("ui_editor_properties_overbright")),
        1.,
        0.,
        16.,
        &material,
    )?;
    let textures = components(pass, "textures", &material)?;
    let refract = pass.get("combos").and_then(|v| v.get("REFRACT")) == Some(&Value::from(1));
    let texture_count = if refract { 2 } else { 1 };
    if textures.len() < texture_count || textures[texture_count..].iter().any(|v| !v.is_null()) {
        return unsupported(omissions, "extra material textures");
    }
    let reference = textures[0]
        .as_str()
        .ok_or_else(|| InspectionError::invalid(&material, "missing particle texture"))?;
    let texture = asset_key(&format!("materials/{reference}.tex"))?.to_ascii_lowercase();
    let refraction = if refract {
        if pass["blending"] != "translucent" {
            return unsupported(omissions, "additive refraction");
        }
        let amount = number(
            constants.and_then(|v| v.get("ui_editor_properties_refract_amount")),
            0.05,
            0.,
            1.,
            &material,
        )?;
        let normal = textures[1]
            .as_str()
            .ok_or_else(|| InspectionError::invalid(&material, "missing refraction normal map"))?;
        let normal = asset_key(&format!("materials/{normal}.tex"))?;
        let page = snapshot::page(decoder, &normal, 0, pages, bytes)?;
        let extent = page.extent();
        let region = TextureRegion::new(page, [0, 0], extent)
            .map_err(|error| InspectionError::invalid(&normal, error.to_string()))?;
        Some((Arc::new(region), amount))
    } else {
        None
    };
    let mut emitters = Vec::new();
    for e in components(&value, "emitter", asset)? {
        let shape = match e["name"].as_str() {
            Some("boxrandom")
                if fields(
                    e,
                    &[
                        "name",
                        "rate",
                        "instantaneous",
                        "origin",
                        "distancemin",
                        "distancemax",
                    ],
                ) =>
            {
                Shape::Box(
                    vector(e.get("distancemin"), [0.; 3], asset)?,
                    vector(e.get("distancemax"), [0.; 3], asset)?,
                )
            }
            Some("sphererandom")
                if fields(
                    e,
                    &[
                        "name",
                        "rate",
                        "instantaneous",
                        "origin",
                        "distancemin",
                        "distancemax",
                        "directions",
                        "speedmin",
                        "speedmax",
                    ],
                ) =>
            {
                Shape::Sphere {
                    min: number(e.get("distancemin"), 0., 0., 16384., asset)?,
                    max: number(e.get("distancemax"), 0., 0., 16384., asset)?,
                    directions: vector(e.get("directions"), [1., 1., 0.], asset)?,
                    speed: Range {
                        min: number(e.get("speedmin"), 0., 0., 16384., asset)?,
                        max: number(e.get("speedmax"), 0., 0., 16384., asset)?,
                        exponent: 1.,
                    },
                }
            }
            _ => return unsupported(omissions, "emitter component"),
        };
        let instantaneous = number(e.get("instantaneous"), 0., 0., 4096., asset)?;
        if instantaneous.fract() != 0. {
            return Err(InspectionError::invalid(
                asset,
                "instantaneous count must be integer",
            ));
        }
        emitters.push(Emitter {
            shape,
            origin: vector(e.get("origin"), [0.; 3], asset)?,
            rate: number(e.get("rate"), 0., 0., 4096., asset)?,
            instantaneous: instantaneous as usize,
        });
    }
    let (mut lifetime, mut size, mut alpha) =
        (Range::fixed(1.), Range::fixed(1.), Range::fixed(1.));
    let (mut color, mut velocity) = ([Range::fixed(255.); 3], [Range::fixed(0.); 3]);
    let (mut rotation, mut angular_velocity, mut turbulence) =
        (Range::fixed(0.), Range::fixed(0.), None);
    for init in components(&value, "initializer", asset)? {
        if !fields(
            init,
            &[
                "name", "min", "max", "exponent", "offset", "scale", "speedmin", "speedmax",
            ],
        ) {
            return unsupported(omissions, "initializer properties");
        }
        match init["name"].as_str() {
            Some("lifetimerandom") => lifetime = range(init, [1., 1.], 0.001, 120., asset)?,
            Some("sizerandom") => size = range(init, [1., 1.], 0., 16384., asset)?,
            Some("alpharandom") => alpha = range(init, [1., 1.], 0., 1., asset)?,
            Some("colorrandom") => {
                color = vector_range(init, [255.; 3], asset)?;
                if color
                    .iter()
                    .any(|r| r.min < 0. || r.max < 0. || r.maximum() > 255.)
                {
                    return Err(InspectionError::invalid(
                        asset,
                        "particle color outside 0..255",
                    ));
                }
            }
            Some("velocityrandom") => velocity = vector_range(init, [0.; 3], asset)?,
            Some("rotationrandom") => rotation = range(init, [0., 360.], -3600., 3600., asset)?,
            Some("angularvelocityrandom") => {
                let values = vector_range(init, [0.; 3], asset)?;
                if values[..2].iter().any(|v| v.min != 0. || v.max != 0.) {
                    return unsupported(omissions, "3D angular velocity");
                }
                angular_velocity = values[2];
            }
            Some("turbulentvelocityrandom")
                if fields(init, &["name", "offset", "scale", "speedmin", "speedmax"]) =>
            {
                turbulence = Some((
                    Range {
                        min: number(init.get("speedmin"), 0., 0., 16384., asset)?,
                        max: number(init.get("speedmax"), 1., 0., 16384., asset)?,
                        exponent: 1.,
                    },
                    number(init.get("scale"), 1., 0.001, 128., asset)?,
                    number(init.get("offset"), 0., -16384., 16384., asset)?,
                ));
            }
            _ => return unsupported(omissions, "initializer component"),
        }
    }
    let mut movement = None;
    let mut angular_force = 0.;
    let mut operators = Vec::new();
    for op in components(&value, "operator", asset)? {
        match op["name"].as_str() {
            Some("angularmovement") if fields(op, &["name", "force"]) => {
                let force = vector(op.get("force"), [0.; 3], asset)?;
                if force[..2] != [0., 0.] {
                    return unsupported(omissions, "3D angular force");
                }
                angular_force = force[2];
            }
            Some("movement") if fields(op, &["name", "gravity", "drag"]) => {
                if movement.is_some() {
                    return unsupported(omissions, "multiple movement operators");
                }
                movement = Some((
                    vector(op.get("gravity"), [0.; 3], asset)?,
                    number(op.get("drag"), 0., 0., 32., asset)?,
                ));
            }
            Some("alphafade") if fields(op, &["name", "fadeintime", "fadeouttime"]) => operators
                .push(Operator::Fade(
                    number(op.get("fadeintime"), 0.1, 0., 1., asset)?,
                    number(op.get("fadeouttime"), 0.9, 0., 1., asset)?,
                )),
            Some("sizechange")
                if fields(
                    op,
                    &["name", "starttime", "endtime", "startvalue", "endvalue"],
                ) =>
            {
                let start = number(op.get("starttime"), 0., 0., 1., asset)?;
                let end = number(op.get("endtime"), 1., 0., 1., asset)?;
                if end <= start {
                    return Err(InspectionError::invalid(
                        asset,
                        "size change requires increasing time",
                    ));
                }
                operators.push(Operator::Size {
                    start,
                    end,
                    from: number(op.get("startvalue"), 1., 0., 16., asset)?,
                    to: number(op.get("endvalue"), 0., 0., 16., asset)?,
                });
            }
            Some("oscillatealpha")
                if fields(
                    op,
                    &[
                        "name",
                        "frequencymin",
                        "frequencymax",
                        "scalemin",
                        "scalemax",
                        "phasemin",
                        "phasemax",
                    ],
                ) =>
            {
                operators.push(Operator::Oscillate {
                    frequency: Range {
                        min: number(op.get("frequencymin"), 1., 0., 100., asset)?,
                        max: number(op.get("frequencymax"), 1., 0., 100., asset)?,
                        exponent: 1.,
                    },
                    scale: Range {
                        min: number(op.get("scalemin"), 0., 0., 1., asset)?,
                        max: number(op.get("scalemax"), 1., 0., 1., asset)?,
                        exponent: 1.,
                    },
                    phase: Range {
                        min: number(op.get("phasemin"), 0., -100., 100., asset)?,
                        max: number(
                            op.get("phasemax"),
                            std::f32::consts::TAU,
                            -100.,
                            100.,
                            asset,
                        )?,
                        exponent: 1.,
                    },
                })
            }
            _ => return unsupported(omissions, "operator component"),
        }
    }
    let renderers = components(&value, "renderer", asset)?;
    if renderers.len() != 1 {
        return unsupported(omissions, "renderer count");
    }
    let renderer = &renderers[0];
    if renderer.get("orientation").is_some_and(|v| v != "screen") {
        return unsupported(omissions, "3D renderer orientation");
    }
    let renderer = match renderer["name"].as_str() {
        Some("sprite") if fields(renderer, &["name", "orientation"]) => Draw::Sprite,
        Some("spritetrail")
            if fields(
                renderer,
                &["name", "orientation", "length", "minlength", "maxlength"],
            ) =>
        {
            Draw::Trail {
                length: number(renderer.get("length"), 0.05, 0., 10., asset)?,
                min: number(renderer.get("minlength"), 0., 0., 16384., asset)?,
                max: number(renderer.get("maxlength"), 10., 0., 16384., asset)?,
            }
        }
        Some("ropetrail") if fields(renderer, &["name", "orientation", "length", "segments"]) => {
            Draw::Rope {
                length: number(renderer.get("length"), 0.1, 0., 10., asset)?,
                segments: number(renderer.get("segments"), 8., 1., 16., asset)? as usize,
            }
        }
        _ => return unsupported(omissions, "renderer component"),
    };
    let mut children = Vec::new();
    let mut horizon = lifetime.maximum();
    for child in components(&value, "children", asset)? {
        if !fields(child, &["name", "type"]) {
            omissions.push(format!(
                "Particle {asset}: child transforms/probability omitted."
            ));
            continue;
        }
        let event = match child["type"].as_str() {
            None if child.get("type").is_none() => Event::Independent,
            Some("eventdeath") => Event::Death,
            Some("eventfollow") => Event::Follow,
            _ => {
                omissions.push(format!(
                    "Particle {asset}: unsupported child event omitted."
                ));
                continue;
            }
        };
        let name = asset_key(
            child["name"]
                .as_str()
                .ok_or_else(|| InspectionError::invalid(asset, "missing child path"))?,
        )?;
        if let Some(system) = load(decoder, &name, pages, bytes, omissions, active, nodes)? {
            horizon = horizon.max(lifetime.maximum() + system.horizon);
            children.push((event, system));
        }
    }
    let texture_key = texture.clone();
    let page = snapshot::page(decoder, &texture, 0, pages, bytes)?;
    let extent = page.extent();
    let texture = Arc::new(
        TextureRegion::new(page, [0, 0], extent)
            .map_err(|error| InspectionError::invalid(asset, error.to_string()))?,
    );
    let mut frames = Vec::new();
    for frame in decoder.particle_sprite_frames(&texture_key)? {
        let page = snapshot::page(decoder, &texture_key, frame.page, pages, bytes)?;
        let [width, height] = page.extent().map(|v| v as f32);
        let extent = page.extent();
        let [x, y, wx, wy, hx, hy] = frame.rectangle;
        frames.push((
            Arc::new(
                TextureRegion::new(page, [0, 0], extent)
                    .map_err(|e| InspectionError::invalid(asset, e.to_string()))?,
            ),
            frame.duration,
            [
                x / width,
                y / height,
                wx / width,
                wy / height,
                hx / width,
                hy / height,
            ],
        ));
    }
    let sequence_multiplier = number(
        value.get("sequencemultiplier").filter(|v| !v.is_null()),
        1.,
        0.,
        128.,
        asset,
    )?;
    Ok(Some(System {
        texture,
        frames,
        sequence_multiplier,
        rotation,
        angular_velocity,
        angular_force,
        turbulence,
        refraction,
        additive: pass["blending"] == "additive",
        brightness,
        max_count: max_count as usize,
        start,
        horizon,
        emitters,
        lifetime,
        size,
        alpha,
        color,
        velocity,
        movement,
        operators,
        renderer,
        children,
    }))
}

struct Particle {
    origin: [f32; 3],
    velocity: [f32; 3],
    life: f32,
    size: f32,
    alpha: f32,
    color: [f32; 3],
    seed: u64,
    rotation: f32,
    angular_velocity: f32,
}
impl System {
    fn particle(&self, e: &Emitter, seed: u64, appearance: Overrides) -> Particle {
        let mut rng = Random(seed);
        let mut origin = e.origin;
        let mut velocity = [0.; 3];
        match &e.shape {
            Shape::Box(min, max) => {
                for i in 0..3 {
                    origin[i] += (min[i] + (max[i] - min[i]) * rng.unit())
                        * if rng.unit() < 0.5 { -1. } else { 1. };
                }
            }
            Shape::Sphere {
                min,
                max,
                directions,
                speed,
            } => {
                let theta = rng.unit() * std::f32::consts::TAU;
                let z = if directions[2] == 0. {
                    0.
                } else {
                    rng.unit() * 2. - 1.
                };
                let radial = (1. - z * z).sqrt();
                let direction = [
                    theta.cos() * radial * directions[0],
                    theta.sin() * radial * directions[1],
                    z * directions[2],
                ];
                let radius = min + (max - min) * rng.unit().sqrt();
                let speed = speed.sample(&mut rng);
                for i in 0..3 {
                    origin[i] += direction[i] * radius;
                    velocity[i] = direction[i] * speed;
                }
            }
        }
        let life = self.lifetime.sample(&mut rng) * appearance.lifetime;
        let size = self.size.sample(&mut rng) * appearance.size;
        let alpha = self.alpha.sample(&mut rng) * appearance.alpha;
        let color =
            std::array::from_fn(|i| self.color[i].sample(&mut rng) / 255. * appearance.color[i]);
        for (i, v) in velocity.iter_mut().enumerate() {
            *v = (*v + self.velocity[i].sample(&mut rng)) * appearance.speed;
        }
        if let Some((speed, scale, offset)) = self.turbulence {
            // A deterministic smooth direction field; engine noise phase is
            // intentionally not inferred from proprietary shader code.
            let angle = (origin[0] * scale * 0.01 + offset).sin()
                + (origin[1] * scale * 0.01 - offset).cos();
            let speed = speed.sample(&mut rng) * appearance.speed;
            velocity[0] += angle.cos() * speed;
            velocity[1] += angle.sin() * speed;
        }
        let rotation = self.rotation.sample(&mut rng);
        let angular_velocity = self.angular_velocity.sample(&mut rng);
        Particle {
            origin,
            velocity,
            life,
            size,
            alpha,
            color,
            seed,
            rotation,
            angular_velocity,
        }
    }

    fn position(&self, p: &Particle, age: f32) -> [f32; 3] {
        let Some((gravity, drag)) = self.movement else {
            return p.origin;
        };
        if drag == 0. {
            return std::array::from_fn(|i| {
                p.origin[i] + p.velocity[i] * age + gravity[i] * age * age * 0.5
            });
        }
        let integral = -(-drag * age).exp_m1() / drag;
        std::array::from_fn(|i| {
            p.origin[i] + p.velocity[i] * integral + gravity[i] * (age - integral) / drag
        })
    }

    fn velocity(&self, p: &Particle, age: f32) -> [f32; 3] {
        let Some((gravity, drag)) = self.movement else {
            return p.velocity;
        };
        if drag == 0. {
            return std::array::from_fn(|i| p.velocity[i] + gravity[i] * age);
        }
        let decay = (-drag * age).exp();
        std::array::from_fn(|i| p.velocity[i] * decay + gravity[i] * (1. - decay) / drag)
    }

    fn appearance(&self, p: &Particle, age: f32) -> (f32, f32) {
        let progress = age / p.life;
        let (mut size, mut alpha) = (p.size, p.alpha);
        let mut rng = Random(p.seed ^ 0x6a09e667f3bcc909);
        for op in &self.operators {
            match op {
                Operator::Fade(start, end) => {
                    if *start > 0. {
                        alpha *= (progress / start).clamp(0., 1.);
                    }
                    if *end < 1. {
                        alpha *= ((1. - progress) / (1. - end)).clamp(0., 1.);
                    }
                }
                Operator::Size {
                    start,
                    end,
                    from,
                    to,
                } => {
                    size *= from + (to - from) * ((progress - start) / (end - start)).clamp(0., 1.)
                }
                Operator::Oscillate {
                    frequency,
                    scale,
                    phase,
                } => {
                    let f = frequency.sample(&mut rng);
                    let phase = phase.sample(&mut rng);
                    alpha *= scale.min
                        + (scale.max - scale.min)
                            * ((progress * f * std::f32::consts::TAU + phase).sin() * 0.5 + 0.5);
                }
            }
        }
        (size, alpha.clamp(0., 1.))
    }
}

impl Particles {
    pub(super) fn append(
        &self,
        time: Duration,
        layers: &mut Vec<ImageLayer>,
        work: &mut usize,
        draws: &mut usize,
    ) -> Result<(), InspectionError> {
        self.sample_system(
            &self.root,
            time.as_secs_f64(),
            [0.; 3],
            self.id,
            layers,
            work,
            draws,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn sample_system(
        &self,
        system: &System,
        time: f64,
        base: [f32; 3],
        seed: u64,
        layers: &mut Vec<ImageLayer>,
        work: &mut usize,
        draws: &mut usize,
    ) -> Result<(), InspectionError> {
        for (index, (event, child)) in system.children.iter().enumerate() {
            if matches!(event, Event::Independent) {
                self.sample_system(
                    child,
                    time,
                    base,
                    seed.wrapping_add(index as u64 + 1),
                    layers,
                    work,
                    draws,
                )?;
            }
        }
        let time = time + f64::from(system.start);
        let mut alive = 0;
        let maximum =
            ((system.max_count as f32 * self.appearance.count).ceil() as usize).min(MAX_DRAWS);
        for (emitter_index, e) in system.emitters.iter().enumerate() {
            let rate = f64::from(e.rate * self.appearance.rate);
            let instantaneous =
                ((e.instantaneous as f32 * self.appearance.count).ceil() as usize).min(maximum);
            let last = if rate > 0. {
                (time * rate).floor().min(u64::MAX as f64) as u64
            } else {
                0
            };
            let first = if rate > 0. {
                (((time - f64::from(system.horizon * self.appearance.lifetime)).max(0.) * rate)
                    .floor() as u64)
                    .saturating_add(1)
            } else {
                1
            };
            // Bounds independent of uptime. No catch-up replay on the host thread.
            for (index, born) in (0..instantaneous as u64).map(|i| (i, 0.)).chain(
                (first..=last).map(|i| (i.saturating_add(instantaneous as u64), i as f64 / rate)),
            ) {
                if *work >= MAX_WORK || *draws >= MAX_DRAWS {
                    return Ok(());
                }
                *work += 1;
                let age = (time - born) as f32;
                let particle_seed = seed
                    .wrapping_mul(0x9e3779b97f4a7c15)
                    .wrapping_add(index)
                    .wrapping_add((emitter_index as u64).wrapping_mul(0x94d049bb133111eb));
                let p = system.particle(e, particle_seed, self.appearance);
                if age >= 0. && age < p.life && alive < maximum {
                    alive += 1;
                    self.draw(system, &p, age, base, layers, draws)?;
                }
                for (child_index, (event, child)) in system.children.iter().enumerate() {
                    let (child_age, position) = match event {
                        Event::Death if age >= p.life => {
                            (age - p.life, system.position(&p, p.life))
                        }
                        Event::Follow if age >= 0. && age < p.life => {
                            (age, system.position(&p, age))
                        }
                        _ => continue,
                    };
                    let position = std::array::from_fn(|i| base[i] + position[i]);
                    self.sample_system(
                        child,
                        f64::from(child_age),
                        position,
                        particle_seed.wrapping_add(child_index as u64 + 1),
                        layers,
                        work,
                        draws,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn draw(
        &self,
        system: &System,
        p: &Particle,
        age: f32,
        base: [f32; 3],
        layers: &mut Vec<ImageLayer>,
        draws: &mut usize,
    ) -> Result<(), InspectionError> {
        let (size, alpha) = system.appearance(p, age);
        // Reference probes show the displayed quad spans half the authored size.
        let size = size * 0.5;
        if size <= 0. || alpha <= 0. {
            return Ok(());
        }
        let position = system.position(p, age);
        match system.renderer {
            Draw::Sprite => self.quad(
                system,
                p,
                position,
                base,
                [size, size],
                self.angle
                    + p.rotation
                    + (p.angular_velocity * age + system.angular_force * age * age * 0.5)
                        .to_degrees(),
                alpha,
                layers,
                draws,
                age,
            ),
            Draw::Trail { length, min, max } => {
                let velocity = system.velocity(p, age);
                let stretched = size * (velocity[0].hypot(velocity[1]) * length).max(min).min(max);
                if stretched <= 0. {
                    return Ok(());
                }
                let angle = velocity[1].atan2(velocity[0]).to_degrees() - 90. + self.angle;
                self.quad(
                    system,
                    p,
                    position,
                    base,
                    [size, stretched],
                    angle,
                    alpha,
                    layers,
                    draws,
                    age,
                )
            }
            Draw::Rope { length, segments } => {
                // shortcut: bounded chord strips approximate the rope mesh;
                // upgrade when reference probes require subdivision/UV scrolling.
                let duration = length.min(age);
                for segment in 0..segments {
                    let a = system.position(p, age - duration * segment as f32 / segments as f32);
                    let b =
                        system.position(p, age - duration * (segment + 1) as f32 / segments as f32);
                    let delta = [(a[0] - b[0]) * self.scale[0], (a[1] - b[1]) * self.scale[1]];
                    let center = std::array::from_fn(|i| (a[i] + b[i]) * 0.5);
                    let angle = delta[1].atan2(delta[0]).to_degrees() - 90. + self.angle;
                    let length = delta[0].hypot(delta[1]) / self.scale[1];
                    if length > 0. {
                        self.quad(
                            system,
                            p,
                            center,
                            base,
                            [size, length],
                            angle,
                            alpha * (1. - segment as f32 / segments as f32),
                            layers,
                            draws,
                            age,
                        )?;
                    }
                }
                Ok(())
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn quad(
        &self,
        system: &System,
        p: &Particle,
        position: [f32; 3],
        base: [f32; 3],
        size: [f32; 2],
        angle: f32,
        alpha: f32,
        layers: &mut Vec<ImageLayer>,
        draws: &mut usize,
        age: f32,
    ) -> Result<(), InspectionError> {
        if *draws >= MAX_DRAWS {
            return Ok(());
        }
        let (sin, cos) = self.angle.to_radians().sin_cos();
        let local = [
            (base[0] + position[0]) * self.scale[0],
            (base[1] + position[1]) * self.scale[1],
        ];
        let center = [
            self.origin[0] + local[0] * cos - local[1] * sin,
            self.origin[1] + local[0] * sin + local[1] * cos,
        ];
        let size = [size[0] * self.scale[0].abs(), size[1] * self.scale[1].abs()];
        if center.iter().any(|v| v.abs() > 65536.) || size.iter().any(|v| *v > 65536.) {
            return Ok(());
        }
        let total: f32 = system.frames.iter().map(|(_, duration, _)| duration).sum();
        // Particle sequences span the lifetime; TEX durations weight the frames.
        let mut local = if total > 0. {
            (age / p.life * system.sequence_multiplier * total).rem_euclid(total)
        } else {
            0.
        };
        let selected = system.frames.iter().position(|(_, duration, _)| {
            if local < *duration {
                true
            } else {
                local -= duration;
                false
            }
        });
        let (texture, mut uv) = selected.map_or(
            (system.texture.clone(), [0., 0., 1., 0., 0., 1.]),
            |index| (system.frames[index].0.clone(), system.frames[index].2),
        );
        let mut next = selected.map(|index| {
            let frame = &system.frames[(index + 1) % system.frames.len()];
            (frame.0.clone(), frame.2, local / system.frames[index].1)
        });
        if self.scale[0] < 0. {
            uv[0] += uv[2];
            uv[1] += uv[3];
            uv[2] = -uv[2];
            uv[3] = -uv[3];
        }
        if self.scale[1] < 0. {
            uv[0] += uv[4];
            uv[1] += uv[5];
            uv[4] = -uv[4];
            uv[5] = -uv[5];
        }
        if let Some((_, next_uv, _)) = &mut next {
            if self.scale[0] < 0. {
                next_uv[0] += next_uv[2];
                next_uv[1] += next_uv[3];
                next_uv[2] = -next_uv[2];
                next_uv[3] = -next_uv[3];
            }
            if self.scale[1] < 0. {
                next_uv[0] += next_uv[4];
                next_uv[1] += next_uv[5];
                next_uv[4] = -next_uv[4];
                next_uv[5] = -next_uv[5];
            }
        }
        let mut layer = ImageLayer::new(texture, center, size, angle, alpha, system.brightness)
            .map_err(|error| InspectionError::invalid("particle sample", error.to_string()))?;
        layer.particle_color = Some((p.color, system.additive));
        layer.particle_uv = Some(uv);
        layer.particle_next = next;
        layer.refraction = system.refraction.clone();
        layers.push(layer);
        *draws += 1;
        Ok(())
    }
}
