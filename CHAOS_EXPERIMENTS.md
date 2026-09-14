# High-Visibility Chaos Experiments

Start with TEST multicast emission disabled. These experiments mutate only the internal chaos copy.

## 1. Hard blackout

Preset: **DEMO: Hard blackout**

Expected at Bx01 nominal conditions:

- Clean: ~500 packets/s, ~250 kSa/s
- Chaos: 0 packets/s while active
- Waterfall: hard blank history row/stripe while the chaos branch is not emitting
- Engine dropped counter: rises ~500/s
- Parse errors: should remain zero

Use this to verify the entire clean-vs-chaos visualization path.

## 2. Repeating burst loss

Preset: **DEMO: Repeating burst loss**

- drop 250 packets
- every 2500 packets
- Bx01 ~500 packets/s
- approximately 0.5 s outage every 5 s

This should make repeating blank history stripes in the waterfall and sharp jumps in sequence/timestamp health counters.

## 3. Timing collapse

Preset: **Timing collapse**

Packet delivery can remain near 500 packets/s while timestamp drift/jitter and periodic sequence jumps damage temporal integrity. This demonstrates a sensor stream that is alive at the network layer but unsafe for coherent/distributed processing.

## 4. RF degradation

Preset: **RF degradation**

Gain reduction + AWGN + sample dropout + injected tone should visibly raise the noise floor, introduce a spur, and diverge clean/chaos waveforms while the transport remains mostly healthy.

## 5. Clone detected emitters

Preset: **DEMO: Clone detected emitters**

This fault detects strong spectral peaks already present in the VITA sample payload and creates frequency-shifted ghost copies of them. It is designed to make emitter-processing resilience failures visually obvious without replacing the clean reference.

Default preset:

- signal-domain mutation enabled
- up to 2 detected source emitters per VITA packet
- 2 clones per selected source
- 12.5 kHz nominal spacing
- 0.70× clone amplitude
- 10 dB detection threshold above the median FFT magnitude
- ±2 FFT bins copied around each source peak
- clone placement alternates above/below the source when the requested target remains in band

Expected display behavior:

- clean spectrum retains only the original emitter peaks
- chaos spectrum develops additional peaks offset from the detected sources
- packet rate and VITA geometry remain essentially unchanged
- the measured `ghosts`/emitter-clone engine counter rises in the scorecard
- FAULT EVENTS records `emitter_clone` with source frequencies, clone count, spacing, and gain

For Bx01 at 250 kSa/s and a 512-point packet FFT, one bin is approximately 488.3 Hz, so requested clone offsets are quantized to that resolution.

## 6. Emitter swarm

Preset: **DEMO: Emitter swarm**

This is the higher-visibility RF-scene experiment. It can select up to three strong source peaks and create four decaying copies per source. Nominal spacing is 8 kHz with seeded ±900 Hz dither before FFT-bin quantization. The occupancy guard skips already-strong destinations.

Expected display behavior:

- the spectrum overlay shows source markers and nominal ghost destinations;
- the emitter-scene table lists the strongest detected sources;
- the chaos spectrum becomes visibly denser without changing packet rate;
- `ghosts` rises quickly while `occupied-target skips` shows placement conflicts;
- waveform correlation and spectral-divergence gauges make the scene change obvious even when transport metrics remain healthy.

## 7. Power-neutral ghosts

Preset: **DEMO: Power-neutral ghosts**

This experiment clones detected emitters but renormalizes each mutated payload back to its pre-clone RMS. It is useful for separating **spectral-topology** sensitivity from simple total-power sensitivity.

Expected behavior:

- clean and chaos packet rates remain aligned;
- RMS Δ remains close to 0 dB;
- spectral divergence increases;
- ghost markers and clone counters increase;
- parse health and VITA packet geometry remain stable.

## 8. Mixed failure

Preset: **Mixed failure**

Combines packet loss/duplication/jitter/reordering with timestamp faults and signal degradation. Use only after the individual domains have been validated.

## 9. Automatic cascading incident

Preset: **DEMO: Cascading incident**

The sequence is automatic:

| Time | Phase |
|---:|---|
| 0–5 s | clean baseline |
| 5–10 s | +10% packet loss |
| 10–15 s | +20 ms Gaussian jitter |
| 15–20 s | +25% reordering, window 8 |
| 20–25 s | +250 ppm timestamp drift + 150 µs jitter |
| 25–30 s | +AWGN at 6 dB target SNR |
| 30–35 s | recover signal domain |
| 35–40 s | recover timing domain |
| 40–45 s | recover transport domain |
| 45 s | automatic pass-through / experiment complete |

This is the best end-to-end demonstration because the clean observer remains a simultaneous reference while failures accumulate and then recover.
