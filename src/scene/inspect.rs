use super::*;
use crate::assets::package::asset_key;
use crate::assets::{package::AssetReader, texture};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(super) fn run(source: &SceneSource) -> Result<SceneInspection, InspectionError> {
    let package = Package::open(source)?;
    let mut reader = package
        .reader("scene.json")?
        .ok_or_else(|| InspectionError::invalid("scene.json", "missing scene definition"))?;
    let hash = reader.hash()?;
    let bytes = reader.bytes(reader.length, 4 * 1024 * 1024)?;
    let raw: RawScene =
        serde_json::from_slice(&bytes).map_err(|error| reader.invalid(error.to_string()))?;
    if raw.version != 5 {
        return Err(reader.invalid(format!("unsupported scene version {}", raw.version)));
    }
    let canvas = [
        raw.general.orthogonalprojection.width,
        raw.general.orthogonalprojection.height,
    ];
    if canvas
        .iter()
        .any(|&dimension| dimension == 0 || dimension > 16_384)
    {
        return Err(reader.invalid("orthographic dimensions must be between 1 and 16384"));
    }
    if raw.objects.len() > 4096 {
        return Err(reader.invalid("scene exceeds 4096 objects"));
    }
    let mut seen = BTreeSet::new();
    let mut objects = Vec::new();
    let mut graph = Graph::new(&package);
    graph.assets.push(AssetSummary {
        path: "scene.json".into(),
        origin: reader.origin.into(),
        bytes: reader.length,
        sha256: hash,
    });
    graph.active.push("scene.json".into());
    graph.check_fields(
        raw.general.properties.keys(),
        &[
            "ambientcolor",
            "bloom",
            "bloomhdrfeather",
            "bloomhdriterations",
            "bloomhdrscatter",
            "bloomhdrstrength",
            "bloomhdrthreshold",
            "bloomstrength",
            "bloomthreshold",
            "bloomtint",
            "camerafade",
            "cameraparallax",
            "cameraparallaxamount",
            "cameraparallaxdelay",
            "cameraparallaxmouseinfluence",
            "camerapreview",
            "camerashake",
            "camerashakeamplitude",
            "camerashakeroughness",
            "camerashakespeed",
            "clearcolor",
            "clearenabled",
            "farz",
            "fov",
            "gravitydirection",
            "gravitystrength",
            "hdr",
            "nearz",
            "perspectiveoverridefov",
            "skylightcolor",
            "winddirection",
            "windenabled",
            "windstrength",
            "zoom",
        ],
        "scene.json",
        None,
    );
    validate_flags(
        &raw.general.properties,
        "scene.json",
        &[
            "hdr",
            "bloom",
            "camerashake",
            "windenabled",
            "cameraparallax",
            "clearenabled",
            "camerafade",
            "camerapreview",
        ],
    )?;
    for feature in ["hdr", "bloom", "camerashake", "windenabled"] {
        if raw
            .general
            .properties
            .get(feature)
            .and_then(|value| value.as_bool())
            == Some(true)
        {
            graph.finding(
                DiagnosticSeverity::Unsupported,
                "unsupported-general-feature",
                "scene.json",
                None,
                format!("enabled {feature} is outside the MVP profile"),
            );
        }
    }
    if raw
        .general
        .properties
        .get("cameraparallax")
        .and_then(|value| value.as_bool())
        == Some(true)
    {
        graph.features.insert("parallax".into());
    }
    for key in raw.properties.keys() {
        if key != "camera" {
            graph.finding(
                DiagnosticSeverity::Unsupported,
                "unsupported-scene-field",
                "scene.json",
                None,
                format!("unrecognized scene field {key}"),
            );
        }
    }
    for mut object in raw.objects {
        if !seen.insert(object.id) {
            return Err(reader.invalid(format!("duplicate object id {}", object.id)));
        }
        let mut kind = if object.image.is_some() {
            ObjectKind::Image
        } else if object.particle.is_some() {
            ObjectKind::Particle
        } else if object.shape.is_some() {
            ObjectKind::Shape
        } else if object.sound.is_some() {
            ObjectKind::Sound
        } else {
            ObjectKind::Unknown
        };
        for path in object
            .image
            .iter_mut()
            .chain(object.particle.iter_mut())
            .chain(object.sound.iter_mut().flatten())
        {
            *path = asset_key(path)?;
        }
        for effect in &mut object.effects {
            effect.file = asset_key(&effect.file)?;
        }
        let selectors = [
            object.image.is_some(),
            object.particle.is_some(),
            object.shape.is_some(),
            object.sound.is_some(),
        ];
        if selectors.iter().filter(|&&selected| selected).count() > 1 {
            return Err(reader.invalid(format!("object {} has ambiguous content types", object.id)));
        }
        if kind == ObjectKind::Unknown {
            graph.finding(
                DiagnosticSeverity::Unsupported,
                "unsupported-object",
                "scene.json",
                Some(object.id),
                "object has no recognized content type".into(),
            );
        }
        if let Some(shape) = &object.shape
            && shape != "quad"
        {
            graph.finding(
                DiagnosticSeverity::Unsupported,
                "unsupported-shape",
                "scene.json",
                Some(object.id),
                format!("unsupported shape {shape}"),
            );
        }
        for (name, value) in &object.properties {
            let components = match name.as_str() {
                "origin" | "scale" | "angles" => 3,
                "size" | "parallaxDepth" => 2,
                _ => continue,
            };
            let text = value.as_str().ok_or_else(|| {
                reader.invalid(format!(
                    "object {}: {name} must be a vector string",
                    object.id
                ))
            })?;
            let vector: Vec<f32> = text
                .split_whitespace()
                .map(str::parse)
                .collect::<Result<_, _>>()
                .map_err(|_| reader.invalid(format!("object {}: invalid {name}", object.id)))?;
            if vector.len() != components || vector.iter().any(|value| !value.is_finite()) {
                return Err(reader.invalid(format!(
                    "object {}: nonfinite or incorrect {name} vector",
                    object.id
                )));
            }
            if name == "size" && vector.iter().any(|&value| value <= 0.) {
                return Err(reader.invalid(format!("object {}: size must be positive", object.id)));
            }
        }
        validate_flags(
            &object.properties,
            "scene.json",
            &[
                "visible",
                "solid",
                "disablepropagation",
                "muteineditor",
                "startsilent",
            ],
        )?;
        for (name, value) in &object.properties {
            if !matches!(
                name.as_str(),
                "origin"
                    | "scale"
                    | "angles"
                    | "size"
                    | "parallaxDepth"
                    | "alpha"
                    | "brightness"
                    | "visible"
                    | "solid"
                    | "disablepropagation"
                    | "instanceoverride"
                    | "maxtime"
                    | "mintime"
                    | "muteineditor"
                    | "playbackmode"
                    | "startsilent"
                    | "volume"
            ) {
                graph.finding(
                    DiagnosticSeverity::Unsupported,
                    "unsupported-object-field",
                    "scene.json",
                    Some(object.id),
                    format!("unrecognized object field {name}"),
                );
            }
            if name == "disablepropagation" && value.as_bool() == Some(true) {
                graph.finding(
                    DiagnosticSeverity::Unsupported,
                    "unsupported-propagation",
                    "scene.json",
                    Some(object.id),
                    "disabled transform propagation is outside the MVP profile".into(),
                );
            }
            if name == "alpha" || name == "brightness" {
                let scalar = value.as_f64().ok_or_else(|| {
                    reader.invalid(format!("object {}: {name} must be a number", object.id))
                })?;
                if scalar < 0. || scalar > f64::from(f32::MAX) || (name == "alpha" && scalar > 1.) {
                    return Err(reader.invalid(format!("object {}: invalid {name}", object.id)));
                }
            }
        }
        if let Some(image) = &object.image {
            graph.edge(
                "scene.json",
                image,
                AssetKind::Model,
                "image",
                Some(object.id),
                false,
            )?;
            if graph.project_layers.contains(&image.to_ascii_lowercase()) {
                kind = ObjectKind::Composition;
            }
        }
        if let Some(particle) = &object.particle {
            graph.edge(
                "scene.json",
                particle,
                AssetKind::Particle,
                "particle",
                Some(object.id),
                false,
            )?;
        }
        if let Some(sounds) = &object.sound {
            graph.finding(
                DiagnosticSeverity::Info,
                "muted-audio",
                "scene.json",
                Some(object.id),
                "sound playback is outside the visual MVP".into(),
            );
            for sound in sounds {
                graph.edge(
                    "scene.json",
                    sound,
                    AssetKind::Binary,
                    "sound",
                    Some(object.id),
                    false,
                )?;
            }
        }
        for effect in &object.effects {
            graph.check_fields(
                effect.properties.keys(),
                &["id", "name", "visible"],
                "scene.json",
                Some(object.id),
            );
            validate_flags(&effect.properties, "scene.json", &["visible"])?;
            graph.edge(
                "scene.json",
                &effect.file,
                AssetKind::Effect,
                "effect",
                Some(object.id),
                false,
            )?;
            for (index, pass) in effect.passes.iter().enumerate() {
                graph.json_fields(
                    pass,
                    &["id", "textures", "constantshadervalues", "combos"],
                    "scene.json",
                    Some(object.id),
                )?;
                graph.override_textures("scene.json", pass, Some(object.id))?;
                validate_constants(pass, "scene.json")?;
                let shader = graph
                    .effects
                    .iter()
                    .find(|definition| definition.path.eq_ignore_ascii_case(&effect.file))
                    .and_then(|definition| definition.passes.get(index))
                    .and_then(|effect_pass| {
                        graph.materials.iter().find(|material| {
                            material.path.eq_ignore_ascii_case(&effect_pass.material)
                        })
                    })
                    .and_then(|material| material.passes.first())
                    .map(|pass| pass.shader.clone());
                if let Some(shader) = shader {
                    graph.combinations("scene.json", &shader, pass, Some(object.id))?;
                }
            }
        }
        objects.push(ObjectSummary {
            id: object.id,
            name: object.name,
            kind,
            effects: object
                .effects
                .into_iter()
                .map(|effect| effect.file)
                .collect(),
        });
    }
    graph.resolve_requirements();
    let Graph {
        assets,
        dependencies,
        textures,
        materials,
        effects,
        diagnostics,
        features,
        ..
    } = graph;
    Ok(SceneInspection {
        schema_version: 1,
        rendering_supported: false,
        dependency_scope: "declared-and-conditional".into(),
        package: package.summary,
        scene: SceneSummary {
            version: raw.version,
            canvas,
            objects,
            required_features: features.into_iter().collect(),
        },
        assets,
        dependencies,
        textures,
        materials,
        effects,
        diagnostics,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AssetKind {
    Model,
    Material,
    Effect,
    Particle,
    Shader,
    Texture,
    Binary,
}

struct Graph<'a> {
    package: &'a Package,
    visited: BTreeMap<String, AssetKind>,
    active: Vec<String>,
    project_layers: BTreeSet<String>,
    assets: Vec<AssetSummary>,
    dependencies: Vec<DependencySummary>,
    textures: Vec<TextureSummary>,
    materials: Vec<MaterialSummary>,
    effects: Vec<EffectSummary>,
    features: BTreeSet<String>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Graph<'a> {
    fn new(package: &'a Package) -> Self {
        Self {
            package,
            visited: BTreeMap::new(),
            active: Vec::new(),
            project_layers: BTreeSet::new(),
            assets: Vec::new(),
            dependencies: Vec::new(),
            textures: Vec::new(),
            materials: Vec::new(),
            effects: Vec::new(),
            features: BTreeSet::new(),
            diagnostics: Vec::new(),
        }
    }

    fn finding(
        &mut self,
        severity: DiagnosticSeverity,
        code: &str,
        asset: &str,
        object_id: Option<u64>,
        message: String,
    ) {
        self.diagnostics.push(Diagnostic {
            severity,
            code: code.into(),
            asset: asset.into(),
            object_id,
            message,
        });
    }

    fn check_fields<'b>(
        &mut self,
        fields: impl Iterator<Item = &'b String>,
        allowed: &[&str],
        key: &str,
        object: Option<u64>,
    ) {
        for field in fields {
            if !allowed.contains(&field.as_str()) {
                self.finding(
                    DiagnosticSeverity::Unsupported,
                    "unsupported-field",
                    key,
                    object,
                    format!("unrecognized field {field}"),
                );
            }
        }
    }

    fn json_fields(
        &mut self,
        value: &serde_json::Value,
        allowed: &[&str],
        key: &str,
        object: Option<u64>,
    ) -> Result<(), InspectionError> {
        let fields = value
            .as_object()
            .ok_or_else(|| InspectionError::invalid(key, "definition must be a JSON object"))?;
        self.check_fields(fields.keys(), allowed, key, object);
        Ok(())
    }

    fn resolve_requirements(&mut self) {
        // Discovery records local conditions. Reachability resolves inherited
        // conditions after every shared asset and incoming edge is known.
        let mut outgoing: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (index, edge) in self.dependencies.iter().enumerate() {
            outgoing
                .entry(edge.from.to_ascii_lowercase())
                .or_default()
                .push(index);
        }
        let mut required = BTreeMap::from([("scene.json".to_owned(), None)]);
        let mut pending = VecDeque::from(["scene.json".to_owned()]);
        while let Some(key) = pending.pop_front() {
            if let Some(edges) = outgoing.get(&key) {
                for &index in edges {
                    let edge = &self.dependencies[index];
                    let target = edge.to.to_ascii_lowercase();
                    if !edge.conditional && !required.contains_key(&target) {
                        required.insert(target.clone(), required[&key].or(edge.object_id));
                        pending.push_back(target);
                    }
                }
            }
        }
        for edge in &mut self.dependencies {
            edge.conditional |= !required.contains_key(&edge.from.to_ascii_lowercase());
        }
        for finding in &mut self.diagnostics {
            if finding.code == "missing-asset"
                && let Some(&object) = required.get(&finding.asset.to_ascii_lowercase())
            {
                finding.severity = DiagnosticSeverity::Error;
                finding.object_id = object;
            }
        }
    }

    fn edge(
        &mut self,
        from: &str,
        to: &str,
        kind: AssetKind,
        role: &str,
        object_id: Option<u64>,
        conditional: bool,
    ) -> Result<(), InspectionError> {
        let to = asset_key(to)?;
        if self.dependencies.len() == 65_536 {
            return Err(InspectionError::invalid(
                from,
                "dependency graph exceeds 65536 edges",
            ));
        }
        self.dependencies.push(DependencySummary {
            from: from.into(),
            to: to.clone(),
            role: role.into(),
            conditional,
            object_id,
        });
        if virtual_texture(&to) {
            return Ok(());
        }
        self.visit(&to, kind, object_id)
    }

    fn visit(
        &mut self,
        key: &str,
        kind: AssetKind,
        object: Option<u64>,
    ) -> Result<(), InspectionError> {
        let normalized = key.to_ascii_lowercase();
        if self.active.contains(&normalized) {
            self.finding(
                DiagnosticSeverity::Error,
                "dependency-cycle",
                key,
                object,
                format!("cycle through {}", self.active.join(" -> ")),
            );
            return Ok(());
        }
        let first_visit = !self.visited.contains_key(&normalized);
        if let Some(&previous) = self.visited.get(&normalized) {
            if previous == kind || kind == AssetKind::Binary {
                return Ok(());
            }
            if previous != AssetKind::Binary {
                self.finding(
                    DiagnosticSeverity::Error,
                    "asset-kind-conflict",
                    key,
                    object,
                    "asset is referenced with incompatible content types".into(),
                );
                return Ok(());
            }
        }
        self.visited.insert(normalized.clone(), kind);
        if self.visited.len() > 4096 || self.active.len() > 64 {
            return Err(InspectionError::invalid(
                key,
                "dependency graph size or depth limit exceeded",
            ));
        }
        let Some(mut reader) = self.package.reader(key)? else {
            if first_visit {
                self.finding(
                    DiagnosticSeverity::Warning,
                    "missing-asset",
                    key,
                    object,
                    "asset was not found in the package or engine root".into(),
                );
            }
            return Ok(());
        };
        if first_visit {
            let hash = reader.hash()?;
            self.assets.push(AssetSummary {
                path: key.into(),
                origin: reader.origin.into(),
                bytes: reader.length,
                sha256: hash,
            });
        }
        self.active.push(normalized);
        let result = match kind {
            AssetKind::Texture => texture::inspect(&mut reader).map(|texture| {
                if texture.frames > 0 {
                    self.features.insert("sprites".into());
                }
                self.textures.push(texture);
            }),
            AssetKind::Shader => self.shader(&mut reader, object),
            AssetKind::Binary => Ok(()),
            AssetKind::Model | AssetKind::Material | AssetKind::Effect | AssetKind::Particle => {
                self.json(&mut reader, kind, object)
            }
        };
        self.active.pop();
        if let Err(error) = result {
            self.finding(
                DiagnosticSeverity::Error,
                "invalid-asset",
                key,
                object,
                error.to_string(),
            );
        }
        Ok(())
    }

    fn json(
        &mut self,
        reader: &mut AssetReader,
        kind: AssetKind,
        object: Option<u64>,
    ) -> Result<(), InspectionError> {
        let bytes = reader.bytes(reader.length, 4 * 1024 * 1024)?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| reader.invalid(error.to_string()))?;
        let key = &reader.asset;
        match kind {
            AssetKind::Model => {
                self.json_fields(
                    &value,
                    &[
                        "material",
                        "autosize",
                        "cropoffset",
                        "passthrough",
                        "projectlayer",
                    ],
                    key,
                    object,
                )?;
                let material = string(&value, "material", key)?;
                if value.get("projectlayer").and_then(|v| v.as_bool()) == Some(true) {
                    self.project_layers.insert(key.to_ascii_lowercase());
                }
                self.edge(
                    key,
                    material,
                    AssetKind::Material,
                    "material",
                    object,
                    false,
                )?;
            }
            AssetKind::Material => {
                self.json_fields(&value, &["passes"], key, object)?;
                let mut summaries = Vec::new();
                for pass in array(&value, "passes", key)? {
                    self.json_fields(
                        pass,
                        &[
                            "shader",
                            "combos",
                            "textures",
                            "constantshadervalues",
                            "blending",
                            "cullmode",
                            "depthtest",
                            "depthwrite",
                        ],
                        key,
                        object,
                    )?;
                    let shader = asset_key(string(pass, "shader", key)?)?;
                    let combos = self.combinations(key, &shader, pass, object)?;
                    for (state, allowed) in [
                        ("blending", ["normal", "translucent", "additive"].as_slice()),
                        ("cullmode", ["nocull"].as_slice()),
                        ("depthtest", ["disabled"].as_slice()),
                        ("depthwrite", ["disabled"].as_slice()),
                    ] {
                        if let Some(authored) = pass.get(state) {
                            let authored = authored.as_str().ok_or_else(|| {
                                reader.invalid(format!("{state} must be a string"))
                            })?;
                            if !allowed.contains(&authored) {
                                self.finding(
                                    DiagnosticSeverity::Unsupported,
                                    "unsupported-material-state",
                                    key,
                                    object,
                                    format!("{state}={authored} is outside the MVP profile"),
                                );
                            }
                        }
                    }
                    validate_constants(pass, key)?;
                    summaries.push(MaterialPassSummary {
                        shader: shader.clone(),
                        blending: pass
                            .get("blending")
                            .and_then(|value| value.as_str())
                            .map(str::to_owned),
                        combos,
                    });
                    for suffix in ["vert", "frag"] {
                        self.edge(
                            key,
                            &format!("shaders/{shader}.{suffix}"),
                            AssetKind::Shader,
                            "shader",
                            object,
                            false,
                        )?;
                    }
                    self.override_textures(key, pass, object)?;
                }
                if summaries.is_empty() {
                    return Err(reader.invalid("material has no passes"));
                }
                self.materials.push(MaterialSummary {
                    path: key.clone(),
                    passes: summaries,
                });
            }
            AssetKind::Effect => {
                self.json_fields(
                    &value,
                    &[
                        "version",
                        "replacementkey",
                        "name",
                        "description",
                        "group",
                        "performance",
                        "preview",
                        "passes",
                        "fbos",
                        "dependencies",
                        "gizmos",
                    ],
                    key,
                    object,
                )?;
                let version = value
                    .get("version")
                    .and_then(|value| value.as_u64())
                    .and_then(|version| u32::try_from(version).ok())
                    .ok_or_else(|| reader.invalid("effect version must be a u32"))?;
                let replacement = string(&value, "replacementkey", key)?;
                let supported = matches!(
                    (replacement, version),
                    ("cloudmotion", 2)
                        | ("blur_precise", 1)
                        | ("lightshafts", 1)
                        | ("chromatic_aberration", 1)
                );
                if !supported {
                    self.finding(
                        DiagnosticSeverity::Unsupported,
                        "unsupported-effect",
                        key,
                        object,
                        format!(
                            "effect {replacement} version {version} is outside the MVP profile"
                        ),
                    );
                }
                self.features.insert(format!("effect/{replacement}"));
                let mut targets = BTreeSet::new();
                if let Some(fbos) = value.get("fbos") {
                    for fbo in fbos
                        .as_array()
                        .ok_or_else(|| reader.invalid("fbos must be an array"))?
                    {
                        self.json_fields(fbo, &["name", "format"], key, object)?;
                        let name = string(fbo, "name", key)?;
                        if !name.starts_with("_rt_") || !targets.insert(name.to_owned()) {
                            return Err(reader.invalid("invalid or duplicate effect target"));
                        }
                        if string(fbo, "format", key)? != "rgba_backbuffer" {
                            self.finding(
                                DiagnosticSeverity::Unsupported,
                                "unsupported-target-format",
                                key,
                                object,
                                "effect target format is outside the MVP profile".into(),
                            );
                        }
                    }
                }
                let mut written = BTreeSet::new();
                let mut summaries = Vec::new();
                for pass in array(&value, "passes", key)? {
                    self.json_fields(pass, &["material", "target", "bind"], key, object)?;
                    let material = asset_key(string(pass, "material", key)?)?;
                    let mut bindings = Vec::new();
                    let mut indices = BTreeSet::new();
                    if let Some(bind) = pass.get("bind") {
                        for input in bind
                            .as_array()
                            .ok_or_else(|| reader.invalid("bind must be an array"))?
                        {
                            self.json_fields(input, &["name", "index"], key, object)?;
                            let name = string(input, "name", key)?;
                            let index = input
                                .get("index")
                                .and_then(|value| value.as_u64())
                                .filter(|&value| value < 8)
                                .ok_or_else(|| {
                                    reader.invalid("binding index must be between 0 and 7")
                                })? as u32;
                            if !indices.insert(index)
                                || (name != "previous" && !written.contains(name))
                            {
                                return Err(reader.invalid(
                                    "duplicate binding index or target read before write",
                                ));
                            }
                            bindings.push(EffectBindingSummary {
                                index,
                                source: name.into(),
                            });
                        }
                    }
                    let target = pass
                        .get("target")
                        .map(|value| {
                            value
                                .as_str()
                                .ok_or_else(|| reader.invalid("target must be a string"))
                        })
                        .transpose()?;
                    if let Some(target) = target {
                        if !targets.contains(target) {
                            return Err(reader.invalid("pass uses an undeclared target"));
                        }
                        if bindings.iter().any(|input| input.source == target) {
                            return Err(reader.invalid("pass reads and writes the same target"));
                        }
                        written.insert(target.to_owned());
                    }
                    self.edge(
                        key,
                        &material,
                        AssetKind::Material,
                        "effect-pass",
                        object,
                        false,
                    )?;
                    summaries.push(EffectPassSummary {
                        material,
                        target: target.map(str::to_owned),
                        bindings,
                    });
                }
                if summaries.is_empty() || summaries.len() > 8 {
                    return Err(reader.invalid("effect pass count must be between 1 and 8"));
                }
                if supported && !self.effect_profile_matches(replacement, &summaries) {
                    self.finding(
                        DiagnosticSeverity::Unsupported,
                        "unsupported-effect-profile",
                        key,
                        object,
                        "effect pass layout differs from the inspected profile".into(),
                    );
                }
                self.effects.push(EffectSummary {
                    path: key.clone(),
                    replacement_key: replacement.into(),
                    version,
                    passes: summaries,
                });
                if let Some(dependencies) = value.get("dependencies") {
                    for dependency in dependencies
                        .as_array()
                        .ok_or_else(|| reader.invalid("dependencies must be an array"))?
                    {
                        let dependency = asset_key(
                            dependency
                                .as_str()
                                .ok_or_else(|| reader.invalid("dependency must be a path"))?,
                        )?;
                        let kind = if dependency.ends_with(".vert")
                            || dependency.ends_with(".frag")
                            || dependency.ends_with(".h")
                        {
                            AssetKind::Shader
                        } else if dependency.starts_with("materials/")
                            && dependency.ends_with(".json")
                        {
                            AssetKind::Material
                        } else {
                            AssetKind::Binary
                        };
                        self.edge(key, &dependency, kind, "declared-dependency", object, false)?;
                    }
                }
            }
            AssetKind::Particle => {
                self.json_fields(
                    &value,
                    &[
                        "controlpoint",
                        "emitter",
                        "initializer",
                        "material",
                        "maxcount",
                        "operator",
                        "renderer",
                        "starttime",
                    ],
                    key,
                    object,
                )?;
                self.features.insert("particles".into());
                if value
                    .get("maxcount")
                    .and_then(|value| value.as_u64())
                    .filter(|&count| count > 0 && count <= 4096)
                    .is_none()
                {
                    return Err(reader.invalid("particle maxcount must be between 1 and 4096"));
                }
                for (group, allowed) in [
                    ("emitter", ["sphererandom"].as_slice()),
                    (
                        "initializer",
                        [
                            "lifetimerandom",
                            "sizerandom",
                            "velocityrandom",
                            "colorrandom",
                            "rotationrandom",
                            "alpharandom",
                        ]
                        .as_slice(),
                    ),
                    ("operator", ["movement", "alphafade"].as_slice()),
                    ("renderer", ["sprite"].as_slice()),
                ] {
                    for component in array(&value, group, key)? {
                        let name = string(component, "name", key)?;
                        if !allowed.contains(&name) {
                            self.finding(
                                DiagnosticSeverity::Unsupported,
                                "unsupported-particle-component",
                                key,
                                object,
                                format!("{group}/{name} is outside the MVP profile"),
                            );
                        }
                    }
                }
                self.edge(
                    key,
                    string(&value, "material", key)?,
                    AssetKind::Material,
                    "particle-material",
                    object,
                    false,
                )?;
            }
            AssetKind::Shader | AssetKind::Texture | AssetKind::Binary => {
                unreachable!("JSON kinds selected by visit")
            }
        }
        Ok(())
    }

    fn effect_profile_matches(&self, replacement: &str, passes: &[EffectPassSummary]) -> bool {
        let expected_shader = match replacement {
            "cloudmotion" => "effects/cloudmotion",
            "blur_precise" => "effects/blur_precise_gaussian",
            "lightshafts" => "effects/lightshafts",
            "chromatic_aberration" => "effects/chromatic_aberration",
            _ => return false,
        };
        let count = if replacement == "blur_precise" { 2 } else { 1 };
        if passes.len() != count {
            return false;
        }
        for (index, pass) in passes.iter().enumerate() {
            let Some(material) = self
                .materials
                .iter()
                .find(|material| material.path.eq_ignore_ascii_case(&pass.material))
            else {
                return false;
            };
            if material.passes.len() != 1 || material.passes[0].shader != expected_shader {
                return false;
            }
            if replacement == "blur_precise" {
                let vertical = material.passes[0]
                    .combos
                    .get("VERTICAL")
                    .copied()
                    .unwrap_or(0);
                if vertical != index as i32 {
                    return false;
                }
            }
        }
        if replacement == "blur_precise" {
            let Some(target) = &passes[0].target else {
                return false;
            };
            passes[0].bindings.is_empty()
                && passes[1].target.is_none()
                && passes[1].bindings.len() == 2
                && passes[1]
                    .bindings
                    .iter()
                    .any(|binding| binding.index == 0 && &binding.source == target)
                && passes[1]
                    .bindings
                    .iter()
                    .any(|binding| binding.index == 1 && binding.source == "previous")
        } else {
            passes[0].target.is_none() && passes[0].bindings.is_empty()
        }
    }

    fn override_textures(
        &mut self,
        key: &str,
        pass: &serde_json::Value,
        object: Option<u64>,
    ) -> Result<(), InspectionError> {
        if let Some(textures) = pass.get("textures") {
            for name in textures
                .as_array()
                .ok_or_else(|| InspectionError::invalid(key, "textures must be an array"))?
            {
                if name.is_null() {
                    continue;
                }
                let name = name.as_str().ok_or_else(|| {
                    InspectionError::invalid(key, "texture must be a string or null")
                })?;
                let texture = texture_path(name)?;
                self.edge(key, &texture, AssetKind::Texture, "texture", object, false)?;
            }
        }
        Ok(())
    }

    fn combinations(
        &mut self,
        key: &str,
        shader: &str,
        pass: &serde_json::Value,
        object: Option<u64>,
    ) -> Result<BTreeMap<String, i32>, InspectionError> {
        let combos = pass
            .get("combos")
            .map(|value| serde_json::from_value::<BTreeMap<String, i32>>(value.clone()))
            .transpose()
            .map_err(|error| InspectionError::invalid(key, format!("invalid combos: {error}")))?
            .unwrap_or_default();
        if let Some(allowed) = shader_combos(shader) {
            for (name, &value) in &combos {
                if !allowed.contains(&name.as_str()) {
                    self.finding(
                        DiagnosticSeverity::Unsupported,
                        "unsupported-combo",
                        key,
                        object,
                        format!("{shader} combination {name} is outside the MVP profile"),
                    );
                } else if !(matches!(value, 0 | 1) || name == "RAYMODE" && value == 2) {
                    self.finding(
                        DiagnosticSeverity::Unsupported,
                        "unsupported-combo-value",
                        key,
                        object,
                        format!("{shader} combination {name}={value} is outside the MVP profile"),
                    );
                }
            }
        } else {
            self.finding(
                DiagnosticSeverity::Unsupported,
                "unsupported-shader",
                key,
                object,
                format!("shader {shader} is outside the MVP profile"),
            );
        }
        Ok(combos)
    }

    fn shader(
        &mut self,
        reader: &mut AssetReader,
        object: Option<u64>,
    ) -> Result<(), InspectionError> {
        let bytes = reader.bytes(reader.length, 1024 * 1024)?;
        let source = std::str::from_utf8(&bytes)
            .map_err(|_| reader.invalid("shader metadata is not UTF-8"))?;
        let mut depth = 0_u32;
        for line in source.lines() {
            let line = line.trim_start().trim_start_matches('\u{feff}');
            if line.starts_with("#if") {
                depth += 1;
            }
            if line.starts_with("#endif") {
                depth = depth.saturating_sub(1);
            }
            if let Some(include) = line.strip_prefix("#include") {
                let include = include.trim();
                let name = include
                    .strip_prefix('"')
                    .and_then(|text| text.split_once('"').map(|pair| pair.0));
                if let Some(name) = name {
                    self.edge(
                        &reader.asset,
                        &format!("shaders/{name}"),
                        AssetKind::Shader,
                        "include",
                        object,
                        depth > 0,
                    )?;
                } else {
                    return Err(reader.invalid("unsupported shader include syntax"));
                }
            }
            if let Some((declaration, annotation)) = line.split_once("//")
                && declaration.contains("sampler")
                && annotation.trim_start().starts_with('{')
            {
                let annotation: serde_json::Value =
                    serde_json::from_str(annotation).map_err(|error| {
                        reader.invalid(format!("invalid sampler annotation: {error}"))
                    })?;
                if let Some(default) = annotation.get("default").and_then(|v| v.as_str()) {
                    self.edge(
                        &reader.asset,
                        &texture_path(default)?,
                        AssetKind::Texture,
                        "shader-default",
                        object,
                        true,
                    )?;
                }
            }
        }
        Ok(())
    }
}

fn virtual_texture(name: &str) -> bool {
    name.starts_with("_rt_") || name.starts_with("_alias_") || name == "previous"
}

fn texture_path(name: &str) -> Result<String, InspectionError> {
    let name = asset_key(name)?;
    if virtual_texture(&name) || (name.starts_with("materials/") && name.ends_with(".tex")) {
        Ok(name)
    } else {
        Ok(format!("materials/{name}.tex"))
    }
}

fn string<'a>(
    value: &'a serde_json::Value,
    field: &str,
    key: &str,
) -> Result<&'a str, InspectionError> {
    value
        .get(field)
        .and_then(|value| value.as_str())
        .ok_or_else(|| InspectionError::invalid(key, format!("{field} must be a string")))
}

fn array<'a>(
    value: &'a serde_json::Value,
    field: &str,
    key: &str,
) -> Result<&'a [serde_json::Value], InspectionError> {
    value
        .get(field)
        .and_then(|value| value.as_array())
        .map(Vec::as_slice)
        .ok_or_else(|| InspectionError::invalid(key, format!("{field} must be an array")))
}

fn shader_combos(shader: &str) -> Option<&'static [&'static str]> {
    match shader {
        "genericimage4" => Some(&["SPRITESHEET"]),
        "genericparticle" | "composelayer" => Some(&[]),
        "effects/cloudmotion" | "effects/chromatic_aberration" => Some(&["MASK"]),
        "effects/blur_precise_gaussian" => Some(&["VERTICAL", "ENABLEMASK", "MASK"]),
        "effects/lightshafts" => Some(&["DIRECTDRAW", "RAYCORNER", "RAYMODE", "RENDERING", "MASK"]),
        _ => None,
    }
}

fn validate_constants(pass: &serde_json::Value, key: &str) -> Result<(), InspectionError> {
    if let Some(constants) = pass.get("constantshadervalues") {
        for (name, value) in constants.as_object().ok_or_else(|| {
            InspectionError::invalid(key, "constantshadervalues must be an object")
        })? {
            let valid = if let Some(number) = value.as_f64() {
                number.is_finite() && number.abs() <= f64::from(f32::MAX)
            } else if let Some(vector) = value.as_str() {
                let values: Vec<_> = vector.split_whitespace().collect();
                !values.is_empty()
                    && values.len() <= 4
                    && values
                        .iter()
                        .all(|value| value.parse::<f32>().is_ok_and(f32::is_finite))
            } else {
                false
            };
            if !valid {
                return Err(InspectionError::invalid(
                    key,
                    format!("shader constant {name} must be a finite scalar or vector"),
                ));
            }
        }
    }
    Ok(())
}

fn validate_flags(
    properties: &BTreeMap<String, serde_json::Value>,
    key: &str,
    flags: &[&str],
) -> Result<(), InspectionError> {
    for &flag in flags {
        if let Some(value) = properties.get(flag)
            && !value.is_boolean()
        {
            return Err(InspectionError::invalid(
                key,
                format!("{flag} must be a boolean; property bindings are outside the MVP profile"),
            ));
        }
    }
    Ok(())
}
