use crate::chaos::{ChaosConfig, SeqMode};

pub const CASCADE_NAME: &str = "DEMO: Cascading incident";

pub const NAMES: [&str; 14] = [
    "Clean / pass-through",
    "DEMO: Hard blackout",
    "DEMO: Repeating burst loss",
    CASCADE_NAME,
    "Packet path degradation",
    "Timing collapse",
    "Protocol corruption",
    "RF degradation",
    "DEMO: Clone detected emitters",
    "DEMO: Emitter swarm",
    "DEMO: Power-neutral ghosts",
    "I/Q path fault",
    "Consumer stall invariant",
    "Mixed failure",
];

pub fn named(name: &str) -> ChaosConfig {
    let mut c = ChaosConfig::default();
    match name {
        "DEMO: Hard blackout" => c.transport.blackout = true,
        "DEMO: Repeating burst loss" => {
            c.transport.burst_enabled = true;
            c.transport.burst_drop_packets = 250;
            c.transport.burst_every_packets = 2500;
        }
        "Packet path degradation" => {
            c.transport.drop_pct = 5.0;
            c.transport.duplicate_pct = 1.0;
            c.transport.jitter_ms = 8.0;
            c.transport.reorder_pct = 8.0;
            c.transport.reorder_window = 5;
        }
        "Timing collapse" => {
            c.protocol.ts_drift_ppm = 250.0;
            c.protocol.ts_jitter_us = 150.0;
            c.protocol.seq_mode = SeqMode::Jump;
            c.protocol.seq_jump = 3;
            c.protocol.seq_every_packets = 64;
        }
        "Protocol corruption" => {
            c.protocol.seq_mode = SeqMode::Random;
            c.protocol.seq_every_packets = 24;
            c.protocol.truncate_pct = 1.0;
            c.protocol.truncate_bytes = 8;
            c.protocol.header_bitflip_pct = 0.5;
        }
        "RF degradation" => {
            c.signal.enabled = true;
            c.signal.gain = 0.65;
            c.signal.noise_enabled = true;
            c.signal.noise_snr_db = 14.0;
            c.signal.sample_dropout_pct = 0.5;
            c.signal.tone_enabled = true;
            c.signal.tone_hz = 3200.0;
            c.signal.tone_amp = 500.0;
        }
        "DEMO: Clone detected emitters" => {
            c.signal.enabled = true;
            c.signal.emitter_clone_enabled = true;
            c.signal.emitter_clone_copies = 2;
            c.signal.emitter_clone_spacing_hz = 12_500.0;
            c.signal.emitter_clone_gain = 0.70;
            c.signal.emitter_clone_threshold_db = 10.0;
            c.signal.emitter_clone_max_emitters = 2;
            c.signal.emitter_clone_half_width_bins = 2;
            c.signal.emitter_clone_bidirectional = true;
            c.signal.emitter_clone_gain_decay = 0.82;
            c.signal.emitter_clone_occupancy_guard_db = 8.0;
        }
        "DEMO: Emitter swarm" => {
            c.signal.enabled = true;
            c.signal.emitter_clone_enabled = true;
            c.signal.emitter_clone_copies = 4;
            c.signal.emitter_clone_spacing_hz = 8_000.0;
            c.signal.emitter_clone_gain = 0.62;
            c.signal.emitter_clone_gain_decay = 0.74;
            c.signal.emitter_clone_jitter_hz = 900.0;
            c.signal.emitter_clone_threshold_db = 9.0;
            c.signal.emitter_clone_max_emitters = 3;
            c.signal.emitter_clone_half_width_bins = 2;
            c.signal.emitter_clone_bidirectional = true;
            c.signal.emitter_clone_occupancy_guard_db = 7.0;
        }
        "DEMO: Power-neutral ghosts" => {
            c.signal.enabled = true;
            c.signal.emitter_clone_enabled = true;
            c.signal.emitter_clone_copies = 2;
            c.signal.emitter_clone_spacing_hz = 15_000.0;
            c.signal.emitter_clone_gain = 0.55;
            c.signal.emitter_clone_gain_decay = 0.88;
            c.signal.emitter_clone_threshold_db = 10.0;
            c.signal.emitter_clone_max_emitters = 2;
            c.signal.emitter_clone_half_width_bins = 3;
            c.signal.emitter_clone_bidirectional = true;
            c.signal.emitter_clone_occupancy_guard_db = 8.0;
            c.signal.emitter_clone_preserve_rms = true;
        }
        "I/Q path fault" => {
            c.signal.enabled = true;
            c.signal.iq_swap = true;
            c.signal.iq_conjugate = true;
            c.signal.phase_deg = 30.0;
        }
        "Consumer stall invariant" => {
            c.system.consumer_stall_every = 200;
            c.system.consumer_stall_ms = 300.0;
        }
        "Mixed failure" => {
            c.transport.drop_pct = 5.0;
            c.transport.duplicate_pct = 1.0;
            c.transport.jitter_ms = 8.0;
            c.transport.reorder_pct = 8.0;
            c.transport.reorder_window = 5;
            c.protocol.ts_drift_ppm = 80.0;
            c.protocol.ts_jitter_us = 50.0;
            c.protocol.seq_mode = SeqMode::Jump;
            c.protocol.seq_jump = 2;
            c.protocol.seq_every_packets = 100;
            c.signal.enabled = true;
            c.signal.gain = 0.8;
            c.signal.noise_enabled = true;
            c.signal.noise_snr_db = 18.0;
            c.signal.sample_dropout_pct = 0.2;
        }
        _ => {}
    }
    c
}


/// A deliberately legible 45-second incident sequence. `None` means the
/// sequence is complete and the worker should return to pass-through.
pub fn cascade_phase(elapsed_s: f64) -> Option<(usize, &'static str, ChaosConfig)> {
    let mut c = ChaosConfig::default();
    let (idx, label) = if elapsed_s < 5.0 {
        (0, "Baseline")
    } else if elapsed_s < 10.0 {
        c.transport.drop_pct = 10.0;
        (1, "+10% packet loss")
    } else if elapsed_s < 15.0 {
        c.transport.drop_pct = 10.0;
        c.transport.jitter_ms = 20.0;
        (2, "+20 ms jitter")
    } else if elapsed_s < 20.0 {
        c.transport.drop_pct = 10.0;
        c.transport.jitter_ms = 20.0;
        c.transport.reorder_pct = 25.0;
        c.transport.reorder_window = 8;
        (3, "+25% reorder")
    } else if elapsed_s < 25.0 {
        c.transport.drop_pct = 10.0;
        c.transport.jitter_ms = 20.0;
        c.transport.reorder_pct = 25.0;
        c.transport.reorder_window = 8;
        c.protocol.ts_drift_ppm = 250.0;
        c.protocol.ts_jitter_us = 150.0;
        (4, "+timing corruption")
    } else if elapsed_s < 30.0 {
        c.transport.drop_pct = 10.0;
        c.transport.jitter_ms = 20.0;
        c.transport.reorder_pct = 25.0;
        c.transport.reorder_window = 8;
        c.protocol.ts_drift_ppm = 250.0;
        c.protocol.ts_jitter_us = 150.0;
        c.signal.enabled = true;
        c.signal.noise_enabled = true;
        c.signal.noise_snr_db = 6.0;
        (5, "+AWGN @ 6 dB SNR")
    } else if elapsed_s < 35.0 {
        c.transport.drop_pct = 10.0;
        c.transport.jitter_ms = 20.0;
        c.transport.reorder_pct = 25.0;
        c.transport.reorder_window = 8;
        c.protocol.ts_drift_ppm = 250.0;
        c.protocol.ts_jitter_us = 150.0;
        (6, "recover signal domain")
    } else if elapsed_s < 40.0 {
        c.transport.drop_pct = 10.0;
        c.transport.jitter_ms = 20.0;
        c.transport.reorder_pct = 25.0;
        c.transport.reorder_window = 8;
        (7, "recover timing domain")
    } else if elapsed_s < 45.0 {
        (8, "recover transport domain")
    } else {
        return None;
    };
    Some((idx, label, c))
}
