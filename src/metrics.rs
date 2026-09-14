use std::collections::VecDeque;
use std::time::Instant;

use num_complex::Complex64;
use serde::{Deserialize, Serialize};

use crate::cli::SampleFormat;
use crate::vita49::{decode_payload, VrtFrame};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// All parsed VITA packets (data + context/auxiliary).
    pub packets: u64,
    /// Data-bearing VITA packets only (types 0..=3).
    pub data_packets: u64,
    /// Context packets only (types 4/5).
    pub context_packets: u64,
    /// Data-packet rate only.
    pub pps: f64,
    pub samples: u64,
    pub samples_per_sec: f64,
    pub mbps: f64,
    pub seq_gaps: u64,
    pub reorders: u64,
    pub ts_drops: u64,
    pub glitches: u64,
    pub parse_errors: u64,
    pub sid: Option<u32>,
    pub class_id: Option<u64>,
    pub last_packet_bytes: usize,
    pub last_payload_bytes: usize,
}

pub struct StreamMetrics {
    fs: f64,
    fmt: SampleFormat,
    iq: bool,
    t0: Instant,
    packets: u64,
    data_packets: u64,
    context_packets: u64,
    bytes: u64,
    samples: u64,
    seq_gaps: u64,
    reorders: u64,
    ts_drops: u64,
    glitches: u64,
    parse_errors: u64,
    sid: Option<u32>,
    class_id: Option<u64>,
    last_seq: Option<u8>,
    last_ts: Option<f64>,
    last_packet_bytes: usize,
    last_payload_bytes: usize,
    _ts_deltas: VecDeque<f64>,
    // (arrival, decoded samples, wire bytes, is_data_packet)
    rate_window: VecDeque<(Instant, usize, usize, bool)>,
}

impl StreamMetrics {
    pub fn new(fs: f64, fmt: SampleFormat, iq: bool) -> Self {
        Self { fs, fmt, iq, t0: Instant::now(), packets: 0, data_packets: 0, context_packets: 0, bytes: 0, samples: 0, seq_gaps: 0, reorders: 0, ts_drops: 0, glitches: 0, parse_errors: 0, sid: None, class_id: None, last_seq: None, last_ts: None, last_packet_bytes: 0, last_payload_bytes: 0, _ts_deltas: VecDeque::with_capacity(256), rate_window: VecDeque::with_capacity(2048) }
    }
    pub fn reset(&mut self) { *self = Self::new(self.fs, self.fmt, self.iq); }

    pub fn update(&mut self, raw: &[u8]) -> Vec<Complex64> {
        let f = match VrtFrame::parse(raw) {
            Ok(f) => f,
            Err(_) => {
                self.parse_errors += 1; self.packets += 1; self.bytes += raw.len() as u64; self.last_packet_bytes = raw.len();
                self.rate_window.push_back((Instant::now(), 0, raw.len(), false));
                while self.rate_window.len() > 4096 { self.rate_window.pop_front(); }
                return Vec::new();
            }
        };
        let is_data = f.is_data_packet();
        let is_context = f.is_context_packet();
        let x = if is_data { decode_payload(&f, self.fmt, self.iq) } else { Vec::new() };
        let ts = if is_data { f.timestamp_seconds() } else { None };
        self.packets += 1;
        if is_data { self.data_packets += 1; }
        if is_context { self.context_packets += 1; }
        self.bytes += raw.len() as u64;
        self.samples += x.len() as u64;
        if is_data {
            self.last_packet_bytes = raw.len();
            self.last_payload_bytes = f.payload().len();
        }
        self.rate_window.push_back((Instant::now(), x.len(), raw.len(), is_data));
        while self.rate_window.len() > 4096 { self.rate_window.pop_front(); }
        if is_data {
            if let Some(v) = f.stream_id() { self.sid = Some(v); }
            if let Some(v) = f.class_id() { self.class_id = Some(v); }

            // Packet-count sequences are independent across VITA packet classes.
            // Track continuity only across the sample-bearing data stream so periodic
            // context packets cannot manufacture false gaps/reorders.
            if let Some(last) = self.last_seq {
                let expected = (last + 1) & 0x0f;
                if f.packet_count != expected {
                    let forward = f.packet_count.wrapping_sub(expected) & 0x0f;
                    let backward = expected.wrapping_sub(f.packet_count) & 0x0f;
                    if forward <= backward { self.seq_gaps += forward.max(1) as u64; }
                    else { self.reorders += 1; }
                }
            }
            self.last_seq = Some(f.packet_count);

            if let (Some(ts), Some(last_ts)) = (ts, self.last_ts) {
                let dt = ts - last_ts;
                if dt <= 0.0 { self.glitches += 1; }
                else {
                    if self._ts_deltas.len() >= 256 { self._ts_deltas.pop_front(); }
                    self._ts_deltas.push_back(dt);
                    let expected = if x.is_empty() { 0.0 } else { x.len() as f64 / self.fs };
                    if expected > 0.0 && dt > expected * 1.5 { self.ts_drops += 1; }
                    if expected > 0.0 && (dt - expected).abs() > (expected * 0.25).max(1e-4) { self.glitches += 1; }
                }
            }
            if ts.is_some() { self.last_ts = ts; }
        }
        x
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let elapsed = self.t0.elapsed().as_secs_f64().max(1e-6);
        let window_s = elapsed.min(2.0).max(0.10);
        let now = Instant::now();
        let mut win_packets = 0u64;
        let mut win_samples = 0u64;
        let mut win_bytes = 0u64;
        for (t, samples, bytes, is_data) in &self.rate_window {
            if now.duration_since(*t).as_secs_f64() <= 2.0 {
                if *is_data { win_packets += 1; }
                win_samples += *samples as u64;
                win_bytes += *bytes as u64;
            }
        }
        MetricsSnapshot {
            packets: self.packets,
            data_packets: self.data_packets,
            context_packets: self.context_packets,
            pps: win_packets as f64 / window_s,
            samples: self.samples,
            samples_per_sec: win_samples as f64 / window_s,
            mbps: win_bytes as f64 * 8.0 / 1e6 / window_s,
            seq_gaps: self.seq_gaps,
            reorders: self.reorders,
            ts_drops: self.ts_drops,
            glitches: self.glitches,
            parse_errors: self.parse_errors,
            sid: self.sid,
            class_id: self.class_id,
            last_packet_bytes: self.last_packet_bytes,
            last_payload_bytes: self.last_payload_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vita49::{build_demo_context_packet, build_demo_packet};

    #[test]
    fn context_packets_do_not_pollute_data_metrics() {
        let mut m = StreamMetrics::new(250_000.0, SampleFormat::BeI32, false);
        let samples = vec![Complex64::new(100.0, 0.0); 500];
        let d1 = build_demo_packet(&samples, 0x42783031, 1, SampleFormat::BeI32, false, 1000.000);
        let c = build_demo_context_packet(0x42783031, 9, 1000.001);
        let d2 = build_demo_packet(&samples, 0x42783031, 2, SampleFormat::BeI32, false, 1000.002);

        assert_eq!(m.update(&d1).len(), 500);
        assert!(m.update(&c).is_empty());
        assert_eq!(m.update(&d2).len(), 500);

        let s = m.snapshot();
        assert_eq!(s.packets, 3);
        assert_eq!(s.data_packets, 2);
        assert_eq!(s.context_packets, 1);
        assert_eq!(s.samples, 1000);
        assert_eq!(s.seq_gaps, 0);
        assert_eq!(s.reorders, 0);
        assert_eq!(s.ts_drops, 0);
        assert_eq!(s.glitches, 0);
        assert_eq!(s.last_packet_bytes, 2032);
        assert_eq!(s.last_payload_bytes, 2000);
    }
}
