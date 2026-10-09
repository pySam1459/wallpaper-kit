//! Native window consumer with line-delimited JSON control over stdin/stdout.

mod presentation;
#[path = "../support/mod.rs"]
mod support;

use presentation::Presentation;
use scene_renderer::{DeviceMode, Frame, Renderer, Size};
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
    ready: bool,
    output: PathBuf,
    mode: DeviceMode,
    proxy: winit::event_loop::EventLoopProxy<Event>,
    failure: Option<String>,
    presented: u64,
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
        Ok(())
    }

    fn inspect(&self) -> Value {
        json!({"pid": std::process::id(), "consumer": "native-viewer", "ready": self.ready,
            "frame": self.frame.as_ref().map(|frame| support::frame_info(frame)), "presented": self.presented,
            "continuous_rendering": false, "output": self.output, "scene_support": false})
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
                if self.pending_resize.is_some() {
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
                emit(
                    json!({"id": command["id"], "ok": true, "capture": name, "state": self.inspect(),
                        "readback": true, "uniform_rgba": support::uniform_rgba(&pixels)}),
                );
            }
            "stop" => {
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
                    .with_title(
                        "Scene renderer | blank GPU background | resize to test | Esc to close",
                    )
                    .with_inner_size(PhysicalSize::new(640, 360)),
            )?;
            let size = window.inner_size();
            let (renderer, events) = Renderer::new(Size::new(size.width, size.height)?, self.mode)?;
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
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
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
    let (output, mode) = support::options()?;
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
        ready: false,
        output,
        mode,
        proxy,
        failure: None,
        presented: 0,
    };
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.failure {
        return Err(error.into());
    }
    Ok(())
}
