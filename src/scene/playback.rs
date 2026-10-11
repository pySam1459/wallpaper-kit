use super::{
    CloudMotion, CloudMotionReport, ImageComposition, InspectionError, PlaybackSettings,
    SceneSource, particles, snapshot,
};
use crate::{SpritePager, TextureDecoder, TexturePage, TextureRegion};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

/// Identity and explicit omissions of image playback.
#[derive(Debug, Serialize)]
pub struct ImagePlaybackReport {
    /// Consumer policy used to prepare this playback.
    pub settings: PlaybackSettings,
    /// Hash of the package used to prepare image descriptions and static assets.
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
    /// Image objects with the checked two-phase water-flow profile.
    pub water_flow_objects: Vec<u64>,
    /// Standalone corner light-shaft objects prepared in authored draw order.
    pub shape_objects: Vec<u64>,
    /// Prepared static or recognized calendar/clock text objects, in authored order.
    pub text_objects: Vec<u64>,
    /// Unsupported and unselected behavior. This is not full scene support.
    pub omissions: Vec<String>,
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
}

enum Source {
    Static(Arc<TextureRegion>),
    Animated(SpritePager),
}

/// Prepared selected-image composition with a shared monotonic playback clock.
///
/// Static sources decode once; duplicate animated references share one pager.
/// The host schedules sampling/presentation from its event loop. Sampling never
/// waits for codecs and returns `None` until every source for that time is ready.
/// Default unmasked precise blur, cloud motion and a bounded 2D particle profile
/// are supported by scene discovery. Other effects,
/// parallax, camera/color settings and animated crop offsets are unsupported,
/// as reported or rejected by [`Self::open`]. Existing per-
/// pager and composition bounds apply; aggregate scene memory is not budgeted.
/// Opening and joined shutdown belong outside latency-critical UI updates.
pub struct ImagePlayback {
    // Keep the package read guard across asynchronous pager opening so all
    // sources use the reported package version, including static-only loads.
    _decoder: TextureDecoder,
    plan: snapshot::ImagePlan,
    layer_sources: Vec<usize>,
    sources: Vec<Source>,
    cloud_motions: Vec<Option<Arc<CloudMotion>>>,
    flow_motions: Vec<Option<Arc<super::flow::FlowMotion>>>,
    particles: Vec<particles::Particles>,
    particle_pages: Vec<Arc<TexturePage>>,
    rays: Vec<super::rays::RayLayer>,
    texts: Vec<super::text::TextLayer>,
    report: ImagePlaybackReport,
    base: Duration,
    started: Option<Instant>,
    frozen: Option<ImagePlaybackSample>,
}

impl ImagePlayback {
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
        Self::prepare(source, Some(objects), settings)
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
        Self::prepare(source, None, settings)
    }

    fn prepare(
        source: &SceneSource,
        objects: Option<&[u64]>,
        settings: PlaybackSettings,
    ) -> Result<Self, InspectionError> {
        let decoder = TextureDecoder::open(source)?;
        let mut plan = snapshot::plan(&decoder, objects, settings)?;
        let mut indexed = BTreeMap::new();
        let mut sources = Vec::new();
        let mut layer_sources = Vec::new();
        let mut animated_assets = Vec::new();
        let mut static_bytes = 0;
        let mut static_pages = BTreeMap::new();
        let mut cloud_motions = Vec::new();
        let mut flow_motions = Vec::new();
        for layer in &plan.layers {
            // Include interpretation in the key: the same TEX cannot silently
            // become a static page for one layer and an animation for another.
            let key = (layer.asset.clone(), layer.sprite);
            let index = if let Some(index) = indexed.get(&key) {
                *index
            } else {
                let prepared = if layer.sprite {
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
            cloud_motions.push(
                layer
                    .cloud_profile()
                    .map(|profile| profile.prepare(&decoder, &mut static_pages, &mut static_bytes))
                    .transpose()?,
            );
            flow_motions.push(match &layer.effect {
                Some(snapshot::EffectDescription::Flow(profile)) => {
                    Some(profile.prepare(&decoder, &mut static_pages, &mut static_bytes)?)
                }
                _ => None,
            });
        }
        let particles = particles::prepare(
            &decoder,
            &plan.particles,
            &mut static_pages,
            &mut static_bytes,
            &mut plan.omissions,
        )?;
        let mut particle_pages = if particles.is_empty() {
            Vec::new()
        } else {
            static_pages.values().cloned().collect()
        };
        let rays = super::rays::prepare(
            &decoder,
            &plan.ray_objects,
            plan.canvas,
            &mut static_pages,
            &mut static_bytes,
        )?;
        let texts = super::text::prepare(
            &decoder,
            &plan.text_objects,
            settings.visual_effects,
            &mut static_bytes,
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
                .filter_map(|(id, layer)| match layer.effect {
                    Some(snapshot::EffectDescription::Precise(blur)) => Some((*id, blur.scale())),
                    _ => None,
                })
                .collect(),
            cloud_motions: plan
                .object_ids
                .iter()
                .zip(&plan.layers)
                .filter_map(|(id, layer)| {
                    layer.cloud_profile().map(|profile| (*id, profile.clone()))
                })
                .collect(),
            particle_objects: particles.iter().map(|particles| particles.id).collect(),
            hdr_bloom: plan.hdr.is_some(),
            water_flow_objects: plan
                .object_ids
                .iter()
                .zip(&plan.layers)
                .filter_map(|(id, l)| {
                    matches!(l.effect, Some(snapshot::EffectDescription::Flow(_))).then_some(*id)
                })
                .collect(),
            shape_objects: rays.iter().map(|r| r.id).collect(),
            text_objects: texts.iter().map(|t| t.id).collect(),
            omissions: plan.omissions.clone(),
        };
        Ok(Self {
            _decoder: decoder,
            plan,
            layer_sources,
            sources,
            cloud_motions,
            flow_motions,
            particles,
            particle_pages,
            rays,
            texts,
            report,
            base: Duration::ZERO,
            started: None,
            frozen: None,
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
        self.base = self.time();
        self.started = None;
    }

    /// Pause and select an exact time. A sample may be unavailable during seek.
    pub fn seek(&mut self, time: Duration) {
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
        self.base = sample.time;
        self.started = None;
        self.frozen = Some(sample.clone());
    }

    /// Sample every source at one captured clock time without I/O, decoding or
    /// waiting for metadata. Returns `None` when any source is unavailable; keep
    /// the previous completed GPU frame and retry. Returns contextual asset or
    /// composition errors. Construction copies descriptors, not image pixels.
    pub fn try_sample(&self) -> Result<Option<ImagePlaybackSample>, InspectionError> {
        if let Some(sample) = &self.frozen {
            return Ok(Some(sample.clone()));
        }
        let time = self.time();
        let mut regions = Vec::with_capacity(self.sources.len());
        let mut frame_indices = Vec::new();
        for source in &self.sources {
            regions.push(match source {
                Source::Static(region) => Some(region.clone()),
                Source::Animated(pager) => pager.try_sample_at(time)?.map(|sample| {
                    frame_indices.push(sample.frame_index());
                    sample.region().clone()
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
            .zip(&self.cloud_motions)
            .zip(&self.flow_motions)
            .map(|(((description, index), cloud), flow)| {
                description
                    .layer(
                        regions[*index].as_ref().expect("all sources ready").clone(),
                        cloud.as_ref(),
                        time,
                    )
                    .map(|layer| {
                        let layer = if let Some(flow) = flow {
                            layer.with_flow(flow.clone(), time)
                        } else {
                            layer
                        };
                        layer.with_image_quality(self.report.settings.image_quality)
                    })
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
        let now = chrono::Local::now();
        let (mut work, mut draws) = (0, 0);
        for id in &self.plan.draw_order {
            if let Some(layer) = by_id.remove(id) {
                layers.push(layer);
            }
            if let Some(rays) = self.rays.iter().find(|rays| rays.id == *id) {
                layers.push(rays.layer(time)?);
            }
            if let Some(text) = self.texts.iter().find(|text| text.id == *id) {
                text.append(now, &mut layers)?;
            }
            if let Some(particles) = self.particles.iter().find(|particles| particles.id == *id) {
                particles.append(time, &mut layers, &mut work, &mut draws)?;
            }
        }
        let mut composition =
            ImageComposition::scene(self.plan.canvas, layers, &self.particle_pages)
                .map_err(|error| InspectionError::invalid("image playback", error.to_string()))?;
        composition.hdr = self.plan.hdr;
        let composition = Arc::new(composition);
        Ok(Some(ImagePlaybackSample {
            composition,
            time,
            frame_indices,
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
