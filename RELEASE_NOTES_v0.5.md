# VITA-49 RF Chaos Workbench v0.5.0

## Operator Workstation fidelity rebuild

v0.5 is the first pass driven directly by the on-box RS5 screenshot rather than the design mockup alone. The goal is to make the actual Linux application read like a mature RF engineering workstation while keeping every displayed datum traceable to real runtime state.

### Visual / UX

- Fixed left and right operator rails so persisted panel widths cannot collapse the layout into narrow strips.
- Increased typography, interaction height, panel spacing, header weight, and metric-card size for RDP/VNC/high-DPI Linux sessions.
- Added instrument-panel framing and accent rails to separate analysis surfaces without decorative fake telemetry.
- Increased waterfall, spectrum, and waveform plot heights.
- Added a more legible analysis toolbar, status treatment, and source/comparison panels.
- The right health/event rail is now scroll-safe and wide enough for complete labels and values.

### Real-signal display fixes

- Waterfall levels now auto-range from robust percentiles of the **actual dBFS spectrum**. This fixes the nearly-black waterfall seen when Bx01 energy sits below the old hard-coded -120 dBFS floor.
- Spectrum Y range now follows the real signal/noise distribution rather than forcing -120..0 dBFS.
- Time waveform now plots **raw decoded sample units with automatic Y range** instead of forcing full-scale-normalized samples into ±1, which made low-amplitude real Bx01 data appear flat.
- Emitter detection adds local-prominence rejection and the displayed tracker list is bounded, reducing noise-bin clutter in the source table.

### Telemetry correctness

- Linux process CPU is normalized to total logical host capacity while retaining raw core-equivalent usage internally. The previous display could legitimately show >100% but read like an error (for example ~720% = ~7.2 saturated cores).
- System health degrades when the clean observer has real sequence/timing anomalies instead of showing NOMINAL merely because parsing and geometry are valid.

### Probe improvements

- `probe` now reports **wall rate** and **capture-time wire rate** separately.
- Wire rate uses `frame.time_epoch`, avoiding TShark startup/drain bias in short and medium probes.
- Probe prints packet/payload geometry and VITA packet-type histograms so `geometry diff` counts can be explained instead of treated as an opaque number.
- It also reports a capture-time-derived wire sample rate.

### Safety

No emission policy was relaxed. TEST multicast remains launch-policy gated and disabled by default. Clean capture remains immutable.
