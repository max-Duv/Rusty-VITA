# VITA-49 RF Chaos Workbench — Rust v0.4

A native Rust VITA-49/VRT chaos-engineering workstation built around the **real RS-34 Bx01 stream on RS5**. v0.4 replaces the prototype dashboard with the selected **Operator Workstation** interface and removes the production synthetic/demo packet source.

The design reference is preserved at `docs/operator_workstation_reference.png`.

## What v0.4 means by “real data”

The production GUI does not fabricate packet rates, spectra, waveforms, emitters, CPU/memory values, event rows, comparison metrics, or health state.

- **Capture** comes from live TShark/native-pcap input or a user-supplied PCAP.
- **Clean metrics** are measured from received VITA packets.
- **Chaos metrics** are measured from the actual packets emitted by the internal fault engine.
- **Spectrum/waterfall** are FFTs of decoded VITA payload samples and are normalized to dBFS for the configured sample format.
- **Waveform** comes from decoded clean/chaos sample buffers.
- **Emitter/source rows** come from live spectral peak tracking. They are explicitly detector outputs, not modulation classifications.
- **Planned ghost rows** are derived from detected sources plus the currently configured cloning parameters and are clearly labeled as planned/configured.
- **Process CPU/RSS** come from Linux `/proc`; unsupported/unavailable values render as `—` rather than placeholders.
- **Event log** contains only source, worker, and chaos-engine events that actually occurred.
- **Health state** is computed from real source freshness, parser state, and capture backlog.
- **TEST multicast** is off by default and still requires both launch-time permission and an explicit GUI enable.

There is intentionally **no production `demo`/synthetic-source subcommand** in v0.4. Synthetic packets remain only in Rust unit tests, where deterministic fixtures are necessary to test parsing/DSP/fault behavior.

See [`REAL_DATA_CONTRACT.md`](REAL_DATA_CONTRACT.md) for the exact provenance of every displayed value.

## Bx01 profile

`--profile bx01` resolves to the observed RS-34 Bx01 configuration:

| Property | Value |
|---|---|
| Broadcaster | `174.168.1.189` |
| Source MAC | `7c:c2:55:ea:e4:93` |
| Multicast group | `239.254.253.252` |
| Multicast MAC | `01:00:5e:7e:fd:fc` |
| Destination UDP | `52101` |
| Source UDP | `60739` |
| Logical capture interface | `bridge0` |
| Physical ingress | `enp4s0f0` |
| Sample format | big-endian signed 32-bit |
| Sample rate | `250000 Sa/s` |
| Stream ID | `0x42783031` (`Bx01`) |
| Class ID | `0x004f505452577834` |
| VITA packet | `2032 B` / `508` words |
| VITA data | `2000 B` / `500` real samples |
| Nominal rate | approximately `500 VITA packets/s` |

## Capture architecture

The recommended RS5 path deliberately keeps the capture mechanism that has already been proven on the host:

```text
bridge0
   |
   v
TShark 2.6.2
  group-only BPF
  IPv4 reassembly
  data.data extraction
   |
   v
Rust VITA parser
   |
   +------------------ CLEAN observer
   |
   +--> chaos engine --> CHAOS observer --> optional TEST multicast
                         |
                         +--> real DSP / analysis / event log / GUI
```

TShark 2.6.2 on RS5 exposes the reassembled payload through `data.data`; newer versions may expose `udp.payload`. The program detects the supported field at runtime.

## Operator Workstation UI

The chosen layout is implemented directly in `src/gui.rs`:

- instrument-style product header with real stream/rate/interface/mode
- ARM / RUN CHAOS / STOP command group
- pass-through / armed / active status strip
- left experiment/protocol/system fault rail
- six live metric cards with real sparklines
- center Spectrum/Waterfall, Live Analysis, Health + Scorecard, Fault Events workspaces
- real dBFS waterfall with fault boundaries and true transport outage columns
- clean/chaos spectrum and time-domain waveform
- live emitter/source tracking and planned ghost targets
- real comparison metrics (RMS/power, occupied bandwidth, peak, spectral flatness)
- right system-health, experiment-impact, and actual event-log rail
- Linux process CPU/RSS and source queue-depth telemetry

## Build on RS5

No Python virtual environment is used.

```bash
cd vita49_chaos_rust_v0.4
./scripts/doctor_rs5.sh
./scripts/build_rhel8.sh
```

Equivalent build:

```bash
cargo test
cargo build --release
```

The release binary is:

```text
target/release/vita49-chaos
```

## Validate the real stream first

```bash
sudo -v
./scripts/probe_bx01.sh 5
```

A healthy result should be in the neighborhood of:

```text
backend       : sudo-tshark
payload field : data.data
bytes         : 2032
size words    : 508
stream ID     : 0x42783031
class ID      : 0x004F505452577834
payload bytes : 2000
samples       : 500
receive rate  : ~500 packets/s
parse errors  : 0
```

The values above are **expected reference geometry**, not GUI-generated runtime readings. `probe` prints the observed values and mismatch counters so deviations are visible.

## Run the real live workstation

```bash
sudo -v
./scripts/run_bx01.sh
```

Equivalent:

```bash
./target/release/vita49-chaos \
  --profile bx01 \
  live \
  --capture-backend tshark
```

Explicit form:

```bash
./target/release/vita49-chaos \
  live \
  --group 239.254.253.252 \
  --port 52101 \
  --source 174.168.1.189 \
  --source-port 60739 \
  --interface bridge0 \
  --dtype be-i32 \
  --fs 250000 \
  --capture-backend tshark
```

## PCAP replay

A real capture can be replayed through the exact same parser/DSP/GUI path:

```bash
./target/release/vita49-chaos \
  --profile bx01 \
  pcap bx01_baseline.pcap \
  --speed 1.0
```

## TEST multicast output

Permission and activation remain intentionally separate:

```bash
./scripts/run_bx01_emit_test.sh
```

This only **permits** output. You must then explicitly enable TEST output in the GUI and ARM/RUN the experiment. The default TEST destination is `239.255.77.77:52101`, and the program rejects an output group+port equal to the input destination.

## Source layout

```text
src/
├── main.rs        entry point
├── cli.rs         production CLI (live / pcap / probe)
├── profile.rs     Bx01 profile + emission safety
├── capture.rs     TShark / PCAP / optional native libpcap
├── vita49.rs      VRT parser + sample codec
├── chaos.rs       transport / protocol / signal / system fault engine
├── metrics.rs     packet, sequence, timestamp and parser metrics
├── dsp.rs         dBFS FFT and signal statistics
├── analytics.rs   live spectral source tracking + comparison analytics
├── telemetry.rs   actual Linux process CPU/RSS telemetry
├── emit.rs        guarded TEST multicast emitter
├── logging.rs     JSONL experiment evidence
├── scenario.rs    reusable chaos configurations
├── state.rs       worker/GUI state messages
├── worker.rs      capture + processing worker
└── gui.rs         Operator Workstation GUI
```

## Validation status in this artifact

The artifact-generation environment used to prepare v0.4 does not contain `cargo`/`rustc`, so the release binary could not be compiled here. The package therefore includes static validation and makes **RS5 `cargo test` + `cargo build --release` the authoritative gate**. See [`BUILD_VALIDATION.md`](BUILD_VALIDATION.md).
