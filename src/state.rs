use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::analytics::AnalysisSnapshot;
use crate::capture::SourceStatus;
use crate::chaos::{ChaosConfig, EngineEvent, EngineStats};
use crate::metrics::MetricsSnapshot;
use crate::telemetry::ProcessTelemetry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayEvent {
    pub wall_time: f64,
    pub name: String,
    pub detail: String,
}

impl From<EngineEvent> for DisplayEvent {
    fn from(v: EngineEvent) -> Self {
        Self { wall_time: v.wall_time, name: v.name, detail: v.detail }
    }
}

#[derive(Debug, Clone)]
pub struct WorkerUpdate {
    pub clean: MetricsSnapshot,
    pub chaos: MetricsSnapshot,
    pub engine: EngineStats,
    pub source: SourceStatus,
    pub analysis: AnalysisSnapshot,
    pub process: ProcessTelemetry,
    pub active: bool,
    pub armed_config: ChaosConfig,
    pub emit_enabled: bool,
    pub log_path: Option<String>,
    pub error: Option<String>,
    pub clean_recent: Vec<Complex64>,
    pub chaos_recent: Vec<Complex64>,
    pub events: Vec<DisplayEvent>,
}

impl Default for WorkerUpdate {
    fn default() -> Self {
        Self {
            clean: MetricsSnapshot::default(),
            chaos: MetricsSnapshot::default(),
            engine: EngineStats::default(),
            source: SourceStatus::default(),
            analysis: AnalysisSnapshot::default(),
            process: ProcessTelemetry::default(),
            active: false,
            armed_config: ChaosConfig::default(),
            emit_enabled: false,
            log_path: None,
            error: None,
            clean_recent: Vec::new(),
            chaos_recent: Vec::new(),
            events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum WorkerCommand {
    Start { config: ChaosConfig, seed: u64, emit: bool, label: String },
    /// Record an actual operator/UI action in the event stream without changing
    /// packet-processing state. This is used for auditable actions such as ARM,
    /// DISARM, and preset selection.
    Audit { name: String, detail: String },
    Stop,
    Shutdown,
}
