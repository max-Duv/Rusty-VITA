# Operator Workstation UI Conformance

Reference: `docs/operator_workstation_reference.png`

The selected concept is treated as a structural design target rather than a source of dummy content. v0.5 implements its visual grammar while binding every field to actual runtime state.

## Region mapping

- **Header:** product identity, configured live source, real sample rate/interface, mode, command controls.
- **Pass-through strip:** reflects worker state (`pass-through`, `armed`, `chaos active`) and measured clean/chaos PPS.
- **Left rail:** scenario selection and all transport/VITA/signal/system fault parameters. Nothing is hidden behind mock controls.
- **Metric strip:** six cards populated from real metrics and rolling real-value sparklines.
- **Waterfall:** rolling real chaos-branch FFT output in dBFS. Fault boundaries are annotations tied to actual engine-state transitions. A blank/floor column is inserted only for a real internally-induced transport outage.
- **Spectrum:** real clean/chaos FFTs plus measured noise floor and real-source detector markers. Planned clone markers are explicitly planned targets derived from detected sources.
- **Waveform:** real decoded sample windows.
- **Emitter/source table:** tracked measured spectral sources; separate tab for planned clone targets.
- **Comparison metrics:** mathematically derived from current clean/chaos sample windows.
- **System-health rail:** real source rate, process telemetry, queue depth, drops, and parse errors.
- **Experiment-impact rail:** computed from engine counters and current clean/chaos divergence.
- **Event log:** actual worker/engine events only.
- **Footer:** actual capture backend, source counters, process telemetry, health, and UTC time.

## Intentional differences from the concept art

Where the concept showed a plausible but invented value, v0.5 prefers correctness:

- The mockup's synthetic/live selector is replaced by actual mode (`LIVE` or `PCAP REPLAY`); the production synthetic source is removed.
- Center/span and RBW are derived from sample rate, IQ/real mode, and FFT length rather than hard-coded to the mockup.
- CPU/RSS and queue depth are observed from Linux/runtime state.
- Missing metrics display `—`/waiting states.
- Event rows are not seeded for visual effect.
- Source rows appear only after real spectral peaks satisfy the detector threshold.
