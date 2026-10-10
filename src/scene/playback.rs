use super::{ImageComposition, InspectionError, SceneSource, snapshot};
use crate::{SpritePager, TextureDecoder, TextureRegion};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

/// Identity and explicit omissions of selected-image playback.
#[derive(Debug, Serialize)]
pub struct ImagePlaybackReport {
    /// Hash of the package used to prepare image descriptions and static assets.
    pub package_sha256: String,
    /// Selected visible images in authored draw order.
    pub object_ids: Vec<u64>,
    /// Unique animated assets, in the order used by sample frame indices.
    pub animated_assets: Vec<String>,
    /// Unsupported and unselected behavior. This is not full scene support.
    pub omissions: Vec<String>,
}

/// One complete selection at a common playback time.
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
/// Effects, particles, parallax, camera/color settings and nonzero crop offsets
/// are unsupported, as reported or rejected by [`Self::open`]. Existing per-
/// pager and composition bounds apply; aggregate scene memory is not budgeted.
/// Opening and joined shutdown belong outside latency-critical UI updates.
pub struct ImagePlayback {
    // Keep the package read guard across asynchronous pager opening so all
    // sources use the reported package version, including static-only loads.
    _decoder: TextureDecoder,
    plan: snapshot::ImagePlan,
    layer_sources: Vec<usize>,
    sources: Vec<Source>,
    report: ImagePlaybackReport,
    base: Duration,
    started: Option<Instant>,
}

impl ImagePlayback {
    /// Blocking preparation of selected image descriptions and static sources.
    /// Starts unique sprite pagers in the background. Open on an asset thread
    /// or before creating the host window, never during UI updates. Playback
    /// starts paused at zero; initial animated samples may still be unavailable.
    /// Uses the same checked selected-image profile as the snapshot API.
    pub fn open(source: &SceneSource, objects: &[u64]) -> Result<Self, InspectionError> {
        let decoder = TextureDecoder::open(source)?;
        let plan = snapshot::plan(&decoder, objects)?;
        let mut indexed = BTreeMap::new();
        let mut sources = Vec::new();
        let mut layer_sources = Vec::new();
        let mut animated_assets = Vec::new();
        let mut static_bytes = 0;
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
                    let page = Arc::new(decoder.decode_page_admitted(
                        &layer.asset,
                        0,
                        super::layers::MAX_SOURCE_BYTES - static_bytes,
                    )?);
                    static_bytes += page.data().len() as u64;
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
        }
        let report = ImagePlaybackReport {
            package_sha256: decoder.package_sha256().into(),
            object_ids: plan.object_ids.clone(),
            animated_assets,
            omissions: plan.omissions.clone(),
        };
        Ok(Self {
            _decoder: decoder,
            plan,
            layer_sources,
            sources,
            report,
            base: Duration::ZERO,
            started: None,
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
    pub fn play(&mut self) {
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
        self.base = time;
        self.started = None;
    }

    /// Sample every source at one captured clock time without I/O, decoding or
    /// waiting for metadata. Returns `None` when any source is unavailable; keep
    /// the previous completed GPU frame and retry. Returns contextual asset or
    /// composition errors. Construction copies descriptors, not image pixels.
    pub fn try_sample(&self) -> Result<Option<ImagePlaybackSample>, InspectionError> {
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
        let layers = self
            .plan
            .layers
            .iter()
            .zip(&self.layer_sources)
            .map(|(description, index)| {
                description.layer(regions[*index].as_ref().expect("all sources ready").clone())
            })
            .collect::<Result<_, _>>()?;
        let composition = Arc::new(
            ImageComposition::new(self.plan.canvas, layers)
                .map_err(|error| InspectionError::invalid("image playback", error.to_string()))?,
        );
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
