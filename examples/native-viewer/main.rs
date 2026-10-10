//! Native window consumer with line-delimited JSON control over stdin/stdout.

mod fixture;
mod images;
#[path = "../support/layers.rs"]
mod layers_checks;
#[path = "../support/motion.rs"]
mod motion_checks;
#[path = "../support/paging.rs"]
mod paging;
#[path = "../support/image_playback.rs"]
mod playback_checks;
mod presentation;
#[path = "../support/sprite.rs"]
mod sprite_checks;
#[path = "../support/mod.rs"]
mod support;
#[path = "../support/texture.rs"]
mod texture_checks;

use presentation::Presentation;
use scene_renderer::{
    DeviceMode, Frame, ImagePlayback, Renderer, SceneSource, Size, SpritePager, SpriteWorker,
    TextureDecoder, TexturePage, TextureRegion,
};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use support::Result;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

enum Event {
    FrameReady,
    Command(Value),
}

struct App {
    presentation: Option<Presentation>,
    window: Option<Window>,
    renderer: Option<Renderer>,
    frame: Option<Arc<Frame>>,
    expected_id: u64,
    pending_resize: Option<Value>,
    pending_fixture: Option<Value>,
    fixture: fixture::Fixture,
    ready: bool,
    output: PathBuf,
    mode: DeviceMode,
    proxy: winit::event_loop::EventLoopProxy<Event>,
    failure: Option<String>,
    presented: u64,
    texture_fixture: Option<(Arc<TexturePage>, texture_checks::Case)>,
    texture_region: Option<Arc<TextureRegion>>,
    layers_fixture: Option<layers_checks::Fixture>,
    images: Option<images::Images>,
}

fn emit(value: Value) {
    println!("{value}");
    let _ = io::stdout().flush();
}

impl App {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl std::fmt::Display) {
        self.failure = Some(error.to_string());
        emit(json!({"event": "failed", "error": error.to_string()}));
        event_loop.exit();
    }

    fn update(&mut self) -> Result<()> {
        let Some(renderer) = &self.renderer else {
            return Ok(());
        };
        let Some(frame) = renderer.try_frame()? else {
            return Ok(());
        };
        if frame.id() != self.expected_id {
            return Ok(());
        }
        if self.images.is_some() && frame.image_layer_count().is_none() {
            return Ok(());
        }
        let window = self.window.as_ref().ok_or("no window")?;
        let actual = window.inner_size();
        if actual.width != frame.size().width() || actual.height != frame.size().height() {
            return Ok(());
        }
        if self.presentation.is_none() {
            self.presentation = Some(Presentation::new(window, &frame)?);
        }
        if !self
            .presentation
            .as_mut()
            .ok_or("no presentation")?
            .draw(&frame)?
        {
            return Ok(());
        }
        if self
            .frame
            .as_ref()
            .is_none_or(|previous| previous.id() != frame.id())
        {
            self.fixture.presented(&frame);
            if let Some(images) = &mut self.images {
                images.presented(&frame);
            }
        }
        self.frame = Some(frame.clone());
        self.presented += 1;
        if !self.ready {
            self.ready = true;
            if let Some(images) = &mut self.images {
                images.play();
            }
            let state = self.inspect();
            support::write_json(&self.output.join("ready.json"), &state)?;
            emit(json!({"event": "ready", "state": state}));
        }
        if let Some(command) = self.pending_resize.take() {
            emit(json!({"id": command["id"], "ok": true, "state": self.inspect()}));
        }
        let selected_time_ready = self.pending_fixture.as_ref().is_none_or(|command| {
            command["command"] != "image_at"
                || self.images.as_ref().is_some_and(|images| {
                    images.presented_at(Duration::from_millis(
                        command["time_ms"].as_u64().expect("validated image time"),
                    ))
                })
        });
        if selected_time_ready && let Some(command) = self.pending_fixture.take() {
            emit(json!({"id":command["id"],"ok":true,"state":self.inspect()}));
        }
        Ok(())
    }

    fn inspect(&self) -> Value {
        let output_pool = match self.renderer.as_ref().map(Renderer::try_output_stats) {
            Some(Ok(Some(stats))) => support::pool_info(stats),
            Some(Err(error)) => json!({"error":error.to_string()}),
            Some(Ok(None)) | None => Value::Null,
        };
        json!({"pid": std::process::id(), "consumer": "native-viewer", "ready": self.ready,
            "frame": self.frame.as_ref().map(|frame| support::frame_info(frame)), "presented": self.presented,
            "output_pool":output_pool,
            "continuous_rendering": self.fixture.running() || self.images.as_ref().is_some_and(images::Images::playing), "output": self.output, "scene_support": false,"image_playback":self.images.as_ref().map(images::Images::inspect)})
    }

    fn resized(&mut self, size: PhysicalSize<u32>) -> Result<()> {
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let size = Size::new(size.width, size.height)?;
        let renderer = self.renderer.as_ref().ok_or("no renderer")?;
        if self
            .frame
            .as_ref()
            .is_some_and(|frame| frame.size() == size)
        {
            return Ok(());
        }
        self.expected_id = renderer.resize(size)?;
        if let Some(images) = &mut self.images {
            images.resized(self.expected_id);
        }
        Ok(())
    }

    fn command(&mut self, event_loop: &ActiveEventLoop, command: Value) -> Result<()> {
        match command["command"].as_str().ok_or("missing command")? {
            "inspect" => emit(json!({"id": command["id"], "ok": true, "state": self.inspect()})),
            "resize" => {
                if self.pending_resize.is_some() || self.pending_fixture.is_some() {
                    return Err("resize already pending".into());
                }
                let width = u32::try_from(command["width"].as_u64().ok_or("invalid width")?)?;
                let height = u32::try_from(command["height"].as_u64().ok_or("invalid height")?)?;
                Size::new(width, height)?;
                let window = self.window.as_ref().ok_or("no window")?;
                let requested = PhysicalSize::new(width, height);
                if window.inner_size() == requested {
                    emit(json!({"id": command["id"], "ok": true, "state": self.inspect()}));
                } else {
                    self.pending_resize = Some(command);
                    if let Some(size) = window.request_inner_size(requested) {
                        self.resized(size)?;
                    }
                }
            }
            "fixture_at" => {
                if self.images.is_some() {
                    return Err("image playback uses image_at".into());
                }
                if self.pending_resize.is_some() || self.pending_fixture.is_some() {
                    return Err("frame command already pending".into());
                }
                let time = Duration::from_millis(
                    command["time_ms"].as_u64().ok_or("invalid fixture time")?,
                );
                self.fixture.step(time);
                self.expected_id = self
                    .renderer
                    .as_ref()
                    .ok_or("no renderer")?
                    .render_fixture_at(time)?;
                self.fixture.requested(self.expected_id);
                self.pending_fixture = Some(command);
            }
            "play_fixture" => {
                if self.images.is_some() {
                    return Err("image playback uses play_images".into());
                }
                self.fixture.play();
                emit(json!({"id":command["id"],"ok":true,"state":self.inspect()}));
            }
            "pause_fixture" => {
                if self.images.is_some() {
                    return Err("image playback uses pause_images".into());
                }
                self.fixture.pause();
                emit(json!({"id":command["id"],"ok":true,"state":self.inspect()}));
            }
            "image_at" => {
                if self.pending_resize.is_some() || self.pending_fixture.is_some() {
                    return Err("frame command already pending".into());
                }
                let time =
                    Duration::from_millis(command["time_ms"].as_u64().ok_or("invalid image time")?);
                self.images.as_mut().ok_or("no image playback")?.seek(time);
                self.pending_fixture = Some(command);
            }
            "play_images" => {
                if self.pending_resize.is_some() || self.pending_fixture.is_some() {
                    return Err("frame command already pending".into());
                }
                self.images.as_mut().ok_or("no image playback")?.play();
                emit(json!({"id":command["id"],"ok":true,"state":self.inspect()}));
            }
            "pause_images" => {
                if self.pending_resize.is_some() || self.pending_fixture.is_some() {
                    return Err("frame command already pending".into());
                }
                self.images.as_mut().ok_or("no image playback")?.pause();
                emit(json!({"id":command["id"],"ok":true,"state":self.inspect()}));
            }
            "capture" => {
                let name = command["name"].as_str().ok_or("missing capture name")?;
                if !name.ends_with(".png")
                    || std::path::Path::new(name)
                        .file_name()
                        .and_then(|name| name.to_str())
                        != Some(name)
                {
                    return Err(
                        "capture name must be a PNG filename inside this run's output directory"
                            .into(),
                    );
                }
                let frame = self.frame.as_ref().ok_or("no completed frame")?;
                let pixels = self
                    .presentation
                    .as_mut()
                    .ok_or("no presentation")?
                    .capture(frame)?;
                support::write_png(&self.output.join(name), &pixels)?;
                let motion_check = if let Some(time) = frame.fixture_time() {
                    motion_checks::check_pixels(&pixels, u64::try_from(time.as_millis())?)?;
                    Some(true)
                } else {
                    None
                };
                let texture_check = if frame.texture_format().is_some() {
                    let (_, case) = self
                        .texture_fixture
                        .as_ref()
                        .ok_or("no texture fixture oracle")?;
                    let (origin, extent) = self
                        .texture_region
                        .as_ref()
                        .map_or(([0, 0], case.extent), |region| {
                            (region.origin(), region.extent())
                        });
                    if frame.texture_region() != Some((origin, extent)) {
                        return Err("native source crop differs".into());
                    }
                    let rgba = texture_checks::crop(&case.rgba, case.extent, origin, extent);
                    texture_checks::check(&pixels, extent, &rgba)?;
                    Some(true)
                } else {
                    None
                };
                let layers_check = if frame.image_layer_count().is_some() {
                    if let Some(images) = &self.images {
                        if images.fixture {
                            playback_checks::check(&pixels, images.capture_time_ms()?)?;
                            Some(true)
                        } else {
                            None
                        }
                    } else {
                        self.layers_fixture
                            .as_ref()
                            .ok_or("no layer fixture oracle")?
                            .check(&pixels)?;
                        Some(true)
                    }
                } else {
                    None
                };
                emit(
                    json!({"id": command["id"], "ok": true, "capture": name, "state": self.inspect(),
                        "readback": true, "uniform_rgba": support::uniform_rgba(&pixels),"motion_check":motion_check,"texture_check":texture_check,"layers_check":layers_check}),
                );
            }
            "stop" => {
                if let Some(images) = &self.images {
                    support::write_json(
                        &self.output.join("image-timeline.json"),
                        &images.report(),
                    )?;
                }
                support::write_json(
                    &self.output.join("motion-timeline.json"),
                    &self.fixture.report(),
                )?;
                support::write_json(&self.output.join("final.json"), &self.inspect())?;
                emit(json!({"id": command["id"], "ok": true, "stopping": true}));
                event_loop.exit();
            }
            other => return Err(format!("unsupported command: {other}").into()),
        }
        Ok(())
    }
}

impl ApplicationHandler<Event> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result: Result<()> = (|| {
            let window = event_loop.create_window(
                Window::default_attributes()
                    .with_title(if self.images.is_some() {
                        "Scene renderer | selected image playback | Esc to close"
                    } else {
                        "Scene renderer | texture and motion diagnostics | Esc to close"
                    })
                    .with_inner_size(PhysicalSize::new(640, 360)),
            )?;
            let size = window.inner_size();
            let (renderer, events) = Renderer::new(Size::new(size.width, size.height)?, self.mode)?;
            if let Some(images) = &mut self.images {
                if let Some(sample) = images.tick()? {
                    self.expected_id = renderer.render_images(sample.composition.clone())?;
                    images.requested(self.expected_id, &sample);
                }
            } else if let Some(layers) = &self.layers_fixture {
                self.expected_id = renderer.render_images(layers.composition.clone())?;
            } else if let Some(region) = &self.texture_region {
                self.expected_id = renderer.render_texture_region(region.clone())?;
            } else if let Some((page, _)) = &self.texture_fixture {
                self.expected_id = renderer.render_texture_page(page.clone())?;
            }
            let proxy = self.proxy.clone();
            std::thread::Builder::new()
                .name("viewer-render-events".into())
                .spawn(move || {
                    while events.recv().is_ok() {
                        if proxy.send_event(Event::FrameReady).is_err() {
                            break;
                        }
                    }
                })?;
            self.renderer = Some(renderer);
            self.window = Some(window);
            Ok(())
        })();
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Event) {
        match event {
            Event::FrameReady => {
                if let Err(error) = self.update() {
                    self.fail(event_loop, error)
                }
            }
            Event::Command(command) => {
                let id = command["id"].clone();
                if let Err(error) = self.command(event_loop, command) {
                    emit(json!({"id": id, "ok": false, "error": error.to_string()}));
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && event.logical_key == Key::Named(NamedKey::Escape) =>
            {
                event_loop.exit()
            }
            WindowEvent::Resized(size) => {
                if let Err(error) = self.resized(size) {
                    self.fail(event_loop, error)
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.update() {
                    self.fail(event_loop, error)
                }
            }
            _ => (),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.images.is_some() {
            let result: Result<()> = (|| {
                let waiting = self
                    .frame
                    .as_ref()
                    .is_some_and(|frame| frame.id() != self.expected_id);
                if !waiting
                    && self.pending_resize.is_none()
                    && let Some(images) = &mut self.images
                    && let Some(sample) = images.tick()?
                {
                    self.expected_id = self
                        .renderer
                        .as_ref()
                        .ok_or("no renderer")?
                        .render_images(sample.composition.clone())?;
                    images.requested(self.expected_id, &sample);
                }
                if self
                    .frame
                    .as_ref()
                    .is_none_or(|frame| frame.id() != self.expected_id)
                {
                    self.update()?;
                }
                let pending = self
                    .frame
                    .as_ref()
                    .is_none_or(|frame| frame.id() != self.expected_id);
                let deadline = if pending {
                    Some(Instant::now() + Duration::from_millis(10))
                } else {
                    self.images.as_ref().and_then(images::Images::deadline)
                };
                event_loop
                    .set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
                Ok(())
            })();
            if let Err(error) = result {
                self.fail(event_loop, error);
            }
            return;
        }
        let waiting = self
            .frame
            .as_ref()
            .is_none_or(|frame| frame.id() != self.expected_id);
        if !waiting
            && self.pending_resize.is_none()
            && self.pending_fixture.is_none()
            && let Some(time) = self.fixture.tick()
        {
            let requested: Result<u64> = (|| {
                Ok(self
                    .renderer
                    .as_ref()
                    .ok_or("no renderer")?
                    .render_fixture_at(time)?)
            })();
            match requested {
                Ok(id) => {
                    self.expected_id = id;
                    self.fixture.requested(id);
                }
                Err(error) => self.fail(event_loop, error),
            }
        }
        let pending = self
            .frame
            .as_ref()
            .is_none_or(|frame| frame.id() != self.expected_id);
        if pending {
            if let Err(error) = self.update() {
                self.fail(event_loop, error);
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(10),
            ));
        } else if let Some(deadline) = self.fixture.deadline() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(images) = &self.images
            && let Err(error) =
                support::write_json(&self.output.join("image-timeline.json"), &images.report())
        {
            self.failure = Some(error.to_string());
        }
        if let Err(error) = support::write_json(
            &self.output.join("motion-timeline.json"),
            &self.fixture.report(),
        ) {
            self.failure = Some(error.to_string());
        }
        self.presentation = None;
        self.window = None;
        if let Some(renderer) = self.renderer.take()
            && let Err(error) = renderer.shutdown()
        {
            self.failure = Some(error.to_string());
        }
        if let Some(images) = self.images.take()
            && let Err(error) = images.shutdown()
        {
            self.failure = Some(error.to_string());
        }
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut common = Vec::new();
    let mut texture = false;
    let mut region = false;
    let mut layers = false;
    let mut extracted = false;
    let mut paged = false;
    let mut image_fixture = false;
    let mut scene_path = None;
    let mut engine_assets = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--image-fixture" => image_fixture = true,
            "--scene" => {
                scene_path = Some(PathBuf::from(args.next().ok_or("--scene needs a package")?))
            }
            "--engine-assets" => {
                engine_assets = Some(PathBuf::from(
                    args.next().ok_or("--engine-assets needs a directory")?,
                ))
            }
            "--texture-fixture" => texture = true,
            "--extracted-fixture" => {
                texture = true;
                extracted = true;
            }
            "--paged-fixture" => {
                texture = true;
                paged = true;
            }
            "--layers-fixture" => layers = true,
            "--region-fixture" => {
                texture = true;
                region = true;
            }
            "--output" => {
                common.push(arg);
                common.push(args.next().ok_or("--output needs a directory")?);
            }
            _ => common.push(arg),
        }
    }
    if layers && texture {
        return Err("choose either layers or texture fixture".into());
    }
    if scene_path.is_some() != engine_assets.is_some() {
        return Err("--scene and --engine-assets must be supplied together".into());
    }
    if (image_fixture || scene_path.is_some()) && (layers || texture)
        || image_fixture && scene_path.is_some()
    {
        return Err("choose one image playback or diagnostic fixture route".into());
    }
    let (output, mode) = support::options(common.into_iter())?;
    let images = if image_fixture || scene_path.is_some() {
        let (source, objects) = if image_fixture {
            let generated = output.join("generated");
            std::fs::create_dir(&generated)?;
            std::fs::create_dir(generated.join("engine"))?;
            std::fs::write(generated.join("scene.pkg"), playback_checks::package())?;
            (
                SceneSource {
                    package: generated.join("scene.pkg"),
                    engine_assets: generated.join("engine"),
                },
                playback_checks::OBJECTS.to_vec(),
            )
        } else {
            (
                SceneSource {
                    package: scene_path.expect("paired source"),
                    engine_assets: engine_assets.expect("paired source"),
                },
                vec![28, 32, 72, 89, 77, 86],
            )
        };
        let playback = ImagePlayback::open(&source, &objects)?;
        support::write_json(
            &output.join("image-playback.json"),
            &serde_json::to_value(playback.report())?,
        )?;
        Some(images::Images::new(playback, image_fixture))
    } else {
        None
    };
    // Diagnostic input is prepared before a window/event loop exists. Host
    // controls only enqueue GPU work; file I/O and decode never run in update.
    let (texture_fixture, texture_region) = if texture {
        let mut case = texture_checks::cases()
            .into_iter()
            .find(|case| case.name == "rgba-png-lz4")
            .expect("PNG/LZ4 fixture exists");
        let generated = output.join("generated");
        std::fs::create_dir(&generated)?;
        std::fs::create_dir(generated.join("engine"))?;
        std::fs::write(
            generated.join("scene.pkg"),
            texture_checks::package(&[(case.name, &case.tex)]),
        )?;
        let source = SceneSource {
            package: generated.join("scene.pkg"),
            engine_assets: generated.join("engine"),
        };
        let (page, crop) = if extracted || paged {
            let sample = if paged {
                let (pager, events) = SpritePager::new(source, case.name.into())?;
                let sample = paging::sample(&pager, &events, Duration::ZERO)?;
                let stats = paging::settled(&pager, &events, 1)?;
                support::write_json(&output.join("sprite-prefetch.json"), &paging::stats(stats))?;
                pager.shutdown()?;
                sample
            } else {
                let (worker, events) = SpriteWorker::new(source, case.name.into())?;
                let id = sprite_checks::request(&worker, Duration::ZERO)?;
                let (result, metrics) = sprite_checks::wait(&worker, &events, id)?;
                support::write_json(&output.join("sprite-preparation.json"), &metrics)?;
                let sample = result.into_sample()?;
                worker.shutdown()?;
                sample
            };
            let authored = sample.authored_frame();
            case.rgba = texture_checks::crop(
                &case.rgba,
                case.extent,
                authored.origin(),
                authored.extent(),
            );
            case.extent = authored.extent();
            (
                sample.region().page().clone(),
                Some(sample.region().clone()),
            )
        } else {
            let decoder = TextureDecoder::open(&source)?;
            let page = Arc::new(decoder.decode_page(case.name, 0)?);
            let crop = if region {
                let metadata = decoder.sprite_frames(case.name)?[0];
                Some(Arc::new(TextureRegion::new(
                    page.clone(),
                    metadata.origin(),
                    metadata.extent(),
                )?))
            } else {
                None
            };
            (page, crop)
        };
        (Some((page, case)), crop)
    } else {
        (None, None)
    };
    let layers_fixture = if layers {
        let generated = output.join("generated");
        std::fs::create_dir(&generated)?;
        std::fs::create_dir(generated.join("engine"))?;
        let cases = texture_checks::cases();
        let entries: Vec<_> = cases
            .iter()
            .map(|case| (case.name, case.tex.as_slice()))
            .collect();
        std::fs::write(
            generated.join("scene.pkg"),
            texture_checks::package(&entries),
        )?;
        let decoder = TextureDecoder::open(&SceneSource {
            package: generated.join("scene.pkg"),
            engine_assets: generated.join("engine"),
        })?;
        Some(layers_checks::Fixture::new(&decoder)?)
    } else {
        None
    };
    let event_loop = EventLoop::<Event>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let command_proxy = proxy.clone();
    std::thread::Builder::new()
        .name("viewer-commands".into())
        .spawn(move || {
            for line in io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                match serde_json::from_str(&line) {
                    Ok(command) => {
                        if command_proxy.send_event(Event::Command(command)).is_err() {
                            break;
                        }
                    }
                    Err(error) => emit(json!({"ok": false, "error": error.to_string()})),
                }
            }
        })?;
    let mut app = App {
        presentation: None,
        window: None,
        renderer: None,
        frame: None,
        expected_id: 1,
        pending_resize: None,
        pending_fixture: None,
        fixture: fixture::Fixture::new(),
        ready: false,
        output,
        mode,
        proxy,
        failure: None,
        presented: 0,
        texture_fixture,
        texture_region,
        layers_fixture,
        images,
    };
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.failure {
        return Err(error.into());
    }
    Ok(())
}
