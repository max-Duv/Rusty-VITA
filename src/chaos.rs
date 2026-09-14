use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use num_complex::Complex64;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rand_distr::{Distribution, Normal};
use rustfft::{Fft, FftPlanner};
use serde::{Deserialize, Serialize};

use crate::cli::SampleFormat;
use crate::vita49::{decode_payload, encode_payload, VrtFrame};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub drop_pct: f64,
    pub burst_enabled: bool,
    pub burst_drop_packets: u64,
    pub burst_every_packets: u64,
    pub blackout: bool,
    pub duplicate_pct: f64,
    pub fixed_delay_ms: f64,
    pub jitter_ms: f64,
    pub reorder_pct: f64,
    pub reorder_window: usize,
    pub throttle_pps: f64,
}
impl Default for TransportConfig {
    fn default() -> Self { Self { drop_pct: 0.0, burst_enabled: false, burst_drop_packets: 0, burst_every_packets: 0, blackout: false, duplicate_pct: 0.0, fixed_delay_ms: 0.0, jitter_ms: 0.0, reorder_pct: 0.0, reorder_window: 4, throttle_pps: 0.0 } }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SeqMode { Off, Jump, Freeze, Random }
impl Default for SeqMode { fn default() -> Self { Self::Off } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    pub seq_mode: SeqMode,
    pub seq_jump: i32,
    pub seq_every_packets: u64,
    pub sid_enabled: bool,
    pub sid_value: u32,
    pub ts_offset_ms: f64,
    pub ts_drift_ppm: f64,
    pub ts_jitter_us: f64,
    pub ts_freeze: bool,
    pub ts_step_ms: f64,
    pub ts_step_every_packets: u64,
    pub truncate_pct: f64,
    pub truncate_bytes: usize,
    pub truncate_keep_size: bool,
    pub header_bitflip_pct: f64,
    pub payload_bitflip_pct: f64,
    pub payload_bitflip_bits: usize,
}
impl Default for ProtocolConfig {
    fn default() -> Self { Self { seq_mode: SeqMode::Off, seq_jump: 1, seq_every_packets: 0, sid_enabled: false, sid_value: 0xDEADBEEF, ts_offset_ms: 0.0, ts_drift_ppm: 0.0, ts_jitter_us: 0.0, ts_freeze: false, ts_step_ms: 0.0, ts_step_every_packets: 0, truncate_pct: 0.0, truncate_bytes: 4, truncate_keep_size: true, header_bitflip_pct: 0.0, payload_bitflip_pct: 0.0, payload_bitflip_bits: 1 } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SignalConfig {
    pub enabled: bool,
    pub gain: f64,
    pub dc_offset: f64,
    pub noise_enabled: bool,
    pub noise_snr_db: f64,
    pub clip_abs: f64,
    pub zero_pct: f64,
    pub sample_dropout_pct: f64,
    pub stuck_enabled: bool,
    pub stuck_value: f64,
    pub tone_enabled: bool,
    pub tone_hz: f64,
    pub tone_amp: f64,
    /// Detect strong narrowband emitters already present in the VITA payload and
    /// create frequency-shifted copies in the CHAOS copy. This is deliberately
    /// signal-domain only; it does not create extra network sources/streams.
    pub emitter_clone_enabled: bool,
    /// Number of ghost copies to create for each selected emitter.
    pub emitter_clone_copies: usize,
    /// Frequency spacing between the original emitter and successive copies.
    pub emitter_clone_spacing_hz: f64,
    /// Linear amplitude applied to each cloned spectral slice.
    pub emitter_clone_gain: f64,
    /// Peak must exceed the median FFT magnitude by this many dB.
    pub emitter_clone_threshold_db: f64,
    /// Maximum number of source emitters to clone per packet.
    pub emitter_clone_max_emitters: usize,
    /// Number of FFT bins on either side of the detected peak copied with it.
    /// Copying a small slice preserves simple modulation better than cloning a
    /// single tone bin.
    pub emitter_clone_half_width_bins: usize,
    /// Alternate copies above and below the source frequency when true.
    pub emitter_clone_bidirectional: bool,
    /// Per-copy amplitude decay. 1.0 keeps every clone at the same level; values
    /// below one make outer ghosts progressively weaker.
    pub emitter_clone_gain_decay: f64,
    /// Optional random frequency dither applied independently to each clone
    /// before quantization to FFT bins. This creates less mechanically-spaced
    /// synthetic scenes while remaining deterministic for a fixed RNG seed.
    pub emitter_clone_jitter_hz: f64,
    /// Skip candidate clone destinations whose existing spectral magnitude is
    /// already this many dB above the median floor. Set <=0 to disable.
    pub emitter_clone_occupancy_guard_db: f64,
    /// Rescale the mutated packet back to its pre-clone RMS. Useful for tests
    /// where spectral topology should change without increasing total power.
    pub emitter_clone_preserve_rms: bool,
    pub iq_swap: bool,
    pub iq_conjugate: bool,
    pub phase_deg: f64,
}
impl Default for SignalConfig {
    fn default() -> Self { Self {
        enabled: false, gain: 1.0, dc_offset: 0.0, noise_enabled: false,
        noise_snr_db: 10.0, clip_abs: 0.0, zero_pct: 0.0,
        sample_dropout_pct: 0.0, stuck_enabled: false, stuck_value: 0.0,
        tone_enabled: false, tone_hz: 1000.0, tone_amp: 0.0,
        emitter_clone_enabled: false, emitter_clone_copies: 2,
        emitter_clone_spacing_hz: 12_500.0, emitter_clone_gain: 0.65,
        emitter_clone_threshold_db: 12.0, emitter_clone_max_emitters: 2,
        emitter_clone_half_width_bins: 2, emitter_clone_bidirectional: true,
        emitter_clone_gain_decay: 0.82, emitter_clone_jitter_hz: 0.0,
        emitter_clone_occupancy_guard_db: 8.0, emitter_clone_preserve_rms: false,
        iq_swap: false, iq_conjugate: false, phase_deg: 0.0,
    } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub processing_delay_ms: f64,
    pub consumer_stall_every: u64,
    pub consumer_stall_ms: f64,
}
impl Default for SystemConfig {
    fn default() -> Self { Self { processing_delay_ms: 0.0, consumer_stall_every: 0, consumer_stall_ms: 0.0 } }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChaosConfig {
    pub transport: TransportConfig,
    pub protocol: ProtocolConfig,
    pub signal: SignalConfig,
    pub system: SystemConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineStats {
    pub seen: u64,
    pub scheduled: u64,
    pub emitted: u64,
    pub dropped: u64,
    pub duplicates: u64,
    pub mutated: u64,
    /// Number of frequency-shifted synthetic emitter copies produced.
    pub emitter_clones: u64,
    /// Clone candidates skipped because the destination was already occupied.
    pub emitter_clone_skipped: u64,
    pub reordered: u64,
}

#[derive(Debug, Clone)]
pub struct ScheduledPacket {
    pub due: Instant,
    pub order: u64,
    pub raw: Vec<u8>,
    pub faults: Vec<String>,
}
impl PartialEq for ScheduledPacket { fn eq(&self, other: &Self) -> bool { self.due == other.due && self.order == other.order } }
impl Eq for ScheduledPacket {}
impl PartialOrd for ScheduledPacket { fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) } }
impl Ord for ScheduledPacket {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse due/order so BinaryHeap behaves as a min-heap.
        other.due.cmp(&self.due).then_with(|| other.order.cmp(&self.order))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineEvent {
    pub wall_time: f64,
    pub name: String,
    pub detail: String,
}

fn epoch_now() -> f64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64() }

#[derive(Debug, Clone, Default)]
struct EmitterCloneReport {
    source_freqs_hz: Vec<f64>,
    target_freqs_hz: Vec<f64>,
    clones_created: usize,
    skipped_occupied: usize,
}

pub struct ChaosEngine {
    fs: f64,
    fmt: SampleFormat,
    iq: bool,
    pub seed: u64,
    rng: ChaCha8Rng,
    pub active: bool,
    pub config: ChaosConfig,
    heap: BinaryHeap<ScheduledPacket>,
    reorder_buf: Vec<(Vec<u8>, Vec<String>)>,
    order: u64,
    last_emit_due: Option<Instant>,
    input_index: u64,
    frozen_seq: Option<u8>,
    frozen_ts: Option<f64>,
    ts_origin_input: Option<f64>,
    tone_phase: f64,
    emitter_fft_n: usize,
    emitter_fft: Option<Arc<dyn Fft<f64>>>,
    emitter_ifft: Option<Arc<dyn Fft<f64>>>,
    pub stats: EngineStats,
    events: VecDeque<EngineEvent>,
}

impl ChaosEngine {
    pub fn new(fs: f64, fmt: SampleFormat, iq: bool, seed: u64) -> Self {
        Self { fs, fmt, iq, seed, rng: ChaCha8Rng::seed_from_u64(seed), active: false, config: ChaosConfig::default(), heap: BinaryHeap::new(), reorder_buf: Vec::new(), order: 0, last_emit_due: None, input_index: 0, frozen_seq: None, frozen_ts: None, ts_origin_input: None, tone_phase: 0.0, emitter_fft_n: 0, emitter_fft: None, emitter_ifft: None, stats: EngineStats::default(), events: VecDeque::with_capacity(4096) }
    }

    pub fn reseed(&mut self, seed: u64) { self.seed = seed; self.rng = ChaCha8Rng::seed_from_u64(seed); }
    pub fn set_config(&mut self, cfg: ChaosConfig) { self.config = cfg; }
    pub fn set_active(&mut self, v: bool) { self.active = v; if !v { self.frozen_seq = None; self.frozen_ts = None; } }
    pub fn reset(&mut self) {
        self.heap.clear(); self.reorder_buf.clear(); self.order = 0; self.last_emit_due = None; self.input_index = 0;
        self.frozen_seq = None; self.frozen_ts = None; self.ts_origin_input = None; self.tone_phase = 0.0;
        self.stats = EngineStats::default(); self.events.clear();
    }
    fn chance(&mut self, pct: f64) -> bool { pct > 0.0 && self.rng.gen::<f64>() < (pct / 100.0).clamp(0.0, 1.0) }
    fn event(&mut self, name: impl Into<String>, detail: impl Into<String>) {
        if self.events.len() >= 4096 { self.events.pop_front(); }
        self.events.push_back(EngineEvent { wall_time: epoch_now(), name: name.into(), detail: detail.into() });
    }
    pub fn drain_events(&mut self) -> Vec<EngineEvent> { self.events.drain(..).collect() }

    pub fn ingest(&mut self, raw: &[u8], now: Instant) -> Vec<ScheduledPacket> {
        self.stats.seen += 1;
        self.input_index += 1;
        if !self.active {
            // Inactive means exact pass-through. Do not route through `schedule`,
            // because `schedule` intentionally applies the configured transport
            // delay/jitter/throttle knobs. A previously selected/run scenario must
            // never leak transport behavior into clean observation mode.
            self.order += 1;
            self.stats.scheduled += 1;
            self.stats.emitted += 1;
            return vec![ScheduledPacket {
                due: now,
                order: self.order,
                raw: raw.to_vec(),
                faults: Vec::new(),
            }];
        }

        if self.config.transport.blackout {
            self.stats.dropped += 1;
            self.event("blackout_drop", "injected");
            return self.poll(now);
        }
        let t = self.config.transport.clone();
        let burst = t.burst_enabled && t.burst_every_packets > 0
            && ((self.input_index - 1) % t.burst_every_packets) < t.burst_drop_packets;
        if burst || self.chance(t.drop_pct) {
            self.stats.dropped += 1;
            self.event(if burst { "burst_drop" } else { "random_drop" }, "injected");
            return self.poll(now);
        }

        let (mutated, mut faults) = self.mutate_packet(raw);
        let copies = if self.chance(t.duplicate_pct) { self.stats.duplicates += 1; faults.push("duplicate".into()); 2 } else { 1 };
        for ci in 0..copies {
            let mut flags = faults.clone();
            if ci == 1 { flags.push("duplicate_copy".into()); }
            if self.chance(t.reorder_pct) {
                flags.push("reorder".into());
                self.reorder_buf.push((mutated.clone(), flags));
                self.stats.reordered += 1;
                if self.reorder_buf.len() >= t.reorder_window.max(2) {
                    let idx = self.rng.gen_range(0..self.reorder_buf.len());
                    let (r, f) = self.reorder_buf.swap_remove(idx);
                    self.schedule(r, now, f);
                }
            } else {
                self.schedule(mutated.clone(), now, flags);
            }
        }
        if !self.reorder_buf.is_empty() && self.input_index % t.reorder_window.max(2) as u64 == 0 {
            let idx = self.rng.gen_range(0..self.reorder_buf.len());
            let (r, f) = self.reorder_buf.swap_remove(idx);
            self.schedule(r, now, f);
        }
        self.poll(now)
    }

    fn schedule(&mut self, raw: Vec<u8>, now: Instant, mut faults: Vec<String>) {
        let t = self.config.transport.clone();
        let mut delay_s = (t.fixed_delay_ms / 1000.0).max(0.0);
        if t.jitter_ms > 0.0 {
            if let Ok(n) = Normal::new(0.0, t.jitter_ms / 1000.0) {
                delay_s += n.sample(&mut self.rng).max(0.0);
                faults.push("jitter".into());
            }
        }
        let mut due = now + Duration::from_secs_f64(delay_s);
        if t.throttle_pps > 0.0 {
            if let Some(last) = self.last_emit_due {
                due = due.max(last + Duration::from_secs_f64(1.0 / t.throttle_pps));
            }
            self.last_emit_due = Some(due);
            faults.push("throttle".into());
        }
        self.order += 1;
        self.heap.push(ScheduledPacket { due, order: self.order, raw, faults });
        self.stats.scheduled += 1;
    }

    pub fn poll(&mut self, now: Instant) -> Vec<ScheduledPacket> {
        let mut out = Vec::new();
        while self.heap.peek().map(|p| p.due <= now).unwrap_or(false) {
            if let Some(p) = self.heap.pop() { self.stats.emitted += 1; out.push(p); }
        }
        out
    }

    pub fn flush(&mut self) -> Vec<ScheduledPacket> {
        let now = Instant::now();
        let pending = std::mem::take(&mut self.reorder_buf);
        for (r, f) in pending { self.schedule(r, now, f); }
        let mut out = Vec::new();
        while let Some(p) = self.heap.pop() { out.push(p); }
        out.sort_by_key(|p| p.order);
        out
    }

    fn mutate_packet(&mut self, raw: &[u8]) -> (Vec<u8>, Vec<String>) {
        let mut frame = match VrtFrame::parse(raw) { Ok(f) => f, Err(_) => return (raw.to_vec(), Vec::new()) };
        let mut faults = Vec::<String>::new();
        let p = self.config.protocol.clone();
        let apply_seq = p.seq_mode != SeqMode::Off && (p.seq_every_packets <= 1 || self.input_index % p.seq_every_packets == 0);
        if apply_seq {
            match p.seq_mode {
                SeqMode::Off => {}
                SeqMode::Jump => frame.set_packet_count(((frame.packet_count as i32 + p.seq_jump) & 0x0f) as u8),
                SeqMode::Freeze => { let v = *self.frozen_seq.get_or_insert(frame.packet_count); frame.set_packet_count(v); }
                SeqMode::Random => frame.set_packet_count(self.rng.gen_range(0..16)),
            }
            faults.push(format!("sequence_{:?}", p.seq_mode).to_ascii_lowercase());
        }
        if p.sid_enabled && frame.set_stream_id(p.sid_value) { faults.push("stream_id".into()); }

        if let Some(ts) = frame.timestamp_seconds() {
            if p.ts_freeze {
                let frozen = *self.frozen_ts.get_or_insert(ts);
                frame.set_timestamp_seconds(frozen);
                faults.push("timestamp_freeze".into());
            } else {
                let mut v = ts + p.ts_offset_ms / 1000.0;
                if p.ts_drift_ppm != 0.0 {
                    let origin = *self.ts_origin_input.get_or_insert(ts);
                    v += (ts - origin) * p.ts_drift_ppm * 1e-6;
                    faults.push("timestamp_drift".into());
                }
                if p.ts_jitter_us > 0.0 {
                    if let Ok(n) = Normal::new(0.0, p.ts_jitter_us * 1e-6) { v += n.sample(&mut self.rng); }
                    faults.push("timestamp_jitter".into());
                }
                if p.ts_step_every_packets > 0 && self.input_index % p.ts_step_every_packets == 0 && p.ts_step_ms != 0.0 {
                    v += p.ts_step_ms / 1000.0;
                    faults.push("timestamp_step".into());
                }
                if (v - ts).abs() > 0.0 {
                    frame.set_timestamp_seconds(v);
                    if p.ts_offset_ms != 0.0 { faults.push("timestamp_offset".into()); }
                }
            }
        }

        if self.chance(p.header_bitflip_pct) && frame.raw.len() >= 4 {
            let bit = self.rng.gen_range(16..32usize);
            let byte_idx = 3usize.saturating_sub(bit / 8);
            frame.raw[byte_idx] ^= 1 << (bit % 8);
            faults.push("header_bitflip".into());
        }
        if self.chance(p.truncate_pct) {
            frame.truncate(p.truncate_bytes, p.truncate_keep_size);
            faults.push("truncate".into());
        }

        let s = self.config.signal.clone();
        // Signal-domain faults operate only on sample-bearing VITA data packets.
        // Context packets (e.g. Bx01 type 4 IF Context) contain metadata/control
        // words and must never be interpreted as RF samples.
        if s.enabled && frame.is_data_packet() {
            let mut x = decode_payload(&frame, self.fmt, self.iq);
            if !x.is_empty() {
                let sf = self.signal_mutate(&mut x, &s);
                if !sf.is_empty() {
                    faults.extend(sf);
                    frame.replace_payload(&encode_payload(&x, self.fmt, self.iq));
                }
            }
        }

        if self.chance(p.payload_bitflip_pct) && frame.payload_end > frame.payload_offset {
            for _ in 0..p.payload_bitflip_bits.max(1) {
                let bi = self.rng.gen_range(frame.payload_offset..frame.payload_end);
                frame.raw[bi] ^= 1 << self.rng.gen_range(0..8);
            }
            faults.push("payload_bitflip".into());
        }

        if !faults.is_empty() {
            self.stats.mutated += 1;
            for f in &faults {
                if f != "emitter_clone" { self.event(f.clone(), "injected"); }
            }
        }
        (frame.raw, faults)
    }

    /// Clone strong narrowband emitters already present in the payload by
    /// copying a small FFT slice to one or more offset frequencies.  The
    /// operation is packet-local, deterministic, and preserves VITA geometry.
    ///
    /// For real-valued streams we only detect positive-frequency peaks and
    /// mirror every clone into the corresponding negative-frequency bins so
    /// the inverse FFT remains real.  For complex I/Q streams signed-frequency
    /// peaks are handled directly.
    fn clone_detected_emitters(&mut self, y: &mut [Complex64], s: &SignalConfig) -> EmitterCloneReport {
        let mut report = EmitterCloneReport::default();
        if y.len() < 16 || self.fs <= 0.0 || s.emitter_clone_copies == 0
            || s.emitter_clone_max_emitters == 0 || s.emitter_clone_gain == 0.0
            || s.emitter_clone_spacing_hz.abs() < f64::EPSILON
        {
            return report;
        }

        let fs = self.fs;
        let pre_clone_rms = (y.iter().map(|z| z.norm_sqr()).sum::<f64>() / y.len().max(1) as f64).sqrt();
        let n = y.len().next_power_of_two().max(64);
        let bin_hz = fs / n as f64;
        if bin_hz <= 0.0 { return report; }

        // Use the unwindowed spectrum for synthesis so the original waveform is
        // reconstructed exactly (aside from floating-point roundoff).  A Hann
        // windowed copy is used only for robust peak detection.
        let mut base = vec![Complex64::new(0.0, 0.0); n];
        base[..y.len()].copy_from_slice(y);
        let mut analysis = base.clone();
        for (i, z) in analysis.iter_mut().enumerate() {
            let w = if n > 1 {
                0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (n - 1) as f64).cos()
            } else { 1.0 };
            *z *= w;
        }

        if self.emitter_fft_n != n || self.emitter_fft.is_none() || self.emitter_ifft.is_none() {
            let mut planner = FftPlanner::<f64>::new();
            self.emitter_fft = Some(planner.plan_fft_forward(n));
            self.emitter_ifft = Some(planner.plan_fft_inverse(n));
            self.emitter_fft_n = n;
        }
        let fft = self.emitter_fft.as_ref().expect("FFT plan initialized").clone();
        let ifft = self.emitter_ifft.as_ref().expect("IFFT plan initialized").clone();
        fft.process(&mut base);
        fft.process(&mut analysis);

        let candidate_bins: Vec<usize> = if self.iq {
            (1..n).filter(|&k| k != n / 2).collect()
        } else {
            (1..(n / 2)).collect()
        };
        if candidate_bins.len() < 3 { return report; }

        let mags: Vec<f64> = analysis.iter().map(|z| z.norm()).collect();
        let mut baseline: Vec<f64> = candidate_bins.iter().map(|&k| mags[k]).collect();
        baseline.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let median = baseline[baseline.len() / 2].max(1e-18);
        let threshold = median * 10f64.powf(s.emitter_clone_threshold_db.max(0.0) / 20.0);
        let occupancy_threshold = if s.emitter_clone_occupancy_guard_db > 0.0 {
            Some(median * 10f64.powf(s.emitter_clone_occupancy_guard_db / 20.0))
        } else { None };

        let signed_freq = |k: usize| -> f64 {
            if k <= n / 2 { k as f64 * bin_hz }
            else { (k as isize - n as isize) as f64 * bin_hz }
        };

        // Reject DC-adjacent leakage and find local maxima.
        let dc_guard_hz = (2.0 * bin_hz).max(1.0);
        let mut peaks: Vec<(usize, f64)> = candidate_bins.iter().copied()
            .filter(|&k| signed_freq(k).abs() >= dc_guard_hz)
            .filter(|&k| {
                let prev = if k == 0 { n - 1 } else { k - 1 };
                let next = (k + 1) % n;
                mags[k] >= threshold && mags[k] > mags[prev] && mags[k] >= mags[next]
            })
            .map(|k| (k, mags[k]))
            .collect();
        peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

        // De-duplicate neighboring FFT leakage peaks.
        let sep_bins = (s.emitter_clone_half_width_bins.max(1) * 2 + 1).max(3);
        let mut selected = Vec::<usize>::new();
        for (k, _) in peaks {
            let far_enough = selected.iter().all(|&q| {
                let d = k.abs_diff(q);
                d.min(n - d) >= sep_bins
            });
            if far_enough {
                selected.push(k);
                if selected.len() >= s.emitter_clone_max_emitters { break; }
            }
        }
        if selected.is_empty() { return report; }

        let original = base.clone();
        let mut out = base;
        let half = s.emitter_clone_half_width_bins.min(n / 8);

        // Occupancy is a *signal-presence* decision, not a raw-magnitude decision.
        // A windowed FFT deliberately spreads a strong source into a spectral
        // skirt.  Comparing one destination bin directly against the median can
        // therefore falsely declare a perfectly empty clone destination
        // "occupied" (the regression caught by the 20 kHz -> 30 kHz test).
        // Require a local spectral maximum inside the destination slice before
        // the occupancy guard can block a clone.  This still rejects a real
        // emitter at the destination while ignoring monotonic leakage skirts.
        let iq = self.iq;
        let destination_is_occupied = |center: usize, limit: f64| -> bool {
            let guard_half = half.max(1) as isize;
            for rel in -guard_half..=guard_half {
                let k = ((center as isize + rel).rem_euclid(n as isize)) as usize;
                if k == 0 || (!iq && k >= n / 2) { continue; }
                let prev = if k == 0 { n - 1 } else { k - 1 };
                let next = (k + 1) % n;
                if mags[k] >= limit && mags[k] > mags[prev] && mags[k] >= mags[next] {
                    return true;
                }
            }
            false
        };

        let freq_to_bin = |f: f64| -> Option<usize> {
            if f < -fs / 2.0 || f >= fs / 2.0 { return None; }
            let k = if f >= 0.0 {
                (f / bin_hz).round() as isize
            } else {
                n as isize + (f / bin_hz).round() as isize
            };
            if k >= 0 && (k as usize) < n { Some(k as usize) } else { None }
        };

        for &peak in &selected {
            let source_f = signed_freq(peak);
            // For real input, the negative-frequency mirror is the same emitter;
            // only report/clone its positive representative.
            if !self.iq && source_f <= 0.0 { continue; }
            report.source_freqs_hz.push(source_f);

            for copy_idx in 0..s.emitter_clone_copies {
                let level = if s.emitter_clone_bidirectional { copy_idx / 2 + 1 } else { copy_idx + 1 };
                let sign = if s.emitter_clone_bidirectional && copy_idx % 2 == 1 { -1.0 } else { 1.0 };
                let jitter = if s.emitter_clone_jitter_hz > 0.0 {
                    self.rng.gen_range(-s.emitter_clone_jitter_hz..=s.emitter_clone_jitter_hz)
                } else { 0.0 };
                let requested_delta_hz = sign * level as f64 * s.emitter_clone_spacing_hz.abs() + jitter;
                let shift_bins = (requested_delta_hz / bin_hz).round() as isize;
                if shift_bins == 0 { continue; }
                let delta_hz = shift_bins as f64 * bin_hz;
                let target_center = source_f + delta_hz;
                let clone_gain = s.emitter_clone_gain * s.emitter_clone_gain_decay.clamp(0.0, 1.0).powi(copy_idx as i32);
                if clone_gain <= 0.0 { continue; }
                if let (Some(limit), Some(target_bin)) = (occupancy_threshold, freq_to_bin(target_center)) {
                    if destination_is_occupied(target_bin, limit) {
                        report.skipped_occupied += 1;
                        continue;
                    }
                }
                // Preserve clone phase across VITA packet boundaries. Since the
                // shift is quantized to an FFT bin, this factor supplies the
                // phase accumulated before the start of the current packet.
                let sample_cursor = self.input_index.saturating_sub(1) as f64 * y.len() as f64;
                let phase0 = Complex64::from_polar(
                    1.0,
                    std::f64::consts::TAU * delta_hz * sample_cursor / fs,
                );

                if self.iq {
                    if freq_to_bin(target_center).is_none() { continue; }
                    let mut copied_any = false;
                    for rel in -(half as isize)..=(half as isize) {
                        let src = ((peak as isize + rel).rem_euclid(n as isize)) as usize;
                        let src_f = signed_freq(src);
                        if let Some(dst) = freq_to_bin(src_f + delta_hz) {
                            out[dst] += original[src] * clone_gain * phase0;
                            copied_any = true;
                        }
                    }
                    if copied_any {
                        report.clones_created += 1;
                        report.target_freqs_hz.push(target_center);
                    }
                } else {
                    // Keep real-valued streams Hermitian: copy the positive
                    // spectral slice and explicitly synthesize its conjugate.
                    if target_center <= 0.0 || target_center >= fs / 2.0 { continue; }
                    let mut copied_any = false;
                    for rel in -(half as isize)..=(half as isize) {
                        let src_i = peak as isize + rel;
                        if src_i <= 0 || src_i >= (n / 2) as isize { continue; }
                        let src = src_i as usize;
                        let src_f = src as f64 * bin_hz;
                        let dst_f = src_f + delta_hz;
                        if dst_f <= 0.0 || dst_f >= fs / 2.0 { continue; }
                        if let Some(dst) = freq_to_bin(dst_f) {
                            if dst == 0 || dst == n / 2 { continue; }
                            let v = original[src] * clone_gain * phase0;
                            out[dst] += v;
                            out[n - dst] += v.conj();
                            copied_any = true;
                        }
                    }
                    if copied_any {
                        report.clones_created += 1;
                        report.target_freqs_hz.push(target_center);
                    }
                }
            }
        }

        if report.clones_created == 0 { return report; }
        ifft.process(&mut out);
        let scale = 1.0 / n as f64;
        for (dst, src) in y.iter_mut().zip(out.into_iter()) {
            *dst = src * scale;
            if !self.iq { dst.im = 0.0; }
        }
        if s.emitter_clone_preserve_rms && pre_clone_rms > 0.0 {
            let post_rms = (y.iter().map(|z| z.norm_sqr()).sum::<f64>() / y.len().max(1) as f64).sqrt();
            if post_rms > 1e-18 {
                let correction = pre_clone_rms / post_rms;
                for z in y.iter_mut() { *z *= correction; }
            }
        }
        report
    }

    fn signal_mutate(&mut self, y: &mut [Complex64], s: &SignalConfig) -> Vec<String> {
        let mut faults = Vec::new();
        if (s.gain - 1.0).abs() > 1e-12 { for z in y.iter_mut() { *z *= s.gain; } faults.push("gain".into()); }
        if s.dc_offset != 0.0 { for z in y.iter_mut() { z.re += s.dc_offset; if self.iq { z.im += s.dc_offset; } } faults.push("dc_offset".into()); }
        if s.emitter_clone_enabled {
            let report = self.clone_detected_emitters(y, s);
            self.stats.emitter_clone_skipped += report.skipped_occupied as u64;
            if report.clones_created > 0 {
                self.stats.emitter_clones += report.clones_created as u64;
                let freqs = report.source_freqs_hz.iter()
                    .map(|f| format!("{:+.0}Hz", f))
                    .collect::<Vec<_>>()
                    .join(",");
                let targets = report.target_freqs_hz.iter()
                    .map(|f| format!("{:+.0}Hz", f))
                    .collect::<Vec<_>>()
                    .join(",");
                self.event(
                    "emitter_clone",
                    format!(
                        "sources=[{}] targets=[{}] clones={} skipped={} spacing={:.0}Hz gain={:.2} decay={:.2} preserve_rms={}",
                        freqs, targets, report.clones_created, report.skipped_occupied,
                        s.emitter_clone_spacing_hz, s.emitter_clone_gain,
                        s.emitter_clone_gain_decay, s.emitter_clone_preserve_rms
                    ),
                );
                faults.push("emitter_clone".into());
            }
        }
        if s.zero_pct > 0.0 && self.chance(s.zero_pct) { for z in y.iter_mut() { *z = Complex64::new(0.0, 0.0); } faults.push("zero_payload".into()); }
        if s.sample_dropout_pct > 0.0 {
            let mut changed = false;
            for z in y.iter_mut() { if self.chance(s.sample_dropout_pct) { *z = Complex64::new(0.0, 0.0); changed = true; } }
            if changed { faults.push("sample_dropout".into()); }
        }
        if s.stuck_enabled { for z in y.iter_mut() { *z = Complex64::new(s.stuck_value, if self.iq { s.stuck_value } else { 0.0 }); } faults.push("stuck_sample".into()); }
        if s.noise_enabled {
            let sigp = y.iter().map(|z| z.norm_sqr()).sum::<f64>() / y.len().max(1) as f64 + 1e-18;
            let npow = sigp / 10f64.powf(s.noise_snr_db / 10.0);
            let sigma = if self.iq { (npow / 2.0).sqrt() } else { npow.sqrt() };
            if sigma > 0.0 {
                if let Ok(n) = Normal::new(0.0, sigma) {
                    for z in y.iter_mut() {
                        z.re += n.sample(&mut self.rng);
                        if self.iq { z.im += n.sample(&mut self.rng); }
                    }
                }
                faults.push("awgn".into());
            }
        }
        if s.tone_enabled && s.tone_amp != 0.0 {
            for (i, z) in y.iter_mut().enumerate() {
                let ph = self.tone_phase + std::f64::consts::TAU * s.tone_hz * i as f64 / self.fs;
                if self.iq { *z += Complex64::from_polar(s.tone_amp, ph); } else { z.re += s.tone_amp * ph.sin(); }
            }
            self.tone_phase = (self.tone_phase + std::f64::consts::TAU * s.tone_hz * y.len() as f64 / self.fs) % std::f64::consts::TAU;
            faults.push("tone".into());
        }
        if s.clip_abs > 0.0 { for z in y.iter_mut() { z.re = z.re.clamp(-s.clip_abs, s.clip_abs); if self.iq { z.im = z.im.clamp(-s.clip_abs, s.clip_abs); } } faults.push("clipping".into()); }
        if self.iq {
            if s.iq_swap { for z in y.iter_mut() { *z = Complex64::new(z.im, z.re); } faults.push("iq_swap".into()); }
            if s.iq_conjugate { for z in y.iter_mut() { *z = z.conj(); } faults.push("iq_conjugate".into()); }
            if s.phase_deg != 0.0 { let r = Complex64::from_polar(1.0, s.phase_deg.to_radians()); for z in y.iter_mut() { *z *= r; } faults.push("phase_rotation".into()); }
        }
        faults
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vita49::{build_demo_context_packet, build_demo_packet};

    #[test]
    fn blackout_drops() {
        let mut e = ChaosEngine::new(250_000.0, SampleFormat::BeI32, false, 1);
        e.config.transport.blackout = true;
        e.set_active(true);
        let x = vec![Complex64::new(1.0, 0.0); 500];
        let raw = build_demo_packet(&x, 0x42783031, 0, SampleFormat::BeI32, false, 1000.0);
        assert!(e.ingest(&raw, Instant::now()).is_empty());
        assert_eq!(e.stats.dropped, 1);
    }

    #[test]
    fn inactive_engine_is_exact_pass_through_even_with_fault_config() {
        let mut e = ChaosEngine::new(250_000.0, SampleFormat::BeI32, false, 123);
        e.config.transport.fixed_delay_ms = 5_000.0;
        e.config.transport.jitter_ms = 500.0;
        e.config.transport.throttle_pps = 1.0;
        e.config.transport.blackout = true;
        e.config.signal.enabled = true;
        e.config.signal.gain = 0.25;
        e.set_active(false);

        let x = vec![Complex64::new(1234.0, 0.0); 500];
        let raw = build_demo_packet(&x, 0x42783031, 0, SampleFormat::BeI32, false, 1000.0);
        let now = Instant::now();
        let out = e.ingest(&raw, now);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].raw, raw);
        assert_eq!(out[0].due, now);
        assert!(out[0].faults.is_empty());
        assert_eq!(e.stats.dropped, 0);
        assert_eq!(e.stats.mutated, 0);
    }

    #[test]
    fn repeating_burst_is_deterministic() {
        let mut e = ChaosEngine::new(250_000.0, SampleFormat::BeI32, false, 7);
        e.config.transport.burst_enabled = true;
        e.config.transport.burst_drop_packets = 2;
        e.config.transport.burst_every_packets = 4;
        e.set_active(true);
        let x = vec![Complex64::new(1.0, 0.0); 500];
        let raw = build_demo_packet(&x, 0x42783031, 0, SampleFormat::BeI32, false, 1000.0);
        assert!(e.ingest(&raw, Instant::now()).is_empty());
        assert!(e.ingest(&raw, Instant::now()).is_empty());
        assert_eq!(e.ingest(&raw, Instant::now()).len(), 1);
        assert_eq!(e.ingest(&raw, Instant::now()).len(), 1);
        assert_eq!(e.stats.dropped, 2);
    }

    #[test]
    fn sequence_jump_rewrites_count() {
        let mut e = ChaosEngine::new(250_000.0, SampleFormat::BeI32, false, 9);
        e.config.protocol.seq_mode = SeqMode::Jump;
        e.config.protocol.seq_jump = 3;
        e.config.protocol.seq_every_packets = 1;
        e.set_active(true);
        let x = vec![Complex64::new(1.0, 0.0); 500];
        let raw = build_demo_packet(&x, 0x42783031, 4, SampleFormat::BeI32, false, 1000.0);
        let out = e.ingest(&raw, Instant::now());
        assert_eq!(out.len(), 1);
        let f = VrtFrame::parse(&out[0].raw).unwrap();
        assert_eq!(f.packet_count, 7);
    }

    #[test]
    fn signal_gain_changes_payload_without_changing_geometry() {
        let mut e = ChaosEngine::new(250_000.0, SampleFormat::BeI32, false, 11);
        e.config.signal.enabled = true;
        e.config.signal.gain = 0.5;
        e.set_active(true);
        let x = vec![Complex64::new(1000.0, 0.0); 500];
        let raw = build_demo_packet(&x, 0x42783031, 1, SampleFormat::BeI32, false, 1000.0);
        let out = e.ingest(&raw, Instant::now());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].raw.len(), raw.len());
        let f = VrtFrame::parse(&out[0].raw).unwrap();
        let y = decode_payload(&f, SampleFormat::BeI32, false);
        assert_eq!(y[0].re, 500.0);
    }

    #[test]
    fn emitter_clone_creates_frequency_shifted_ghost() {
        let fs = 256_000.0;
        let n = 512usize;
        let source_hz = 20_000.0;
        let target_hz = 30_000.0;
        let x: Vec<_> = (0..n).map(|i| {
            let ph = std::f64::consts::TAU * source_hz * i as f64 / fs;
            Complex64::new(20_000.0 * ph.sin(), 0.0)
        }).collect();
        let raw = build_demo_packet(&x, 0x42783031, 1, SampleFormat::BeI32, false, 1000.0);

        let projection = |v: &[Complex64], f: f64| -> f64 {
            v.iter().enumerate().map(|(i, z)| {
                let ph = -std::f64::consts::TAU * f * i as f64 / fs;
                *z * Complex64::from_polar(1.0, ph)
            }).sum::<Complex64>().norm() / v.len() as f64
        };

        let before_frame = VrtFrame::parse(&raw).unwrap();
        let before = decode_payload(&before_frame, SampleFormat::BeI32, false);
        let before_target = projection(&before, target_hz);

        let mut e = ChaosEngine::new(fs, SampleFormat::BeI32, false, 1234);
        e.config.signal.enabled = true;
        e.config.signal.emitter_clone_enabled = true;
        e.config.signal.emitter_clone_copies = 1;
        e.config.signal.emitter_clone_spacing_hz = 10_000.0;
        e.config.signal.emitter_clone_gain = 0.5;
        e.config.signal.emitter_clone_threshold_db = 6.0;
        e.config.signal.emitter_clone_max_emitters = 1;
        e.config.signal.emitter_clone_half_width_bins = 1;
        e.config.signal.emitter_clone_bidirectional = false;
        e.set_active(true);

        let out = e.ingest(&raw, Instant::now());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].raw.len(), raw.len());
        let frame = VrtFrame::parse(&out[0].raw).unwrap();
        let after = decode_payload(&frame, SampleFormat::BeI32, false);
        let after_target = projection(&after, target_hz);

        assert!(after_target > before_target + 1_000.0,
            "expected a visible cloned emitter at {target_hz} Hz: before={before_target}, after={after_target}");
        assert!(e.stats.emitter_clones >= 1);
    }

    #[test]
    fn emitter_clone_occupancy_guard_ignores_source_window_skirt() {
        let fs = 256_000.0;
        let n = 512usize;
        let x: Vec<_> = (0..n).map(|i| {
            let p = std::f64::consts::TAU * 20_000.0 * i as f64 / fs;
            Complex64::new(20_000.0 * p.sin(), 0.0)
        }).collect();
        let raw = build_demo_packet(&x, 0x42783031, 2, SampleFormat::BeI32, false, 1000.0);

        let mut e = ChaosEngine::new(fs, SampleFormat::BeI32, false, 99);
        e.config.signal.enabled = true;
        e.config.signal.emitter_clone_enabled = true;
        e.config.signal.emitter_clone_copies = 1;
        e.config.signal.emitter_clone_spacing_hz = 10_000.0;
        e.config.signal.emitter_clone_gain = 0.5;
        e.config.signal.emitter_clone_threshold_db = 6.0;
        e.config.signal.emitter_clone_max_emitters = 1;
        e.config.signal.emitter_clone_half_width_bins = 1;
        e.config.signal.emitter_clone_bidirectional = false;
        e.config.signal.emitter_clone_occupancy_guard_db = 8.0;
        e.set_active(true);

        let out = e.ingest(&raw, Instant::now());
        assert_eq!(out.len(), 1);
        assert!(e.stats.emitter_clones >= 1,
            "a monotonic Hann-window leakage skirt must not be treated as a real occupied emitter");
        assert_eq!(e.stats.emitter_clone_skipped, 0);
    }

    #[test]
    fn emitter_clone_occupancy_guard_skips_strong_destination() {
        let fs = 256_000.0;
        let n = 512usize;
        let x: Vec<_> = (0..n).map(|i| {
            let p1 = std::f64::consts::TAU * 20_000.0 * i as f64 / fs;
            let p2 = std::f64::consts::TAU * 30_000.0 * i as f64 / fs;
            Complex64::new(30_000.0 * p1.sin() + 12_000.0 * p2.sin(), 0.0)
        }).collect();
        let raw = build_demo_packet(&x, 0x42783031, 3, SampleFormat::BeI32, false, 1000.0);

        let mut e = ChaosEngine::new(fs, SampleFormat::BeI32, false, 17);
        e.config.signal.enabled = true;
        e.config.signal.emitter_clone_enabled = true;
        e.config.signal.emitter_clone_copies = 1;
        e.config.signal.emitter_clone_spacing_hz = 10_000.0;
        e.config.signal.emitter_clone_gain = 0.8;
        e.config.signal.emitter_clone_threshold_db = 6.0;
        e.config.signal.emitter_clone_max_emitters = 1;
        e.config.signal.emitter_clone_half_width_bins = 1;
        e.config.signal.emitter_clone_bidirectional = false;
        e.config.signal.emitter_clone_occupancy_guard_db = 6.0;
        e.set_active(true);

        let out = e.ingest(&raw, Instant::now());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].raw.len(), raw.len());
        assert_eq!(e.stats.emitter_clones, 0);
        assert!(e.stats.emitter_clone_skipped >= 1);
    }

    #[test]
    fn emitter_clone_preserve_rms_keeps_packet_power_near_baseline() {
        let fs = 256_000.0;
        let n = 512usize;
        let source_hz = 20_000.0;
        let x: Vec<_> = (0..n).map(|i| {
            let ph = std::f64::consts::TAU * source_hz * i as f64 / fs;
            Complex64::new(18_000.0 * ph.sin(), 0.0)
        }).collect();
        let before_rms = (x.iter().map(|z| z.norm_sqr()).sum::<f64>() / x.len() as f64).sqrt();
        let raw = build_demo_packet(&x, 0x42783031, 2, SampleFormat::BeI32, false, 1000.0);

        let mut e = ChaosEngine::new(fs, SampleFormat::BeI32, false, 99);
        e.config.signal.enabled = true;
        e.config.signal.emitter_clone_enabled = true;
        e.config.signal.emitter_clone_copies = 2;
        e.config.signal.emitter_clone_spacing_hz = 10_000.0;
        e.config.signal.emitter_clone_gain = 0.8;
        e.config.signal.emitter_clone_gain_decay = 0.8;
        e.config.signal.emitter_clone_threshold_db = 6.0;
        e.config.signal.emitter_clone_max_emitters = 1;
        e.config.signal.emitter_clone_half_width_bins = 1;
        e.config.signal.emitter_clone_occupancy_guard_db = 0.0;
        e.config.signal.emitter_clone_preserve_rms = true;
        e.set_active(true);

        let out = e.ingest(&raw, Instant::now());
        assert_eq!(out.len(), 1);
        let frame = VrtFrame::parse(&out[0].raw).unwrap();
        let after = decode_payload(&frame, SampleFormat::BeI32, false);
        let after_rms = (after.iter().map(|z| z.norm_sqr()).sum::<f64>() / after.len() as f64).sqrt();
        let rel = ((after_rms / before_rms) - 1.0).abs();
        assert!(rel < 0.03, "RMS should remain near baseline: before={before_rms}, after={after_rms}, rel={rel}");
        assert!(e.stats.emitter_clones >= 1);
    }

    #[test]
    fn signal_faults_leave_context_packets_byte_exact() {
        let raw = build_demo_context_packet(0x42783031, 5, 1000.0);
        let mut e = ChaosEngine::new(250_000.0, SampleFormat::BeI32, false, 7);
        e.config.signal.enabled = true;
        e.config.signal.gain = 0.25;
        e.config.signal.emitter_clone_enabled = true;
        e.set_active(true);

        let out = e.ingest(&raw, Instant::now());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].raw, raw, "signal-domain faults must not reinterpret context payload as RF samples");
    }


}
