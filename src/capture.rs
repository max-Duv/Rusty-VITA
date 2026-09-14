use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use crossbeam_channel::{bounded, Receiver};

use crate::profile::{InputMode, ResolvedConfig};
use crate::vita49::{decode_payload, VrtFrame};

#[derive(Debug, Clone)]
pub struct PacketEvent {
    pub raw: Vec<u8>,
    pub capture_time: f64,
    pub backend: String,
}

pub trait PacketSource: Send {
    fn read(&mut self, timeout: Duration) -> Result<Option<PacketEvent>>;
    fn status(&self) -> SourceStatus;
    fn close(&mut self);
}

#[derive(Debug, Clone, Default)]
pub struct SourceStatus {
    pub backend: String,
    pub detail: String,
    pub rx_packets: u64,
    pub rx_bytes: u64,
    pub payload_field: Option<String>,
    /// Current bounded reader-channel backlog. This is observed, not estimated.
    pub queue_depth: usize,
    /// Epoch timestamp of the most recently received/replayed datagram.
    pub last_rx_epoch: Option<f64>,
    pub finished: bool,
}

fn now_epoch() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64()
}

pub fn tshark_payload_field() -> String {
    let out = Command::new("tshark").args(["-G", "fields"]).output();
    if let Ok(out) = out {
        let s = String::from_utf8_lossy(&out.stdout);
        if s.lines().any(|l| l.split('\t').any(|f| f == "udp.payload")) {
            return "udp.payload".into();
        }
        if s.lines().any(|l| l.split('\t').any(|f| f == "data.data")) {
            return "data.data".into();
        }
    }
    // Wireshark/TShark 2.6.x on RS5 exposes data.data but not udp.payload.
    "data.data".into()
}

fn spawn_tshark_reader(mut cmd: Command, backend: String, speed: Option<f64>) -> Result<(Child, Receiver<Result<PacketEvent>>, String)> {
    let payload_field = tshark_payload_field();
    cmd.args(["-T", "fields", "-E", "separator=|", "-E", "occurrence=f"])
        .args(["-e", "frame.time_epoch", "-e", "udp.length", "-e", &payload_field])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().context("failed to start tshark")?;
    let stdout = child.stdout.take().ok_or_else(|| anyhow!("tshark stdout unavailable"))?;
    let stderr = child.stderr.take().ok_or_else(|| anyhow!("tshark stderr unavailable"))?;
    let (tx, rx) = bounded::<Result<PacketEvent>>(2048);

    let tx_err = tx.clone();
    thread::spawn(move || {
        let mut r = BufReader::new(stderr);
        let mut err = String::new();
        let _ = r.read_to_string(&mut err);
        let err = err.trim();
        if !err.is_empty() && !err.contains("Capturing on") {
            let _ = tx_err.send(Err(anyhow!("tshark: {err}")));
        }
    });

    thread::spawn(move || {
        let r = BufReader::new(stdout);
        let mut first_capture: Option<f64> = None;
        let first_wall = Instant::now();
        for line in r.lines() {
            let line = match line {
                Ok(v) => v,
                Err(e) => { let _ = tx.send(Err(e.into())); break; }
            };
            let mut parts = line.splitn(3, '|');
            let ts_s = parts.next().unwrap_or("");
            let _udp_len = parts.next().unwrap_or("");
            let hex_s = parts.next().unwrap_or("").replace(':', "");
            if hex_s.is_empty() { continue; }
            let raw = match hex::decode(&hex_s) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let ts = ts_s.parse::<f64>().unwrap_or_else(|_| now_epoch());
            if let Some(speed) = speed {
                let f0 = *first_capture.get_or_insert(ts);
                let target = Duration::from_secs_f64(((ts - f0).max(0.0)) / speed.max(0.01));
                let elapsed = first_wall.elapsed();
                if target > elapsed { thread::sleep(target - elapsed); }
            }
            if tx.send(Ok(PacketEvent { raw, capture_time: ts, backend: backend.clone() })).is_err() { break; }
        }
    });

    Ok((child, rx, payload_field))
}

pub struct TsharkSource {
    child: Child,
    rx: Receiver<Result<PacketEvent>>,
    status: SourceStatus,
}

impl TsharkSource {
    pub fn live(cfg: &ResolvedConfig, sudo: bool) -> Result<Self> {
        let p = &cfg.stream;
        let mut cmd = if sudo {
            let mut c = Command::new("sudo");
            c.args(["-n", "tshark"]);
            c
        } else {
            Command::new("tshark")
        };
        let cap = if let Some(src) = &p.source_ip {
            format!("src host {src} and dst host {}", p.group)
        } else {
            format!("dst host {}", p.group)
        };
        let mut display = format!("ip.dst=={} && udp.dstport=={}", p.group, p.dst_port);
        if let Some(src) = &p.source_ip { display.push_str(&format!(" && ip.src=={src}")); }
        if let Some(sp) = p.source_port { display.push_str(&format!(" && udp.srcport=={sp}")); }

        cmd.args(["-l", "-n", "-i", &p.interface])
            .args(["-f", &cap])
            .args(["-o", "ip.defragment:TRUE"])
            .args(["-Y", &display]);

        let backend = if sudo { "sudo-tshark" } else { "tshark" }.to_string();
        let (child, rx, field) = spawn_tshark_reader(cmd, backend.clone(), None)?;
        Ok(Self {
            child,
            rx,
            status: SourceStatus {
                backend,
                detail: format!("{}:{} on {} (fragment-aware; display filter after reassembly)", p.group, p.dst_port, p.interface),
                rx_packets: 0,
                rx_bytes: 0,
                payload_field: Some(field),
                queue_depth: 0,
                last_rx_epoch: None,
                finished: false,
            },
        })
    }

    pub fn pcap(cfg: &ResolvedConfig, path: &str, speed: f64) -> Result<Self> {
        let p = &cfg.stream;
        let mut cmd = Command::new("tshark");
        cmd.args(["-l", "-n", "-r", path])
            .args(["-o", "ip.defragment:TRUE"]);
        let mut display = format!("udp.dstport=={}", p.dst_port);
        if !p.group.is_empty() { display.push_str(&format!(" && ip.dst=={}", p.group)); }
        if let Some(src) = &p.source_ip { display.push_str(&format!(" && ip.src=={src}")); }
        if let Some(sp) = p.source_port { display.push_str(&format!(" && udp.srcport=={sp}")); }
        cmd.args(["-Y", &display]);
        let (child, rx, field) = spawn_tshark_reader(cmd, "pcap-tshark".into(), Some(speed))?;
        Ok(Self {
            child,
            rx,
            status: SourceStatus {
                backend: "pcap-tshark".into(),
                detail: format!("{} @ {:.2}x", path, speed),
                rx_packets: 0,
                rx_bytes: 0,
                payload_field: Some(field),
                queue_depth: 0,
                last_rx_epoch: None,
                finished: false,
            },
        })
    }
}

impl PacketSource for TsharkSource {
    fn read(&mut self, timeout: Duration) -> Result<Option<PacketEvent>> {
        match self.rx.recv_timeout(timeout) {
            Ok(Ok(ev)) => {
                self.status.rx_packets += 1;
                self.status.rx_bytes += ev.raw.len() as u64;
                self.status.last_rx_epoch = Some(ev.capture_time);
                self.status.queue_depth = self.rx.len();
                Ok(Some(ev))
            }
            Ok(Err(e)) => Err(e),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => Ok(None),
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                if let Ok(Some(code)) = self.child.try_wait() {
                    if code.success() {
                        self.status.finished = true;
                        return Ok(None);
                    }
                    bail!("tshark capture exited with {code}");
                }
                Ok(None)
            }
        }
    }

    fn status(&self) -> SourceStatus {
        let mut s = self.status.clone();
        s.queue_depth = self.rx.len();
        s
    }

    fn close(&mut self) { let _ = self.child.kill(); }
}

pub fn create_source(cfg: &ResolvedConfig) -> Result<Box<dyn PacketSource>> {
    match &cfg.input {
        InputMode::Live { backend, sudo } => match backend {
            crate::cli::CaptureBackend::Tshark | crate::cli::CaptureBackend::Auto => Ok(Box::new(TsharkSource::live(cfg, *sudo)?)),
            #[cfg(feature = "native-pcap")]
            crate::cli::CaptureBackend::NativePcap => Ok(Box::new(native::NativePcapSource::live(cfg)?)),
        },
        InputMode::Pcap { path, speed } => Ok(Box::new(TsharkSource::pcap(cfg, path, *speed)?)),
    }
}

pub fn run_probe(cfg: &ResolvedConfig, seconds: f64) -> Result<()> {
    println!("VITA-49 Rust receive probe\n");
    println!("Profile   : {}", cfg.stream.name);
    println!("Input     : {}:{} via {}", cfg.stream.group, cfg.stream.dst_port, cfg.stream.interface);
    println!("Expected  : {:.0} Sa/s", cfg.stream.sample_rate);
    let mut src = create_source(cfg)?;
    let until = Instant::now() + Duration::from_secs_f64(seconds.max(0.5));
    let mut good = 0u64;
    let mut parse_errors = 0u64;
    let mut bytes = 0u64;
    let mut geometry_mismatches = 0u64;
    let mut sid_mismatches = 0u64;
    let mut class_mismatches = 0u64;
    let mut first = true;
    while Instant::now() < until {
        if let Some(ev) = src.read(Duration::from_millis(100))? {
            bytes += ev.raw.len() as u64;
            match VrtFrame::parse(&ev.raw) {
                Ok(f) => {
                    good += 1;
                    if cfg.stream.packet_bytes.map(|n| n != ev.raw.len()).unwrap_or(false)
                        || cfg.stream.data_bytes.map(|n| n != f.payload().len()).unwrap_or(false) { geometry_mismatches += 1; }
                    if cfg.stream.stream_id.map(|sid| Some(sid) != f.stream_id()).unwrap_or(false) { sid_mismatches += 1; }
                    if cfg.stream.class_id.map(|cid| Some(cid) != f.class_id()).unwrap_or(false) { class_mismatches += 1; }
                    if first {
                        first = false;
                        let samples = decode_payload(&f, cfg.format, cfg.stream.iq).len();
                        println!("FIRST VITA PACKET");
                        println!("  backend       : {}", ev.backend);
                        println!("  bytes         : {}", ev.raw.len());
                        println!("  type          : {}", f.packet_type);
                        println!("  packet count  : {}", f.packet_count);
                        println!("  size words    : {}", f.packet_size_words);
                        println!("  stream ID     : {}", f.stream_id().map(|v| format!("0x{v:08X}")).unwrap_or_else(|| "?".into()));
                        println!("  class ID      : {}", f.class_id().map(|v| format!("0x{v:016X}")).unwrap_or_else(|| "?".into()));
                        println!("  payload bytes : {}", f.payload().len());
                        println!("  samples       : {}", samples);
                    }
                }
                Err(_) => parse_errors += 1,
            }
        }
    }
    let elapsed = seconds.max(0.5);
    let st = src.status();
    src.close();
    println!("\nRESULT");
    println!("  backend       : {}", st.backend);
    println!("  payload field : {}", st.payload_field.unwrap_or_else(|| "n/a".into()));
    println!("  packets       : {}", good + parse_errors);
    println!("  parsed        : {}", good);
    println!("  parse errors  : {}", parse_errors);
    println!("  geometry diff : {}", geometry_mismatches);
    println!("  SID mismatch  : {}", sid_mismatches);
    println!("  class mismatch: {}", class_mismatches);
    println!("  receive rate  : {:.1} packets/s", (good + parse_errors) as f64 / elapsed);
    println!("  throughput    : {:.3} Mbit/s", bytes as f64 * 8.0 / elapsed / 1e6);
    Ok(())
}

#[cfg(feature = "native-pcap")]
mod native {
    use std::collections::{BTreeMap, HashMap};
    use std::net::Ipv4Addr;
    use std::str::FromStr;
    use std::time::{Duration, Instant};

    use anyhow::{bail, Context, Result};
    use pcap::{Active, Capture};

    use super::{PacketEvent, PacketSource, SourceStatus, now_epoch};
    use crate::profile::ResolvedConfig;

    #[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
    struct Key { src: [u8;4], dst: [u8;4], id: u16, proto: u8 }

    struct Assembly {
        pieces: BTreeMap<usize, Vec<u8>>,
        total: Option<usize>,
        born: Instant,
    }

    struct Reassembler { map: HashMap<Key, Assembly> }
    impl Reassembler {
        fn new() -> Self { Self { map: HashMap::new() } }
        fn gc(&mut self) { self.map.retain(|_, a| a.born.elapsed() < Duration::from_secs(3)); }
        fn push(&mut self, key: Key, offset: usize, more: bool, bytes: &[u8]) -> Option<Vec<u8>> {
            let a = self.map.entry(key).or_insert_with(|| Assembly { pieces: BTreeMap::new(), total: None, born: Instant::now() });
            a.pieces.insert(offset, bytes.to_vec());
            if !more { a.total = Some(offset + bytes.len()); }
            let total = a.total?;
            let mut cursor = 0usize;
            for (&off, p) in &a.pieces {
                if off > cursor { return None; }
                cursor = cursor.max(off + p.len());
            }
            if cursor < total { return None; }
            let mut out = vec![0u8; total];
            for (&off, p) in &a.pieces {
                let end = (off + p.len()).min(total);
                if off < total { out[off..end].copy_from_slice(&p[..end-off]); }
            }
            self.map.remove(&key);
            Some(out)
        }
    }

    pub struct NativePcapSource {
        cap: Capture<Active>,
        reasm: Reassembler,
        group: Ipv4Addr,
        port: u16,
        source: Option<Ipv4Addr>,
        source_port: Option<u16>,
        status: SourceStatus,
    }

    impl NativePcapSource {
        pub fn live(cfg: &ResolvedConfig) -> Result<Self> {
            let p = &cfg.stream;
            let mut cap = Capture::from_device(p.interface.as_str())?
                .promisc(true)
                .immediate_mode(true)
                .timeout(20)
                .open()
                .with_context(|| format!("open libpcap on {}", p.interface))?;
            // Capture all IPv4 fragments for the multicast destination. Do not
            // filter by UDP port before reassembly.
            let filter = if let Some(src) = &p.source_ip {
                format!("src host {src} and dst host {}", p.group)
            } else { format!("dst host {}", p.group) };
            cap.filter(&filter, true)?;
            Ok(Self {
                cap, reasm: Reassembler::new(),
                group: Ipv4Addr::from_str(&p.group)?,
                port: p.dst_port,
                source: p.source_ip.as_deref().map(Ipv4Addr::from_str).transpose()?,
                source_port: p.source_port,
                status: SourceStatus { backend: "native-pcap".into(), detail: format!("{} group-only BPF + Rust IPv4 reassembly", p.interface), ..Default::default() },
            })
        }

        fn decode_ethernet(&mut self, frame: &[u8]) -> Option<Vec<u8>> {
            if frame.len() < 14 { return None; }
            let mut off = 14usize;
            let mut ethertype = u16::from_be_bytes([frame[12], frame[13]]);
            while ethertype == 0x8100 || ethertype == 0x88a8 {
                if frame.len() < off + 4 { return None; }
                ethertype = u16::from_be_bytes([frame[off+2], frame[off+3]]);
                off += 4;
            }
            if ethertype != 0x0800 || frame.len() < off + 20 { return None; }
            let ip = &frame[off..];
            if ip[0] >> 4 != 4 { return None; }
            let ihl = (ip[0] as usize & 0x0f) * 4;
            if ihl < 20 || ip.len() < ihl { return None; }
            let total = u16::from_be_bytes([ip[2], ip[3]]) as usize;
            if total < ihl || ip.len() < total { return None; }
            let src = [ip[12],ip[13],ip[14],ip[15]];
            let dst = [ip[16],ip[17],ip[18],ip[19]];
            if Ipv4Addr::from(dst) != self.group { return None; }
            if let Some(s) = self.source { if Ipv4Addr::from(src) != s { return None; } }
            let proto = ip[9];
            if proto != 17 { return None; }
            let id = u16::from_be_bytes([ip[4], ip[5]]);
            let flags_off = u16::from_be_bytes([ip[6], ip[7]]);
            let more = (flags_off & 0x2000) != 0;
            let frag_off = (flags_off & 0x1fff) as usize * 8;
            let payload = &ip[ihl..total];
            let key = Key { src, dst, id, proto };
            let udp = self.reasm.push(key, frag_off, more, payload)?;
            if udp.len() < 8 { return None; }
            let sp = u16::from_be_bytes([udp[0], udp[1]]);
            let dp = u16::from_be_bytes([udp[2], udp[3]]);
            if dp != self.port { return None; }
            if let Some(want) = self.source_port { if sp != want { return None; } }
            let ulen = u16::from_be_bytes([udp[4], udp[5]]) as usize;
            if ulen < 8 || ulen > udp.len() { return None; }
            Some(udp[8..ulen].to_vec())
        }
    }

    impl PacketSource for NativePcapSource {
        fn read(&mut self, _timeout: Duration) -> Result<Option<PacketEvent>> {
            self.reasm.gc();
            match self.cap.next_packet() {
                Ok(pkt) => {
                    // Copy out of libpcap's borrowed packet before mutably borrowing
                    // the reassembler stored on self.
                    let frame = pkt.data.to_vec();
                    if let Some(raw) = self.decode_ethernet(&frame) {
                        self.status.rx_packets += 1;
                        self.status.rx_bytes += raw.len() as u64;
                        let capture_time = now_epoch();
                        self.status.last_rx_epoch = Some(capture_time);
                        return Ok(Some(PacketEvent { raw, capture_time, backend: "native-pcap".into() }));
                    }
                    Ok(None)
                }
                Err(pcap::Error::TimeoutExpired) => Ok(None),
                Err(e) => bail!("libpcap: {e}"),
            }
        }
        fn status(&self) -> SourceStatus { self.status.clone() }
        fn close(&mut self) {}
    }
}
