use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::cli::{CaptureBackend, Cli, Command, SampleFormat};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamProfile {
    pub name: String,
    pub group: String,
    pub dst_port: u16,
    pub source_ip: Option<String>,
    pub source_port: Option<u16>,
    pub interface: String,
    pub physical_ingress: Option<String>,
    pub source_mac: Option<String>,
    pub multicast_mac: Option<String>,
    pub ttl: Option<u8>,
    pub sample_rate: f64,
    pub sample_format: String,
    pub iq: bool,
    pub stream_id: Option<u32>,
    pub class_id: Option<u64>,
    pub packet_bytes: Option<usize>,
    pub data_bytes: Option<usize>,
    pub samples_per_packet: Option<usize>,
}

impl StreamProfile {
    pub fn bx01() -> Self {
        Self {
            name: "RS-34 Bx01".into(),
            group: "239.254.253.252".into(),
            dst_port: 52101,
            source_ip: Some("174.168.1.189".into()),
            source_port: Some(60739),
            interface: "bridge0".into(),
            physical_ingress: Some("enp4s0f0".into()),
            source_mac: Some("7c:c2:55:ea:e4:93".into()),
            multicast_mac: Some("01:00:5e:7e:fd:fc".into()),
            ttl: Some(1),
            sample_rate: 250_000.0,
            sample_format: "be-i32".into(),
            iq: false,
            stream_id: Some(0x4278_3031),
            class_id: Some(0x004f_5054_5257_7834),
            packet_bytes: Some(2032),
            data_bytes: Some(2000),
            samples_per_packet: Some(500),
        }
    }
}

#[derive(Debug, Clone)]
pub enum InputMode {
    Live {
        backend: CaptureBackend,
        sudo: bool,
    },
    Pcap {
        path: String,
        speed: f64,
    },
}

#[derive(Debug, Clone)]
pub struct EmitConfig {
    pub allowed: bool,
    pub group: String,
    pub port: u16,
    pub interface_ip: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub stream: StreamProfile,
    pub format: SampleFormat,
    pub input: InputMode,
    pub emit: EmitConfig,
    pub seed: u64,
    pub log_dir: String,
    pub headless_preset: Option<String>,
    pub headless_duration: Option<f64>,
    pub headless_emit: bool,
}

fn parse_profile_format(p: &StreamProfile) -> SampleFormat {
    match p.sample_format.as_str() {
        "be-i8" => SampleFormat::BeI8,
        "be-i16" => SampleFormat::BeI16,
        "be-f32" => SampleFormat::BeF32,
        "be-f64" => SampleFormat::BeF64,
        "le-i8" => SampleFormat::LeI8,
        "le-i16" => SampleFormat::LeI16,
        "le-i32" => SampleFormat::LeI32,
        "le-f32" => SampleFormat::LeF32,
        "le-f64" => SampleFormat::LeF64,
        _ => SampleFormat::BeI32,
    }
}

impl ResolvedConfig {
    pub fn from_cli(cli: &Cli) -> Result<Self> {
        let mut p = match cli.profile.to_ascii_lowercase().as_str() {
            "bx01" | "bx" | "rs34-bx01" => StreamProfile::bx01(),
            other => bail!("unknown profile {other:?}; currently supported: bx01"),
        };

        let (common, input, emit) = match &cli.command {
            Command::Live(a) => (
                &a.stream,
                InputMode::Live {
                    backend: a.capture_backend,
                    sudo: a.tshark_sudo,
                },
                EmitConfig {
                    allowed: a.allow_emit,
                    group: a.out_group.clone(),
                    port: a.out_port,
                    interface_ip: a.out_interface_ip.clone(),
                },
            ),
            Command::Probe(a) => (
                &a.stream,
                InputMode::Live {
                    backend: a.capture_backend,
                    sudo: a.tshark_sudo,
                },
                EmitConfig {
                    allowed: false,
                    group: "239.255.77.77".into(),
                    port: 52101,
                    interface_ip: None,
                },
            ),
            Command::Pcap(a) => (
                &a.stream,
                InputMode::Pcap {
                    path: a.path.clone(),
                    speed: a.speed,
                },
                EmitConfig {
                    allowed: false,
                    group: "239.255.77.77".into(),
                    port: 52101,
                    interface_ip: None,
                },
            ),
        };

        if let Some(v) = &common.group { p.group = v.clone(); }
        if let Some(v) = common.port { p.dst_port = v; }
        if let Some(v) = &common.source { p.source_ip = Some(v.clone()); }
        if let Some(v) = common.source_port { p.source_port = Some(v); }
        if let Some(v) = &common.interface { p.interface = v.clone(); }
        if let Some(v) = common.fs { p.sample_rate = v; }
        if common.iq { p.iq = true; }

        if !p.sample_rate.is_finite() || p.sample_rate <= 0.0 {
            bail!("sample rate must be finite and > 0");
        }
        // Protect the GUI from accidental 250 MHz-style typos.
        if p.sample_rate > 20_000_000.0 {
            bail!(
                "refusing sample rate {:.0} Sa/s for this GUI profile. Bx01 is 250000 Sa/s; did you add extra zeros?",
                p.sample_rate
            );
        }

        let format = common.dtype.unwrap_or_else(|| parse_profile_format(&p));
        if emit.allowed && emit.group == p.group && emit.port == p.dst_port {
            bail!("safety boundary: TEST output may not equal the input multicast group+port");
        }
        if let Some(d) = cli.duration {
            if !d.is_finite() || d <= 0.0 { bail!("--duration must be finite and > 0 seconds"); }
        }
        if cli.headless_emit && !emit.allowed {
            bail!("--headless-emit requires live mode with --allow-emit");
        }

        Ok(Self {
            stream: p,
            format,
            input,
            emit,
            seed: cli.seed,
            log_dir: cli.log_dir.clone(),
            headless_preset: cli.preset.clone(),
            headless_duration: cli.duration,
            headless_emit: cli.headless_emit,
        })
    }
}
