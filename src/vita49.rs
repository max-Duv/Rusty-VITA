use anyhow::{bail, Result};
use num_complex::Complex64;

use crate::cli::SampleFormat;

const STREAM_ID_TYPES: [u8; 6] = [1, 3, 4, 5, 6, 7];

#[derive(Debug, Clone)]
pub struct VrtFrame {
    pub raw: Vec<u8>,
    pub packet_type: u8,
    pub class_id_present: bool,
    pub trailer_present: bool,
    pub tsi: u8,
    pub tsf: u8,
    pub packet_count: u8,
    pub packet_size_words: u16,
    pub stream_id_offset: Option<usize>,
    pub class_id_offset: Option<usize>,
    pub int_ts_offset: Option<usize>,
    pub frac_ts_offset: Option<usize>,
    pub payload_offset: usize,
    pub payload_end: usize,
}

fn read_be_u32(b: &[u8], off: usize) -> Option<u32> {
    b.get(off..off + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_be_bytes)
}

fn read_be_u64(b: &[u8], off: usize) -> Option<u64> {
    b.get(off..off + 8)
        .and_then(|s| s.try_into().ok())
        .map(u64::from_be_bytes)
}

fn write_be_u32(b: &mut [u8], off: usize, v: u32) -> bool {
    if off + 4 > b.len() { return false; }
    b[off..off + 4].copy_from_slice(&v.to_be_bytes());
    true
}

fn write_be_u64(b: &mut [u8], off: usize, v: u64) -> bool {
    if off + 8 > b.len() { return false; }
    b[off..off + 8].copy_from_slice(&v.to_be_bytes());
    true
}

impl VrtFrame {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 4 { bail!("VRT packet shorter than 4-byte header"); }
        let word0 = read_be_u32(data, 0).unwrap();
        let packet_type = ((word0 >> 28) & 0x0f) as u8;
        let class_id_present = ((word0 >> 27) & 1) != 0;
        let trailer_present = ((word0 >> 26) & 1) != 0;
        let tsi = ((word0 >> 22) & 0x03) as u8;
        let tsf = ((word0 >> 20) & 0x03) as u8;
        let packet_count = ((word0 >> 16) & 0x0f) as u8;
        let packet_size_words = (word0 & 0xffff) as u16;

        let declared = if packet_size_words == 0 {
            data.len()
        } else {
            packet_size_words as usize * 4
        };
        if packet_size_words != 0 && declared > data.len() {
            bail!("VRT packet truncated: header declares {declared} bytes, capture has {}", data.len());
        }
        if declared < 4 { bail!("invalid VRT packet size: {declared} bytes"); }
        let logical_end = data.len().min(declared);
        let mut pos = 4usize;
        let mut stream_id_offset = None;
        let mut class_id_offset = None;
        let mut int_ts_offset = None;
        let mut frac_ts_offset = None;

        if STREAM_ID_TYPES.contains(&packet_type) {
            if pos + 4 > logical_end { bail!("VRT stream-ID flag/type requires 4 bytes beyond header"); }
            stream_id_offset = Some(pos);
            pos += 4;
        }
        if class_id_present {
            if pos + 8 > logical_end { bail!("VRT class-ID flag set but class ID is truncated"); }
            class_id_offset = Some(pos);
            pos += 8;
        }
        if tsi != 0 {
            if pos + 4 > logical_end { bail!("VRT integer timestamp is truncated"); }
            int_ts_offset = Some(pos);
            pos += 4;
        }
        if tsf != 0 {
            if pos + 8 > logical_end { bail!("VRT fractional timestamp is truncated"); }
            frac_ts_offset = Some(pos);
            pos += 8;
        }

        if trailer_present && logical_end.saturating_sub(pos) < 4 {
            bail!("VRT trailer flag set but trailer is truncated");
        }
        let payload_end = if trailer_present { logical_end - 4 } else { logical_end };

        Ok(Self {
            raw: data.to_vec(),
            packet_type,
            class_id_present,
            trailer_present,
            tsi,
            tsf,
            packet_count,
            packet_size_words,
            stream_id_offset,
            class_id_offset,
            int_ts_offset,
            frac_ts_offset,
            payload_offset: pos,
            payload_end,
        })
    }

    /// VITA-49 packet types 0..=3 carry sample/data payloads.
    pub fn is_data_packet(&self) -> bool {
        matches!(self.packet_type, 0..=3)
    }

    /// VITA-49 packet types 4 and 5 are context packets, not sample payloads.
    pub fn is_context_packet(&self) -> bool {
        matches!(self.packet_type, 4 | 5)
    }

    pub fn packet_type_name(&self) -> &'static str {
        match self.packet_type {
            0 => "IF data (no Stream ID)",
            1 => "IF data (Stream ID)",
            2 => "Ext data (no Stream ID)",
            3 => "Ext data (Stream ID)",
            4 => "IF context",
            5 => "Ext context",
            6 => "Command",
            7 => "Extension command",
            _ => "reserved",
        }
    }

    pub fn stream_id(&self) -> Option<u32> {
        self.stream_id_offset.and_then(|o| read_be_u32(&self.raw, o))
    }

    pub fn class_id(&self) -> Option<u64> {
        self.class_id_offset.and_then(|o| read_be_u64(&self.raw, o))
    }

    pub fn set_stream_id(&mut self, v: u32) -> bool {
        self.stream_id_offset.map(|o| write_be_u32(&mut self.raw, o, v)).unwrap_or(false)
    }

    pub fn set_packet_count(&mut self, v: u8) {
        if let Some(mut w) = read_be_u32(&self.raw, 0) {
            w = (w & !(0x0f << 16)) | (((v as u32) & 0x0f) << 16);
            write_be_u32(&mut self.raw, 0, w);
            self.packet_count = v & 0x0f;
        }
    }

    pub fn set_packet_size_words(&mut self, words: u16) {
        if let Some(mut w) = read_be_u32(&self.raw, 0) {
            w = (w & !0xffff) | words as u32;
            write_be_u32(&mut self.raw, 0, w);
            self.packet_size_words = words;
        }
    }

    pub fn payload(&self) -> &[u8] {
        &self.raw[self.payload_offset.min(self.raw.len())..self.payload_end.min(self.raw.len())]
    }

    pub fn replace_payload(&mut self, payload: &[u8]) {
        let before = self.raw[..self.payload_offset.min(self.raw.len())].to_vec();
        let trailer = if self.trailer_present && self.payload_end <= self.raw.len() {
            self.raw[self.payload_end..].to_vec()
        } else { Vec::new() };

        let mut new_raw = before;
        new_raw.extend_from_slice(payload);
        while (new_raw.len() + trailer.len()) % 4 != 0 { new_raw.push(0); }
        self.payload_end = new_raw.len();
        new_raw.extend_from_slice(&trailer);
        self.raw = new_raw;
        self.set_packet_size_words((self.raw.len() / 4) as u16);
    }

    pub fn truncate(&mut self, remove_bytes: usize, keep_declared_size: bool) {
        if remove_bytes == 0 || self.raw.len() <= 4 { return; }
        let n = remove_bytes.min(self.raw.len() - 4);
        self.raw.truncate(self.raw.len() - n);
        self.payload_end = self.payload_end.min(self.raw.len());
        if !keep_declared_size {
            self.set_packet_size_words(((self.raw.len() + 3) / 4) as u16);
        }
    }

    /// Convert a VITA timestamp to seconds when its representation permits it.
    /// TSI values are integer seconds. For TSF=2 (real-time), VITA-49 encodes
    /// the fractional field in picoseconds; this is the mode used by Bx01.
    /// TSF=1/3 are count domains and cannot be converted to wall-clock seconds
    /// without additional stream metadata, so only the integer component is used.
    pub fn timestamp_seconds(&self) -> Option<f64> {
        if self.int_ts_offset.is_none() && self.frac_ts_offset.is_none() { return None; }
        let mut sec = 0.0;
        if let Some(o) = self.int_ts_offset {
            sec += read_be_u32(&self.raw, o)? as f64;
        }
        if let Some(o) = self.frac_ts_offset {
            let frac = read_be_u64(&self.raw, o)?;
            if self.tsf == 2 { sec += frac as f64 * 1.0e-12; }
        }
        Some(sec)
    }

    /// Rewrite a timestamp in seconds. Fractional rewriting is defined here for
    /// TSF=2 (real-time/picoseconds), which matches the RS-34 Bx01 stream.
    pub fn set_timestamp_seconds(&mut self, value: f64) -> bool {
        if self.int_ts_offset.is_none() && self.frac_ts_offset.is_none() { return false; }
        let v = value.max(0.0);
        let whole_f = v.floor();
        let whole = whole_f as u64;
        let frac = v - whole_f;
        if let Some(o) = self.int_ts_offset {
            if !write_be_u32(&mut self.raw, o, whole.min(u32::MAX as u64) as u32) { return false; }
        }
        if let Some(o) = self.frac_ts_offset {
            if self.tsf != 2 { return false; }
            let ps = (frac * 1.0e12).round().clamp(0.0, 999_999_999_999.0) as u64;
            if !write_be_u64(&mut self.raw, o, ps) { return false; }
        }
        true
    }
}

pub fn decode_payload(frame: &VrtFrame, fmt: SampleFormat, iq: bool) -> Vec<Complex64> {
    let b = frame.payload();
    let step = fmt.bytes_per_scalar();
    if step == 0 { return Vec::new(); }
    let n = b.len() / step;
    let mut scalars = Vec::with_capacity(n);
    for i in 0..n {
        let s = &b[i * step..(i + 1) * step];
        let v = match fmt {
            SampleFormat::BeI8 | SampleFormat::LeI8 => (s[0] as i8) as f64,
            SampleFormat::BeI16 => i16::from_be_bytes([s[0], s[1]]) as f64,
            SampleFormat::LeI16 => i16::from_le_bytes([s[0], s[1]]) as f64,
            SampleFormat::BeI32 => i32::from_be_bytes([s[0], s[1], s[2], s[3]]) as f64,
            SampleFormat::LeI32 => i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f64,
            SampleFormat::BeF32 => f32::from_be_bytes([s[0], s[1], s[2], s[3]]) as f64,
            SampleFormat::LeF32 => f32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f64,
            SampleFormat::BeF64 => f64::from_be_bytes(s.try_into().unwrap()),
            SampleFormat::LeF64 => f64::from_le_bytes(s.try_into().unwrap()),
        };
        scalars.push(v);
    }
    if iq {
        scalars.chunks_exact(2).map(|q| Complex64::new(q[0], q[1])).collect()
    } else {
        scalars.into_iter().map(|x| Complex64::new(x, 0.0)).collect()
    }
}

fn clamp_round(v: f64, lo: f64, hi: f64) -> f64 { v.round().clamp(lo, hi) }

pub fn encode_payload(samples: &[Complex64], fmt: SampleFormat, iq: bool) -> Vec<u8> {
    let mut vals = Vec::with_capacity(samples.len() * if iq { 2 } else { 1 });
    for z in samples {
        vals.push(z.re);
        if iq { vals.push(z.im); }
    }
    let mut out = Vec::with_capacity(vals.len() * fmt.bytes_per_scalar());
    for v in vals {
        match fmt {
            SampleFormat::BeI8 | SampleFormat::LeI8 => out.push(clamp_round(v, i8::MIN as f64, i8::MAX as f64) as i8 as u8),
            SampleFormat::BeI16 => out.extend_from_slice(&(clamp_round(v, i16::MIN as f64, i16::MAX as f64) as i16).to_be_bytes()),
            SampleFormat::LeI16 => out.extend_from_slice(&(clamp_round(v, i16::MIN as f64, i16::MAX as f64) as i16).to_le_bytes()),
            SampleFormat::BeI32 => out.extend_from_slice(&(clamp_round(v, i32::MIN as f64, i32::MAX as f64) as i32).to_be_bytes()),
            SampleFormat::LeI32 => out.extend_from_slice(&(clamp_round(v, i32::MIN as f64, i32::MAX as f64) as i32).to_le_bytes()),
            SampleFormat::BeF32 => out.extend_from_slice(&(v as f32).to_be_bytes()),
            SampleFormat::LeF32 => out.extend_from_slice(&(v as f32).to_le_bytes()),
            SampleFormat::BeF64 => out.extend_from_slice(&v.to_be_bytes()),
            SampleFormat::LeF64 => out.extend_from_slice(&v.to_le_bytes()),
        }
    }
    out
}

pub fn build_demo_packet(
    samples: &[Complex64], stream_id: u32, packet_count: u8,
    fmt: SampleFormat, iq: bool, timestamp: f64,
) -> Vec<u8> {
    let tsi = 1u32;
    let tsf = 2u32;
    let ptype = 1u32;
    let class_id = 0x004f_5054_5257_7834u64;
    let mut payload = encode_payload(samples, fmt, iq);
    while payload.len() % 4 != 0 { payload.push(0); }
    // Mirror the observed Bx01 wire geometry: header + stream ID + class ID +
    // integer timestamp + fractional timestamp + payload + trailer.
    let total_words = 1 + 1 + 2 + 1 + 2 + payload.len() / 4 + 1;
    let word0 = (ptype << 28) | (1u32 << 27) | (1u32 << 26) | (tsi << 22) | (tsf << 20)
        | (((packet_count as u32) & 0x0f) << 16) | total_words as u32;
    let whole = timestamp.floor() as u32;
    // TSF=2 is real-time fractional timestamp in picoseconds.
    let frac = ((timestamp - timestamp.floor()) * 1.0e12).round().clamp(0.0, 999_999_999_999.0) as u64;
    let mut out = Vec::with_capacity(total_words * 4);
    out.extend_from_slice(&word0.to_be_bytes());
    out.extend_from_slice(&stream_id.to_be_bytes());
    out.extend_from_slice(&class_id.to_be_bytes());
    out.extend_from_slice(&whole.to_be_bytes());
    out.extend_from_slice(&frac.to_be_bytes());
    out.extend_from_slice(&payload);
    out.extend_from_slice(&0u32.to_be_bytes()); // VITA trailer placeholder for demo
    out
}

#[cfg(test)]
pub fn build_demo_context_packet(stream_id: u32, packet_count: u8, timestamp: f64) -> Vec<u8> {
    let tsi = 1u32;
    let tsf = 2u32;
    let ptype = 4u32;
    let class_id = 0x004f_5054_5257_7834u64;
    let payload = [0u8; 48];
    let total_words = 1 + 1 + 2 + 1 + 2 + payload.len() / 4;
    let word0 = (ptype << 28) | (1u32 << 27) | (tsi << 22) | (tsf << 20)
        | (((packet_count as u32) & 0x0f) << 16) | total_words as u32;
    let whole = timestamp.floor() as u32;
    let frac = ((timestamp - timestamp.floor()) * 1.0e12).round().clamp(0.0, 999_999_999_999.0) as u64;
    let mut out = Vec::with_capacity(total_words * 4);
    out.extend_from_slice(&word0.to_be_bytes());
    out.extend_from_slice(&stream_id.to_be_bytes());
    out.extend_from_slice(&class_id.to_be_bytes());
    out.extend_from_slice(&whole.to_be_bytes());
    out.extend_from_slice(&frac.to_be_bytes());
    out.extend_from_slice(&payload);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_round_trip() {
        let x: Vec<_> = (0..500).map(|i| Complex64::new(i as f64, 0.0)).collect();
        let raw = build_demo_packet(&x, 0x42783031, 3, SampleFormat::BeI32, false, 1_700_000_000.25);
        let f = VrtFrame::parse(&raw).unwrap();
        assert_eq!(f.stream_id(), Some(0x42783031));
        assert_eq!(f.packet_count, 3);
        assert!(f.is_data_packet());
        assert!(!f.is_context_packet());
        assert_eq!(f.class_id(), Some(0x004f_5054_5257_7834));
        assert_eq!(raw.len(), 2032);
        assert_eq!(f.packet_size_words, 508);
        let y = decode_payload(&f, SampleFormat::BeI32, false);
        assert_eq!(y.len(), 500);
        assert_eq!(y[123].re, 123.0);
        let ts = f.timestamp_seconds().unwrap();
        assert!((ts - 1_700_000_000.25).abs() < 1e-9);
    }

    #[test]
    fn context_packet_is_not_sample_data() {
        let raw = build_demo_context_packet(0x42783031, 7, 1_700_000_001.0);
        let f = VrtFrame::parse(&raw).unwrap();
        assert_eq!(raw.len(), 76);
        assert_eq!(f.payload().len(), 48);
        assert_eq!(f.packet_type, 4);
        assert!(f.is_context_packet());
        assert!(!f.is_data_packet());
        assert_eq!(f.packet_type_name(), "IF context");
    }

    #[test]
    fn stale_size_truncation_is_rejected() {
        let x = vec![Complex64::new(1.0, 0.0); 16];
        let mut raw = build_demo_packet(&x, 0x42783031, 0, SampleFormat::BeI32, false, 1000.0);
        raw.truncate(raw.len() - 8);
        assert!(VrtFrame::parse(&raw).is_err());
    }
}
