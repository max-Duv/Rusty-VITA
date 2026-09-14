use std::collections::VecDeque;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossbeam_channel::{bounded, Receiver, Sender};
use num_complex::Complex64;
use serde_json::json;

use crate::analytics::AnalysisEngine;
use crate::capture::create_source;
use crate::chaos::{ChaosConfig, ChaosEngine};
use crate::emit::TestEmitter;
use crate::logging::ExperimentLogger;
use crate::metrics::StreamMetrics;
use crate::profile::ResolvedConfig;
use crate::state::{DisplayEvent, WorkerCommand, WorkerUpdate};
use crate::telemetry::ProcessMonitor;

pub struct WorkerHandle {
    pub commands: Sender<WorkerCommand>,
    pub updates: Receiver<WorkerUpdate>,
}

fn push_recent(dst: &mut VecDeque<Complex64>, x: &[Complex64], max: usize) {
    if x.len() >= max {
        dst.clear();
        dst.extend(x[x.len() - max..].iter().copied());
        return;
    }
    while dst.len() + x.len() > max { dst.pop_front(); }
    dst.extend(x.iter().copied());
}

pub fn spawn(cfg: ResolvedConfig) -> WorkerHandle {
    let (cmd_tx, cmd_rx) = bounded::<WorkerCommand>(64);
    let (upd_tx, upd_rx) = bounded::<WorkerUpdate>(4);
    thread::spawn(move || {
        if let Err(e) = worker_loop(cfg.clone(), cmd_rx, upd_tx.clone()) {
            let mut u = WorkerUpdate::default();
            u.error = Some(format!("{e:#}"));
            let _ = upd_tx.send(u);
        }
    });
    WorkerHandle { commands: cmd_tx, updates: upd_rx }
}

fn worker_loop(cfg: ResolvedConfig, commands: Receiver<WorkerCommand>, updates: Sender<WorkerUpdate>) -> Result<()> {
    let mut source = create_source(&cfg)?;
    let mut engine = ChaosEngine::new(cfg.stream.sample_rate, cfg.format, cfg.stream.iq, cfg.seed);
    let mut clean_metrics = StreamMetrics::new(cfg.stream.sample_rate, cfg.format, cfg.stream.iq);
    let mut chaos_metrics = StreamMetrics::new(cfg.stream.sample_rate, cfg.format, cfg.stream.iq);
    let mut clean_recent = VecDeque::<Complex64>::with_capacity(32768);
    let mut chaos_recent = VecDeque::<Complex64>::with_capacity(32768);
    let mut events = VecDeque::<DisplayEvent>::with_capacity(4096);
    let mut analysis_engine = AnalysisEngine::new();
    let mut process_monitor = ProcessMonitor::new();
    let source_status = source.status();
    events.push_back(DisplayEvent {
        wall_time: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
        name: "SOURCE_READY".into(),
        detail: format!("{} / {}", source_status.backend, source_status.detail),
    });
    let mut logger = ExperimentLogger::new(&cfg.log_dir);
    let mut emitter: Option<TestEmitter> = None;
    let mut emit_enabled = false;
    let mut active_label = String::new();
    let mut packet_index = 0u64;
    let mut next_update = Instant::now();
    let mut next_log = Instant::now();
    let mut cascade_started: Option<Instant> = None;
    let mut cascade_phase_idx: Option<usize> = None;
    let mut shutdown = false;

    while !shutdown {
        while let Ok(cmd) = commands.try_recv() {
            match cmd {
                WorkerCommand::Start { config, seed, emit, label } => {
                    engine.reseed(seed);
                    engine.reset();
                    engine.set_config(config.clone());
                    engine.set_active(true);
                    clean_metrics.reset(); chaos_metrics.reset();
                    clean_recent.clear(); chaos_recent.clear();
                    packet_index = 0;
                    active_label = label.clone();
                    cascade_started = if label == crate::scenario::CASCADE_NAME { Some(Instant::now()) } else { None };
                    cascade_phase_idx = None;
                    emit_enabled = emit && cfg.emit.allowed;
                    emitter = if emit_enabled {
                        match TestEmitter::new(&cfg) {
                            Ok(v) => Some(v),
                            Err(e) => {
                                emit_enabled = false;
                                events.push_back(DisplayEvent { wall_time: chrono::Utc::now().timestamp_millis() as f64 / 1000.0, name: "emit_error".into(), detail: e.to_string() });
                                None
                            }
                        }
                    } else { None };
                    let meta = json!({ "seed": seed, "label": label, "config": config, "emit_enabled": emit_enabled, "profile": cfg.stream.name.clone() });
                    let _ = logger.start(&meta);
                    events.push_back(DisplayEvent { wall_time: chrono::Utc::now().timestamp_millis() as f64 / 1000.0, name: "START".into(), detail: active_label.clone() });
                }
                WorkerCommand::Audit { name, detail } => {
                    let de = DisplayEvent {
                        wall_time: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
                        name,
                        detail,
                    };
                    if events.len() >= 4096 { events.pop_front(); }
                    if logger.active() { let _ = logger.write("operator", &de); }
                    events.push_back(de);
                }
                WorkerCommand::Stop => {
                    let summary = json!({ "clean": clean_metrics.snapshot(), "chaos": chaos_metrics.snapshot(), "engine": engine.stats.clone(), "label": active_label.clone() });
                    if logger.active() { let _ = logger.close(&summary); }

                    // STOP is a hard experiment boundary: cancel any delayed or
                    // reordered experiment packets and restore a default config.
                    // Otherwise an old fixed-delay/jitter/throttle scenario could
                    // continue affecting the output branch after the UI says
                    // PASS-THROUGH.
                    engine.set_active(false);
                    engine.set_config(ChaosConfig::default());
                    engine.reset();
                    chaos_metrics.reset();
                    chaos_recent.clear();
                    cascade_started = None;
                    cascade_phase_idx = None;
                    emit_enabled = false;
                    emitter = None;
                    events.push_back(DisplayEvent { wall_time: chrono::Utc::now().timestamp_millis() as f64 / 1000.0, name: "STOP".into(), detail: format!("{}; exact pass-through restored", active_label) });
                }
                WorkerCommand::Shutdown => shutdown = true,
            }
        }
        if shutdown { break; }

        if engine.active {
            if let Some(t0) = cascade_started {
                match crate::scenario::cascade_phase(t0.elapsed().as_secs_f64()) {
                    Some((idx, label, phase_cfg)) if cascade_phase_idx != Some(idx) => {
                        engine.set_config(phase_cfg);
                        cascade_phase_idx = Some(idx);
                        let de = DisplayEvent {
                            wall_time: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
                            name: "CASCADE_PHASE".into(),
                            detail: format!("T+{:.1}s — {label}", t0.elapsed().as_secs_f64()),
                        };
                        if logger.active() { let _ = logger.write("cascade_phase", &de); }
                        events.push_back(de);
                    }
                    None => {
                        engine.set_config(ChaosConfig::default());
                        engine.set_active(false);
                        emit_enabled = false;
                        emitter = None;
                        cascade_started = None;
                        cascade_phase_idx = None;
                        let de = DisplayEvent {
                            wall_time: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
                            name: "CASCADE_COMPLETE".into(),
                            detail: "45-second sequence complete; pass-through restored".into(),
                        };
                        events.push_back(de.clone());
                        if logger.active() {
                            let summary = json!({ "clean": clean_metrics.snapshot(), "chaos": chaos_metrics.snapshot(), "engine": engine.stats.clone(), "label": active_label.clone(), "auto_complete": true });
                            let _ = logger.write("cascade_complete", &de);
                            let _ = logger.close(&summary);
                        }
                    }
                    _ => {}
                }
            }
        }

        match source.read(Duration::from_millis(20)) {
            Ok(Some(ev)) => {
                packet_index += 1;
                let x = clean_metrics.update(&ev.raw);
                push_recent(&mut clean_recent, &x, 32768);
                let out = engine.ingest(&ev.raw, Instant::now());
                handle_outputs(&out, &mut chaos_metrics, &mut chaos_recent, &mut emitter, emit_enabled, &mut events, &mut logger);

                if engine.active {
                    let sys = engine.config.system.clone();
                    if sys.processing_delay_ms > 0.0 { thread::sleep(Duration::from_secs_f64(sys.processing_delay_ms / 1000.0)); }
                    if sys.consumer_stall_every > 0 && sys.consumer_stall_ms > 0.0 && packet_index % sys.consumer_stall_every == 0 {
                        events.push_back(DisplayEvent { wall_time: chrono::Utc::now().timestamp_millis() as f64 / 1000.0, name: "consumer_stall".into(), detail: format!("{:.1} ms", sys.consumer_stall_ms) });
                        thread::sleep(Duration::from_secs_f64(sys.consumer_stall_ms / 1000.0));
                    }
                }
            }
            Ok(None) => {
                let out = engine.poll(Instant::now());
                handle_outputs(&out, &mut chaos_metrics, &mut chaos_recent, &mut emitter, emit_enabled, &mut events, &mut logger);
                if source.status().finished {
                    let mut u = make_update(&cfg, &source.status(), &clean_metrics, &chaos_metrics, &engine, emit_enabled, logger.path(), &clean_recent, &chaos_recent, &events, &mut analysis_engine, &mut process_monitor);
                    u.error = None;
                    let _ = updates.send(u);
                    break;
                }
            }
            Err(e) => {
                let detail = e.to_string();
                let should_record = events.back().map(|v| v.name.as_str() != "CAPTURE_ERROR" || v.detail != detail).unwrap_or(true);
                if should_record {
                    if events.len() >= 4096 { events.pop_front(); }
                    events.push_back(DisplayEvent {
                        wall_time: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
                        name: "CAPTURE_ERROR".into(),
                        detail: detail.clone(),
                    });
                }
                let mut u = make_update(&cfg, &source.status(), &clean_metrics, &chaos_metrics, &engine, emit_enabled, logger.path(), &clean_recent, &chaos_recent, &events, &mut analysis_engine, &mut process_monitor);
                u.error = Some(detail);
                let _ = updates.send(u);
                thread::sleep(Duration::from_millis(250));
            }
        }

        for e in engine.drain_events() {
            if events.len() >= 4096 { events.pop_front(); }
            let de: DisplayEvent = e.into();
            if logger.active() { let _ = logger.write("fault", &de); }
            events.push_back(de);
        }

        let now = Instant::now();
        if now >= next_log && logger.active() {
            next_log = now + Duration::from_secs(1);
            let row = json!({ "clean": clean_metrics.snapshot(), "chaos": chaos_metrics.snapshot(), "engine": engine.stats.clone() });
            let _ = logger.write("metrics", &row);
            let _ = logger.flush();
        }
        if now >= next_update {
            next_update = now + Duration::from_millis(100);
            let u = make_update(&cfg, &source.status(), &clean_metrics, &chaos_metrics, &engine, emit_enabled, logger.path(), &clean_recent, &chaos_recent, &events, &mut analysis_engine, &mut process_monitor);
            let _ = updates.try_send(u);
        }
    }

    let out = engine.flush();
    handle_outputs(&out, &mut chaos_metrics, &mut chaos_recent, &mut emitter, emit_enabled, &mut events, &mut logger);
    source.close();
    Ok(())
}

fn handle_outputs(
    out: &[crate::chaos::ScheduledPacket], metrics: &mut StreamMetrics, recent: &mut VecDeque<Complex64>,
    emitter: &mut Option<TestEmitter>, emit_enabled: bool, events: &mut VecDeque<DisplayEvent>, logger: &mut ExperimentLogger,
) {
    for p in out {
        let y = metrics.update(&p.raw);
        push_recent(recent, &y, 32768);
        if emit_enabled {
            if let Some(e) = emitter.as_ref() { let _ = e.send(&p.raw); }
        }
        for f in &p.faults {
            if events.len() >= 4096 { events.pop_front(); }
            let de = DisplayEvent { wall_time: chrono::Utc::now().timestamp_millis() as f64 / 1000.0, name: f.clone(), detail: "observed".into() };
            if logger.active() { let _ = logger.write("fault_output", &de); }
            events.push_back(de);
        }
    }
}

fn make_update(
    cfg: &ResolvedConfig,
    source: &crate::capture::SourceStatus,
    clean: &StreamMetrics,
    chaos: &StreamMetrics,
    engine: &ChaosEngine,
    emit_enabled: bool,
    log_path: Option<String>,
    clean_recent: &VecDeque<Complex64>,
    chaos_recent: &VecDeque<Complex64>,
    events: &VecDeque<DisplayEvent>,
    analysis_engine: &mut AnalysisEngine,
    process_monitor: &mut ProcessMonitor,
) -> WorkerUpdate {
    let clean_vec: Vec<Complex64> = clean_recent.iter().copied().collect();
    let chaos_vec: Vec<Complex64> = chaos_recent.iter().copied().collect();
    let now_epoch = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
    let analysis = analysis_engine.update(
        &clean_vec,
        &chaos_vec,
        cfg.stream.sample_rate,
        cfg.stream.iq,
        cfg.format,
        engine.config.signal.emitter_clone_threshold_db,
        engine.config.signal.emitter_clone_max_emitters.max(12),
        now_epoch,
    );
    WorkerUpdate {
        clean: clean.snapshot(),
        chaos: chaos.snapshot(),
        engine: engine.stats.clone(),
        source: source.clone(),
        analysis,
        process: process_monitor.sample(),
        active: engine.active,
        armed_config: engine.config.clone(),
        emit_enabled,
        log_path,
        error: None,
        clean_recent: clean_vec,
        chaos_recent: chaos_vec,
        events: events.iter().skip(events.len().saturating_sub(512)).cloned().collect(),
    }
}

pub fn run_headless(cfg: ResolvedConfig) -> Result<()> {
    let preset = cfg.headless_preset.clone();
    let duration = cfg.headless_duration.or_else(|| {
        if cfg.headless_preset.as_deref() == Some(crate::scenario::CASCADE_NAME) { Some(46.0) } else { None }
    });
    let seed = cfg.seed;
    let emit = cfg.headless_emit;
    let h = spawn(cfg);

    if let Some(name) = preset.clone() {
        if !crate::scenario::NAMES.iter().any(|n| *n == name) {
            anyhow::bail!("unknown preset {name:?}; use one of: {}", crate::scenario::NAMES.join(", "));
        }
        h.commands.send(WorkerCommand::Start {
            config: crate::scenario::named(&name),
            seed,
            emit,
            label: name.clone(),
        })?;
        println!("Headless experiment started: {name}");
    } else {
        println!("Headless VITA-49 observer started (pass-through). Ctrl-C to terminate.");
    }

    let t0 = Instant::now();
    let mut stopped = false;
    loop {
        if let Ok(u) = h.updates.recv_timeout(Duration::from_millis(500)) {
            if let Some(e) = u.error { eprintln!("ERROR: {e}"); }
            println!("clean {:7.1} pps {:9.0} Sa/s | chaos {:7.1} pps {:9.0} Sa/s | drops {} mutated {} parse {}",
                u.clean.pps, u.clean.samples_per_sec, u.chaos.pps, u.chaos.samples_per_sec, u.engine.dropped, u.engine.mutated, u.chaos.parse_errors);
        }
        if let Some(seconds) = duration {
            if !stopped && t0.elapsed().as_secs_f64() >= seconds {
                h.commands.send(WorkerCommand::Stop)?;
                stopped = true;
                println!("Experiment stopped; observing recovery for 3 seconds…");
            }
            if stopped && t0.elapsed().as_secs_f64() >= seconds + 3.0 {
                let _ = h.commands.send(WorkerCommand::Shutdown);
                return Ok(());
            }
        }
    }
}
