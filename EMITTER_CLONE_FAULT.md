# Detected-Emitter Clone Chaos Fault

## Purpose

This signal-domain fault takes emitters already present in a decoded VITA-49 sample payload and creates synthetic frequency-shifted duplicates in the **CHAOS** copy. The clean observer is never modified. Network emission remains disabled unless the workbench is separately launched with `--allow-emit` and TEST emission is enabled in the GUI.

## What “detected emitter” means

The current implementation is intentionally deterministic and explainable: an emitter candidate is a strong local FFT peak above the median spectral magnitude by the configured threshold. It is not a semantic RF classifier and does not claim to identify modulation type, protocol, or transmitter identity.

## Processing chain

```text
VITA payload
    |
    v
decode samples
    |
    +---------------------> CLEAN observer (unchanged)
    |
    v
FFT + peak detector
    |
    v
select strongest in-band peaks
    |
    v
copy ±N spectral bins around each peak
    |
    v
frequency-shift copies by configurable spacing
    |
    v
IFFT
    |
    v
encode into original VITA packet geometry
    |
    v
CHAOS observer / optional TEST multicast
```

## GUI controls

- **Clone detected emitters into synthetic ghost emitters** — master switch.
- **Copies / emitter** — how many duplicates each detected source may produce.
- **Spacing Hz** — nominal frequency offset between source and clone positions.
- **Clone gain ×** — amplitude of copied spectral content relative to the source slice.
- **Detect threshold dB** — required peak height above the median FFT magnitude.
- **Max source emitters** — maximum detected source peaks processed per packet.
- **Spectral half-width bins** — number of neighboring bins copied on either side of the peak. Larger values preserve more sideband/modulation structure but consume more nearby spectrum.
- **Alternate clones above / below source frequency** — places successive copies on opposite sides of the source when those destinations remain in band.
- **Per-copy decay ×** — progressively reduces successive ghost amplitudes.
- **Frequency dither Hz** — adds deterministic seeded offset jitter before FFT-bin quantization.
- **Occupancy guard dB** — skips a candidate target when that bin already contains a strong signal; set to zero to disable.
- **Preserve packet RMS** — renormalizes the mutated payload to its pre-clone RMS after IFFT, allowing a spectral-topology fault without an overall power increase.

## Recommended first run

Use the preset **DEMO: Clone detected emitters** with TEST emission disabled. Start from a VITA stream containing one or more obvious synthetic tones or narrowband emitters. ARM and RUN CHAOS, then compare the Clean and Chaos spectrum panels.

Expected behavior is additional chaos-only peaks offset from the detected source peaks, while packet rate, Stream ID, Class ID, packet size, and parse health remain unchanged.

## Bx01 defaults

At 250 kSa/s, the 500-sample Bx01 payload is zero-padded to a 512-point FFT for this fault, giving approximately 488.28125 Hz/bin. The built-in preset requests 12.5 kHz spacing; the workbench quantizes the realized offset to the nearest FFT bin and preserves phase progression across packet boundaries.

## Observability

The engine records:

- cumulative `emitter_clones` count
- cumulative `emitter_clone_skipped` occupancy-guard count
- `emitter_clone` events
- detected source frequencies and realized target frequencies for each event
- number of copies actually created and occupied targets skipped
- requested spacing, base gain, gain decay, and RMS-preservation state

This makes the experiment distinguishable from ordinary packet duplication: the packet count can remain stable while the **signal scene** acquires extra synthetic emitters.


## v0.3 presets

- **DEMO: Emitter swarm** creates a denser scene with four decaying copies per selected emitter, up to three source emitters, 8 kHz nominal spacing, and seeded frequency dither.
- **DEMO: Power-neutral ghosts** creates two bidirectional clones per source and restores the packet RMS after cloning so total packet power stays close to baseline.
