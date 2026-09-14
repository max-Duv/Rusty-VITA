use num_complex::Complex64;
use rustfft::{num_complex::Complex, FftPlanner};

use crate::cli::SampleFormat;

#[derive(Debug, Clone, Default)]
pub struct SpectrumTrace {
    pub freq_khz: Vec<f64>,
    pub dbfs: Vec<f64>,
}

pub fn full_scale(fmt: SampleFormat) -> f64 {
    match fmt {
        SampleFormat::BeI8 | SampleFormat::LeI8 => i8::MAX as f64,
        SampleFormat::BeI16 | SampleFormat::LeI16 => i16::MAX as f64,
        SampleFormat::BeI32 | SampleFormat::LeI32 => i32::MAX as f64,
        SampleFormat::BeF32 | SampleFormat::LeF32 | SampleFormat::BeF64 | SampleFormat::LeF64 => 1.0,
    }
}

/// Windowed FFT with amplitude normalized to the configured sample representation's
/// full-scale value. The result is therefore suitable for a dBFS UI rather than the
/// arbitrary raw-unit dB values used by the early prototype.
pub fn spectrum_dbfs(
    samples: &[Complex64],
    fs: f64,
    iq: bool,
    fmt: SampleFormat,
    nfft: usize,
) -> SpectrumTrace {
    let n = nfft.max(64).next_power_of_two();
    if samples.len() < n || !fs.is_finite() || fs <= 0.0 {
        return SpectrumTrace::default();
    }

    let src = &samples[samples.len() - n..];
    let mean = src.iter().copied().sum::<Complex64>() / n as f64;
    let mut window_sum = 0.0;
    let mut buf: Vec<Complex<f64>> = src
        .iter()
        .enumerate()
        .map(|(i, z)| {
            let w = 0.5
                - 0.5
                    * (std::f64::consts::TAU * i as f64 / (n.saturating_sub(1).max(1)) as f64)
                        .cos();
            window_sum += w;
            let v = (*z - mean) * w;
            Complex { re: v.re, im: v.im }
        })
        .collect();

    let mut planner = FftPlanner::<f64>::new();
    planner.plan_fft_forward(n).process(&mut buf);

    let fs_amp = full_scale(fmt).max(f64::MIN_POSITIVE);
    let coherent = window_sum.max(f64::MIN_POSITIVE);
    let db = |amp: f64| 20.0 * (amp.max(1e-15) / fs_amp).log10();

    if iq {
        let half = n / 2;
        let mut freq_khz = Vec::with_capacity(n);
        let mut dbfs = Vec::with_capacity(n);
        for k in 0..n {
            let idx = (k + half) % n;
            let amp = buf[idx].norm() / coherent;
            freq_khz.push((k as f64 - half as f64) * fs / n as f64 / 1000.0);
            dbfs.push(db(amp));
        }
        SpectrumTrace { freq_khz, dbfs }
    } else {
        let bins = n / 2 + 1;
        let mut freq_khz = Vec::with_capacity(bins);
        let mut dbfs = Vec::with_capacity(bins);
        for k in 0..bins {
            let one_sided = if k == 0 || (n % 2 == 0 && k == n / 2) { 1.0 } else { 2.0 };
            let amp = one_sided * buf[k].norm() / coherent;
            freq_khz.push(k as f64 * fs / n as f64 / 1000.0);
            dbfs.push(db(amp));
        }
        SpectrumTrace { freq_khz, dbfs }
    }
}

/// Retained for internal backwards compatibility. New UI/analytics code should use
/// spectrum_dbfs so the plotted units have physical meaning.
pub fn spectrum_db(samples: &[Complex64], fs: f64, iq: bool, nfft: usize) -> (Vec<f64>, Vec<f64>) {
    let n = nfft.max(64).next_power_of_two();
    if samples.len() < n {
        return (Vec::new(), Vec::new());
    }
    let src = &samples[samples.len() - n..];
    let mean = src.iter().copied().sum::<Complex64>() / n as f64;
    let mut buf: Vec<Complex<f64>> = src
        .iter()
        .enumerate()
        .map(|(i, z)| {
            let w = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / (n - 1) as f64).cos();
            let v = (*z - mean) * w;
            Complex { re: v.re, im: v.im }
        })
        .collect();
    let mut planner = FftPlanner::<f64>::new();
    planner.plan_fft_forward(n).process(&mut buf);
    let to_db = |z: &Complex<f64>| 20.0 * (z.norm() + 1e-12).log10();
    if iq {
        let half = n / 2;
        let mut f = Vec::with_capacity(n);
        let mut db = Vec::with_capacity(n);
        for k in 0..n {
            let idx = (k + half) % n;
            f.push((k as f64 - half as f64) * fs / n as f64 / 1000.0);
            db.push(to_db(&buf[idx]));
        }
        (f, db)
    } else {
        let bins = n / 2 + 1;
        let mut f = Vec::with_capacity(bins);
        let mut db = Vec::with_capacity(bins);
        for k in 0..bins {
            f.push(k as f64 * fs / n as f64 / 1000.0);
            db.push(to_db(&buf[k]));
        }
        (f, db)
    }
}

pub fn rms(samples: &[Complex64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|z| z.norm_sqr()).sum::<f64>() / samples.len() as f64).sqrt()
}

pub fn peak_abs(samples: &[Complex64]) -> f64 {
    samples.iter().map(|z| z.norm()).fold(0.0, f64::max)
}

pub fn rms_dbfs(samples: &[Complex64], fmt: SampleFormat) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let v = rms(samples) / full_scale(fmt).max(f64::MIN_POSITIVE);
    Some(20.0 * v.max(1e-15).log10())
}

pub fn peak_dbfs(samples: &[Complex64], fmt: SampleFormat) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let v = peak_abs(samples) / full_scale(fmt).max(f64::MIN_POSITIVE);
    Some(20.0 * v.max(1e-15).log10())
}

pub fn normalized_correlation(a: &[Complex64], b: &[Complex64]) -> Option<f64> {
    let n = a.len().min(b.len()).min(8192);
    if n < 16 {
        return None;
    }
    let aa = &a[a.len() - n..];
    let bb = &b[b.len() - n..];
    let ma = aa.iter().copied().sum::<Complex64>() / n as f64;
    let mb = bb.iter().copied().sum::<Complex64>() / n as f64;
    let mut num = Complex64::new(0.0, 0.0);
    let mut da = 0.0;
    let mut db = 0.0;
    for (x, y) in aa.iter().zip(bb.iter()) {
        let xa = *x - ma;
        let yb = *y - mb;
        num += xa.conj() * yb;
        da += xa.norm_sqr();
        db += yb.norm_sqr();
    }
    if da <= 0.0 || db <= 0.0 {
        return Some(1.0);
    }
    Some((num.norm() / (da.sqrt() * db.sqrt())).clamp(0.0, 1.0))
}

pub fn spectral_rms_delta_db(clean: &SpectrumTrace, chaos: &SpectrumTrace) -> Option<f64> {
    let n = clean.dbfs.len().min(chaos.dbfs.len());
    if n == 0 {
        return None;
    }
    let mse = clean
        .dbfs
        .iter()
        .zip(chaos.dbfs.iter())
        .take(n)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        / n as f64;
    Some(mse.sqrt())
}

pub fn occupied_bandwidth_khz(trace: &SpectrumTrace, fraction: f64) -> Option<f64> {
    let n = trace.freq_khz.len().min(trace.dbfs.len());
    if n < 3 {
        return None;
    }
    let fraction = fraction.clamp(0.5, 0.9999);
    let powers: Vec<f64> = trace
        .dbfs
        .iter()
        .take(n)
        .map(|db| 10f64.powf(*db / 10.0))
        .collect();
    let total: f64 = powers.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        return None;
    }
    let tail = (1.0 - fraction) / 2.0;
    let lo_target = total * tail;
    let hi_target = total * (1.0 - tail);
    let mut sum = 0.0;
    let mut lo = 0usize;
    let mut hi = n - 1;
    let mut lo_set = false;
    for (i, p) in powers.iter().enumerate() {
        sum += *p;
        if !lo_set && sum >= lo_target {
            lo = i;
            lo_set = true;
        }
        if sum >= hi_target {
            hi = i;
            break;
        }
    }
    Some((trace.freq_khz[hi] - trace.freq_khz[lo]).abs())
}

pub fn spectral_flatness(trace: &SpectrumTrace) -> Option<f64> {
    if trace.dbfs.is_empty() {
        return None;
    }
    let mut log_sum = 0.0;
    let mut lin_sum = 0.0;
    let mut n = 0usize;
    for db in &trace.dbfs {
        let p = 10f64.powf(*db / 10.0).max(1e-30);
        if p.is_finite() {
            log_sum += p.ln();
            lin_sum += p;
            n += 1;
        }
    }
    if n == 0 || lin_sum <= 0.0 {
        return None;
    }
    let geom = (log_sum / n as f64).exp();
    let arith = lin_sum / n as f64;
    Some((geom / arith).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbfs_full_scale_sine_is_near_zero_peak_bin() {
        let fs = 250_000.0;
        let n = 2048usize;
        let amp = i32::MAX as f64;
        let bin = 128usize;
        let freq = bin as f64 * fs / n as f64;
        let x: Vec<_> = (0..n)
            .map(|i| Complex64::new(amp * (std::f64::consts::TAU * freq * i as f64 / fs).sin(), 0.0))
            .collect();
        let s = spectrum_dbfs(&x, fs, false, SampleFormat::BeI32, n);
        let peak = s.dbfs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert!(peak > -0.5 && peak < 0.5, "peak was {peak} dBFS");
    }
}
