use std::collections::HashMap;

use num_complex::Complex64;

use crate::cli::SampleFormat;
use crate::dsp::{
    normalized_correlation, occupied_bandwidth_khz, peak_dbfs, rms_dbfs, spectral_flatness,
    spectral_rms_delta_db, spectrum_dbfs, SpectrumTrace,
};

#[derive(Debug, Clone, Default)]
pub struct SignalStatistics {
    pub total_power_dbfs: Option<f64>,
    pub peak_dbfs: Option<f64>,
    pub occupied_bw_khz: Option<f64>,
    pub spectral_flatness: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct ComparisonSnapshot {
    pub clean: SignalStatistics,
    pub chaos: SignalStatistics,
    pub waveform_correlation: Option<f64>,
    pub spectral_delta_db_rms: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct EmitterObservation {
    pub id: u64,
    pub frequency_khz: f64,
    pub level_dbfs: f64,
    pub first_seen_epoch: f64,
    pub last_seen_epoch: f64,
    pub age_s: f64,
}

#[derive(Debug, Clone, Default)]
pub struct AnalysisSnapshot {
    pub clean_spectrum: SpectrumTrace,
    pub chaos_spectrum: SpectrumTrace,
    pub emitters: Vec<EmitterObservation>,
    pub comparison: ComparisonSnapshot,
    pub rbw_hz: f64,
    pub span_khz: f64,
    pub center_khz: f64,
}

#[derive(Debug, Clone)]
struct Track {
    id: u64,
    frequency_khz: f64,
    level_dbfs: f64,
    first_seen_epoch: f64,
    last_seen_epoch: f64,
}

#[derive(Debug, Default)]
pub struct AnalysisEngine {
    tracks: HashMap<i64, Track>,
    next_track_id: u64,
}

impl AnalysisEngine {
    pub fn new() -> Self {
        Self { tracks: HashMap::new(), next_track_id: 1 }
    }

    pub fn update(
        &mut self,
        clean_samples: &[Complex64],
        chaos_samples: &[Complex64],
        fs: f64,
        iq: bool,
        fmt: SampleFormat,
        detection_threshold_db: f64,
        max_emitters: usize,
        now_epoch: f64,
    ) -> AnalysisSnapshot {
        let nfft = choose_nfft(clean_samples.len().max(chaos_samples.len()));
        let clean_spectrum = spectrum_dbfs(clean_samples, fs, iq, fmt, nfft);
        let chaos_spectrum = spectrum_dbfs(chaos_samples, fs, iq, fmt, nfft);
        let rbw_hz = if nfft > 0 { fs / nfft as f64 } else { 0.0 };
        let span_khz = if iq { fs / 1000.0 } else { fs / 2000.0 };
        let center_khz = if iq { 0.0 } else { span_khz / 2.0 };

        let detections = detect_emitters(&clean_spectrum, detection_threshold_db.max(3.0), max_emitters.max(1));
        self.update_tracks(&detections, rbw_hz / 1000.0, now_epoch);
        let emitters = self.current_tracks(now_epoch);

        let clean_stats = SignalStatistics {
            total_power_dbfs: rms_dbfs(clean_samples, fmt),
            peak_dbfs: peak_dbfs(clean_samples, fmt),
            occupied_bw_khz: occupied_bandwidth_khz(&clean_spectrum, 0.99),
            spectral_flatness: spectral_flatness(&clean_spectrum),
        };
        let chaos_stats = SignalStatistics {
            total_power_dbfs: rms_dbfs(chaos_samples, fmt),
            peak_dbfs: peak_dbfs(chaos_samples, fmt),
            occupied_bw_khz: occupied_bandwidth_khz(&chaos_spectrum, 0.99),
            spectral_flatness: spectral_flatness(&chaos_spectrum),
        };
        // Reuse the spectra already computed above; do not perform a second
        // pair of FFTs solely for the comparison metric.
        let spectral_delta = spectral_rms_delta_db(&clean_spectrum, &chaos_spectrum);

        AnalysisSnapshot {
            clean_spectrum,
            chaos_spectrum,
            emitters,
            comparison: ComparisonSnapshot {
                clean: clean_stats,
                chaos: chaos_stats,
                waveform_correlation: normalized_correlation(clean_samples, chaos_samples),
                spectral_delta_db_rms: spectral_delta,
            },
            rbw_hz,
            span_khz,
            center_khz,
        }
    }

    fn update_tracks(&mut self, detections: &[(f64, f64)], bin_khz: f64, now_epoch: f64) {
        let quant = bin_khz.max(0.001) * 2.0;
        for (freq_khz, level_dbfs) in detections {
            let key = (*freq_khz / quant).round() as i64;
            if let Some(track) = self.tracks.get_mut(&key) {
                // Smooth frequency and level slightly to keep a stable engineering table.
                track.frequency_khz = 0.75 * track.frequency_khz + 0.25 * *freq_khz;
                track.level_dbfs = 0.70 * track.level_dbfs + 0.30 * *level_dbfs;
                track.last_seen_epoch = now_epoch;
            } else {
                let id = self.next_track_id;
                self.next_track_id = self.next_track_id.saturating_add(1);
                self.tracks.insert(
                    key,
                    Track {
                        id,
                        frequency_khz: *freq_khz,
                        level_dbfs: *level_dbfs,
                        first_seen_epoch: now_epoch,
                        last_seen_epoch: now_epoch,
                    },
                );
            }
        }
        self.tracks.retain(|_, t| now_epoch - t.last_seen_epoch <= 2.0);
    }

    fn current_tracks(&self, now_epoch: f64) -> Vec<EmitterObservation> {
        let mut out: Vec<_> = self
            .tracks
            .values()
            .map(|t| EmitterObservation {
                id: t.id,
                frequency_khz: t.frequency_khz,
                level_dbfs: t.level_dbfs,
                first_seen_epoch: t.first_seen_epoch,
                last_seen_epoch: t.last_seen_epoch,
                age_s: (now_epoch - t.first_seen_epoch).max(0.0),
            })
            .collect();
        out.sort_by(|a, b| b.level_dbfs.total_cmp(&a.level_dbfs));
        out
    }
}

fn choose_nfft(sample_count: usize) -> usize {
    if sample_count >= 4096 {
        4096
    } else if sample_count >= 2048 {
        2048
    } else if sample_count >= 1024 {
        1024
    } else if sample_count >= 512 {
        512
    } else {
        256
    }
}

fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v: Vec<_> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let m = v.len() / 2;
    Some(if v.len() % 2 == 0 { (v[m - 1] + v[m]) * 0.5 } else { v[m] })
}

/// Detect narrow/local spectral maxima above a robust median floor. This is not a
/// modulation classifier; the UI labels these honestly as detected spectral sources.
fn detect_emitters(trace: &SpectrumTrace, threshold_above_median_db: f64, max_count: usize) -> Vec<(f64, f64)> {
    let n = trace.freq_khz.len().min(trace.dbfs.len());
    if n < 7 {
        return Vec::new();
    }
    let floor = median(&trace.dbfs[..n]).unwrap_or(-120.0);
    let threshold = floor + threshold_above_median_db.max(3.0);
    let mut candidates = Vec::<(usize, f64)>::new();
    for i in 3..n.saturating_sub(3) {
        let y = trace.dbfs[i];
        if y < threshold {
            continue;
        }
        if y >= trace.dbfs[i - 1]
            && y >= trace.dbfs[i + 1]
            && y >= trace.dbfs[i - 2]
            && y >= trace.dbfs[i + 2]
        {
            candidates.push((i, y));
        }
    }
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1));

    let mut selected = Vec::<(usize, f64)>::new();
    for (idx, db) in candidates {
        if selected.iter().any(|(j, _)| idx.abs_diff(*j) < 4) {
            continue;
        }
        selected.push((idx, db));
        if selected.len() >= max_count.max(1) {
            break;
        }
    }
    selected
        .into_iter()
        .map(|(i, db)| (trace.freq_khz[i], db))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_uses_real_fft_peak() {
        let fs = 250_000.0;
        let n = 4096usize;
        let amp = i32::MAX as f64 * 0.25;
        let f0 = 31_250.0;
        let samples: Vec<_> = (0..n)
            .map(|i| Complex64::new(amp * (std::f64::consts::TAU * f0 * i as f64 / fs).sin(), 0.0))
            .collect();
        let mut engine = AnalysisEngine::new();
        let a = engine.update(&samples, &samples, fs, false, SampleFormat::BeI32, 9.0, 12, 1000.0);
        assert!(a.emitters.iter().any(|e| (e.frequency_khz - 31.25).abs() < 0.5));
        assert!(a.comparison.waveform_correlation.unwrap_or(0.0) > 0.999);
    }
}
