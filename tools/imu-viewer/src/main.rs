use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::Read;
use std::io::{BufRead, BufReader};
use std::process::Child;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui;
use egui_plot::{Legend, Line, Plot, PlotPoints};
use smartimu::{
    DeviceFrame, ImuId, ImuInfo, OrientationFrame, SampleFrame, WireFrame, decode_binary_packet,
    decode_json,
};

enum ViewerEvent {
    Frame(DeviceFrame),
    Status(String),
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "imu-viewer",
        native_options,
        Box::new(|_cc| Ok(Box::<ViewerApp>::default())),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputMode {
    Auto,
    Json,
    Binary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewMode {
    Raw6Axis,
    Quaternion,
}

#[derive(Clone, Copy, Debug, Default)]
struct OrientationState {
    roll: f32,
    pitch: f32,
    yaw: f32,
    last_sample_timestamp_us: Option<u64>,
}

const GYRO_DPS_PER_LSB: f32 = 1.0 / 16.0;

struct ViewerApp {
    ports: Vec<String>,
    selected_port: usize,
    baud_rate: u32,
    receiver: Option<Receiver<ViewerEvent>>,
    status: String,
    imu_infos: HashMap<ImuId, ImuInfo>,
    latest_samples: HashMap<ImuId, SampleFrame>,
    history: HashMap<ImuId, VecDeque<[f64; 7]>>,
    errors: VecDeque<String>,
    active_imus: u16,
    last_seq: Option<u32>,
    selected_imu: Option<ImuId>,
    recording: bool,
    recorded_frames: Vec<DeviceFrame>,
    export_status: String,
    input_mode: InputMode,
    view_mode: ViewMode,
    orientation: HashMap<ImuId, OrientationState>,
    latest_orientation: HashMap<ImuId, OrientationFrame>,
    quat_history: HashMap<ImuId, VecDeque<[f64; 5]>>,
    replay_path: String,
    replay_frames: Vec<DeviceFrame>,
    replay_cursor: usize,
    replaying: bool,
    powershell_child: Option<Child>,
    collapsed_imus: HashMap<ImuId, bool>,
}

impl Default for ViewerApp {
    fn default() -> Self {
        Self {
            ports: available_ports(),
            selected_port: 0,
            baud_rate: 115_200,
            receiver: None,
            status: String::from("disconnected"),
            imu_infos: HashMap::new(),
            latest_samples: HashMap::new(),
            history: HashMap::new(),
            errors: VecDeque::new(),
            active_imus: 0,
            last_seq: None,
            selected_imu: None,
            recording: false,
            recorded_frames: Vec::new(),
            export_status: String::new(),
            input_mode: InputMode::Auto,
            view_mode: ViewMode::Raw6Axis,
            orientation: HashMap::new(),
            latest_orientation: HashMap::new(),
            quat_history: HashMap::new(),
            replay_path: String::from("imu-recording.jsonl"),
            replay_frames: Vec::new(),
            replay_cursor: 0,
            replaying: false,
            powershell_child: None,
            collapsed_imus: HashMap::new(),
        }
    }
}

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_frames();
        self.step_replay();

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("refresh ports").clicked() {
                    self.ports = available_ports();
                    self.selected_port = self.selected_port.min(self.ports.len().saturating_sub(1));
                }

                let selected = self
                    .ports
                    .get(self.selected_port)
                    .cloned()
                    .unwrap_or_else(|| String::from("no ports"));

                egui::ComboBox::from_label("port")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (index, port) in self.ports.iter().enumerate() {
                            ui.selectable_value(&mut self.selected_port, index, port);
                        }
                    });

                ui.add(
                    egui::DragValue::new(&mut self.baud_rate)
                        .speed(100.0)
                        .prefix("baud "),
                );

                egui::ComboBox::from_label("mode")
                    .selected_text(match self.input_mode {
                        InputMode::Auto => "auto",
                        InputMode::Json => "json",
                        InputMode::Binary => "binary",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.input_mode, InputMode::Auto, "auto");
                        ui.selectable_value(&mut self.input_mode, InputMode::Json, "json");
                        ui.selectable_value(&mut self.input_mode, InputMode::Binary, "binary");
                    });

                egui::ComboBox::from_label("view")
                    .selected_text(match self.view_mode {
                        ViewMode::Raw6Axis => "raw 6-axis",
                        ViewMode::Quaternion => "quaternion",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.view_mode, ViewMode::Raw6Axis, "raw 6-axis");
                        ui.selectable_value(
                            &mut self.view_mode,
                            ViewMode::Quaternion,
                            "quaternion",
                        );
                    });

                if ui.button("connect").clicked() {
                    self.connect();
                }
                if ui.button("disconnect").clicked() {
                    self.disconnect();
                }

                let record_label = if self.recording {
                    "stop recording"
                } else {
                    "start recording"
                };
                if ui.button(record_label).clicked() {
                    self.toggle_recording();
                }

                if ui.button("export jsonl").clicked() {
                    self.export_jsonl();
                }

                if ui.button("export csv").clicked() {
                    self.export_csv();
                }

                ui.separator();
                ui.label("replay");
                ui.text_edit_singleline(&mut self.replay_path);
                if ui.button("load replay").clicked() {
                    self.load_replay();
                }
                let replay_label = if self.replaying {
                    "stop replay"
                } else {
                    "play replay"
                };
                if ui.button(replay_label).clicked() {
                    self.toggle_replay();
                }

                ui.label(format!("status: {}", self.status));
                if !self.export_status.is_empty() {
                    ui.label(format!("export: {}", self.export_status));
                }
            });
        });

        egui::SidePanel::left("imu-list")
            .resizable(true)
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.heading("IMUs");
                for (imu_id, info) in &self.imu_infos {
                    let selected = self.selected_imu == Some(*imu_id);
                    ui.group(|ui| {
                        if ui
                            .selectable_label(selected, info_label(info, *imu_id))
                            .clicked()
                        {
                            self.selected_imu = Some(*imu_id);
                        }
                        ui.label(format!("{:?}", info.chip_profile.chip));
                        ui.label(format!("imu={}/{}", imu_id.system_id, imu_id.sensor_id));
                        let collapsed = self.collapsed_imus.entry(*imu_id).or_insert(false);
                        let label = if *collapsed { "expand" } else { "collapse" };
                        if ui.small_button(label).clicked() {
                            *collapsed = !*collapsed;
                        }
                    });
                }
            });

        egui::SidePanel::right("status-panel")
            .resizable(true)
            .default_width(270.0)
            .show(ctx, |ui| {
                ui.heading("Status");
                ui.label(format!("stream: {}", self.status));
                ui.label(format!("active imus: {}", self.active_imus));
                ui.label(format!("recording: {}", self.recording));
                ui.label(format!("recorded frames: {}", self.recorded_frames.len()));
                ui.label(format!("replay frames: {}", self.replay_frames.len()));
                ui.label(format!("replaying: {}", self.replaying));
                if let Some(last_seq) = self.last_seq {
                    ui.label(format!("last seq: {}", last_seq));
                }

                ui.separator();
                ui.heading("3D Preview");
                if let Some(imu_id) = self
                    .selected_imu
                    .or_else(|| self.latest_samples.keys().next().copied())
                {
                    match self.view_mode {
                        ViewMode::Raw6Axis => {
                            if let Some(sample) = self.latest_samples.get(&imu_id) {
                                let orientation = self.orientation.get(&imu_id).copied();
                                draw_orientation_preview(ui, sample, orientation);
                            } else {
                                ui.label("no sample available");
                            }
                        }
                        ViewMode::Quaternion => {
                            if let Some(orientation) = self.latest_orientation.get(&imu_id) {
                                draw_quaternion_preview(ui, orientation);
                            } else {
                                ui.label("no quaternion available");
                            }
                        }
                    }
                } else {
                    ui.label("select an IMU");
                }

                ui.separator();
                ui.heading("Recent Errors");
                if self.errors.is_empty() {
                    ui.label("none");
                } else {
                    for error in self.errors.iter().rev().take(8) {
                        ui.label(error);
                    }
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.heading("2D Dashboard");

                for (imu_id, sample) in &self.latest_samples {
                    ui.separator();
                    let title = self
                        .imu_infos
                        .get(imu_id)
                        .map(|info| info_label(info, sample.imu_id))
                        .unwrap_or_else(|| String::from("unknown"));

                    ui.label(format!(
                        "{} imu={}/{} idx={} t={}us",
                        title,
                        imu_id.system_id,
                        imu_id.sensor_id,
                        sample.sample_index,
                        sample.sample_timestamp_us
                    ));
                    if let Some(scale) = self
                        .imu_infos
                        .get(imu_id)
                        .map(|info| smartimu::Imu6Scale::from(info.sample_config))
                    {
                        let physical = sample.sample.imu6.to_physical(scale);
                        ui.label(format!(
                            "{}",
                            match self.view_mode {
                                ViewMode::Raw6Axis => format!(
                                    "accel[g]=({:.3},{:.3},{:.3}) gyro[dps]=({:.2},{:.2},{:.2}) status=0x{:04X}",
                                    physical.accel_g[0],
                                    physical.accel_g[1],
                                    physical.accel_g[2],
                                    physical.gyro_dps[0],
                                    physical.gyro_dps[1],
                                    physical.gyro_dps[2],
                                    sample.status_bits
                                ),
                                ViewMode::Quaternion => {
                                    if let Some(orientation) = self.latest_orientation.get(imu_id) {
                                        format!(
                                            "quat=({:.4},{:.4},{:.4},{:.4})",
                                            orientation.quaternion.w,
                                            orientation.quaternion.x,
                                            orientation.quaternion.y,
                                            orientation.quaternion.z
                                        )
                                    } else {
                                        String::from("quat=unavailable")
                                    }
                                }
                            }
                        ));
                    } else {
                        ui.label(format!(
                            "accel(raw)=({},{},{}) gyro(raw)=({},{},{}) status=0x{:04X}",
                            sample.sample.imu6.accel[0],
                            sample.sample.imu6.accel[1],
                            sample.sample.imu6.accel[2],
                            sample.sample.imu6.gyro[0],
                            sample.sample.imu6.gyro[1],
                            sample.sample.imu6.gyro[2],
                            sample.status_bits
                        ));
                    }

                    let collapsed = self.collapsed_imus.entry(*imu_id).or_insert(false);
                    if *collapsed {
                        continue;
                    }

                    match self.view_mode {
                        ViewMode::Raw6Axis => {
                            if let Some(history) = self.history.get(imu_id) {
                                let accel_x = PlotPoints::from_iter(history.iter().map(|point| [point[0], point[1]]));
                                let accel_y = PlotPoints::from_iter(history.iter().map(|point| [point[0], point[2]]));
                                let accel_z = PlotPoints::from_iter(history.iter().map(|point| [point[0], point[3]]));
                                let gyro_x = PlotPoints::from_iter(history.iter().map(|point| [point[0], point[4]]));
                                let gyro_y = PlotPoints::from_iter(history.iter().map(|point| [point[0], point[5]]));
                                let gyro_z = PlotPoints::from_iter(history.iter().map(|point| [point[0], point[6]]));

                                ui.label("Accel [g]");
                                Plot::new(format!("accel-plot-{}-{}", imu_id.system_id, imu_id.sensor_id))
                                    .legend(Legend::default())
                                    .height(140.0)
                                    .show(ui, |plot_ui| {
                                        plot_ui.line(Line::new("ax", accel_x));
                                        plot_ui.line(Line::new("ay", accel_y));
                                        plot_ui.line(Line::new("az", accel_z));
                                    });

                                ui.label("Gyro [dps]");
                                Plot::new(format!("gyro-plot-{}-{}", imu_id.system_id, imu_id.sensor_id))
                                    .legend(Legend::default())
                                    .height(140.0)
                                    .show(ui, |plot_ui| {
                                        plot_ui.line(Line::new("gx", gyro_x));
                                        plot_ui.line(Line::new("gy", gyro_y));
                                        plot_ui.line(Line::new("gz", gyro_z));
                                    });
                            }
                        }
                        ViewMode::Quaternion => {
                            if let Some(history) = self.quat_history.get(imu_id) {
                                let qw = PlotPoints::from_iter(history.iter().map(|point| [point[0], point[1]]));
                                let qx = PlotPoints::from_iter(history.iter().map(|point| [point[0], point[2]]));
                                let qy = PlotPoints::from_iter(history.iter().map(|point| [point[0], point[3]]));
                                let qz = PlotPoints::from_iter(history.iter().map(|point| [point[0], point[4]]));

                                ui.label("Quaternion");
                                Plot::new(format!("quat-plot-{}-{}", imu_id.system_id, imu_id.sensor_id))
                                    .legend(Legend::default())
                                    .height(180.0)
                                    .show(ui, |plot_ui| {
                                        plot_ui.line(Line::new("qw", qw));
                                        plot_ui.line(Line::new("qx", qx));
                                        plot_ui.line(Line::new("qy", qy));
                                        plot_ui.line(Line::new("qz", qz));
                                    });
                            }
                        }
                    }
                }
            });
        });

        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

impl ViewerApp {
    fn connect(&mut self) {
        self.disconnect();

        let Some(port_name) = self.ports.get(self.selected_port).cloned() else {
            self.status = String::from("no serial ports");
            return;
        };

        #[cfg(windows)]
        cleanup_powershell_serial_readers(&port_name);

        let open_name = normalize_serial_port_name(&port_name);
        let baud_rate = self.baud_rate;
        let input_mode = self.input_mode;
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.status = format!("connecting {}", port_name);

        #[cfg(windows)]
        {
            if matches!(input_mode, InputMode::Json) {
                match spawn_powershell_serial_reader(&port_name, baud_rate, tx.clone()) {
                    Ok(child) => {
                        self.powershell_child = Some(child);
                        return;
                    }
                    Err(_) => {
                        self.status = format!("failed to open {} via powershell", port_name);
                        return;
                    }
                }
            }
        }

        thread::spawn(move || {
            let port_result = serialport::new(open_name.clone(), baud_rate)
                .timeout(Duration::from_millis(200))
                .open();

            let Ok(mut port) = port_result else {
                let _ = tx.send(ViewerEvent::Status(format!("failed to open {}", port_name)));
                return;
            };
            let _ = tx.send(ViewerEvent::Status(format!("opened {}", port_name)));

            let mut chunk = [0u8; 256];
            let mut line = Vec::<u8>::new();
            let mut packet = Vec::<u8>::new();
            let mut detected = input_mode;
            let mut saw_frame = false;
            let mut idle_count = 0u32;

            loop {
                match port.read(&mut chunk) {
                    Ok(0) => {
                        idle_count = idle_count.saturating_add(1);
                        if idle_count == 20 && !saw_frame {
                            let _ = tx.send(ViewerEvent::Status(String::from(
                                "opened port, waiting for valid frames",
                            )));
                        }
                    }
                    Ok(read) => {
                        idle_count = 0;
                        for byte in &chunk[..read] {
                            match detected {
                                InputMode::Json => {
                                    if *byte == b'\n' {
                                        if let Some(frame) = parse_json_line(&line) {
                                            saw_frame = true;
                                            let _ = tx.send(ViewerEvent::Status(String::from(
                                                "json stream",
                                            )));
                                            if tx.send(ViewerEvent::Frame(frame)).is_err() {
                                                return;
                                            }
                                        }
                                        line.clear();
                                    } else {
                                        push_bounded(&mut line, *byte, 4096);
                                    }
                                }
                                InputMode::Binary => {
                                    if *byte == 0 {
                                        packet.push(0);
                                        if let Some(frame) = parse_binary_packet(&packet) {
                                            saw_frame = true;
                                            let _ = tx.send(ViewerEvent::Status(String::from(
                                                "binary stream",
                                            )));
                                            if tx.send(ViewerEvent::Frame(frame)).is_err() {
                                                return;
                                            }
                                        }
                                        packet.clear();
                                    } else {
                                        push_bounded(&mut packet, *byte, 4096);
                                    }
                                }
                                InputMode::Auto => {
                                    if *byte == b'\n' {
                                        if let Some(frame) = parse_json_line(&line) {
                                            detected = InputMode::Json;
                                            saw_frame = true;
                                            let _ = tx.send(ViewerEvent::Status(String::from(
                                                "auto -> json",
                                            )));
                                            if tx.send(ViewerEvent::Frame(frame)).is_err() {
                                                return;
                                            }
                                        }
                                        line.clear();
                                    } else if *byte == 0 {
                                        packet.push(0);
                                        if let Some(frame) = parse_binary_packet(&packet) {
                                            detected = InputMode::Binary;
                                            saw_frame = true;
                                            let _ = tx.send(ViewerEvent::Status(String::from(
                                                "auto -> binary",
                                            )));
                                            if tx.send(ViewerEvent::Frame(frame)).is_err() {
                                                return;
                                            }
                                        }
                                        packet.clear();
                                    } else {
                                        push_bounded(&mut line, *byte, 4096);
                                        push_bounded(&mut packet, *byte, 4096);
                                    }
                                }
                            }
                        }
                    }
                    Err(error) => {
                        let _ =
                            tx.send(ViewerEvent::Status(format!("serial read error: {}", error)));
                        thread::sleep(Duration::from_millis(20));
                    }
                }
            }
        });
    }

    fn poll_frames(&mut self) {
        if self.receiver.is_none() {
            if !self.replaying {
                self.status = String::from("disconnected");
            }
            return;
        }

        loop {
            let event = match self
                .receiver
                .as_ref()
                .and_then(|receiver| receiver.try_recv().ok())
            {
                Some(event) => event,
                None => break,
            };
            match event {
                ViewerEvent::Frame(frame) => self.handle_frame(frame, true),
                ViewerEvent::Status(status) => self.status = status,
            }
        }
    }

    fn disconnect(&mut self) {
        if let Some(mut child) = self.powershell_child.take() {
            let _ = child.kill();
        }
        self.receiver = None;
        if !self.replaying {
            self.status = String::from("disconnected");
        }
    }

    fn toggle_recording(&mut self) {
        self.recording = !self.recording;
        if self.recording {
            self.recorded_frames.clear();
            self.export_status = String::from("recording started");
        } else {
            self.export_status = format!(
                "recording stopped with {} frames",
                self.recorded_frames.len()
            );
        }
    }

    fn export_jsonl(&mut self) {
        if self.recorded_frames.is_empty() {
            self.export_status = String::from("no frames to export");
            return;
        }

        let path = export_path("imu-recording", "jsonl");
        let mut content = String::new();
        for frame in &self.recorded_frames {
            match serde_json::to_string(&WireFrame::Device(frame.clone())) {
                Ok(line) => {
                    content.push_str(&line);
                    content.push('\n');
                }
                Err(error) => {
                    self.export_status = format!("json export failed: {}", error);
                    return;
                }
            }
        }

        match fs::write(&path, content) {
            Ok(()) => self.export_status = format!("saved {}", path),
            Err(error) => self.export_status = format!("write failed: {}", error),
        }
    }

    fn export_csv(&mut self) {
        let mut content = String::from(
            "system_id,sensor_id,seq,sample_index,sample_timestamp_us,ax,ay,az,gx,gy,gz,status_bits\n",
        );
        let mut rows = 0usize;

        for frame in &self.recorded_frames {
            if let DeviceFrame::Sample(sample) = frame {
                rows += 1;
                let _ = std::fmt::Write::write_fmt(
                    &mut content,
                    format_args!(
                        "{},{},{},{},{},{},{},{},{},{},{},{}\n",
                        sample.imu_id.system_id,
                        sample.imu_id.sensor_id,
                        sample.header.seq,
                        sample.sample_index,
                        sample.sample_timestamp_us,
                        sample.sample.imu6.accel[0],
                        sample.sample.imu6.accel[1],
                        sample.sample.imu6.accel[2],
                        sample.sample.imu6.gyro[0],
                        sample.sample.imu6.gyro[1],
                        sample.sample.imu6.gyro[2],
                        sample.status_bits
                    ),
                );
            }
        }

        if rows == 0 {
            self.export_status = String::from("no sample frames to export");
            return;
        }

        let path = export_path("imu-samples", "csv");
        match fs::write(&path, content) {
            Ok(()) => self.export_status = format!("saved {}", path),
            Err(error) => self.export_status = format!("write failed: {}", error),
        }
    }

    fn load_replay(&mut self) {
        let content = match fs::read_to_string(&self.replay_path) {
            Ok(content) => content,
            Err(error) => {
                self.export_status = format!("replay load failed: {}", error);
                return;
            }
        };

        let mut frames = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match decode_json(trimmed).ok().and_then(wire_to_device) {
                Some(frame) => frames.push(frame),
                None => {
                    self.export_status = String::from("replay parse failed");
                    return;
                }
            }
        }

        self.replay_frames = frames;
        self.replay_cursor = 0;
        self.export_status = format!("loaded {} replay frames", self.replay_frames.len());
    }

    fn toggle_replay(&mut self) {
        if self.replaying {
            self.replaying = false;
            self.status = String::from("replay stopped");
            return;
        }

        if self.replay_frames.is_empty() {
            self.export_status = String::from("no replay loaded");
            return;
        }

        self.reset_session_view();
        self.replay_cursor = 0;
        self.replaying = true;
        self.status = String::from("replaying");
    }

    fn step_replay(&mut self) {
        if !self.replaying {
            return;
        }

        let mut budget = 4usize;
        while budget > 0 && self.replay_cursor < self.replay_frames.len() {
            let frame = self.replay_frames[self.replay_cursor].clone();
            self.handle_frame(frame, false);
            self.replay_cursor += 1;
            budget -= 1;
        }

        if self.replay_cursor >= self.replay_frames.len() {
            self.replaying = false;
            self.status = String::from("replay finished");
        }
    }

    fn handle_frame(&mut self, frame: DeviceFrame, allow_recording: bool) {
        if allow_recording && self.recording {
            self.recorded_frames.push(frame.clone());
        }

        match frame {
            DeviceFrame::Ping(ping) => {
                self.status = format!(
                    "streaming {:?} session={} emit={}us",
                    ping.header.format, ping.header.session_id, ping.header.emit_timestamp_us
                );
                self.last_seq = Some(ping.header.seq);
            }
            DeviceFrame::Inventory(topology) => {
                self.last_seq = Some(topology.header.seq);
                self.imu_infos.clear();
                for info in topology.imus {
                    self.imu_infos.insert(info.id, info);
                }
            }
            DeviceFrame::ImuInfo(frame) => {
                self.last_seq = Some(frame.header.seq);
                if let Some(info) = frame.info {
                    self.imu_infos.insert(info.id, info);
                }
            }
            DeviceFrame::Sample(sample) => {
                self.last_seq = Some(sample.header.seq);
                self.update_orientation(&sample);
                self.imu_infos
                    .entry(sample.imu_id)
                    .or_insert_with(|| ImuInfo {
                        id: sample.imu_id,
                        bus_id: smartimu::BusId(0),
                        chip_profile: fallback_chip_profile(sample.imu_chip),
                        label: None,
                        sample_config: fallback_sample_config(),
                    });
                let entry = self.history.entry(sample.imu_id).or_default();
                let values = if let Some(scale) = self
                    .imu_infos
                    .get(&sample.imu_id)
                    .map(|info| smartimu::Imu6Scale::from(info.sample_config))
                {
                    let physical = sample.sample.imu6.to_physical(scale);
                    [
                        sample.sample_timestamp_us as f64 / 1_000_000.0,
                        physical.accel_g[0] as f64,
                        physical.accel_g[1] as f64,
                        physical.accel_g[2] as f64,
                        physical.gyro_dps[0] as f64,
                        physical.gyro_dps[1] as f64,
                        physical.gyro_dps[2] as f64,
                    ]
                } else {
                    [
                        sample.sample_timestamp_us as f64 / 1_000_000.0,
                        sample.sample.imu6.accel[0] as f64,
                        sample.sample.imu6.accel[1] as f64,
                        sample.sample.imu6.accel[2] as f64,
                        sample.sample.imu6.gyro[0] as f64,
                        sample.sample.imu6.gyro[1] as f64,
                        sample.sample.imu6.gyro[2] as f64,
                    ]
                };
                entry.push_back(values);
                while entry.len() > 256 {
                    let _ = entry.pop_front();
                }
                if self.selected_imu.is_none() {
                    self.selected_imu = Some(sample.imu_id);
                }
                self.latest_samples.insert(sample.imu_id, sample);
            }
            DeviceFrame::Error(error) => {
                self.last_seq = Some(error.header.seq);
                self.status = format!("device error: {:?}", error.error);
                self.errors
                    .push_back(format!("{:?}: {}", error.error, error.message));
                while self.errors.len() > 32 {
                    let _ = self.errors.pop_front();
                }
            }
            DeviceFrame::ProbeResult(probe) => {
                self.last_seq = Some(probe.header.seq);
                match probe.result {
                    smartimu::ProbeResult::Detected { .. } => {}
                    smartimu::ProbeResult::NotDetected => self
                        .errors
                        .push_back(format!("probe {} found no chip", probe.probe_label)),
                    smartimu::ProbeResult::Failed { error } => self
                        .errors
                        .push_back(format!("probe {} failed: {:?}", probe.probe_label, error)),
                }
                while self.errors.len() > 32 {
                    let _ = self.errors.pop_front();
                }
            }
            DeviceFrame::Heartbeat(heartbeat) => {
                self.last_seq = Some(heartbeat.header.seq);
                self.active_imus = heartbeat.active_imus;
            }
            DeviceFrame::Orientation(orientation) => {
                self.last_seq = Some(orientation.header.seq);
                self.latest_orientation
                    .insert(orientation.imu_id, orientation.clone());
                let entry = self.quat_history.entry(orientation.imu_id).or_default();
                entry.push_back([
                    orientation.sample_timestamp_us as f64 / 1_000_000.0,
                    orientation.quaternion.w as f64,
                    orientation.quaternion.x as f64,
                    orientation.quaternion.y as f64,
                    orientation.quaternion.z as f64,
                ]);
                while entry.len() > 256 {
                    let _ = entry.pop_front();
                }
            }
        }
    }

    fn update_orientation(&mut self, sample: &SampleFrame) {
        let state = self
            .orientation
            .entry(sample.imu_id)
            .or_insert_with(OrientationState::default);

        let dt = if let Some(last_timestamp_us) = state.last_sample_timestamp_us {
            ((sample.sample_timestamp_us.saturating_sub(last_timestamp_us)) as f32 / 1_000_000.0)
                .clamp(0.0, 0.1)
        } else {
            0.0
        };
        state.last_sample_timestamp_us = Some(sample.sample_timestamp_us);

        let gx = sample.sample.imu6.gyro[0] as f32 * GYRO_DPS_PER_LSB;
        let gy = sample.sample.imu6.gyro[1] as f32 * GYRO_DPS_PER_LSB;
        let gz = sample.sample.imu6.gyro[2] as f32 * GYRO_DPS_PER_LSB;

        state.roll += gx.to_radians() * dt;
        state.pitch += gy.to_radians() * dt;
        state.yaw += gz.to_radians() * dt;

        let ax = sample.sample.imu6.accel[0] as f32;
        let ay = sample.sample.imu6.accel[1] as f32;
        let az = sample.sample.imu6.accel[2] as f32;
        let accel_norm = (ax * ax + ay * ay + az * az).sqrt().max(1.0);
        let axn = ax / accel_norm;
        let ayn = ay / accel_norm;
        let azn = az / accel_norm;

        let accel_roll = ayn.atan2(azn);
        let accel_pitch = (-axn).atan2((ayn * ayn + azn * azn).sqrt());

        let alpha = 0.98;
        state.roll = alpha * state.roll + (1.0 - alpha) * accel_roll;
        state.pitch = alpha * state.pitch + (1.0 - alpha) * accel_pitch;
    }

    fn reset_session_view(&mut self) {
        self.imu_infos.clear();
        self.latest_samples.clear();
        self.history.clear();
        self.errors.clear();
        self.active_imus = 0;
        self.last_seq = None;
        self.selected_imu = None;
        self.orientation.clear();
        self.latest_orientation.clear();
        self.quat_history.clear();
    }
}

fn available_ports() -> Vec<String> {
    serialport::available_ports()
        .map(|ports| ports.into_iter().map(|port| port.port_name).collect())
        .unwrap_or_default()
}

#[cfg(windows)]
fn spawn_powershell_serial_reader(
    port_name: &str,
    baud_rate: u32,
    tx: mpsc::Sender<ViewerEvent>,
) -> Result<Child, ()> {
    use std::process::{Command, Stdio};

    let script = format!(
        "$utf8 = New-Object System.Text.UTF8Encoding($false); \
         [Console]::OutputEncoding = $utf8; \
         $OutputEncoding = $utf8; \
         $port = New-Object System.IO.Ports.SerialPort '{port}',{baud},'None',8,'one'; \
         $port.ReadTimeout = 1000; \
         $port.Open(); \
         [Console]::WriteLine('__OPENED__'); \
         while ($true) {{ \
           try {{ \
             $line = $port.ReadLine(); \
             [Console]::WriteLine($line); \
           }} catch {{ Start-Sleep -Milliseconds 20 }} \
         }}",
        port = port_name,
        baud = baud_rate
    );

    let mut child = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-WindowStyle")
        .arg("Hidden")
        .arg("-Command")
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| ())?;

    let stdout = child.stdout.take().ok_or(())?;
    let stderr = child.stderr.take().ok_or(())?;
    let port_name = port_name.to_string();
    let tx_stderr = tx.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) if !line.trim().is_empty() => {
                    let _ = tx_stderr.send(ViewerEvent::Status(format!(
                        "powershell stderr: {}",
                        line.trim()
                    )));
                }
                _ => {}
            }
        }
    });

    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed == "__OPENED__" {
                let _ = tx.send(ViewerEvent::Status(format!(
                    "opened {} via powershell",
                    port_name
                )));
                continue;
            }
            if let Some(frame) = decode_json(trimmed).ok().and_then(wire_to_device) {
                let _ = tx.send(ViewerEvent::Status(String::from(
                    "json stream (powershell)",
                )));
                if tx.send(ViewerEvent::Frame(frame)).is_err() {
                    break;
                }
            }
        }
    });

    Ok(child)
}

#[cfg(windows)]
fn cleanup_powershell_serial_readers(port_name: &str) {
    use std::process::Command;

    let escaped = port_name.replace('\'', "''");
    let script = format!(
        "$port = '{port}'; \
         $portRegex = [Regex]::Escape($port); \
         Get-CimInstance Win32_Process | \
         Where-Object {{ \
           $_.Name -eq 'powershell.exe' -and \
           $_.CommandLine -match 'System\\.IO\\.Ports\\.SerialPort' -and \
           $_.CommandLine -match $portRegex \
         }} | \
         ForEach-Object {{ \
           try {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction Stop }} catch {{}} \
         }}",
        port = escaped
    );

    let _ = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-WindowStyle")
        .arg("Hidden")
        .arg("-Command")
        .arg(script)
        .status();
}

fn normalize_serial_port_name(port_name: &str) -> String {
    #[cfg(windows)]
    {
        let upper = port_name.to_ascii_uppercase();
        if upper.starts_with("COM") {
            let suffix = &port_name[3..];
            if suffix.parse::<u32>().map(|n| n >= 10).unwrap_or(false) {
                return format!(r"\\.\{}", port_name);
            }
        }
    }

    port_name.to_string()
}

fn parse_json_line(buffer: &[u8]) -> Option<DeviceFrame> {
    let line = std::str::from_utf8(buffer).ok()?.trim();
    if line.is_empty() {
        return None;
    }
    let json_start = line.find('{')?;
    let candidate = line[json_start..].trim();
    decode_json(candidate).ok().and_then(wire_to_device)
}

fn parse_binary_packet(buffer: &[u8]) -> Option<DeviceFrame> {
    decode_binary_packet::<1024>(buffer)
        .ok()
        .and_then(wire_to_device)
}

fn wire_to_device(frame: WireFrame) -> Option<DeviceFrame> {
    match frame {
        WireFrame::Device(frame) => Some(frame),
        WireFrame::Host(_) => None,
    }
}

fn push_bounded(buffer: &mut Vec<u8>, byte: u8, max: usize) {
    if buffer.len() >= max {
        buffer.clear();
    }
    buffer.push(byte);
}

fn export_path(prefix: &str, extension: &str) -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}-{}.{}", prefix, stamp, extension)
}

fn draw_orientation_preview(
    ui: &mut egui::Ui,
    sample: &SampleFrame,
    orientation: Option<OrientationState>,
) {
    let desired_size = egui::vec2(ui.available_width(), 220.0);
    let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_stroke(
        rect,
        8.0,
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.fg_stroke.color),
        egui::StrokeKind::Inside,
    );

    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.24;

    let ax = sample.sample.imu6.accel[0] as f32;
    let ay = sample.sample.imu6.accel[1] as f32;
    let az = sample.sample.imu6.accel[2] as f32;
    let norm = (ax * ax + ay * ay + az * az).sqrt().max(1.0);
    let vx = ax / norm;
    let vy = ay / norm;
    let vz = az / norm;

    let orientation = orientation.unwrap_or_default();
    draw_wireframe_cube(&painter, center, radius, orientation);

    painter.line_segment(
        [center, center + egui::vec2(vx * radius, -vy * radius)],
        egui::Stroke::new(3.0, egui::Color32::YELLOW),
    );

    painter.text(
        rect.left_top() + egui::vec2(10.0, 10.0),
        egui::Align2::LEFT_TOP,
        format!(
            "r={:.1} p={:.1} y={:.1}  gz={:.2}",
            orientation.roll.to_degrees(),
            orientation.pitch.to_degrees(),
            orientation.yaw.to_degrees(),
            vz
        ),
        egui::TextStyle::Body.resolve(ui.style()),
        ui.visuals().text_color(),
    );
}

fn draw_quaternion_preview(ui: &mut egui::Ui, orientation: &OrientationFrame) {
    let desired_size = egui::vec2(ui.available_width(), 220.0);
    let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_stroke(
        rect,
        8.0,
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.fg_stroke.color),
        egui::StrokeKind::Inside,
    );

    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.24;
    let state = quaternion_to_orientation_state(orientation);
    draw_wireframe_cube(&painter, center, radius, state);

    painter.text(
        rect.left_top() + egui::vec2(10.0, 10.0),
        egui::Align2::LEFT_TOP,
        format!(
            "qw={:.4} qx={:.4} qy={:.4} qz={:.4}",
            orientation.quaternion.w,
            orientation.quaternion.x,
            orientation.quaternion.y,
            orientation.quaternion.z
        ),
        egui::TextStyle::Body.resolve(ui.style()),
        ui.visuals().text_color(),
    );
}

fn quaternion_to_orientation_state(frame: &OrientationFrame) -> OrientationState {
    let q = &frame.quaternion;
    let sinr_cosp = 2.0 * (q.w * q.x + q.y * q.z);
    let cosr_cosp = 1.0 - 2.0 * (q.x * q.x + q.y * q.y);
    let roll = sinr_cosp.atan2(cosr_cosp);

    let sinp = 2.0 * (q.w * q.y - q.z * q.x);
    let pitch = if sinp.abs() >= 1.0 {
        sinp.signum() * core::f32::consts::FRAC_PI_2
    } else {
        sinp.asin()
    };

    let siny_cosp = 2.0 * (q.w * q.z + q.x * q.y);
    let cosy_cosp = 1.0 - 2.0 * (q.y * q.y + q.z * q.z);
    let yaw = siny_cosp.atan2(cosy_cosp);

    OrientationState {
        roll,
        pitch,
        yaw,
        last_sample_timestamp_us: Some(frame.sample_timestamp_us),
    }
}

fn draw_wireframe_cube(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    orientation: OrientationState,
) {
    let vertices = [
        [-1.0, -1.0, -1.0],
        [1.0, -1.0, -1.0],
        [1.0, 1.0, -1.0],
        [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];

    let edges = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];

    let projected: Vec<egui::Pos2> = vertices
        .iter()
        .map(|vertex| project_vertex(center, radius, *vertex, orientation))
        .collect();

    for (a, b) in edges {
        painter.line_segment(
            [projected[a], projected[b]],
            egui::Stroke::new(1.5, egui::Color32::from_rgb(120, 180, 255)),
        );
    }

    painter.line_segment(
        [
            center,
            project_vertex(center, radius * 1.2, [1.6, 0.0, 0.0], orientation),
        ],
        egui::Stroke::new(2.0, egui::Color32::RED),
    );
    painter.line_segment(
        [
            center,
            project_vertex(center, radius * 1.2, [0.0, 1.6, 0.0], orientation),
        ],
        egui::Stroke::new(2.0, egui::Color32::GREEN),
    );
    painter.line_segment(
        [
            center,
            project_vertex(center, radius * 1.2, [0.0, 0.0, 1.6], orientation),
        ],
        egui::Stroke::new(2.0, egui::Color32::BLUE),
    );
}

fn project_vertex(
    center: egui::Pos2,
    radius: f32,
    vertex: [f32; 3],
    orientation: OrientationState,
) -> egui::Pos2 {
    let [mut x, mut y, mut z] = vertex;

    let (sr, cr) = orientation.roll.sin_cos();
    let (sp, cp) = orientation.pitch.sin_cos();
    let (sy, cy) = orientation.yaw.sin_cos();

    let y1 = y * cr - z * sr;
    let z1 = y * sr + z * cr;
    y = y1;
    z = z1;

    let x2 = x * cp + z * sp;
    let z2 = -x * sp + z * cp;
    x = x2;
    z = z2;

    let x3 = x * cy - y * sy;
    let y3 = x * sy + y * cy;
    x = x3;
    y = y3;

    let perspective = 1.0 / (1.0 + z * 0.35);
    egui::pos2(
        center.x + x * radius * perspective,
        center.y - y * radius * perspective,
    )
}

fn info_label(info: &ImuInfo, imu_id: ImuId) -> String {
    info.label
        .clone()
        .unwrap_or_else(|| format!("imu-{}", imu_id.sensor_id))
}

fn fallback_sample_config() -> smartimu::ImuSampleConfig {
    smartimu::ImuSampleConfig {
        accel_range: smartimu::RangeG(2),
        gyro_range: smartimu::RangeDps(2048),
        sample_rate_hz: smartimu::SampleRateHz(100),
    }
}

fn fallback_sample_config_options() -> smartimu::SampleConfigOptions {
    smartimu::SampleConfigOptions::Constrained {
        configs: std::borrow::Cow::Owned(vec![fallback_sample_config()]),
    }
}

fn fallback_chip_profile(chip: smartimu::ImuChip) -> smartimu::ImuChipProfile {
    smartimu::ImuChipProfile {
        chip,
        sample_config_options: fallback_sample_config_options(),
        sample_readout_support: smartimu::SampleReadoutSupport::default(),
        temperature_config: None,
    }
}
