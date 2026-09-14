use std::collections::VecDeque;
use std::fs;
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::{TimeZone, Utc};
use eframe::egui::{self, Align, Color32, FontData, FontDefinitions, FontFamily, FontId, RichText, Sense, Stroke, Vec2};
use egui_plot::{Legend, Line, Plot, PlotPoints};

use crate::analytics::{EmitterObservation, SignalStatistics};
use crate::chaos::{ChaosConfig, SeqMode, SignalConfig};
use crate::dsp::full_scale;
use crate::profile::{InputMode, ResolvedConfig};
use crate::scenario;
use crate::state::{DisplayEvent, WorkerCommand, WorkerUpdate};
use crate::worker::{self, WorkerHandle};

const BG: Color32 = Color32::from_rgb(5, 12, 17);
const PANEL: Color32 = Color32::from_rgb(8, 20, 28);
const PANEL_2: Color32 = Color32::from_rgb(10, 25, 35);
const PANEL_3: Color32 = Color32::from_rgb(12, 29, 40);
const BORDER: Color32 = Color32::from_rgb(27, 55, 70);
const BORDER_HI: Color32 = Color32::from_rgb(38, 83, 102);
const TEXT: Color32 = Color32::from_rgb(218, 234, 241);
const MUTED: Color32 = Color32::from_rgb(118, 151, 166);
const CYAN: Color32 = Color32::from_rgb(28, 213, 226);
const BLUE: Color32 = Color32::from_rgb(44, 176, 234);
const GREEN: Color32 = Color32::from_rgb(35, 232, 164);
const AMBER: Color32 = Color32::from_rgb(244, 176, 74);
const ORANGE: Color32 = Color32::from_rgb(246, 105, 70);
const RED: Color32 = Color32::from_rgb(236, 69, 84);
const MAGENTA: Color32 = Color32::from_rgb(225, 78, 208);

#[derive(PartialEq, Eq, Clone, Copy)]
enum Workspace {
    Monitor,
    Live,
    Health,
    Events,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum ControlTab {
    Experiment,
    Protocol,
    System,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum SourceTableTab {
    Active,
    Planned,
    Suppressed,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum EventFilter {
    All,
    Info,
    Warnings,
}

#[derive(Clone, Copy)]
enum HealthState {
    Waiting,
    Nominal,
    Degraded,
    Fault,
}

#[derive(Clone)]
struct PlannedGhost {
    source_id: u64,
    source_khz: f64,
    target_khz: f64,
    level_dbfs: f64,
}

pub fn run(cfg: ResolvedConfig) -> Result<()> {
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1680.0, 980.0])
            .with_min_inner_size([1280.0, 760.0]),
        ..Default::default()
    };
    let title = format!("VITA-49 RF Chaos Workbench — {}", cfg.stream.name);
    eframe::run_native(
        &title,
        native,
        Box::new(move |cc| Box::new(WorkbenchApp::new(cc, cfg.clone()))),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}

struct WorkbenchApp {
    cfg: ResolvedConfig,
    worker: WorkerHandle,
    latest: WorkerUpdate,
    chaos: ChaosConfig,
    selected_preset: String,
    seed: u64,
    armed: bool,
    emit_requested: bool,
    workspace: Workspace,
    control_tab: ControlTab,
    source_table_tab: SourceTableTab,
    event_filter: EventFilter,

    waterfall: VecDeque<Vec<f64>>,
    waterfall_active: VecDeque<bool>,
    waterfall_boundary: VecDeque<bool>,
    waterfall_tex: Option<egui::TextureHandle>,
    waterfall_floor: f64,
    waterfall_ceil: f64,
    waterfall_history_cols: usize,
    freeze_waterfall: bool,
    waveform_ms: f64,
    show_clean: bool,
    show_chaos: bool,

    clean_pps_history: VecDeque<f64>,
    chaos_pps_history: VecDeque<f64>,
    delta_history: VecDeque<f64>,
    seq_history: VecDeque<f64>,
    timing_history: VecDeque<f64>,
    parse_history: VecDeque<f64>,

    last_active: bool,
    last_clean_packets: u64,
    last_chaos_packets: u64,
    last_output_present: bool,
    run_started: Option<Instant>,
    recovery_started: Option<Instant>,
    last_recovery_ms: Option<f64>,
    prev_worker_active: bool,
}

impl WorkbenchApp {
    fn new(cc: &eframe::CreationContext<'_>, cfg: ResolvedConfig) -> Self {
        install_system_fonts(&cc.egui_ctx);
        install_theme(&cc.egui_ctx);
        let worker = worker::spawn(cfg.clone());
        let seed = cfg.seed;
        let selected_preset = cfg
            .headless_preset
            .clone()
            .filter(|name| scenario::NAMES.iter().any(|known| *known == name.as_str()))
            .unwrap_or_else(|| "Clean / pass-through".to_string());
        let chaos = scenario::named(&selected_preset);
        Self {
            cfg,
            worker,
            latest: WorkerUpdate::default(),
            chaos,
            selected_preset,
            seed,
            armed: false,
            emit_requested: false,
            workspace: Workspace::Monitor,
            control_tab: ControlTab::Experiment,
            source_table_tab: SourceTableTab::Active,
            event_filter: EventFilter::All,
            waterfall: VecDeque::with_capacity(360),
            waterfall_active: VecDeque::with_capacity(360),
            waterfall_boundary: VecDeque::with_capacity(360),
            waterfall_tex: None,
            waterfall_floor: -120.0,
            waterfall_ceil: 0.0,
            waterfall_history_cols: 300,
            freeze_waterfall: false,
            waveform_ms: 4.0,
            show_clean: true,
            show_chaos: true,
            clean_pps_history: VecDeque::with_capacity(120),
            chaos_pps_history: VecDeque::with_capacity(120),
            delta_history: VecDeque::with_capacity(120),
            seq_history: VecDeque::with_capacity(120),
            timing_history: VecDeque::with_capacity(120),
            parse_history: VecDeque::with_capacity(120),
            last_active: false,
            last_clean_packets: 0,
            last_chaos_packets: 0,
            last_output_present: false,
            run_started: None,
            recovery_started: None,
            last_recovery_ms: None,
            prev_worker_active: false,
        }
    }

    fn drain_updates(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        while let Ok(u) = self.worker.updates.try_recv() {
            self.latest = u;
            changed = true;
            push_history(&mut self.clean_pps_history, self.latest.clean.pps, 120);
            push_history(&mut self.chaos_pps_history, self.latest.chaos.pps, 120);
            let delta = if self.latest.clean.pps > 1e-6 {
                (self.latest.chaos.pps / self.latest.clean.pps - 1.0) * 100.0
            } else {
                0.0
            };
            push_history(&mut self.delta_history, delta, 120);
            push_history(&mut self.seq_history, self.latest.chaos.seq_gaps as f64, 120);
            push_history(
                &mut self.timing_history,
                (self.latest.chaos.ts_drops + self.latest.chaos.glitches) as f64,
                120,
            );
            push_history(&mut self.parse_history, self.latest.chaos.parse_errors as f64, 120);
        }
        if !changed {
            return;
        }

        let clean_advanced = self.latest.clean.packets > self.last_clean_packets;
        let chaos_advanced = self.latest.chaos.packets > self.last_chaos_packets;
        let output_present = chaos_advanced;

        if self.prev_worker_active && !self.latest.active {
            self.recovery_started = Some(Instant::now());
            self.run_started = None;
            self.armed = false;
        }
        if !self.latest.active {
            if let Some(t0) = self.recovery_started {
                let recovered = self.latest.clean.pps > 1.0
                    && self.latest.chaos.pps >= self.latest.clean.pps * 0.95;
                if recovered {
                    self.last_recovery_ms = Some(t0.elapsed().as_secs_f64() * 1000.0);
                    self.recovery_started = None;
                }
            }
        }
        self.prev_worker_active = self.latest.active;

        if !self.freeze_waterfall {
            let mut db = self.latest.analysis.chaos_spectrum.dbfs.clone();
            let visible_gap = self.latest.active && clean_advanced && !chaos_advanced;
            if visible_gap {
                let bins = self
                    .latest
                    .analysis
                    .clean_spectrum
                    .dbfs
                    .len()
                    .max(if self.cfg.stream.iq { 512 } else { 257 });
                db = vec![self.waterfall_floor; bins];
            }
            if !db.is_empty() {
                while self.waterfall.len() >= self.waterfall_history_cols.max(60) {
                    self.waterfall.pop_front();
                    self.waterfall_active.pop_front();
                    self.waterfall_boundary.pop_front();
                }
                let boundary = self.latest.active != self.last_active
                    || (self.latest.active && output_present != self.last_output_present);
                self.waterfall.push_back(db);
                self.waterfall_active.push_back(self.latest.active);
                self.waterfall_boundary.push_back(boundary);
                self.last_active = self.latest.active;
                self.last_output_present = output_present;
                self.update_waterfall_texture(ctx);
            }
        }

        self.last_clean_packets = self.latest.clean.packets;
        self.last_chaos_packets = self.latest.chaos.packets;
    }

    fn update_waterfall_texture(&mut self, ctx: &egui::Context) {
        if self.waterfall.is_empty() {
            return;
        }
        // Engineering waterfall convention: frequency on X, history/time on Y.
        // Each deque entry is one real FFT frame. Keeping this orientation makes
        // persistent emitters vertical, matching both spectrum-analyzer practice
        // and the selected Operator Workstation design.
        let w = self.waterfall.back().map(|v| v.len()).unwrap_or(0);
        let h = self.waterfall.len();
        if w == 0 || h == 0 {
            return;
        }
        let mut pixels = Vec::with_capacity(w * h);
        let span = (self.waterfall_ceil - self.waterfall_floor).max(1.0);
        for (row, spectrum) in self.waterfall.iter().enumerate() {
            let active = self.waterfall_active.get(row).copied().unwrap_or(false);
            let boundary = self.waterfall_boundary.get(row).copied().unwrap_or(false);
            for x in 0..w {
                let v = spectrum.get(x).copied().unwrap_or(self.waterfall_floor);
                let t = ((v - self.waterfall_floor) / span).clamp(0.0, 1.0);
                let mut c = operator_colormap(t);
                // An experiment state transition is a real point in waterfall
                // history, so it is rendered as a horizontal annotation row.
                if boundary {
                    c = ORANGE;
                } else if active && x < 2 {
                    // A narrow red activity rail is metadata, not sample data.
                    c = RED;
                }
                pixels.push(c);
            }
        }
        let image = egui::ColorImage { size: [w, h], pixels };
        if let Some(tex) = self.waterfall_tex.as_mut() {
            tex.set(image, egui::TextureOptions::LINEAR);
        } else {
            self.waterfall_tex = Some(ctx.load_texture(
                "operator-waterfall",
                image,
                egui::TextureOptions::LINEAR,
            ));
        }
    }

    fn start(&mut self) {
        if !self.armed {
            return;
        }
        let emit = self.emit_requested && self.cfg.emit.allowed;
        self.run_started = Some(Instant::now());
        self.recovery_started = None;
        self.last_recovery_ms = None;
        let _ = self.worker.commands.send(WorkerCommand::Start {
            config: self.chaos.clone(),
            seed: self.seed,
            emit,
            label: self.selected_preset.clone(),
        });
    }

    fn stop(&mut self) {
        let _ = self.worker.commands.send(WorkerCommand::Stop);
        self.armed = false;
    }

    fn audit(&self, name: &str, detail: impl Into<String>) {
        let _ = self.worker.commands.send(WorkerCommand::Audit {
            name: name.to_string(),
            detail: detail.into(),
        });
    }

    fn profile_matches_last_packet(&self) -> Option<bool> {
        if self.latest.clean.packets == 0 {
            return None;
        }
        let packet_ok = self.cfg.stream.packet_bytes
            .map(|expected| self.latest.clean.last_packet_bytes == expected)
            .unwrap_or(true);
        let payload_ok = self.cfg.stream.data_bytes
            .map(|expected| self.latest.clean.last_payload_bytes == expected)
            .unwrap_or(true);
        let sid_ok = self.cfg.stream.stream_id
            .map(|expected| self.latest.clean.sid == Some(expected))
            .unwrap_or(true);
        let class_ok = self.cfg.stream.class_id
            .map(|expected| self.latest.clean.class_id == Some(expected))
            .unwrap_or(true);
        Some(packet_ok && payload_ok && sid_ok && class_ok)
    }

    fn health_state(&self) -> HealthState {
        if self.latest.error.is_some() {
            return HealthState::Fault;
        }
        if self.latest.source.rx_packets == 0 {
            return HealthState::Waiting;
        }
        if matches!(&self.cfg.input, InputMode::Live { .. }) {
            if let Some(last) = self.latest.source.last_rx_epoch {
                let now = Utc::now().timestamp_millis() as f64 / 1000.0;
                if now - last > 2.0 {
                    return HealthState::Fault;
                }
            }
        }
        if self.latest.clean.parse_errors > 0
            || self.latest.source.queue_depth > 512
            || self.profile_matches_last_packet() == Some(false)
        {
            HealthState::Degraded
        } else {
            HealthState::Nominal
        }
    }

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("operator_header")
            .exact_height(82.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(4, 12, 18))
                    .stroke(Stroke::new(1.0, BORDER))
                    .inner_margin(egui::Margin::symmetric(14.0, 9.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    draw_wave_logo(ui);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("VITA-49 RF CHAOS WORKBENCH")
                                .size(18.5)
                                .strong()
                                .color(Color32::from_rgb(112, 229, 236)),
                        );
                        ui.label(
                            RichText::new("FAULT INJECTION  ·  SPECTRUM OBSERVATION  ·  SYSTEM RESILIENCE")
                                .size(9.5)
                                .color(MUTED),
                        );
                    });
                    ui.add_space(20.0);
                    let input_fresh = self.latest.source.rx_packets > 0
                        && match &self.cfg.input {
                            InputMode::Live { .. } => self.latest.source.last_rx_epoch.map(|last| {
                                Utc::now().timestamp_millis() as f64 / 1000.0 - last <= 2.0
                            }).unwrap_or(false),
                            InputMode::Pcap { .. } => !self.latest.source.finished,
                        };
                    header_field(
                        ui,
                        if input_fresh { "RX" } else { "INPUT" },
                        &format!("{}:{}", self.cfg.stream.group, self.cfg.stream.dst_port),
                        if input_fresh { GREEN } else { AMBER },
                    );
                    header_field(ui, "RATE", &format!("{:.0} kSa/s", self.cfg.stream.sample_rate / 1000.0), CYAN);
                    header_field(ui, "INTERFACE", &self.cfg.stream.interface, BLUE);
                    let (mode, mode_color) = match &self.cfg.input {
                        InputMode::Live { .. } => ("LIVE", GREEN),
                        InputMode::Pcap { .. } => ("PCAP REPLAY", BLUE),
                    };
                    status_pill(ui, mode, mode_color);

                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let stop = ui.add_enabled(
                            self.latest.active,
                            egui::Button::new(RichText::new("■  STOP").strong().color(TEXT))
                                .fill(if self.latest.active { Color32::from_rgb(93, 28, 38) } else { PANEL_2 })
                                .stroke(Stroke::new(1.0, if self.latest.active { RED } else { BORDER })),
                        );
                        if stop.clicked() {
                            self.stop();
                        }
                        let run = ui.add_enabled(
                            self.armed && !self.latest.active,
                            egui::Button::new(RichText::new("▶  RUN CHAOS").strong().color(TEXT))
                                .fill(if self.armed && !self.latest.active { Color32::from_rgb(13, 91, 59) } else { PANEL_2 })
                                .stroke(Stroke::new(1.0, GREEN)),
                        );
                        if run.clicked() {
                            self.start();
                        }
                        let arm_label = if self.armed { "DISARM" } else { "ARM" };
                        if ui
                            .add_enabled(
                                !self.latest.active,
                                egui::Button::new(RichText::new(arm_label).strong())
                                    .min_size(Vec2::new(90.0, 30.0))
                                    .fill(PANEL_3)
                                    .stroke(Stroke::new(1.0, if self.armed { AMBER } else { BORDER_HI })),
                            )
                            .clicked()
                        {
                            self.armed = !self.armed;
                            self.audit(
                                if self.armed { "ARM" } else { "DISARM" },
                                format!("preset={} seed={}", self.selected_preset, self.seed),
                            );
                        }
                    });
                });

                ui.add_space(6.0);
                let (label, c) = if self.latest.active {
                    (format!("CHAOS ACTIVE  /  {}", self.selected_preset), RED)
                } else if self.armed {
                    (format!("ARMED  /  {}", self.selected_preset), AMBER)
                } else {
                    ("PASS-THROUGH  /  CLEAN OBSERVATION".to_string(), GREEN)
                };
                ui.horizontal(|ui| {
                    status_strip(ui, &label, c);
                    if self.latest.clean.pps > 0.0 {
                        ui.label(
                            RichText::new(format!(
                                "clean {:.1} pps    chaos {:.1} pps    Δ {:+.1}%",
                                self.latest.clean.pps,
                                self.latest.chaos.pps,
                                pps_delta(self.latest.clean.pps, self.latest.chaos.pps)
                            ))
                            .small()
                            .monospace()
                            .color(MUTED),
                        );
                    } else {
                        ui.label(RichText::new("waiting for validated VITA packets").small().color(MUTED));
                    }
                    if let Some(t0) = self.run_started {
                        ui.separator();
                        ui.monospace(format!("T+{:05.1}s", t0.elapsed().as_secs_f64()));
                    }
                });
            });
    }

    fn bottom_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("operator_footer")
            .exact_height(28.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(4, 11, 16))
                    .stroke(Stroke::new(1.0, BORDER))
                    .inner_margin(egui::Margin::symmetric(10.0, 4.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("CAPTURE").strong().small().color(CYAN));
                    ui.label(
                        RichText::new(self.latest.source.backend.to_uppercase())
                            .monospace()
                            .small()
                            .color(TEXT),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!("{} raw packets", self.latest.source.rx_packets))
                            .monospace()
                            .small()
                            .color(MUTED),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!("{} parse errors", self.latest.clean.parse_errors))
                            .monospace()
                            .small()
                            .color(if self.latest.clean.parse_errors == 0 { MUTED } else { RED }),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!("{} dropped", self.latest.engine.dropped))
                            .monospace()
                            .small()
                            .color(MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(Utc::now().format("%H:%M:%S UTC").to_string())
                                .small()
                                .monospace()
                                .color(MUTED),
                        );
                        ui.separator();
                        if let Some(mem) = self.latest.process.memory_bytes {
                            ui.label(RichText::new(format!("MEM {}", human_bytes(mem))).small().monospace().color(MUTED));
                        }
                        if let Some(cpu) = self.latest.process.cpu_percent {
                            ui.label(RichText::new(format!("CPU {:.0}%", cpu)).small().monospace().color(MUTED));
                        }
                        ui.separator();
                        let (name, color) = health_label(self.health_state());
                        ui.label(RichText::new(name).small().strong().color(color));
                        ui.colored_label(color, "●");
                    });
                });
            });
    }

    fn left_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("operator_controls")
            .resizable(true)
            .default_width(260.0)
            .min_width(230.0)
            .max_width(360.0)
            .frame(egui::Frame::none().fill(Color32::from_rgb(6, 15, 21)).stroke(Stroke::new(1.0, BORDER)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    control_tab(ui, &mut self.control_tab, ControlTab::Experiment, "EXPERIMENT");
                    control_tab(ui, &mut self.control_tab, ControlTab::Protocol, "PROTOCOL");
                    control_tab(ui, &mut self.control_tab, ControlTab::System, "SYSTEM");
                });
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    match self.control_tab {
                        ControlTab::Experiment => {
                            self.experiment_controls(ui);
                            ui.add_space(8.0);
                            self.network_controls(ui);
                            ui.add_space(8.0);
                            self.transport_controls(ui);
                            ui.add_space(8.0);
                            self.protocol_controls(ui);
                        }
                        ControlTab::Protocol => {
                            self.protocol_controls(ui);
                            ui.add_space(8.0);
                            self.signal_controls(ui);
                        }
                        ControlTab::System => {
                            self.system_controls(ui);
                            ui.add_space(8.0);
                            self.capture_details(ui);
                        }
                    }
                    ui.add_space(20.0);
                });
            });
    }

    fn right_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("operator_health_rail")
            .resizable(true)
            .default_width(270.0)
            .min_width(235.0)
            .max_width(380.0)
            .frame(egui::Frame::none().fill(Color32::from_rgb(6, 15, 21)).stroke(Stroke::new(1.0, BORDER)))
            .show(ctx, |ui| {
                panel_box(ui, |ui| self.system_health_panel(ui));
                ui.add_space(8.0);
                panel_box(ui, |ui| self.impact_panel(ui));
                ui.add_space(8.0);
                panel_box(ui, |ui| self.event_log_panel(ui));
            });
    }

    fn experiment_controls(&mut self, ui: &mut egui::Ui) {
        section_heading(ui, "SCENARIO");
        egui::ComboBox::from_id_source("scenario_selector")
            .selected_text(&self.selected_preset)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for name in scenario::NAMES {
                    if ui.selectable_value(&mut self.selected_preset, name.to_string(), name).clicked() {
                        self.chaos = scenario::named(name);
                        self.armed = false;
                        self.audit("PRESET_SELECTED", name.to_string());
                    }
                }
            });
        ui.label(RichText::new(preset_description(&self.selected_preset)).small().color(MUTED));
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Deterministic seed").small().color(MUTED));
            ui.add(egui::DragValue::new(&mut self.seed).speed(1));
            if ui.button("Reload").clicked() {
                self.chaos = scenario::named(&self.selected_preset);
                self.armed = false;
                self.audit("PRESET_RELOADED", self.selected_preset.clone());
            }
        });
        if ui.button("Clear faults").clicked() {
            self.selected_preset = "Clean / pass-through".into();
            self.chaos = ChaosConfig::default();
            self.armed = false;
            self.audit("FAULT_CONFIGURATION_CLEARED", "pass-through configuration restored");
        }
    }

    fn network_controls(&mut self, ui: &mut egui::Ui) {
        section_heading(ui, "NETWORK SAFETY");
        ui.add_enabled_ui(self.cfg.emit.allowed, |ui| {
            ui.checkbox(&mut self.emit_requested, "Enable TEST output");
        });
        if !self.cfg.emit.allowed {
            ui.label(RichText::new("TEST output locked by launch policy").small().color(MUTED));
        }
        ui.add_space(4.0);
        kv(ui, "INPUT", &format!("{}:{}", self.cfg.stream.group, self.cfg.stream.dst_port));
        kv(ui, "TEST", &format!("{}:{}", self.cfg.emit.group, self.cfg.emit.port));
        kv(ui, "IFACE", &self.cfg.stream.interface);
        if let Some(field) = &self.latest.source.payload_field {
            kv(ui, "PAYLOAD", field);
        }
    }

    fn transport_controls(&mut self, ui: &mut egui::Ui) {
        section_heading(ui, "TRANSPORT / FAULT CONTROLS");
        compact_checkbox(ui, &mut self.chaos.transport.blackout, "Total blackout");
        compact_f64(ui, "Random drop", "%", &mut self.chaos.transport.drop_pct, 0.0, 100.0, 0.5);
        compact_checkbox(ui, &mut self.chaos.transport.burst_enabled, "Repeating burst loss");
        ui.horizontal(|ui| {
            ui.label(RichText::new("Burst packets").small().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.add(egui::DragValue::new(&mut self.chaos.transport.burst_every_packets).clamp_range(0..=10_000_000));
                ui.label(RichText::new("every").small().color(MUTED));
                ui.add(egui::DragValue::new(&mut self.chaos.transport.burst_drop_packets).clamp_range(0..=1_000_000));
            });
        });
        compact_f64(ui, "Duplicate", "%", &mut self.chaos.transport.duplicate_pct, 0.0, 100.0, 0.5);
        compact_f64(ui, "Fixed delay", "ms", &mut self.chaos.transport.fixed_delay_ms, 0.0, 60_000.0, 1.0);
        compact_f64(ui, "Jitter σ", "ms", &mut self.chaos.transport.jitter_ms, 0.0, 10_000.0, 0.5);
        compact_f64(ui, "Reorder", "%", &mut self.chaos.transport.reorder_pct, 0.0, 100.0, 0.5);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Reorder window").small().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.add(egui::DragValue::new(&mut self.chaos.transport.reorder_window).clamp_range(2..=128));
            });
        });
        compact_f64(ui, "Throttle", "pps", &mut self.chaos.transport.throttle_pps, 0.0, 1_000_000.0, 10.0);
    }

    fn protocol_controls(&mut self, ui: &mut egui::Ui) {
        section_heading(ui, "VITA-49 PROTOCOL");
        ui.horizontal(|ui| {
            ui.label(RichText::new("Sequence mode").small().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                egui::ComboBox::from_id_source("seq_mode")
                    .selected_text(format!("{:?}", self.chaos.protocol.seq_mode))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.chaos.protocol.seq_mode, SeqMode::Off, "Off");
                        ui.selectable_value(&mut self.chaos.protocol.seq_mode, SeqMode::Jump, "Jump");
                        ui.selectable_value(&mut self.chaos.protocol.seq_mode, SeqMode::Freeze, "Freeze");
                        ui.selectable_value(&mut self.chaos.protocol.seq_mode, SeqMode::Random, "Random");
                    });
            });
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("Sequence jump / cadence").small().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.add(egui::DragValue::new(&mut self.chaos.protocol.seq_every_packets));
                ui.add(egui::DragValue::new(&mut self.chaos.protocol.seq_jump));
            });
        });
        compact_checkbox(ui, &mut self.chaos.protocol.sid_enabled, "Mutate Stream ID");
        ui.horizontal(|ui| {
            ui.label(RichText::new("SID").small().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.add(egui::DragValue::new(&mut self.chaos.protocol.sid_value));
            });
        });
        compact_f64(ui, "Timestamp offset", "ms", &mut self.chaos.protocol.ts_offset_ms, -60_000.0, 60_000.0, 1.0);
        compact_f64(ui, "Timestamp drift", "ppm", &mut self.chaos.protocol.ts_drift_ppm, -1_000_000.0, 1_000_000.0, 1.0);
        compact_f64(ui, "Timestamp jitter", "µs", &mut self.chaos.protocol.ts_jitter_us, 0.0, 1_000_000.0, 1.0);
        compact_checkbox(ui, &mut self.chaos.protocol.ts_freeze, "Freeze timestamps");
        ui.horizontal(|ui| {
            ui.label(RichText::new("Periodic step / cadence").small().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.add(egui::DragValue::new(&mut self.chaos.protocol.ts_step_every_packets));
                ui.add(egui::DragValue::new(&mut self.chaos.protocol.ts_step_ms));
            });
        });
        compact_f64(ui, "Truncate", "%", &mut self.chaos.protocol.truncate_pct, 0.0, 100.0, 0.25);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Truncate bytes").small().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.add(egui::DragValue::new(&mut self.chaos.protocol.truncate_bytes));
            });
        });
        compact_checkbox(ui, &mut self.chaos.protocol.truncate_keep_size, "Keep stale declared packet size");
        compact_f64(ui, "Header bit flip", "%", &mut self.chaos.protocol.header_bitflip_pct, 0.0, 100.0, 0.1);
        compact_f64(ui, "Payload bit flip", "%", &mut self.chaos.protocol.payload_bitflip_pct, 0.0, 100.0, 0.1);
    }

    fn signal_controls(&mut self, ui: &mut egui::Ui) {
        section_heading(ui, "SIGNAL / RF SCENE");
        compact_checkbox(ui, &mut self.chaos.signal.enabled, "Enable signal mutation");
        compact_f64(ui, "Gain", "×", &mut self.chaos.signal.gain, 0.0, 8.0, 0.05);
        compact_f64(ui, "DC offset", "", &mut self.chaos.signal.dc_offset, -1.0e12, 1.0e12, 100.0);
        compact_checkbox(ui, &mut self.chaos.signal.noise_enabled, "AWGN");
        compact_f64(ui, "Target SNR", "dB", &mut self.chaos.signal.noise_snr_db, -40.0, 100.0, 0.5);
        compact_f64(ui, "Clip abs", "", &mut self.chaos.signal.clip_abs, 0.0, 1.0e12, 100.0);
        compact_f64(ui, "Sample dropout", "%", &mut self.chaos.signal.sample_dropout_pct, 0.0, 100.0, 0.25);
        compact_checkbox(ui, &mut self.chaos.signal.tone_enabled, "Tone / spur injection");
        compact_f64(ui, "Tone frequency", "Hz", &mut self.chaos.signal.tone_hz, -20_000_000.0, 20_000_000.0, 100.0);
        compact_f64(ui, "Tone amplitude", "", &mut self.chaos.signal.tone_amp, 0.0, 1.0e12, 100.0);
        ui.separator();
        compact_checkbox(ui, &mut self.chaos.signal.emitter_clone_enabled, "Clone detected emitters");
        ui.horizontal(|ui| {
            ui.label(RichText::new("Copies / source").small().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.add(egui::DragValue::new(&mut self.chaos.signal.emitter_clone_copies).clamp_range(1..=16));
            });
        });
        compact_f64(ui, "Clone spacing", "Hz", &mut self.chaos.signal.emitter_clone_spacing_hz, 1.0, 10_000_000.0, 100.0);
        compact_f64(ui, "Clone gain", "×", &mut self.chaos.signal.emitter_clone_gain, 0.0, 4.0, 0.05);
        compact_f64(ui, "Gain decay", "×", &mut self.chaos.signal.emitter_clone_gain_decay, 0.0, 1.5, 0.02);
        compact_f64(ui, "Detection threshold", "dB", &mut self.chaos.signal.emitter_clone_threshold_db, 3.0, 80.0, 0.5);
        compact_f64(ui, "Frequency dither", "Hz", &mut self.chaos.signal.emitter_clone_jitter_hz, 0.0, 1_000_000.0, 10.0);
        compact_checkbox(ui, &mut self.chaos.signal.emitter_clone_preserve_rms, "Preserve packet RMS");
        compact_checkbox(ui, &mut self.chaos.signal.iq_swap, "I/Q swap");
        compact_checkbox(ui, &mut self.chaos.signal.iq_conjugate, "I/Q conjugate");
        compact_f64(ui, "Phase rotation", "°", &mut self.chaos.signal.phase_deg, -3600.0, 3600.0, 1.0);
    }

    fn system_controls(&mut self, ui: &mut egui::Ui) {
        section_heading(ui, "SYSTEM / CONSUMER");
        compact_f64(ui, "Processing delay", "ms", &mut self.chaos.system.processing_delay_ms, 0.0, 60_000.0, 1.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Consumer stall every").small().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.add(egui::DragValue::new(&mut self.chaos.system.consumer_stall_every));
            });
        });
        compact_f64(ui, "Consumer stall", "ms", &mut self.chaos.system.consumer_stall_ms, 0.0, 60_000.0, 1.0);
    }

    fn capture_details(&self, ui: &mut egui::Ui) {
        section_heading(ui, "CAPTURE / SOURCE");
        kv(ui, "Backend", &self.latest.source.backend);
        kv(ui, "Packets", &self.latest.source.rx_packets.to_string());
        kv(ui, "Bytes", &human_bytes(self.latest.source.rx_bytes));
        kv(ui, "Queue", &self.latest.source.queue_depth.to_string());
        if let Some(field) = &self.latest.source.payload_field {
            kv(ui, "Payload field", field);
        }
        ui.label(RichText::new(&self.latest.source.detail).small().color(MUTED));
    }

    fn system_health_panel(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(section_title("SYSTEM HEALTH"));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                let (name, color) = health_label(self.health_state());
                ui.label(RichText::new(name).strong().color(color));
                ui.colored_label(color, "●");
            });
        });
        ui.add_space(5.0);
        health_kv(ui, "Input rate", optional_rate(self.latest.clean.samples_per_sec, "Sa/s"));
        health_kv(ui, "Output rate", optional_rate(self.latest.chaos.samples_per_sec, "Sa/s"));
        health_kv(ui, "CPU (process)", self.latest.process.cpu_percent.map(|v| format!("{v:.1}%")).unwrap_or_else(|| "—".into()));
        health_kv(ui, "Memory", self.latest.process.memory_bytes.map(human_bytes).unwrap_or_else(|| "—".into()));
        health_kv(ui, "Queue depth", self.latest.source.queue_depth.to_string());
        health_kv(ui, "Dropped packets", self.latest.engine.dropped.to_string());
        health_kv(ui, "Parse errors", if self.latest.clean.packets > 0 { self.latest.clean.parse_errors.to_string() } else { "—".into() });
        health_kv(
            ui,
            "Packet geometry",
            if self.latest.clean.packets == 0 {
                "—".into()
            } else {
                format!("{} B / {} B payload", self.latest.clean.last_packet_bytes, self.latest.clean.last_payload_bytes)
            },
        );
        health_kv(
            ui,
            "Profile match",
            match self.profile_matches_last_packet() {
                Some(true) => "MATCH".into(),
                Some(false) => "MISMATCH".into(),
                None => "—".into(),
            },
        );
    }

    fn impact_panel(&self, ui: &mut egui::Ui) {
        ui.label(section_title("EXPERIMENT IMPACT"));
        let loss = if self.latest.engine.seen > 0 {
            Some(self.latest.engine.dropped as f64 / self.latest.engine.seen as f64)
        } else {
            None
        };
        impact_row(
            ui,
            "Transport loss",
            loss.unwrap_or(0.0),
            loss.map(|v| format!("{:.2}%", v * 100.0)).unwrap_or_else(|| "—".into()),
            CYAN,
        );
        let anomalies = self.latest.chaos.seq_gaps
            + self.latest.chaos.reorders
            + self.latest.chaos.ts_drops
            + self.latest.chaos.glitches;
        let protocol = if self.latest.chaos.packets > 0 {
            Some((anomalies as f64 / self.latest.chaos.packets as f64).clamp(0.0, 1.0))
        } else {
            None
        };
        impact_row(
            ui,
            "Protocol impact",
            protocol.unwrap_or(0.0),
            protocol.map(|v| format!("{:.2}%", v * 100.0)).unwrap_or_else(|| "—".into()),
            BLUE,
        );
        let spec = self.latest.analysis.comparison.spectral_delta_db_rms;
        impact_row(
            ui,
            "Signal Δ",
            spec.map(|v| (v / 30.0).clamp(0.0, 1.0)).unwrap_or(0.0),
            spec.map(|v| format!("{v:.2} dB rms")).unwrap_or_else(|| "—".into()),
            MAGENTA,
        );
        let corr = self.latest.analysis.comparison.waveform_correlation;
        let decor = corr.map(|v| (1.0 - v).clamp(0.0, 1.0));
        impact_row(
            ui,
            "Waveform decorrelation",
            decor.unwrap_or(0.0),
            decor.map(|v| format!("{:.2}%", v * 100.0)).unwrap_or_else(|| "—".into()),
            GREEN,
        );
    }

    fn event_log_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(section_title("EVENT LOG"));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                egui::ComboBox::from_id_source("event_filter")
                    .selected_text(match self.event_filter {
                        EventFilter::All => "All events",
                        EventFilter::Info => "Info",
                        EventFilter::Warnings => "Warnings",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.event_filter, EventFilter::All, "All events");
                        ui.selectable_value(&mut self.event_filter, EventFilter::Info, "Info");
                        ui.selectable_value(&mut self.event_filter, EventFilter::Warnings, "Warnings");
                    });
            });
        });
        ui.separator();
        egui::ScrollArea::vertical().max_height(330.0).stick_to_bottom(true).show(ui, |ui| {
            for event in self.latest.events.iter().rev().take(120).rev() {
                if !event_visible(event, self.event_filter) {
                    continue;
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(event_time(event.wall_time)).small().monospace().color(MUTED));
                    let (lvl, color) = event_level(&event.name);
                    ui.label(RichText::new(lvl).small().strong().color(color));
                    ui.label(RichText::new(short_event_name(&event.name)).small().color(TEXT));
                });
                if !event.detail.is_empty() && event.detail != "observed" {
                    ui.label(RichText::new(&event.detail).small().color(MUTED));
                }
                ui.separator();
            }
        });
    }

    fn metrics_row(&self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let cards = 6.0;
        let w = ((width - 5.0 * 7.0) / cards).max(115.0);
        ui.horizontal(|ui| {
            metric_card(ui, w, "CLEAN PPS", value_or_dash(self.latest.clean.pps, self.latest.clean.packets > 0, 1), GREEN, &self.clean_pps_history);
            metric_card(ui, w, "CHAOS PPS", value_or_dash(self.latest.chaos.pps, self.latest.chaos.packets > 0, 1), BLUE, &self.chaos_pps_history);
            metric_card(ui, w, "PPS DELTA", if self.latest.clean.packets > 0 { format!("{:+.1}%", pps_delta(self.latest.clean.pps, self.latest.chaos.pps)) } else { "—".into() }, CYAN, &self.delta_history);
            metric_card(ui, w, "SEQ GAPS", if self.latest.chaos.packets > 0 { self.latest.chaos.seq_gaps.to_string() } else { "—".into() }, if self.latest.chaos.seq_gaps == 0 { BLUE } else { AMBER }, &self.seq_history);
            metric_card(ui, w, "TS / GLITCH", if self.latest.chaos.packets > 0 { format!("{} / {}", self.latest.chaos.ts_drops, self.latest.chaos.glitches) } else { "—".into() }, if self.latest.chaos.ts_drops + self.latest.chaos.glitches == 0 { BLUE } else { AMBER }, &self.timing_history);
            metric_card(ui, w, "PARSE ERR", if self.latest.chaos.packets > 0 { self.latest.chaos.parse_errors.to_string() } else { "—".into() }, if self.latest.chaos.parse_errors == 0 { BLUE } else { RED }, &self.parse_history);
        });
    }

    fn monitor_view(&mut self, ui: &mut egui::Ui) {
        self.analysis_tabs(ui);
        ui.add_space(6.0);
        panel_box(ui, |ui| self.waterfall_panel(ui));
        ui.add_space(7.0);
        ui.columns(2, |cols| {
            panel_box(&mut cols[0], |ui| self.spectrum_panel(ui));
            panel_box(&mut cols[1], |ui| self.waveform_panel(ui));
        });
        ui.add_space(7.0);
        ui.columns(2, |cols| {
            panel_box(&mut cols[0], |ui| self.source_table_panel(ui));
            panel_box(&mut cols[1], |ui| self.comparison_panel(ui));
        });
    }

    fn analysis_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            workspace_tab(ui, &mut self.workspace, Workspace::Monitor, "SPECTRUM / WATERFALL");
            workspace_tab(ui, &mut self.workspace, Workspace::Live, "LIVE ANALYSIS");
            workspace_tab(ui, &mut self.workspace, Workspace::Health, "HEALTH + SCORECARD");
            workspace_tab(ui, &mut self.workspace, Workspace::Events, "FAULT EVENTS");
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new("Scale").small().color(MUTED));
                status_pill(ui, "dBFS", BLUE);
                ui.label(RichText::new(format!("RBW {:.1} Hz", self.latest.analysis.rbw_hz)).small().monospace().color(MUTED));
                ui.label(RichText::new(format!("Span {:.1} kHz", self.latest.analysis.span_khz)).small().monospace().color(MUTED));
                ui.label(RichText::new(format!("Center {:.1} kHz", self.latest.analysis.center_khz)).small().monospace().color(MUTED));
            });
        });
    }

    fn waterfall_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(section_title("WATERFALL"));
            ui.label(RichText::new("frequency →  ·  history ↓  ·  orange row = fault boundary").small().color(MUTED));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Clear").clicked() {
                    self.waterfall.clear();
                    self.waterfall_active.clear();
                    self.waterfall_boundary.clear();
                    self.waterfall_tex = None;
                }
                ui.checkbox(&mut self.freeze_waterfall, "Hold");
                ui.label(RichText::new("lines").small().color(MUTED));
                ui.add(egui::DragValue::new(&mut self.waterfall_history_cols).clamp_range(60..=600));
            });
        });
        ui.add_space(5.0);
        let available = ui.available_width();
        let legend_w = 42.0;
        ui.horizontal(|ui| {
            if let Some(tex) = &self.waterfall_tex {
                egui::Frame::none().fill(Color32::BLACK).stroke(Stroke::new(1.0, BORDER_HI)).show(ui, |ui| {
                    ui.add(egui::Image::new((tex.id(), egui::vec2((available - legend_w - 8.0).max(200.0), 178.0))));
                });
            } else {
                egui::Frame::none().fill(Color32::BLACK).stroke(Stroke::new(1.0, BORDER_HI)).show(ui, |ui| {
                    ui.allocate_ui(egui::vec2((available - legend_w - 8.0).max(200.0), 178.0), |ui| {
                        ui.centered_and_justified(|ui| {
                            ui.label(RichText::new("WAITING FOR VALIDATED VITA SAMPLE DATA").small().color(MUTED));
                        });
                    });
                });
            }
            draw_color_scale(ui, self.waterfall_floor, self.waterfall_ceil, 178.0);
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new(if self.cfg.stream.iq { format!("{:+.1} kHz", -self.latest.analysis.span_khz / 2.0) } else { "0 kHz".into() }).small().monospace().color(MUTED));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                let maxf = if self.cfg.stream.iq { self.latest.analysis.span_khz / 2.0 } else { self.latest.analysis.span_khz };
                ui.label(RichText::new(format!("{maxf:+.1} kHz")).small().monospace().color(MUTED));
                ui.label(RichText::new("Frequency").small().color(MUTED));
            });
        });
    }

    fn spectrum_panel(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(section_title("SPECTRUM"));
            ui.label(RichText::new("live / dBFS").small().color(MUTED));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(format!("RBW {:.1} Hz", self.latest.analysis.rbw_hz)).small().monospace().color(MUTED));
            });
        });
        let clean = &self.latest.analysis.clean_spectrum;
        let chaos = &self.latest.analysis.chaos_spectrum;
        let ghosts = self.planned_ghosts();
        let noise_floor = median_finite(&clean.dbfs);
        Plot::new("operator_spectrum")
            .height(210.0)
            .legend(Legend::default())
            .x_axis_label("Frequency (kHz)")
            .y_axis_label("Magnitude (dBFS)")
            .include_y(-120.0)
            .include_y(0.0)
            .show(ui, |p| {
                if self.show_chaos && !chaos.dbfs.is_empty() {
                    p.line(Line::new(PlotPoints::from_iter(chaos.freq_khz.iter().zip(chaos.dbfs.iter()).map(|(&x, &y)| [x, y]))).name("Chaos").color(CYAN));
                }
                if self.show_clean && !clean.dbfs.is_empty() {
                    p.line(Line::new(PlotPoints::from_iter(clean.freq_khz.iter().zip(clean.dbfs.iter()).map(|(&x, &y)| [x, y]))).name("Clean").color(GREEN));
                }
                if let Some(nf) = noise_floor {
                    if let (Some(x0), Some(x1)) = (clean.freq_khz.first(), clean.freq_khz.last()) {
                        p.line(Line::new(PlotPoints::from(vec![[*x0, nf], [*x1, nf]])).name("Noise floor").color(Color32::from_gray(145)));
                    }
                }
                for e in &self.latest.analysis.emitters {
                    p.line(Line::new(PlotPoints::from(vec![[e.frequency_khz, -120.0], [e.frequency_khz, 0.0]])).color(BLUE));
                }
                for g in ghosts.iter().take(24) {
                    p.line(Line::new(PlotPoints::from(vec![[g.target_khz, -120.0], [g.target_khz, 0.0]])).color(MAGENTA));
                }
            });
    }

    fn waveform_panel(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(section_title("TIME DOMAIN WAVEFORM"));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(format!("{:.1} ms", self.waveform_ms)).small().monospace().color(MUTED));
            });
        });
        let full = full_scale(self.cfg.format).max(1.0);
        let clean = normalized_waveform(&self.latest.clean_recent, self.cfg.stream.sample_rate, self.waveform_ms, full, 900);
        let chaos = normalized_waveform(&self.latest.chaos_recent, self.cfg.stream.sample_rate, self.waveform_ms, full, 900);
        Plot::new("operator_waveform")
            .height(210.0)
            .legend(Legend::default())
            .x_axis_label("Time (ms)")
            .y_axis_label("Normalized amplitude")
            .include_y(-1.05)
            .include_y(1.05)
            .show(ui, |p| {
                if self.show_chaos && !chaos.is_empty() {
                    p.line(Line::new(PlotPoints::from(chaos)).name("Chaos").color(CYAN));
                }
                if self.show_clean && !clean.is_empty() {
                    p.line(Line::new(PlotPoints::from(clean)).name("Clean").color(GREEN));
                }
            });
    }

    fn source_table_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(section_title("EMITTER / SOURCE TABLE"));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(format!("{} sources", self.latest.analysis.emitters.len())).small().monospace().color(MUTED));
            });
        });
        ui.horizontal(|ui| {
            source_tab(ui, &mut self.source_table_tab, SourceTableTab::Active, "ACTIVE SOURCES");
            source_tab(ui, &mut self.source_table_tab, SourceTableTab::Planned, "PLANNED (GHOSTS)");
            source_tab(ui, &mut self.source_table_tab, SourceTableTab::Suppressed, "SUPPRESSED");
        });
        ui.separator();
        egui::ScrollArea::vertical().max_height(185.0).show(ui, |ui| {
            match self.source_table_tab {
                SourceTableTab::Active => self.active_source_table(ui),
                SourceTableTab::Planned => self.planned_source_table(ui),
                SourceTableTab::Suppressed => {
                    ui.label(RichText::new("No suppression engine is configured; no suppressed-source data exists.").small().color(MUTED));
                }
            }
        });
    }

    fn active_source_table(&self, ui: &mut egui::Ui) {
        egui::Grid::new("active_sources_grid").striped(true).min_col_width(58.0).show(ui, |ui| {
            ui.strong("#");
            ui.strong("Frequency (kHz)");
            ui.strong("Level (dBFS)");
            ui.strong("Type");
            ui.strong("State");
            ui.strong("Age (s)");
            ui.end_row();
            for e in self.latest.analysis.emitters.iter().take(24) {
                ui.label(RichText::new("●").color(CYAN));
                ui.monospace(format!("{:+.3}", e.frequency_khz));
                ui.monospace(format!("{:.1}", e.level_dbfs));
                ui.label("detected");
                ui.label(RichText::new("active").color(GREEN));
                ui.monospace(format!("{:.1}", e.age_s));
                ui.end_row();
            }
        });
        if self.latest.analysis.emitters.is_empty() {
            ui.label(RichText::new("No spectral sources meet the live detection threshold.").small().color(MUTED));
        }
    }

    fn planned_source_table(&self, ui: &mut egui::Ui) {
        let ghosts = self.planned_ghosts();
        egui::Grid::new("planned_sources_grid").striped(true).min_col_width(60.0).show(ui, |ui| {
            ui.strong("Source #");
            ui.strong("Source (kHz)");
            ui.strong("Target (kHz)");
            ui.strong("Estimated level");
            ui.strong("State");
            ui.end_row();
            for g in ghosts.iter().take(32) {
                ui.monospace(g.source_id.to_string());
                ui.monospace(format!("{:+.3}", g.source_khz));
                ui.monospace(format!("{:+.3}", g.target_khz));
                ui.monospace(format!("{:.1} dBFS", g.level_dbfs));
                ui.label(RichText::new(if self.chaos.signal.emitter_clone_enabled { "configured" } else { "inactive" }).color(if self.chaos.signal.emitter_clone_enabled { MAGENTA } else { MUTED }));
                ui.end_row();
            }
        });
        if ghosts.is_empty() {
            ui.label(RichText::new("No ghost targets can be planned until a real source is detected and cloning is configured.").small().color(MUTED));
        }
    }

    fn comparison_panel(&self, ui: &mut egui::Ui) {
        ui.label(section_title("COMPARISON METRICS"));
        egui::Grid::new("comparison_grid").striped(true).min_col_width(76.0).show(ui, |ui| {
            ui.strong("Metric");
            ui.strong("Clean");
            ui.strong("Chaos");
            ui.strong("Delta");
            ui.end_row();
            comparison_row(ui, "Total Power (dBFS)", self.latest.analysis.comparison.clean.total_power_dbfs, self.latest.analysis.comparison.chaos.total_power_dbfs, "dB");
            comparison_row(ui, "Occupied BW (kHz)", self.latest.analysis.comparison.clean.occupied_bw_khz, self.latest.analysis.comparison.chaos.occupied_bw_khz, "kHz");
            comparison_row(ui, "Peak (dBFS)", self.latest.analysis.comparison.clean.peak_dbfs, self.latest.analysis.comparison.chaos.peak_dbfs, "dB");
            comparison_row(ui, "Spectral Flatness", self.latest.analysis.comparison.clean.spectral_flatness, self.latest.analysis.comparison.chaos.spectral_flatness, "");
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            counter_chip(ui, "seen", self.latest.engine.seen);
            counter_chip(ui, "emitted", self.latest.engine.emitted);
            counter_chip(ui, "dropped", self.latest.engine.dropped);
            counter_chip(ui, "mutated", self.latest.engine.mutated);
            counter_chip(ui, "reordered", self.latest.engine.reordered);
            counter_chip(ui, "ghosts", self.latest.engine.emitter_clones);
        });
    }

    fn planned_ghosts(&self) -> Vec<PlannedGhost> {
        planned_ghosts_from_emitters(
            &self.latest.analysis.emitters,
            &self.chaos.signal,
            self.cfg.stream.sample_rate,
            self.cfg.stream.iq,
        )
    }

    fn health_view(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("CLEAN ↔ CHAOS HEALTH SCORECARD").size(15.0).strong().color(TEXT));
        ui.add_space(7.0);
        panel_box(ui, |ui| {
            egui::Grid::new("health_scorecard_grid").striped(true).min_col_width(125.0).show(ui, |ui| {
                ui.strong("Metric");
                ui.strong("Clean");
                ui.strong("Chaos");
                ui.strong("Delta / state");
                ui.end_row();
                health_metric_row(ui, "Packets/s", Some(self.latest.clean.pps), Some(self.latest.chaos.pps));
                health_metric_row(ui, "Samples/s", Some(self.latest.clean.samples_per_sec), Some(self.latest.chaos.samples_per_sec));
                health_metric_row(ui, "Mbps", Some(self.latest.clean.mbps), Some(self.latest.chaos.mbps));
                health_count_row(ui, "Sequence gaps", self.latest.clean.seq_gaps, self.latest.chaos.seq_gaps);
                health_count_row(ui, "Reorders", self.latest.clean.reorders, self.latest.chaos.reorders);
                health_count_row(ui, "Timestamp drops", self.latest.clean.ts_drops, self.latest.chaos.ts_drops);
                health_count_row(ui, "Timing glitches", self.latest.clean.glitches, self.latest.chaos.glitches);
                health_count_row(ui, "Parse errors", self.latest.clean.parse_errors, self.latest.chaos.parse_errors);
                ui.label("Stream ID");
                ui.monospace(opt_hex32(self.latest.clean.sid));
                ui.monospace(opt_hex32(self.latest.chaos.sid));
                ui.label(if self.latest.clean.sid == self.latest.chaos.sid { "same" } else { "changed" });
                ui.end_row();
                ui.label("Class ID");
                ui.monospace(opt_hex64(self.latest.clean.class_id));
                ui.monospace(opt_hex64(self.latest.chaos.class_id));
                ui.label(if self.latest.clean.class_id == self.latest.chaos.class_id { "same" } else { "changed" });
                ui.end_row();
            });
        });
        ui.add_space(8.0);
        ui.columns(2, |cols| {
            panel_box(&mut cols[0], |ui| {
                ui.label(section_title("CAPTURE BACKEND"));
                kv(ui, "Backend", &self.latest.source.backend);
                kv(ui, "Detail", &self.latest.source.detail);
                kv(ui, "Packets", &self.latest.source.rx_packets.to_string());
                kv(ui, "Bytes", &human_bytes(self.latest.source.rx_bytes));
                kv(ui, "Queue", &self.latest.source.queue_depth.to_string());
            });
            panel_box(&mut cols[1], |ui| {
                ui.label(section_title("PROCESS / EXPERIMENT"));
                health_kv(ui, "CPU", self.latest.process.cpu_percent.map(|v| format!("{v:.1}%")).unwrap_or_else(|| "—".into()));
                health_kv(ui, "Memory", self.latest.process.memory_bytes.map(human_bytes).unwrap_or_else(|| "—".into()));
                health_kv(ui, "Log", self.latest.log_path.clone().unwrap_or_else(|| "idle".into()));
                if let Some(ms) = self.last_recovery_ms {
                    health_kv(ui, "Last recovery", format!("{ms:.0} ms"));
                }
            });
        });
    }

    fn events_view(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(format!("FAULT EVENTS / {}", self.latest.events.len())).size(15.0).strong().color(TEXT));
        ui.add_space(7.0);
        panel_box(ui, |ui| {
            egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                for e in self.latest.events.iter().rev().take(1000).rev() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(event_time(e.wall_time)).small().monospace().color(MUTED));
                        let (lvl, color) = event_level(&e.name);
                        ui.label(RichText::new(lvl).small().strong().color(color));
                        ui.label(RichText::new(&e.name).small().strong().color(TEXT));
                        ui.label(RichText::new(&e.detail).small().monospace().color(MUTED));
                    });
                    ui.separator();
                }
            });
        });
    }
}

impl eframe::App for WorkbenchApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_updates(ctx);
        self.top_bar(ctx);
        self.bottom_bar(ctx);
        self.left_panel(ctx);
        self.right_panel(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin::same(8.0)))
            .show(ctx, |ui| {
                self.metrics_row(ui);
                ui.add_space(7.0);
                if let Some(err) = &self.latest.error {
                    egui::Frame::none()
                        .fill(Color32::from_rgb(68, 18, 25))
                        .stroke(Stroke::new(1.0, RED))
                        .rounding(4.0)
                        .inner_margin(egui::Margin::same(7.0))
                        .show(ui, |ui| {
                            ui.label(RichText::new(format!("CAPTURE / WORKER ERROR: {err}")).strong().color(Color32::from_rgb(255, 174, 181)));
                        });
                    ui.add_space(6.0);
                }
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    match self.workspace {
                        Workspace::Monitor | Workspace::Live => self.monitor_view(ui),
                        Workspace::Health => self.health_view(ui),
                        Workspace::Events => self.events_view(ui),
                    }
                });
            });
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.worker.commands.send(WorkerCommand::Shutdown);
    }
}

fn install_system_fonts(ctx: &egui::Context) {
    // Do not bundle proprietary or environment-specific font assets. Prefer a
    // locally installed condensed/technical sans face when RS5 provides one,
    // otherwise retain egui's built-in font set.
    let candidates = [
        "/usr/share/fonts/dejavu/DejaVuSansCondensed.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    ];
    if let Some((_, bytes)) = candidates
        .iter()
        .find_map(|path| fs::read(path).ok().map(|bytes| (*path, bytes)))
    {
        let mut fonts = FontDefinitions::default();
        let key = "operator-sans".to_string();
        fonts.font_data.insert(key.clone(), FontData::from_owned(bytes));
        if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
            family.insert(0, key);
        }
        ctx.set_fonts(fonts);
    }
}

fn install_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = Color32::from_rgb(3, 9, 13);
    style.visuals.faint_bg_color = PANEL_2;
    style.visuals.code_bg_color = PANEL_2;
    style.visuals.widgets.noninteractive.bg_fill = PANEL;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.inactive.bg_fill = PANEL_2;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(14, 39, 51);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_HI);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(15, 49, 61);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, CYAN);
    style.visuals.selection.bg_fill = Color32::from_rgb(11, 67, 84);
    style.visuals.selection.stroke = Stroke::new(1.0, CYAN);
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size.y = 24.0;
    style.visuals.window_rounding = 3.0.into();
    ctx.set_style(style);
}

fn draw_wave_logo(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(44.0, 37.0), Sense::hover());
    let pts = [
        (0.0, 18.0), (7.0, 18.0), (10.0, 11.0), (14.0, 27.0), (19.0, 3.0),
        (24.0, 34.0), (29.0, 12.0), (33.0, 22.0), (37.0, 18.0), (44.0, 18.0),
    ];
    let points: Vec<_> = pts.iter().map(|(x, y)| egui::pos2(rect.left() + *x, rect.top() + *y)).collect();
    ui.painter().add(egui::Shape::line(points, Stroke::new(1.8, CYAN)));
}

fn header_field(ui: &mut egui::Ui, label: &str, value: &str, color: Color32) {
    egui::Frame::none().fill(PANEL).stroke(Stroke::new(1.0, BORDER)).rounding(4.0).inner_margin(egui::Margin::symmetric(9.0, 5.0)).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(label).size(8.5).color(MUTED));
            ui.label(RichText::new(value).small().monospace().color(color));
        });
    });
}

fn status_pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::none().fill(PANEL_2).stroke(Stroke::new(1.0, color)).rounding(4.0).inner_margin(egui::Margin::symmetric(8.0, 4.0)).show(ui, |ui| {
        ui.label(RichText::new(text).small().strong().color(color));
    });
}

fn status_strip(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::none().fill(color.gamma_multiply(0.12)).stroke(Stroke::new(1.0, color.gamma_multiply(0.55))).rounding(4.0).inner_margin(egui::Margin::symmetric(8.0, 3.0)).show(ui, |ui| {
        ui.label(RichText::new(text).small().strong().color(color));
    });
}

fn control_tab(ui: &mut egui::Ui, selected: &mut ControlTab, value: ControlTab, label: &str) {
    let active = *selected == value;
    if ui.add(egui::SelectableLabel::new(active, RichText::new(label).small().strong())).clicked() {
        *selected = value;
    }
}

fn workspace_tab(ui: &mut egui::Ui, selected: &mut Workspace, value: Workspace, label: &str) {
    let active = *selected == value;
    let text = RichText::new(label).small().strong().color(if active { CYAN } else { MUTED });
    if ui.add(egui::SelectableLabel::new(active, text)).clicked() {
        *selected = value;
    }
}

fn source_tab(ui: &mut egui::Ui, selected: &mut SourceTableTab, value: SourceTableTab, label: &str) {
    let active = *selected == value;
    let text = RichText::new(label).size(9.0).strong().color(if active { CYAN } else { MUTED });
    if ui.add(egui::SelectableLabel::new(active, text)).clicked() {
        *selected = value;
    }
}

fn panel_box<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none().fill(PANEL).stroke(Stroke::new(1.0, BORDER)).rounding(4.0).inner_margin(egui::Margin::same(8.0)).show(ui, add).inner
}

fn section_title(text: &str) -> RichText {
    RichText::new(text).size(11.5).strong().color(Color32::from_rgb(147, 219, 238))
}

fn section_heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(3.0);
    ui.label(section_title(text));
    ui.separator();
}

fn kv(ui: &mut egui::Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).size(9.0).color(MUTED));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).small().monospace().color(TEXT));
        });
    });
}

fn health_kv(ui: &mut egui::Ui, key: &str, value: String) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).small().color(MUTED));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).small().monospace().color(TEXT));
        });
    });
    ui.separator();
}

fn compact_checkbox(ui: &mut egui::Ui, value: &mut bool, label: &str) {
    ui.checkbox(value, RichText::new(label).small().color(TEXT));
}

fn compact_f64(ui: &mut egui::Ui, label: &str, unit: &str, value: &mut f64, min: f64, max: f64, speed: f64) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).small().color(TEXT));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            if !unit.is_empty() {
                ui.label(RichText::new(unit).small().color(MUTED));
            }
            ui.add(egui::DragValue::new(value).speed(speed).clamp_range(min..=max));
        });
    });
}

fn metric_card(ui: &mut egui::Ui, width: f32, label: &str, value: String, color: Color32, history: &VecDeque<f64>) {
    egui::Frame::none().fill(PANEL_2).stroke(Stroke::new(1.0, BORDER)).rounding(5.0).inner_margin(egui::Margin::symmetric(9.0, 6.0)).show(ui, |ui| {
        ui.set_width(width);
        ui.label(RichText::new(label).size(8.5).strong().color(MUTED));
        ui.horizontal(|ui| {
            ui.label(RichText::new(value).size(19.0).strong().monospace().color(color));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                draw_sparkline(ui, history, color, egui::vec2((width * 0.42).max(35.0), 22.0));
            });
        });
    });
}

fn draw_sparkline(ui: &mut egui::Ui, values: &VecDeque<f64>, color: Color32, size: Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if values.len() < 2 {
        ui.painter().line_segment([rect.left_center(), rect.right_center()], Stroke::new(1.0, BORDER));
        return;
    }
    let min = values.iter().copied().filter(|v| v.is_finite()).fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().filter(|v| v.is_finite()).fold(f64::NEG_INFINITY, f64::max);
    let span = (max - min).abs().max(1e-9);
    let n = values.len().max(2) - 1;
    let points: Vec<_> = values.iter().enumerate().map(|(i, v)| {
        let x = rect.left() + rect.width() * i as f32 / n as f32;
        let t = ((*v - min) / span).clamp(0.0, 1.0) as f32;
        let y = rect.bottom() - rect.height() * t;
        egui::pos2(x, y)
    }).collect();
    ui.painter().add(egui::Shape::line(points, Stroke::new(1.2, color.gamma_multiply(0.85))));
}

fn draw_color_scale(ui: &mut egui::Ui, floor: f64, ceil: f64, height: f32) {
    let width = 14.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width + 30.0, height), Sense::hover());
    let bar = egui::Rect::from_min_size(rect.min, egui::vec2(width, height));
    let steps = 64;
    for i in 0..steps {
        let y0 = bar.top() + bar.height() * i as f32 / steps as f32;
        let y1 = bar.top() + bar.height() * (i + 1) as f32 / steps as f32;
        let t = 1.0 - i as f64 / (steps - 1) as f64;
        ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(bar.left(), y0), egui::pos2(bar.right(), y1)), 0.0, operator_colormap(t));
    }
    ui.painter().rect_stroke(bar, 0.0, Stroke::new(1.0, BORDER_HI));
    let font = FontId::monospace(8.0);
    for i in 0..=4 {
        let t = i as f64 / 4.0;
        let y = bar.top() + bar.height() * t as f32;
        let v = ceil + (floor - ceil) * t;
        ui.painter().text(egui::pos2(bar.right() + 4.0, y), egui::Align2::LEFT_CENTER, format!("{v:.0}"), font.clone(), MUTED);
    }
}

fn impact_row(ui: &mut egui::Ui, label: &str, fraction: f64, value_text: String, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).small().color(MUTED));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value_text).small().monospace().color(TEXT));
            ui.add(egui::ProgressBar::new(fraction.clamp(0.0, 1.0) as f32).desired_width(92.0).fill(color));
        });
    });
}

fn comparison_row(ui: &mut egui::Ui, name: &str, clean: Option<f64>, chaos: Option<f64>, unit: &str) {
    ui.label(name);
    ui.monospace(format_opt(clean));
    ui.monospace(format_opt(chaos));
    let delta = match (clean, chaos) { (Some(a), Some(b)) => Some(b - a), _ => None };
    let txt = delta.map(|v| if unit.is_empty() { format!("{v:+.3}") } else { format!("{v:+.2}") }).unwrap_or_else(|| "—".into());
    ui.label(RichText::new(txt).monospace().color(delta.map(|v| if v.abs() < 1e-9 { MUTED } else if v > 0.0 { ORANGE } else { CYAN }).unwrap_or(MUTED)));
    ui.end_row();
}

fn counter_chip(ui: &mut egui::Ui, label: &str, value: u64) {
    egui::Frame::none().fill(PANEL_2).stroke(Stroke::new(1.0, BORDER)).rounding(3.0).inner_margin(egui::Margin::symmetric(6.0, 3.0)).show(ui, |ui| {
        ui.label(RichText::new(format!("{label} {value}")).small().monospace().color(MUTED));
    });
}

fn health_metric_row(ui: &mut egui::Ui, label: &str, clean: Option<f64>, chaos: Option<f64>) {
    ui.label(label);
    ui.monospace(format_opt(clean));
    ui.monospace(format_opt(chaos));
    let d = match (clean, chaos) { (Some(a), Some(b)) => Some(b - a), _ => None };
    ui.monospace(d.map(|v| format!("{v:+.3}")).unwrap_or_else(|| "—".into()));
    ui.end_row();
}

fn health_count_row(ui: &mut egui::Ui, label: &str, clean: u64, chaos: u64) {
    ui.label(label);
    ui.monospace(clean.to_string());
    ui.monospace(chaos.to_string());
    ui.monospace(format!("{:+}", chaos as i128 - clean as i128));
    ui.end_row();
}

fn planned_ghosts_from_emitters(emitters: &[EmitterObservation], cfg: &SignalConfig, fs: f64, iq: bool) -> Vec<PlannedGhost> {
    if emitters.is_empty() || cfg.emitter_clone_copies == 0 {
        return Vec::new();
    }
    let lo_hz = if iq { -fs / 2.0 } else { 0.0 };
    let hi_hz = fs / 2.0;
    let mut out = Vec::new();
    for source in emitters.iter().take(cfg.emitter_clone_max_emitters.max(1)) {
        for copy in 0..cfg.emitter_clone_copies {
            let step = copy + 1;
            let sign = if cfg.emitter_clone_bidirectional && copy % 2 == 1 { -1.0 } else { 1.0 };
            let ordinal = if cfg.emitter_clone_bidirectional { (copy / 2 + 1) as f64 } else { step as f64 };
            let target_hz = source.frequency_khz * 1000.0 + sign * ordinal * cfg.emitter_clone_spacing_hz;
            if target_hz < lo_hz || target_hz > hi_hz {
                continue;
            }
            let gain = (cfg.emitter_clone_gain * cfg.emitter_clone_gain_decay.powi(copy as i32)).max(1e-12);
            out.push(PlannedGhost {
                source_id: source.id,
                source_khz: source.frequency_khz,
                target_khz: target_hz / 1000.0,
                level_dbfs: source.level_dbfs + 20.0 * gain.log10(),
            });
        }
    }
    out
}

fn normalized_waveform(samples: &[num_complex::Complex64], fs: f64, window_ms: f64, full_scale: f64, max_points: usize) -> Vec<[f64; 2]> {
    if samples.is_empty() || fs <= 0.0 {
        return Vec::new();
    }
    let want = ((fs * window_ms / 1000.0) as usize).max(16).min(samples.len());
    let start = samples.len() - want;
    let stride = (want / max_points.max(1)).max(1);
    samples[start..]
        .iter()
        .step_by(stride)
        .enumerate()
        .map(|(i, z)| [i as f64 * stride as f64 / fs * 1000.0, z.re / full_scale])
        .collect()
}

fn operator_colormap(t: f64) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let stops = [
        (0.00, (2, 10, 32)),
        (0.16, (0, 41, 111)),
        (0.34, (0, 113, 177)),
        (0.53, (0, 205, 199)),
        (0.70, (66, 221, 145)),
        (0.84, (224, 222, 71)),
        (0.94, (255, 135, 38)),
        (1.00, (255, 236, 185)),
    ];
    for pair in stops.windows(2) {
        let (a_t, a) = pair[0];
        let (b_t, b) = pair[1];
        if t <= b_t {
            let u = ((t - a_t) / (b_t - a_t)).clamp(0.0, 1.0);
            return Color32::from_rgb(
                lerp_u8(a.0, b.0, u),
                lerp_u8(a.1, b.1, u),
                lerp_u8(a.2, b.2, u),
            );
        }
    }
    Color32::WHITE
}

fn lerp_u8(a: u8, b: u8, t: f64) -> u8 {
    (a as f64 + (b as f64 - a as f64) * t).round().clamp(0.0, 255.0) as u8
}

fn push_history(dst: &mut VecDeque<f64>, value: f64, max: usize) {
    if dst.len() >= max {
        dst.pop_front();
    }
    dst.push_back(value);
}

fn pps_delta(clean: f64, chaos: f64) -> f64 {
    if clean.abs() < 1e-9 { 0.0 } else { (chaos / clean - 1.0) * 100.0 }
}

fn value_or_dash(value: f64, valid: bool, decimals: usize) -> String {
    if !valid { return "—".into(); }
    match decimals {
        0 => format!("{value:.0}"),
        1 => format!("{value:.1}"),
        2 => format!("{value:.2}"),
        _ => value.to_string(),
    }
}

fn format_opt(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.3}")).unwrap_or_else(|| "—".into())
}

fn human_bytes(bytes: u64) -> String {
    const K: f64 = 1024.0;
    let b = bytes as f64;
    if b >= K * K * K { format!("{:.2} GiB", b / (K * K * K)) }
    else if b >= K * K { format!("{:.1} MiB", b / (K * K)) }
    else if b >= K { format!("{:.1} KiB", b / K) }
    else { format!("{bytes} B") }
}

fn optional_rate(v: f64, unit: &str) -> String {
    if v <= 0.0 { "—".into() }
    else if v >= 1_000_000.0 { format!("{:.2} M{unit}", v / 1_000_000.0) }
    else if v >= 1_000.0 { format!("{:.2} k{unit}", v / 1_000.0) }
    else { format!("{v:.1} {unit}") }
}

fn health_label(state: HealthState) -> (&'static str, Color32) {
    match state {
        HealthState::Waiting => ("WAITING", AMBER),
        HealthState::Nominal => ("NOMINAL", GREEN),
        HealthState::Degraded => ("DEGRADED", AMBER),
        HealthState::Fault => ("FAULT", RED),
    }
}

fn median_finite(values: &[f64]) -> Option<f64> {
    let mut v: Vec<_> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() { return None; }
    v.sort_by(|a, b| a.total_cmp(b));
    let m = v.len() / 2;
    Some(if v.len() % 2 == 0 { (v[m - 1] + v[m]) * 0.5 } else { v[m] })
}

fn event_time(epoch: f64) -> String {
    if !epoch.is_finite() || epoch <= 0.0 { return "—".into(); }
    let secs = epoch.floor() as i64;
    let nanos = ((epoch - secs as f64) * 1e9).round().clamp(0.0, 999_999_999.0) as u32;
    Utc.timestamp_opt(secs, nanos)
        .single()
        .map(|dt| dt.format("%H:%M:%S%.3f").to_string())
        .unwrap_or_else(|| "—".into())
}

fn event_level(name: &str) -> (&'static str, Color32) {
    let n = name.to_ascii_lowercase();
    if n.contains("error") || n.contains("blackout") { ("ERROR", RED) }
    else if n.contains("drop") || n.contains("truncate") || n.contains("reorder") || n.contains("jitter") { ("WARN", AMBER) }
    else { ("INFO", BLUE) }
}

fn event_visible(event: &DisplayEvent, filter: EventFilter) -> bool {
    let (level, _) = event_level(&event.name);
    match filter {
        EventFilter::All => true,
        EventFilter::Info => level == "INFO",
        EventFilter::Warnings => level == "WARN" || level == "ERROR",
    }
}

fn short_event_name(name: &str) -> String {
    name.replace('_', " ")
}

fn opt_hex32(v: Option<u32>) -> String {
    v.map(|x| format!("0x{x:08X}")).unwrap_or_else(|| "—".into())
}

fn opt_hex64(v: Option<u64>) -> String {
    v.map(|x| format!("0x{x:016X}")).unwrap_or_else(|| "—".into())
}

fn preset_description(name: &str) -> &'static str {
    match name {
        "DEMO: Hard blackout" => "Complete CHAOS-branch transport outage while clean capture remains untouched.",
        "DEMO: Repeating burst loss" => "Repeatable burst loss intended to create obvious transport outages.",
        "DEMO: Cascading incident" => "Staged transport, timing, and signal faults with controlled recovery.",
        "DEMO: Clone detected emitters" => "Clone spectral slices around real detected peaks into configured ghost offsets.",
        "DEMO: Emitter swarm" => "Create a denser planned ghost scene from live detected spectral sources.",
        "DEMO: Power-neutral ghosts" => "Redistribute spectral topology while restoring packet RMS after cloning.",
        "Timing collapse" => "Corrupt temporal semantics while preserving packet flow.",
        "Protocol corruption" => "Exercise sequence, header, payload, and VITA parser resilience.",
        "RF degradation" => "Apply controlled signal-domain degradation to the CHAOS copy.",
        "I/Q path fault" => "Exercise complex-sample path configuration faults.",
        "Consumer stall invariant" => "Delay the consumer path to validate observability boundaries.",
        "Mixed failure" => "Combine transport, protocol, timing, and RF perturbations.",
        _ => "Observe the validated input stream without applying faults.",
    }
}
