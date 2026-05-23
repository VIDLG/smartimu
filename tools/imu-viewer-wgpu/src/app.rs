use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Context, ViewportId};
use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor};
use egui_winit::State as EguiWinitState;
use pollster::block_on;
use wgpu::{
    CommandEncoderDescriptor, CompositeAlphaMode, Device, DeviceDescriptor, Features, Instance,
    InstanceDescriptor, Limits, LoadOp, Operations, PowerPreference, PresentMode, Queue,
    RenderPassColorAttachment, RenderPassDescriptor, RequestAdapterOptions, StoreOp, Surface,
    SurfaceConfiguration, TextureUsages, TextureViewDescriptor,
};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::render::{Renderer, SceneStats};
use crate::replay::{ReplayClock, find_default_replay_path, load_replay_frames};
use crate::serial::{InputMode, SerialConnection, SerialEvent, available_ports, connect};
use crate::state::{PlaybackState, ViewerState};
use crate::ui::{UiAction, UiStatus};

const REDRAW_INTERVAL: Duration = Duration::from_millis(8);

pub fn run() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("event loop failed");
}

struct GpuHost<'w> {
    surface: Surface<'w>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    scene: Renderer,
    egui_renderer: EguiRenderer,
}

impl<'w> GpuHost<'w> {
    async fn new(window: &'w Window) -> Self {
        let size = window.inner_size();
        let instance = Instance::new(&InstanceDescriptor::default());
        let surface = instance
            .create_surface(window)
            .expect("failed to create surface");
        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("failed to find adapter");
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("imu-viewer-wgpu-device"),
                required_features: Features::empty(),
                required_limits: Limits::default(),
                experimental_features: Default::default(),
                memory_hints: Default::default(),
                trace: Default::default(),
            })
            .await
            .expect("failed to create device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: PresentMode::AutoNoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps
                .alpha_modes
                .first()
                .copied()
                .unwrap_or(CompositeAlphaMode::Auto),
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let scene = Renderer::new(&device, format, size);
        let egui_renderer = EguiRenderer::new(&device, format, RendererOptions::default());
        Self {
            surface,
            device,
            queue,
            config,
            scene,
            egui_renderer,
        }
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.config.width = size.width.max(1);
        self.config.height = size.height.max(1);
        self.surface.configure(&self.device, &self.config);
        self.scene.resize(&self.device, &self.queue, size);
    }

    fn render(
        &mut self,
        window: &Window,
        egui_ctx: &Context,
        full_output: egui::FullOutput,
        state: &ViewerState,
    ) -> SceneStats {
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(_) => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    Ok(frame) => frame,
                    Err(_) => return SceneStats { instance_count: 0 },
                }
            }
        };
        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("imu-viewer-wgpu-encoder"),
            });

        let stats = self
            .scene
            .render(&self.device, &self.queue, &mut encoder, &view, state);

        let pixels_per_point = egui_winit::pixels_per_point(egui_ctx, window);
        let paint_jobs = egui_ctx.tessellate(full_output.shapes, pixels_per_point);
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point,
        };
        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }
        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );
        {
            let pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("imu-viewer-wgpu-egui-pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.egui_renderer
                .render(&mut pass.forget_lifetime(), &paint_jobs, &screen_descriptor);
        }
        self.queue.submit([encoder.finish()]);
        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
        frame.present();
        stats
    }
}

struct FrameStats {
    last_frame: Instant,
    frame_time: Duration,
    fps: f32,
    instance_count: usize,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self {
            last_frame: Instant::now(),
            frame_time: REDRAW_INTERVAL,
            fps: 0.0,
            instance_count: 0,
        }
    }
}

struct App {
    window: Option<&'static Window>,
    window_id: Option<WindowId>,
    gpu: Option<GpuHost<'static>>,
    egui_ctx: Context,
    egui_state: Option<EguiWinitState>,
    state: ViewerState,
    playback_state: PlaybackState,
    replay_frames: Vec<smartimu::DeviceFrame>,
    replay_cursor: usize,
    replay_clock: Option<ReplayClock>,
    replay_path: Option<PathBuf>,
    ports: Vec<String>,
    selected_port: usize,
    baud_rate: u32,
    input_mode: InputMode,
    serial: Option<SerialConnection>,
    serial_log: VecDeque<String>,
    status: String,
    frames: FrameStats,
    shutting_down: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            window: None,
            window_id: None,
            gpu: None,
            egui_ctx: Context::default(),
            egui_state: None,
            state: ViewerState::default(),
            playback_state: PlaybackState::Playing,
            replay_frames: Vec::new(),
            replay_cursor: 0,
            replay_clock: None,
            replay_path: None,
            ports: available_ports(),
            selected_port: 0,
            baud_rate: 115_200,
            input_mode: InputMode::Auto,
            serial: None,
            serial_log: VecDeque::new(),
            status: String::from("starting"),
            frames: FrameStats::default(),
            shutting_down: false,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title("imu-viewer-wgpu")
            .with_inner_size(PhysicalSize::new(1440, 960));
        let window = event_loop
            .create_window(attrs)
            .expect("failed to create window");
        let leaked_window: &'static Window = Box::leak(Box::new(window));
        self.window_id = Some(leaked_window.id());
        self.egui_state = Some(EguiWinitState::new(
            self.egui_ctx.clone(),
            ViewportId::ROOT,
            event_loop,
            Some(leaked_window.scale_factor() as f32),
            leaked_window.theme(),
            None,
        ));
        self.gpu = Some(block_on(GpuHost::new(leaked_window)));
        self.window = Some(leaked_window);
        self.reload_replay();
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + REDRAW_INTERVAL));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window else {
            return;
        };
        let consumed = self
            .egui_state
            .as_mut()
            .map(|state| state.on_window_event(window, &event).consumed)
            .unwrap_or(false);

        match event {
            WindowEvent::CloseRequested => {
                self.shutting_down = true;
                self.gpu = None;
                self.window = None;
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size);
                }
            }
            WindowEvent::KeyboardInput { event, .. } if !consumed => {
                if event.state == ElementState::Pressed {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Space) => self.toggle_playback(),
                        PhysicalKey::Code(KeyCode::Tab) => self.state.select_next_imu(),
                        PhysicalKey::Code(KeyCode::KeyV) => self.state.toggle_view_mode(),
                        PhysicalKey::Code(KeyCode::KeyR) => self.reload_replay(),
                        PhysicalKey::Code(KeyCode::ArrowRight) => self.step_one_frame(),
                        PhysicalKey::Code(KeyCode::ArrowLeft) => self.step_back(),
                        PhysicalKey::Code(KeyCode::Home) => self.restart_replay(),
                        _ => {}
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.draw_frame();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.shutting_down {
            return;
        }
        if self.playback_state == PlaybackState::Playing {
            self.step_replay();
        }
        self.poll_serial();
        self.state.update_interpolated_orientations();
        if let Some(window) = self.window {
            window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + REDRAW_INTERVAL));
    }
}

impl App {
    fn draw_frame(&mut self) {
        let Some(window) = self.window else {
            return;
        };

        let now = Instant::now();
        self.frames.frame_time = now.saturating_duration_since(self.frames.last_frame);
        self.frames.last_frame = now;
        let dt = self.frames.frame_time.as_secs_f32();
        if dt > 0.0 {
            let current_fps = 1.0 / dt;
            self.frames.fps = if self.frames.fps == 0.0 {
                current_fps
            } else {
                self.frames.fps * 0.9 + current_fps * 0.1
            };
        }

        let raw_input = match self.egui_state.as_mut() {
            Some(egui_state) => egui_state.take_egui_input(window),
            None => return,
        };
        let mut action = UiAction::None;
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            action = crate::ui::show(
                ctx,
                &mut self.state,
                UiStatus {
                    playback_state: self.playback_state,
                    replay_path: self.replay_path.as_deref(),
                    replay_cursor: self.replay_cursor,
                    replay_len: self.replay_frames.len(),
                    status: &self.status,
                    fps: self.frames.fps,
                    frame_time: self.frames.frame_time,
                    instance_count: self.frames.instance_count,
                    ports: &self.ports,
                    selected_port: &mut self.selected_port,
                    baud_rate: &mut self.baud_rate,
                    input_mode: &mut self.input_mode,
                    serial_connected: self.serial.is_some(),
                    serial_log: &self.serial_log,
                },
            );
        });
        self.apply_ui_action(action);
        if let Some(egui_state) = self.egui_state.as_mut() {
            egui_state.handle_platform_output(window, full_output.platform_output.clone());
        }
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        let stats = gpu.render(window, &self.egui_ctx, full_output, &self.state);
        self.frames.instance_count = stats.instance_count;
    }

    fn apply_ui_action(&mut self, action: UiAction) {
        match action {
            UiAction::None
            | UiAction::SelectImu
            | UiAction::ToggleViewMode
            | UiAction::ClearSelection => {}
            UiAction::TogglePlayback => self.toggle_playback(),
            UiAction::RestartReplay => self.restart_replay(),
            UiAction::StepForward => self.step_one_frame(),
            UiAction::StepBack => self.step_back(),
            UiAction::ReloadReplay => self.reload_replay(),
            UiAction::RefreshPorts => self.refresh_ports(),
            UiAction::ConnectSerial => self.connect_serial(),
            UiAction::DisconnectSerial => self.disconnect_serial(),
        }
    }

    fn refresh_ports(&mut self) {
        self.ports = available_ports();
        self.selected_port = self.selected_port.min(self.ports.len().saturating_sub(1));
        self.status = format!("found {} serial ports", self.ports.len());
    }

    fn connect_serial(&mut self) {
        let Some(port_name) = self.ports.get(self.selected_port).cloned() else {
            self.status = String::from("no serial ports");
            return;
        };
        self.serial = None;
        self.playback_state = PlaybackState::Paused;
        self.replay_clock = None;
        self.state.clear();
        self.serial_log.clear();
        self.status = format!("connecting {}", port_name);
        self.serial = Some(connect(port_name, self.baud_rate, self.input_mode));
    }

    fn disconnect_serial(&mut self) {
        self.serial = None;
        self.status = String::from("serial disconnected");
    }

    fn poll_serial(&mut self) {
        let Some(serial) = self.serial.as_ref() else {
            return;
        };
        let mut events = Vec::new();
        while let Ok(event) = serial.receiver().try_recv() {
            events.push(event);
            if events.len() > 512 {
                break;
            }
        }
        for event in events {
            match event {
                SerialEvent::Frame(frame) => self.state.handle_frame(frame),
                SerialEvent::Status(status) => {
                    self.push_serial_log(format!("[status] {}", status));
                    self.status = status;
                }
                SerialEvent::RawLine(line) => {
                    self.push_serial_log(line);
                }
            }
        }
    }

    fn push_serial_log(&mut self, line: String) {
        self.serial_log.push_back(line);
        while self.serial_log.len() > 80 {
            let _ = self.serial_log.pop_front();
        }
    }

    fn reload_replay(&mut self) {
        self.serial = None;
        self.replay_frames.clear();
        self.replay_cursor = 0;
        self.replay_clock = None;
        self.state.clear();

        let Some(path) = find_default_replay_path() else {
            self.replay_path = None;
            self.playback_state = PlaybackState::Paused;
            self.status = String::from("no jsonl replay found in workspace root");
            return;
        };
        match load_replay_frames(&path) {
            Ok(frames) => {
                self.replay_path = Some(path);
                self.replay_frames = frames;
                self.playback_state = PlaybackState::Playing;
                self.status = format!("playing {} replay frames", self.replay_frames.len());
                self.rearm_replay_clock();
            }
            Err(error) => {
                self.replay_path = Some(path);
                self.playback_state = PlaybackState::Paused;
                self.status = format!("failed to load replay: {}", error);
            }
        }
    }

    fn rearm_replay_clock(&mut self) {
        self.replay_clock = self
            .replay_frames
            .get(self.replay_cursor)
            .map(ReplayClock::new);
    }

    fn toggle_playback(&mut self) {
        self.playback_state.toggle();
        self.rearm_replay_clock();
        self.status = format!("{} replay", self.playback_state.label());
    }

    fn restart_replay(&mut self) {
        self.replay_cursor = 0;
        self.state.clear();
        self.rearm_replay_clock();
        self.status = format!("{} from start", self.playback_state.label());
    }

    fn step_replay(&mut self) {
        let Some(clock) = self.replay_clock else {
            return;
        };
        while let Some(frame) = self.replay_frames.get(self.replay_cursor) {
            if !clock.due(frame) {
                break;
            }
            let frame = frame.clone();
            self.replay_cursor += 1;
            self.state.handle_frame(frame);
        }

        if self.replay_cursor >= self.replay_frames.len() {
            self.playback_state = PlaybackState::Paused;
            self.replay_clock = None;
            self.status = String::from("paused at end of replay");
        } else {
            self.status = format!(
                "{} replay ({}/{})",
                self.playback_state.label(),
                self.replay_cursor,
                self.replay_frames.len()
            );
        }
    }

    fn step_one_frame(&mut self) {
        self.playback_state = PlaybackState::Paused;
        if self.replay_cursor >= self.replay_frames.len() {
            self.status = String::from("paused at end of replay");
            return;
        }
        let frame = self.replay_frames[self.replay_cursor].clone();
        self.replay_cursor += 1;
        self.state.handle_frame(frame);
        self.status = format!(
            "paused stepping ({}/{})",
            self.replay_cursor,
            self.replay_frames.len()
        );
    }

    fn step_back(&mut self) {
        self.playback_state = PlaybackState::Paused;
        if self.replay_cursor == 0 {
            self.status = String::from("paused at replay start");
            return;
        }
        let target = self.replay_cursor.saturating_sub(1);
        self.replay_cursor = 0;
        self.state.clear();
        while self.replay_cursor < target {
            let frame = self.replay_frames[self.replay_cursor].clone();
            self.replay_cursor += 1;
            self.state.handle_frame(frame);
        }
        self.status = format!(
            "paused stepping ({}/{})",
            self.replay_cursor,
            self.replay_frames.len()
        );
    }
}
