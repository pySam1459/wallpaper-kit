//! Native window consumer with line-delimited JSON control over stdin/stdout.

mod fixture;
#[path = "../support/motion.rs"]
mod motion_checks;
mod presentation;
#[path = "../support/mod.rs"]
mod support;
#[path = "../support/texture.rs"]
mod texture_checks;

use presentation::Presentation;
use scene_renderer::{DeviceMode, Frame, Renderer, SceneSource, Size, TextureDecoder, TexturePage};
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
        }
        self.frame = Some(frame.clone());
        self.presented += 1;
        if !self.ready {
            self.ready = true;
            let state = self.inspect();
            support::write_json(&self.output.join("ready.json"), &state)?;
            emit(json!({"event": "ready", "state": state}));
        }
        if let Some(command) = self.pending_resize.take() {
            emit(json!({"id": command["id"], "ok": true, "state": self.inspect()}));
        }
        if let Some(command) = self.pending_fixture.take() {
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
            "continuous_rendering": self.fixture.running(), "output": self.output, "scene_support": false})
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
                self.fixture.play();
                emit(json!({"id":command["id"],"ok":true,"state":self.inspect()}));
            }
            "pause_fixture" => {
                self.fixture.pause();
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
                    texture_checks::check(&pixels, case.extent, &case.rgba)?;
                    Some(true)
                } else {
                    None
                };
                emit(
                    json!({"id": command["id"], "ok": true, "capture": name, "state": self.inspect(),
                        "readback": true, "uniform_rgba": support::uniform_rgba(&pixels),"motion_check":motion_check,"texture_check":texture_check}),
                );
            }
            "stop" => {
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
                    .with_title("Scene renderer | texture and motion diagnostics | Esc to close")
                    .with_inner_size(PhysicalSize::new(640, 360)),
            )?;
            let size = window.inner_size();
            let (renderer, events) = Renderer::new(Size::new(size.width, size.height)?, self.mode)?;
            if let Some((page, _)) = &self.texture_fixture {
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
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut common = Vec::new();
    let mut texture = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--texture-fixture" => texture = true,
            "--output" => {
                common.push(arg);
                common.push(args.next().ok_or("--output needs a directory")?);
            }
            _ => common.push(arg),
        }
    }
    let (output, mode) = support::options(common.into_iter())?;
    // Diagnostic input is prepared before a window/event loop exists. Host
    // controls only enqueue GPU work; file I/O and decode never run in update.
    let texture_fixture = if texture {
        let case = texture_checks::cases()
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
        let decoder = TextureDecoder::open(&SceneSource {
            package: generated.join("scene.pkg"),
            engine_assets: generated.join("engine"),
        })?;
        Some((Arc::new(decoder.decode_page(case.name, 0)?), case))
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
    };
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.failure {
        return Err(error.into());
    }
    Ok(())
}
