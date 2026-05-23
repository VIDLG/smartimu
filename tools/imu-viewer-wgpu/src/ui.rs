use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;

use egui::{Context, RichText};
use smartimu::ImuId;

use crate::serial::InputMode;
use crate::state::{PlaybackState, ViewMode, ViewerState};

#[derive(Clone, Debug)]
pub enum UiAction {
    None,
    TogglePlayback,
    RestartReplay,
    StepForward,
    StepBack,
    ReloadReplay,
    ToggleViewMode,
    SelectImu,
    RefreshPorts,
    ConnectSerial,
    DisconnectSerial,
    ClearSelection,
}

pub struct UiStatus<'a> {
    pub playback_state: PlaybackState,
    pub replay_path: Option<&'a Path>,
    pub replay_cursor: usize,
    pub replay_len: usize,
    pub status: &'a str,
    pub fps: f32,
    pub frame_time: Duration,
    pub instance_count: usize,
    pub ports: &'a [String],
    pub selected_port: &'a mut usize,
    pub baud_rate: &'a mut u32,
    pub input_mode: &'a mut InputMode,
    pub serial_connected: bool,
    pub serial_log: &'a VecDeque<String>,
}

pub fn show(ctx: &Context, state: &mut ViewerState, status: UiStatus<'_>) -> UiAction {
    let mut action = UiAction::None;

    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(playback_button_label(status.playback_state))
                .clicked()
            {
                action = UiAction::TogglePlayback;
            }
            if ui.button("Restart").clicked() {
                action = UiAction::RestartReplay;
            }
            if ui.button("Step -").clicked() {
                action = UiAction::StepBack;
            }
            if ui.button("Step +").clicked() {
                action = UiAction::StepForward;
            }
            if ui.button("Reload").clicked() {
                action = UiAction::ReloadReplay;
            }
            if ui.button("Show All").clicked() {
                state.clear_selection();
                action = UiAction::ClearSelection;
            }
            ui.separator();
            if ui.button("Ports").clicked() {
                action = UiAction::RefreshPorts;
            }
            let selected = status
                .ports
                .get(*status.selected_port)
                .cloned()
                .unwrap_or_else(|| String::from("no ports"));
            egui::ComboBox::from_id_salt("serial-port")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (index, port) in status.ports.iter().enumerate() {
                        ui.selectable_value(status.selected_port, index, port);
                    }
                });
            ui.add(
                egui::DragValue::new(status.baud_rate)
                    .range(1_200..=3_000_000)
                    .speed(100.0)
                    .prefix("baud "),
            );
            egui::ComboBox::from_id_salt("input-mode")
                .selected_text(status.input_mode.label())
                .show_ui(ui, |ui| {
                    ui.selectable_value(status.input_mode, InputMode::Auto, "auto");
                    ui.selectable_value(status.input_mode, InputMode::Json, "json");
                    ui.selectable_value(status.input_mode, InputMode::Binary, "binary");
                });
            if ui
                .button(if status.serial_connected {
                    "Disconnect"
                } else {
                    "Connect"
                })
                .clicked()
            {
                action = if status.serial_connected {
                    UiAction::DisconnectSerial
                } else {
                    UiAction::ConnectSerial
                };
            }
            ui.separator();
            if ui
                .selectable_label(state.view_mode == ViewMode::Quaternion, "Quaternion")
                .clicked()
            {
                state.view_mode = ViewMode::Quaternion;
                action = UiAction::ToggleViewMode;
            }
            if ui
                .selectable_label(state.view_mode == ViewMode::Raw6Axis, "Raw 6-Axis")
                .clicked()
            {
                state.view_mode = ViewMode::Raw6Axis;
                action = UiAction::ToggleViewMode;
            }
            ui.separator();
            ui.label(format!("FPS {:.0}", status.fps));
            ui.label(format!(
                "Frame {:.1} ms",
                status.frame_time.as_secs_f64() * 1000.0
            ));
        });
    });

    egui::SidePanel::left("imu-list")
        .resizable(true)
        .default_width(230.0)
        .show(ctx, |ui| {
            ui.heading("IMUs");
            ui.small("Click an IMU to focus it; Show All clears focus.");
            ui.add_space(6.0);
            for imu_id in state.sorted_imu_ids() {
                let label = state
                    .imu_infos
                    .get(&imu_id)
                    .and_then(|info| info.label.clone())
                    .unwrap_or_else(|| format!("imu-{}", imu_id.sensor_id));
                let selected = state.selected_imu == Some(imu_id);
                if ui
                    .selectable_label(
                        selected,
                        format!("{}  {}/{}", label, imu_id.system_id, imu_id.sensor_id),
                    )
                    .clicked()
                {
                    state.selected_imu = Some(imu_id);
                    action = UiAction::SelectImu;
                }
                if let Some(info) = state.imu_infos.get(&imu_id) {
                    ui.small(format!("{:?}", info.chip_profile.chip));
                }
                ui.add_space(3.0);
            }
        });

    egui::SidePanel::right("details")
        .resizable(true)
        .default_width(290.0)
        .show(ctx, |ui| {
            ui.heading("Details");
            ui.add_space(6.0);
            ui.label(format!("status: {}", status.status));
            ui.label(format!(
                "replay: {}/{}",
                status.replay_cursor, status.replay_len
            ));
            if let Some(path) = status.replay_path {
                ui.small(path.display().to_string());
            } else {
                ui.small("no replay loaded");
            }
            ui.separator();
            ui.label(format!("received frames: {}", state.received_frames));
            ui.label(format!("active imus: {}", state.active_imus));
            ui.label(format!("seq gaps: {}", state.dropped_seq_count));
            ui.label(format!("3D instances: {}", status.instance_count));
            ui.separator();
            if let Some(imu_id) = state.selected_imu {
                selected_imu_details(ui, state, imu_id);
            } else {
                ui.label("No IMU selected");
            }
            ui.separator();
            ui.heading("Serial Log");
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .max_height(260.0)
                .show(ui, |ui| {
                    for line in status.serial_log {
                        ui.monospace(line);
                    }
                });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            let painter = ui.painter_at(rect);
            painter.rect_stroke(
                rect.shrink(10.0),
                0.0,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(64, 76, 88)),
                egui::StrokeKind::Inside,
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("3D View").strong());
                ui.separator();
                ui.label(format!("instances {}", status.instance_count));
                ui.separator();
                ui.label(format!("frames {}", state.received_frames));
            });
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.label("GPU scene is drawn behind this overlay");
            });
        });

    egui::TopBottomPanel::bottom("status-bar").show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            let mode = match state.view_mode {
                ViewMode::Raw6Axis => "raw",
                ViewMode::Quaternion => "quaternion",
            };
            ui.label(RichText::new(mode).strong());
            ui.separator();
            ui.label(status.status);
            ui.separator();
            ui.label(format!("rendered {}", status.instance_count));
        });
    });

    action
}

fn selected_imu_details(ui: &mut egui::Ui, state: &ViewerState, imu_id: ImuId) {
    ui.label(format!("imu id: {}/{}", imu_id.system_id, imu_id.sensor_id));
    if let Some(orientation) = state.latest_orientation.get(&imu_id) {
        let q = orientation.quaternion;
        ui.label(format!("qw: {:.4}", q.w));
        ui.label(format!("qx: {:.4}", q.x));
        ui.label(format!("qy: {:.4}", q.y));
        ui.label(format!("qz: {:.4}", q.z));
        ui.label(format!("sample: {}", orientation.sample_index));
    } else {
        ui.label("no orientation frame yet");
    }
    if let Some(sample) = state.latest_samples.get(&imu_id) {
        ui.separator();
        ui.label(format!("accel: {:?}", sample.sample.imu6.accel));
        ui.label(format!("gyro: {:?}", sample.sample.imu6.gyro));
    }
}

fn playback_button_label(playback_state: PlaybackState) -> &'static str {
    match playback_state {
        PlaybackState::Playing => "Pause",
        PlaybackState::Paused => "Play",
    }
}
