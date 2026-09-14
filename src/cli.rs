use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CaptureBackend {
    Tshark,
    Auto,
    #[cfg(feature = "native-pcap")]
    NativePcap,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SampleFormat {
    BeI8,
    BeI16,
    BeI32,
    BeF32,
    BeF64,
    LeI8,
    LeI16,
    LeI32,
    LeF32,
    LeF64,
}

impl SampleFormat {
    pub fn bytes_per_scalar(self) -> usize {
        match self {
            Self::BeI8 | Self::LeI8 => 1,
            Self::BeI16 | Self::LeI16 => 2,
            Self::BeI32 | Self::LeI32 | Self::BeF32 | Self::LeF32 => 4,
            Self::BeF64 | Self::LeF64 => 8,
        }
    }
}

#[derive(Parser, Debug, Clone)]
#[command(name = "vita49-chaos", version, about = "VITA-49/VRT Chaos Engineering Workbench")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Use a built-in stream profile. bx01 fills the known RS-34 Bx01 values.
    #[arg(long, global = true, default_value = "bx01")]
    pub profile: String,

    #[arg(long, global = true)]
    pub headless: bool,

    #[arg(long, global = true, default_value_t = 1)]
    pub seed: u64,

    #[arg(long, global = true, default_value = "logs")]
    pub log_dir: String,

    /// Preselect this built-in chaos preset. In --headless mode it runs immediately;
    /// in GUI mode it opens configured but still requires ARM + RUN CHAOS.
    #[arg(long, global = true)]
    pub preset: Option<String>,

    /// In --headless mode, stop the preset after this many seconds and exit after recovery.
    #[arg(long, global = true)]
    pub duration: Option<f64>,

    /// In --headless mode, emit the chaos copy when live mode was launched with --allow-emit.
    #[arg(long, global = true)]
    pub headless_emit: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Capture a live multicast/VITA stream.
    Live(LiveArgs),
    /// Replay a PCAP through tshark with IP reassembly.
    Pcap(PcapArgs),
    /// Validate receive/parse without opening the GUI.
    Probe(ProbeArgs),
}

#[derive(Args, Debug, Clone)]
pub struct CommonStreamArgs {
    #[arg(long)]
    pub group: Option<String>,
    #[arg(long)]
    pub port: Option<u16>,
    #[arg(long)]
    pub source: Option<String>,
    #[arg(long)]
    pub source_port: Option<u16>,
    #[arg(long)]
    pub interface: Option<String>,
    #[arg(long, value_enum)]
    pub dtype: Option<SampleFormat>,
    #[arg(long)]
    pub fs: Option<f64>,
    #[arg(long)]
    pub iq: bool,
}

#[derive(Args, Debug, Clone)]
pub struct LiveArgs {
    #[command(flatten)]
    pub stream: CommonStreamArgs,

    #[arg(long, value_enum, default_value = "tshark")]
    pub capture_backend: CaptureBackend,

    /// Spawn tshark with sudo -n. Run sudo -v before launching.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub tshark_sudo: bool,

    /// Permit GUI-controlled mutation output to a separate TEST multicast group.
    #[arg(long)]
    pub allow_emit: bool,

    #[arg(long, default_value = "239.255.77.77")]
    pub out_group: String,

    #[arg(long, default_value_t = 52101)]
    pub out_port: u16,

    #[arg(long)]
    pub out_interface_ip: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct PcapArgs {
    pub path: String,
    #[command(flatten)]
    pub stream: CommonStreamArgs,
    #[arg(long, default_value_t = 1.0)]
    pub speed: f64,
}

#[derive(Args, Debug, Clone)]
pub struct ProbeArgs {
    #[command(flatten)]
    pub stream: CommonStreamArgs,
    #[arg(long, value_enum, default_value = "tshark")]
    pub capture_backend: CaptureBackend,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub tshark_sudo: bool,
    #[arg(long, default_value_t = 5.0)]
    pub seconds: f64,
}
