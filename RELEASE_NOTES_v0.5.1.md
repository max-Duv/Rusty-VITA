# Release Notes — v0.5.1

## Bx01 packet-class correctness

RS5 wire validation showed two legitimate VITA-49 packet classes on the same Bx01 multicast:

- type 1 IF Data with Stream ID: 2032 B packet / 2000 B sample payload at ~500 packets/s
- type 4 IF Context: 76 B packet / 48 B context payload at ~1 packet/s

v0.5.1 makes the runtime packet-class aware. Context packets are no longer treated as malformed data geometry, decoded as RF samples, included in data packet-rate calculations, or allowed to perturb sample-stream sequence/timestamp health counters.

### Runtime changes

- DSP buffers accept VITA data packets only (types 0..=3).
- Signal-domain chaos faults never mutate context payloads.
- Sequence and timing health are tracked on the sample-bearing data stream only.
- The live GUI exposes data/context packet counters separately.
- Waterfall advancement is keyed to data packets rather than auxiliary/context packets.
- Last-packet geometry shown in the GUI is the last data-packet geometry.

### Probe changes

`probe_bx01.sh` now separates:

- all-VITA wire rate
- data-packet wire rate
- context-packet rate
- data-only sample rate
- data-geometry mismatches

The geometry histogram labels the observed 76 B / 48 B packets as context/auxiliary geometry rather than a data geometry error.

### Validation added

New deterministic tests verify that context packets:

- are classified as context rather than sample data,
- do not create sequence/timestamp anomalies in the data stream, and
- remain byte-exact under signal-domain fault configurations.

The RS5 compiler remains the authoritative build/test gate.
