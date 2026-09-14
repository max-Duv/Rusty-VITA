# v0.4 — Operator Workstation / Real-Data Pass

## GUI

- Rebuilt the eframe/egui workstation around the selected Operator Workstation concept.
- Added compact command/status header, pass-through/armed/active state strip, left fault rail, right health/event rail, six live metric cards with sparklines, central waterfall/spectrum/waveform workspace, source table, and comparison metrics.
- Replaced arbitrary raw FFT magnitudes with dBFS-normalized spectra.
- Added real waterfall fault-boundary annotations and explicit transport-outage floor columns.
- Added real source tracking, source ages, planned clone targets, comparison metrics, health and event views.

## Runtime / analytics

- Added `analytics.rs` for clean/chaos signal statistics and persistent spectral-source tracking.
- Added `telemetry.rs` for actual Linux process CPU and RSS readings.
- Added source queue depth and last-RX timestamp to capture status.
- Added dBFS RMS/peak, occupied-bandwidth, spectral-flatness, normalized-correlation and spectral-delta calculations.
- Worker snapshots now carry real analytics and process telemetry.

## No-fake-data policy

- Removed the production synthetic/demo packet-source command.
- Removed hardware-free demo launch scripts.
- Missing runtime data now renders as waiting/`—` instead of prefilled plausible values.
- UI event rows are sourced only from actual worker/engine events.
- Planned ghost frequencies are derived from real detected sources and clearly separated from measured sources.

## Safety

The TEST multicast boundary is unchanged: launch-time `--allow-emit` and explicit GUI opt-in are both required, and output cannot equal the input multicast group+port.
