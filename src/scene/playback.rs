use super::{
    CloudMotionReport, ImageComposition, InspectionError, PlaybackSettings, SceneSource, particles,
    snapshot,
};
use crate::{SpritePager, TextureDecoder, TexturePage, TextureRegion};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex, TryLockError},
    time::{Duration, Instant, SystemTime},
};

/// Identity and explicit omissions of image playback.
#[derive(Debug, Default, Serialize)]
pub struct ImagePlaybackReport {
    /// Consumer policy used to prepare this playback.
    pub settings: PlaybackSettings,
    /// Hash of the scene package or standalone video source.
    pub package_sha256: String,
    /// Loaded visible images in authored draw order.
    pub object_ids: Vec<u64>,
    /// Unique animated assets, in the order used by sample frame indices.
    pub animated_assets: Vec<String>,
    /// Applied unmasked default-kernel precise blurs, keyed by scene object ID.
    /// Values are horizontal/vertical source-texel sampling scales, not sigma.
    pub precise_blurs: BTreeMap<u64, [f32; 2]>,
    /// Applied cloud settings and static noise/mask asset identities by object ID.
    pub cloud_motions: BTreeMap<u64, CloudMotionReport>,
    /// Visible particle objects whose supported components were prepared.
    /// Children with unsupported material/features are named in omissions.
    pub particle_objects: Vec<u64>,
    /// Whether HDR accumulation is enabled by scene discovery and VFX policy.
    /// Remains true when authored bloom is disabled or its strength is zero.
    /// The output remains an eight-bit SDR consumer texture.
    pub hdr_bloom: bool,
    /// Opaque background clear color in normalized RGBA code values.
    /// Scene discovery honors enabled `general.clearcolor`; manual selected
    /// playback and standalone video retain [`crate::BLANK_RGBA`].
    pub background_color: [f32; 4],
    /// Image objects with the checked two-phase water-flow profile.
    pub water_flow_objects: Vec<u64>,
    /// Applied basic masked UV animation profiles, keyed by image object ID.
    /// Values identify the first motion pass: `spin`, `waterwaves`, `shake`,
    /// `foliagesway`, `waterripple` or `pulse`. See `effect_chains` for all applied passes.
    pub image_motions: BTreeMap<u64, String>,
    /// Applied image passes in authored order, keyed by scene object ID.
    /// Unsupported passes appear in omissions instead. At most 16 passes per image.
    pub effect_chains: BTreeMap<u64, Vec<String>>,
    /// Supported composition points and their applied profiles in authored order.
    /// Empty lists contain no supported effect profiles; layer appearance can
    /// still apply. Omitted effects remain reported.
    pub composition_layers: BTreeMap<u64, Vec<String>>,
    /// Standalone corner light-shaft objects prepared in authored draw order.
    pub shape_objects: Vec<u64>,
    /// Prepared static or recognized calendar/clock text objects, in authored order.
    pub text_objects: Vec<u64>,
    /// Unsupported and unselected behavior. This is not full scene support.
    pub omissions: Vec<String>,
    /// Direct user-property overrides used during preparation.
    pub user_properties: BTreeMap<String, serde_json::Value>,
    /// Recognized direct links and checked clock/calendar bindings.
    /// Recognition does not imply that an optional object could be rendered.
    pub resolved_bindings: Vec<String>,
}

/// One complete selection at a common playback time.
#[derive(Clone)]
pub struct ImagePlaybackSample {
    /// Ordered image draws ready for [`crate::Renderer::render_images`].
    pub composition: Arc<ImageComposition>,
    /// Library clock time used by every animated source in this sample.
    pub time: Duration,
    /// Frame indices in [`ImagePlaybackReport::animated_assets`] order.
    pub frame_indices: Vec<usize>,
    /// Text values submitted for this sample, keyed by rendered text object ID.
    pub text_values: BTreeMap<u64, String>,
}

enum Source {
    Static(Arc<TextureRegion>),
    Animated(SpritePager),
    Video(crate::assets::video::VideoPager),
}

fn optional_objects<T>(
    objects: &[super::RawObject],
    tolerant: bool,
    omissions: &mut Vec<String>,
    mut prepare: impl FnMut(&[super::RawObject], &mut Vec<String>) -> Result<Vec<T>, InspectionError>,
) -> Result<Vec<T>, InspectionError> {
    if !tolerant {
        return prepare(objects, omissions);
    }
    let mut prepared = Vec::new();
    for object in objects.iter().take(32) {
        match prepare(std::slice::from_ref(object), omissions) {
            Ok(objects) => prepared.extend(objects),
            Err(error) => omissions.push(format!(
                "Object {}: optional content omitted: {error}",
                object.id
            )),
        }
    }
    if objects.len() > 32 {
        omissions
            .push("Optional objects beyond the 32-object preparation limit are omitted.".into());
    }
    Ok(prepared)
}

/// Prepared selected-image composition with a shared monotonic playback clock.
///
/// Static sources decode once; duplicate animated references share one pager.
/// The host schedules sampling/presentation from its event loop. Sampling never
/// waits for codecs and returns `None` until every source for that time is ready.
/// Default unmasked precise blur, cloud motion and a bounded 2D particle profile
/// and masked Spin/Water Waves/periodic Shake/UV Foliage Sway/planar Water Ripple animation are supported by scene discovery. Other effects,
/// parallax, camera/color settings and animated crop offsets are unsupported,
/// as reported or rejected by [`Self::open`]. Existing per-
/// pager and composition bounds apply; aggregate scene memory is not budgeted.
/// Opening and joined shutdown belong outside latency-critical UI updates.
pub struct ImagePlayback {
    // Keep the package read guard across asynchronous pager opening so all
    // sources use the reported package version, including static-only loads.
    _decoder: Option<TextureDecoder>,
    plan: snapshot::ImagePlan,
    layer_sources: Vec<usize>,
    sources: Vec<Source>,
    effects: Vec<Vec<super::layers::LayerEffect>>,
    composition_effects: Vec<Vec<super::layers::LayerEffect>>,
    particles: Vec<particles::Particles>,
    particle_pages: Vec<Arc<TexturePage>>,
    rays: Vec<super::rays::RayLayer>,
    texts: Vec<super::text::TextLayer>,
    report: ImagePlaybackReport,
    base: Duration,
    started: Option<Instant>,
    frozen: Option<ImagePlaybackSample>,
    pending_time: Mutex<Option<Duration>>,
    calendar_time: Option<SystemTime>,
}

impl ImagePlayback {
    /// Open a local video using installed `ffmpeg.exe` and `ffprobe.exe` on PATH.
    /// Metadata and source hashing block during opening. Decode runs on a
    /// dedicated worker; sampling never waits for it. Playback is silent and
    /// loops at the source's rational average frame rate (up to 240 fps), scaled
    /// to `size` before GPU upload. Variable-rate media uses that average cadence;
    /// no intermediate video frames are synthesized. One
    /// ready frame, one active decode and one pending request are retained.
    /// Consumer-held samples and FFmpeg internal buffers are outside this bound.
    /// Missing tools, unsupported media and decode failures are reported.
    /// Shutdown joins and cancels the owned codec process outside UI updates.
    pub fn open_video(
        path: &Path,
        size: crate::Size,
        settings: PlaybackSettings,
    ) -> Result<Self, InspectionError> {
        let (pager, hash) = crate::assets::video::VideoPager::open(path, size)?;
        Ok(Self {
            _decoder: None,
            plan: snapshot::video_plan(size),
            layer_sources: vec![0],
            sources: vec![Source::Video(pager)],
            effects: vec![Vec::new()],
            composition_effects: Vec::new(),
            particles: Vec::new(),
            particle_pages: Vec::new(),
            rays: Vec::new(),
            texts: Vec::new(),
            report: ImagePlaybackReport {
                settings,
                package_sha256: hash,
                object_ids: vec![0],
                background_color: crate::BLANK_RGBA.map(|v| f32::from(v) / 255.),
                animated_assets: vec![path.display().to_string()],
                omissions: vec![
                    "Video audio is omitted; CPU FFmpeg decoding and RGBA uploads are used.".into(),
                ],
                ..Default::default()
            },
            base: Duration::ZERO,
            started: None,
            frozen: None,
            pending_time: Mutex::new(None),
            calendar_time: None,
        })
    }

    /// Fix calendar/clock text to an explicit wall time for repeatable captures.
    /// Does not execute scripts or change the scene animation clock.
    pub fn set_calendar_time(&mut self, time: SystemTime) {
        self.calendar_time = Some(time);
        self.frozen = None;
    }
    /// Blocking preparation of selected image descriptions and static sources.
    /// Starts unique sprite pagers in the background. Open on an asset thread
    /// or before creating the host window, never during UI updates. Playback
    /// starts paused at zero; initial animated samples may still be unavailable.
    /// Uses the same checked selected-image profile as the snapshot API.
    pub fn open(source: &SceneSource, objects: &[u64]) -> Result<Self, InspectionError> {
        Self::open_with_settings(source, objects, PlaybackSettings::default())
    }

    /// Like [`Self::open`], with consumer quality, animation and effect policy.
    pub fn open_with_settings(
        source: &SceneSource,
        objects: &[u64],
        settings: PlaybackSettings,
    ) -> Result<Self, InspectionError> {
        Self::prepare(source, Some(objects), settings, &BTreeMap::new())
    }

    /// Discover image objects in authored order without supplying object IDs.
    /// Uses the checked image profile of [`Self::open`], including its blocking
    /// preparation and resource bounds. Other content and project-layer
    /// composition points are reported as omitted. Supported visible particle
    /// objects are included in authored draw order; unsupported components are
    /// reported. Particle sampling is deterministic at a given clock time and
    /// capped at 4,096 quads and 32,768 birth evaluations per sample. Excess work
    /// is dropped in authored order. Unsupported image materials
    /// or properties fail rather than silently dropping that image. This is a
    /// partial image scene, not full scene compatibility. Starts paused at zero.
    pub fn open_scene_images(source: &SceneSource) -> Result<Self, InspectionError> {
        Self::open_scene_images_with_settings(source, PlaybackSettings::default())
    }

    /// Like [`Self::open_scene_images`], with consumer initialization settings.
    /// Disabled effects/particles are omitted before their assets are loaded.
    pub fn open_scene_images_with_settings(
        source: &SceneSource,
        settings: PlaybackSettings,
    ) -> Result<Self, InspectionError> {
        Self::prepare(source, None, settings, &BTreeMap::new())
    }

    /// Prepare scene playback with direct user-property overrides.
    ///
    /// Values are raw Booleans or strings, not `{value: ...}` wrappers. Supported
    /// links are checkbox/combo visibility and text/color strings. Unset links
    /// retain their authored binding value; project.json is not read implicitly.
    /// At most 128 keys of 1..=64 ASCII letters/digits/underscores and string
    /// values of at most 256 bytes are admitted. Unsupported scripts remain
    /// reported under tolerant settings; invalid overrides return an error.
    ///
    /// This blocks for package reads, decoding and font preparation. Prepare a
    /// replacement on a background worker, retain the current playback until
    /// success, then switch. Sampling still performs no asset I/O. The new
    /// playback starts paused at zero; callers may restore its clock with seek.
    pub fn open_scene_images_with_properties(
        source: &SceneSource,
        settings: PlaybackSettings,
        properties: &BTreeMap<String, serde_json::Value>,
    ) -> Result<Self, InspectionError> {
        Self::prepare(source, None, settings, properties)
    }

    fn prepare(
        source: &SceneSource,
        objects: Option<&[u64]>,
        settings: PlaybackSettings,
        properties: &BTreeMap<String, serde_json::Value>,
    ) -> Result<Self, InspectionError> {
        let decoder = TextureDecoder::open(source)?;
        let mut plan = snapshot::plan_with_properties(&decoder, objects, settings, properties)?;
        let mut indexed = BTreeMap::new();
        let mut sources = Vec::new();
        let mut layer_sources = Vec::new();
        let mut animated_assets = Vec::new();
        let mut static_bytes = 0;
        let mut static_pages = BTreeMap::new();
        let mut effects = Vec::new();
        let tolerant = objects.is_none() && settings.allow_unsupported_features;
        for layer in &mut plan.layers {
            // Include interpretation in the key: the same TEX cannot silently
            // become a static page for one layer and an animation for another.
            let key = (layer.asset.clone(), layer.sprite);
            let index = if let Some(index) = indexed.get(&key) {
                *index
            } else {
                let prepared = if tolerant
                    && let Some(file) = decoder.embedded_video(&layer.asset)?
                {
                    animated_assets.push(layer.asset.clone());
                    Source::Video(crate::assets::video::VideoPager::embedded(file)?)
                } else if layer.sprite {
                    let (pager, _events) = SpritePager::new(source.clone(), layer.asset.clone())?;
                    animated_assets.push(layer.asset.clone());
                    Source::Animated(pager)
                } else {
                    let page = snapshot::page(
                        &decoder,
                        &layer.asset,
                        0,
                        &mut static_pages,
                        &mut static_bytes,
                    )?;
                    let extent = page.extent();
                    Source::Static(Arc::new(TextureRegion::new(page, [0, 0], extent).map_err(
                        |error| InspectionError::invalid(&layer.asset, error.to_string()),
                    )?))
                };
                let index = sources.len();
                sources.push(prepared);
                indexed.insert(key, index);
                index
            };
            layer_sources.push(index);
            let mut prepared = Vec::new();
            let mut retained = Vec::new();
            for effect in layer.effects.drain(..) {
                match effect.prepare(&decoder, &mut static_pages, &mut static_bytes) {
                    Ok(inputs) => {
                        prepared.push(inputs);
                        retained.push(effect);
                    }
                    Err(error) if tolerant => plan.omissions.push(format!(
                        "{}: effect effects/{}/effect.json is omitted: {error}",
                        layer.asset,
                        effect.name()
                    )),
                    Err(error) => return Err(error),
                }
            }
            layer.effects = retained;
            effects.push(prepared);
        }

        let mut composition_effects = Vec::new();
        for point in &mut plan.compositions {
            let mut prepared = Vec::new();
            let mut retained = Vec::new();
            for effect in point.effects.drain(..) {
                match effect.prepare(&decoder, &mut static_pages, &mut static_bytes) {
                    Ok(inputs) => {
                        prepared.push(inputs);
                        retained.push(effect);
                    }
                    Err(error) if tolerant => plan.omissions.push(format!(
                        "Object {}: composition effect effects/{}/effect.json is omitted: {error}",
                        point.id,
                        effect.name()
                    )),
                    Err(error) => return Err(error),
                }
            }
            point.effects = retained;
            composition_effects.push(prepared);
        }
        let particles = optional_objects(
            &plan.particles,
            tolerant,
            &mut plan.omissions,
            |objects, omissions| {
                particles::prepare(
                    &decoder,
                    objects,
                    tolerant,
                    &mut static_pages,
                    &mut static_bytes,
                    omissions,
                )
            },
        )?;
        let mut particle_pages = if particles.is_empty() {
            Vec::new()
        } else {
            static_pages.values().cloned().collect()
        };
        let rays = optional_objects(
            &plan.ray_objects,
            tolerant,
            &mut plan.omissions,
            |objects, _| {
                super::rays::prepare(
                    &decoder,
                    objects,
                    plan.canvas,
                    &mut static_pages,
                    &mut static_bytes,
                )
            },
        )?;
        let texts = optional_objects(
            &plan.text_objects,
            tolerant,
            &mut plan.omissions,
            |objects, _| {
                super::text::prepare(
                    &decoder,
                    objects,
                    settings.visual_effects,
                    &mut static_bytes,
                )
            },
        )?;
        particle_pages.extend(texts.iter().map(|text| text.page()));
        let report = ImagePlaybackReport {
            settings,
            package_sha256: decoder.package_sha256().into(),
            object_ids: plan.object_ids.clone(),
            animated_assets,
            precise_blurs: plan
                .object_ids
                .iter()
                .zip(&plan.layers)
                .filter_map(|(id, layer)| {
                    layer.effects.iter().find_map(|effect| match effect {
                        snapshot::EffectDescription::Precise(blur) => Some((*id, blur.scale())),
                        _ => None,
                    })
                })
                .collect(),
            cloud_motions: plan
                .object_ids
                .iter()
                .zip(&plan.layers)
                .filter_map(|(id, layer)| {
                    layer.effects.iter().find_map(|effect| match effect {
                        snapshot::EffectDescription::Cloud(profile) => Some((*id, profile.clone())),
                        _ => None,
                    })
                })
                .collect(),
            particle_objects: particles.iter().map(|particles| particles.id).collect(),
            hdr_bloom: plan.hdr.is_some(),
            background_color: plan.background,
            water_flow_objects: plan
                .object_ids
                .iter()
                .zip(&plan.layers)
                .filter_map(|(id, l)| {
                    l.effects
                        .iter()
                        .any(|effect| matches!(effect, snapshot::EffectDescription::Flow(_)))
                        .then_some(*id)
                })
                .collect(),
            shape_objects: rays.iter().map(|r| r.id).collect(),
            image_motions: plan
                .object_ids
                .iter()
                .zip(&plan.layers)
                .filter_map(|(id, layer)| {
                    layer.effects.iter().find_map(|effect| match effect {
                        snapshot::EffectDescription::Motion(description) => {
                            Some((*id, description.profile.name().into()))
                        }
                        _ => None,
                    })
                })
                .collect(),
            effect_chains: plan
                .object_ids
                .iter()
                .zip(&plan.layers)
                .filter(|(_, layer)| !layer.effects.is_empty())
                .map(|(id, layer)| {
                    (
                        *id,
                        layer
                            .effects
                            .iter()
                            .map(|effect| effect.name().to_owned())
                            .collect(),
                    )
                })
                .collect(),
            composition_layers: plan
                .compositions
                .iter()
                .map(|point| {
                    (
                        point.id,
                        point
                            .effects
                            .iter()
                            .map(|effect| effect.name().to_owned())
                            .collect(),
                    )
                })
                .collect(),
            text_objects: texts.iter().map(|t| t.id).collect(),
            omissions: plan.omissions.clone(),
            user_properties: properties.clone(),
            resolved_bindings: plan.resolved_bindings.clone(),
        };
        Ok(Self {
            _decoder: Some(decoder),
            plan,
            layer_sources,
            sources,
            effects,
            composition_effects,
            particles,
            particle_pages,
            rays,
            texts,
            report,
            base: Duration::ZERO,
            started: None,
            frozen: None,
            pending_time: Mutex::new(None),
            calendar_time: None,
        })
    }

    /// Immutable asset identity, draw order and omissions from opening.
    pub fn report(&self) -> &ImagePlaybackReport {
        &self.report
    }

    /// Current common time, frozen while paused. Saturates at `Duration::MAX`.
    pub fn time(&self) -> Duration {
        self.base
            .saturating_add(self.started.map_or(Duration::ZERO, |start| start.elapsed()))
    }

    /// Whether the monotonic clock is advancing.
    pub fn playing(&self) -> bool {
        self.started.is_some()
    }

    /// Resume the common clock. Repeated play calls preserve the start instant.
    /// Has no effect when initialization settings disable animation.
    pub fn play(&mut self) {
        if !self.report.settings.animation {
            return;
        }
        self.frozen = None;
        if self.started.is_none() {
            self.started = Some(Instant::now());
        }
    }

    /// Freeze common time. Bounded outstanding sprite preparation may settle.
    pub fn pause(&mut self) {
        if self.started.is_some() {
            self.base = self.time();
            self.started = None;
            *self.pending_time.get_mut().expect("sample state") = None;
        }
    }

    /// Pause and select an exact time. A sample may be unavailable during seek.
    pub fn seek(&mut self, time: Duration) {
        *self.pending_time.get_mut().expect("sample state") = None;
        self.frozen = None;
        self.base = time;
        self.started = None;
    }

    /// Hold a sample from this playback, normally the last one the host presented.
    /// Freezes its exact time and retains its descriptors/pages without decoding,
    /// waiting for the renderer, or generating another sample. Resume with `play`
    /// or choose another still with `seek`. Retention is bounded to one sample.
    ///
    /// The host must stop scheduling renders and retain its completed [`crate::Frame`]
    /// (or copied host texture), ignoring in-flight results while frozen. This
    /// method cannot decide which GPU frame the host last displayed.
    pub fn freeze(&mut self, sample: &ImagePlaybackSample) {
        *self.pending_time.get_mut().expect("sample state") = None;
        self.base = sample.time;
        self.started = None;
        self.frozen = Some(sample.clone());
    }

    /// Sample every source at one captured clock time without I/O, decoding or
    /// waiting for metadata. Returns `None` when any source is unavailable; keep
    /// the previous completed GPU frame and retry. Retries retain one selection
    /// time until all sources are ready; the next selection uses the current
    /// clock. Seek, pause and freeze discard pending selections. Returns asset or
    /// composition errors. Construction copies descriptors, not image pixels.
    pub fn try_sample(&self) -> Result<Option<ImagePlaybackSample>, InspectionError> {
        if let Some(sample) = &self.frozen {
            return Ok(Some(sample.clone()));
        }
        let mut pending = match self.pending_time.try_lock() {
            Ok(pending) => pending,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(_)) => {
                return Err(InspectionError::invalid(
                    "image playback",
                    "sample state poisoned",
                ));
            }
        };
        // Advancing requests before decoding completes can starve live video forever.
        let time = *pending.get_or_insert_with(|| self.time());
        let mut regions = Vec::with_capacity(self.sources.len());
        let mut frame_indices = Vec::new();
        for source in &self.sources {
            regions.push(match source {
                Source::Static(region) => Some(region.clone()),
                Source::Animated(pager) => pager.try_sample_at(time)?.map(|sample| {
                    frame_indices.push(sample.frame_index());
                    sample.region().clone()
                }),
                Source::Video(pager) => pager.try_sample_at(time)?.map(|(region, index)| {
                    frame_indices.push(index);
                    region
                }),
            });
        }
        if regions.iter().any(Option::is_none) {
            return Ok(None);
        }
        let image_layers = self
            .plan
            .layers
            .iter()
            .zip(&self.layer_sources)
            .zip(&self.effects)
            .map(|((description, index), effects)| {
                description
                    .layer(
                        regions[*index].as_ref().expect("all sources ready").clone(),
                        effects,
                        time,
                    )
                    .map(|layer| layer.with_image_quality(self.report.settings.image_quality))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut by_id: BTreeMap<_, _> = self
            .plan
            .object_ids
            .iter()
            .copied()
            .zip(image_layers)
            .collect();
        let mut layers = Vec::new();
        let mut text_values = BTreeMap::new();
        let mut composition_points = Vec::new();
        let now = (!self.texts.is_empty()).then(|| {
            self.calendar_time
                .map_or_else(chrono::Local::now, chrono::DateTime::<chrono::Local>::from)
        });
        let (mut work, mut draws) = (0, 0);
        for id in &self.plan.draw_order {
            if let Some((point, effects)) = self
                .plan
                .compositions
                .iter()
                .zip(&self.composition_effects)
                .find(|(point, _)| point.id == *id)
            {
                composition_points.push(point.point(
                    layers.len(),
                    self.plan.coordinate_scale,
                    effects,
                    time,
                ));
            }
            if let Some(layer) = by_id.remove(id) {
                let mut layer = layer;
                layer.scale_coordinates(self.plan.coordinate_scale);
                layers.push(layer);
            }
            if let Some(rays) = self.rays.iter().find(|rays| rays.id == *id) {
                layers.push(rays.layer(time)?);
            }
            if let Some(text) = self.texts.iter().find(|text| text.id == *id) {
                let start = layers.len();
                let value = text.value(now.expect("text clock prepared"));
                text.append(&value, &mut layers)?;
                text_values.insert(text.id, value);
                for layer in &mut layers[start..] {
                    layer.scale_coordinates(self.plan.coordinate_scale);
                }
            }
            if let Some(particles) = self.particles.iter().find(|particles| particles.id == *id) {
                let start = layers.len();
                particles.append(time, &mut layers, &mut work, &mut draws)?;
                for layer in &mut layers[start..] {
                    layer.scale_coordinates(self.plan.coordinate_scale);
                }
            }
        }
        let mut composition = ImageComposition::scene(
            self.plan.canvas,
            layers,
            &self.particle_pages,
            composition_points,
        )
        .map_err(|error| InspectionError::invalid("image playback", error.to_string()))?;
        composition.hdr = self.plan.hdr;
        composition.background = self.plan.background;
        let composition = Arc::new(composition);
        *pending = None;
        Ok(Some(ImagePlaybackSample {
            composition,
            time,
            frame_indices,
            text_values,
        }))
    }

    /// Join sprite preparation and release owned sources. Active codecs may
    /// finish first. Perform outside latency-critical UI updates. Returned
    /// samples remain valid. A pager panic is reported; remaining sources join
    /// on Drop even when this returns an error.
    pub fn shutdown(self) -> Result<(), InspectionError> {
        for source in self.sources {
            if let Source::Animated(pager) = source {
                pager.shutdown()?;
            }
        }
        Ok(())
    }
}
