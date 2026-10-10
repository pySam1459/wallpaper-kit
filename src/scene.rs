use crate::assets::package::Package;
use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf};

mod inspect;
mod layers;
mod playback;
mod precise;
pub(crate) mod snapshot;
pub use layers::{GaussianBlur, ImageComposition, ImageLayer, PreciseBlur};
pub use playback::{ImagePlayback, ImagePlaybackReport, ImagePlaybackSample};
pub use snapshot::ImageSnapshot;

/// Explicit read-only package and engine roots for inspection or TEX decoding.
#[derive(Debug, Clone)]
pub struct SceneSource {
    /// Path to the local `scene.pkg` file.
    pub package: PathBuf,
    /// Installed engine asset directory used after package-local resolution.
    pub engine_assets: PathBuf,
}

/// Metadata and diagnostics from inspection, independent of a graphics device.
///
/// JSON serialization is intended for inspection tooling. Successful inspection
/// does not mean the scene can be rendered. Image payloads are not decoded.
#[derive(Debug, Serialize)]
pub struct SceneInspection {
    /// Version of this report schema.
    pub schema_version: u32,
    /// Always false in this inspection milestone; no scene renderer exists yet.
    pub rendering_supported: bool,
    /// Scope of dependency discovery, including conditional shader references.
    pub dependency_scope: String,
    /// Package identity and indexed entry count.
    pub package: PackageSummary,
    /// Validated scene metadata.
    pub scene: SceneSummary,
    /// Assets reached through the dependency graph.
    pub assets: Vec<AssetSummary>,
    /// Authored references, including conditional shader references.
    pub dependencies: Vec<DependencySummary>,
    /// Texture page and animation metadata, without payload decoding.
    pub textures: Vec<TextureSummary>,
    /// Material shader names, states and authored combinations.
    pub materials: Vec<MaterialSummary>,
    /// Effect versions and pass bindings in authored order.
    pub effects: Vec<EffectSummary>,
    /// Missing assets, unsupported features and intentional omissions.
    pub diagnostics: Vec<Diagnostic>,
}

/// Package identity recorded without extracting assets.
#[derive(Debug, Serialize)]
pub struct PackageSummary {
    /// Canonical package path.
    pub path: PathBuf,
    /// Validated format signature.
    pub signature: String,
    /// File length in bytes.
    pub bytes: u64,
    /// SHA-256 of the complete package, streamed through a bounded buffer.
    pub sha256: String,
    /// Number of indexed entries.
    pub entries: usize,
}

/// Scene properties used by the inspector.
#[derive(Debug, Serialize)]
pub struct SceneSummary {
    /// Authored scene format version.
    pub version: u32,
    /// Orthographic canvas width and height.
    pub canvas: [u32; 2],
    /// Objects in authored composition order.
    pub objects: Vec<ObjectSummary>,
    /// Visual features identified in this scene, not implemented capabilities.
    pub required_features: Vec<String>,
}

/// An authored object and its resolved type.
#[derive(Debug, Serialize)]
pub struct ObjectSummary {
    /// Authored object identity.
    pub id: u64,
    /// Editor name retained for diagnostics.
    pub name: String,
    /// Image, composition, particle, shape, sound, or unknown visual object.
    pub kind: ObjectKind,
    /// Effect definition references in authored order.
    pub effects: Vec<String>,
}

/// Object category determined from the authored definition.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ObjectKind {
    /// Textured image layer.
    Image,
    /// Accumulated-image composition point.
    Composition,
    /// Particle instance.
    Particle,
    /// Procedural shape.
    Shape,
    /// Muted sound object.
    Sound,
    /// An object outside the inspected profile.
    Unknown,
}

/// Identity of an asset reached from the scene.
#[derive(Debug, Serialize)]
pub struct AssetSummary {
    /// Normalized logical asset path.
    pub path: String,
    /// Package or engine origin.
    pub origin: String,
    /// Asset length in bytes.
    pub bytes: u64,
    /// SHA-256 of the complete asset.
    pub sha256: String,
}

/// A directed edge in the scene dependency graph.
#[derive(Debug, Serialize)]
pub struct DependencySummary {
    /// Referencing asset.
    pub from: String,
    /// Referenced asset or virtual render target.
    pub to: String,
    /// Why the reference exists.
    pub role: String,
    /// Whether a shader condition can make this reference inactive.
    pub conditional: bool,
    /// Scene object that first introduced this reference, when applicable.
    pub object_id: Option<u64>,
}

/// TEX storage and animation metadata.
#[derive(Debug, Serialize)]
pub struct TextureSummary {
    /// Logical asset path.
    pub path: String,
    /// TEX pixel format code.
    pub format: u32,
    /// Header texture dimensions, distinct from output size limits.
    pub extent: [u32; 2],
    /// Validated top-level and image-container signatures.
    pub signatures: Vec<String>,
    /// Top mip storage for every page.
    pub pages: Vec<PageSummary>,
    /// Number of validated sprite frame records.
    pub frames: usize,
    /// Authored sum of sprite durations in seconds, or zero for static textures.
    pub duration_seconds: f64,
}

/// Top mip storage for a texture page.
#[derive(Debug, Serialize)]
pub struct PageSummary {
    /// Pixel dimensions.
    pub extent: [u32; 2],
    /// Number of mip levels.
    pub mips: u32,
    /// Encoded top mip bytes.
    pub stored_bytes: u64,
    /// GPU-format top mip bytes after decoding an image or LZ4 payload.
    pub decoded_bytes: u64,
    /// Sum of GPU-format bytes for all mip levels on this page.
    pub decoded_mip_bytes: u64,
}

/// Shader material metadata retained for compatibility review.
#[derive(Debug, Serialize)]
pub struct MaterialSummary {
    /// Logical material path.
    pub path: String,
    /// Authored material passes.
    pub passes: Vec<MaterialPassSummary>,
}

/// Shader identity and combinations of one material pass.
#[derive(Debug, Serialize)]
pub struct MaterialPassSummary {
    /// Logical shader name without stage suffix.
    pub shader: String,
    /// Explicit blending state, or absent when the asset relies on a default.
    pub blending: Option<String>,
    /// Authored integer combinations.
    pub combos: std::collections::BTreeMap<String, i32>,
}

/// An effect definition with its validated pass inputs and outputs.
#[derive(Debug, Serialize)]
pub struct EffectSummary {
    /// Logical effect definition path.
    pub path: String,
    /// Compatibility replacement key.
    pub replacement_key: String,
    /// Authored effect version.
    pub version: u32,
    /// Passes in authored order.
    pub passes: Vec<EffectPassSummary>,
}

/// One effect pass and its framebuffer bindings.
#[derive(Debug, Serialize)]
pub struct EffectPassSummary {
    /// Referenced material.
    pub material: String,
    /// Declared output target, or the effect's default output.
    pub target: Option<String>,
    /// Explicit sampler bindings, including `previous` input.
    pub bindings: Vec<EffectBindingSummary>,
}

/// Sampler input of an effect pass.
#[derive(Debug, Serialize)]
pub struct EffectBindingSummary {
    /// Zero-based sampler index.
    pub index: u32,
    /// A previously written target or `previous` effect input.
    pub source: String,
}

/// An actionable finding scoped to an asset or object.
#[derive(Debug, Serialize)]
pub struct Diagnostic {
    /// Whether this finding blocks structural/profile inspection.
    pub severity: DiagnosticSeverity,
    /// Stable finding category.
    pub code: String,
    /// Logical asset containing the problem.
    pub asset: String,
    /// Object identity when the finding belongs to a scene object.
    pub object_id: Option<u64>,
    /// Human-readable detail.
    pub message: String,
}

/// Classification of inspection findings, independent of render support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticSeverity {
    /// An intentional omission such as muted audio.
    Info,
    /// A potentially inactive dependency could not be resolved.
    Warning,
    /// Required asset or structure is invalid or missing.
    Error,
    /// Authored behavior is outside the inspected MVP profile.
    Unsupported,
}

impl SceneInspection {
    /// True when an error or unsupported profile needs attention.
    ///
    /// False means inspection passed, not that rendering is available.
    pub fn has_blocking_findings(&self) -> bool {
        self.diagnostics.iter().any(|finding| {
            matches!(
                finding.severity,
                DiagnosticSeverity::Error | DiagnosticSeverity::Unsupported
            )
        })
    }
}

/// Asset-named I/O, structural or payload failure in inspection/TEX decoding.
#[derive(Debug)]
pub struct InspectionError {
    asset: String,
    detail: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl InspectionError {
    pub(crate) fn invalid(asset: &str, detail: impl Into<String>) -> Self {
        Self {
            asset: asset.into(),
            detail: detail.into(),
            source: None,
        }
    }

    pub(crate) fn io(asset: &str, error: std::io::Error) -> Self {
        Self {
            asset: asset.into(),
            detail: error.to_string(),
            source: Some(Box::new(error)),
        }
    }
}

impl fmt::Display for InspectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.asset, self.detail)
    }
}

impl std::error::Error for InspectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|source| &**source as _)
    }
}

/// Inspect indexed metadata and dependencies without creating a GPU device.
///
/// This performs blocking file I/O, hashing and parsing. Run it on a tool or
/// worker thread, not a consuming application's UI thread. Structural failures
/// return an error; graph findings are recorded in the returned diagnostics.
/// The package and engine assets are only opened for reading.
pub fn inspect_scene(source: &SceneSource) -> Result<SceneInspection, InspectionError> {
    inspect::run(source)
}

#[derive(Deserialize)]
struct RawScene {
    version: u32,
    general: General,
    objects: Vec<RawObject>,
    #[serde(flatten)]
    properties: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct General {
    orthogonalprojection: Projection,
    #[serde(flatten)]
    properties: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct Projection {
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct RawObject {
    id: u64,
    #[serde(default)]
    name: String,
    image: Option<String>,
    particle: Option<String>,
    shape: Option<String>,
    sound: Option<Vec<String>>,
    #[serde(default)]
    effects: Vec<RawEffect>,
    #[serde(flatten)]
    properties: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct RawEffect {
    file: String,
    #[serde(default)]
    passes: Vec<serde_json::Value>,
    #[serde(flatten)]
    properties: std::collections::BTreeMap<String, serde_json::Value>,
}
