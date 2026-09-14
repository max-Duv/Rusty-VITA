# v0.2 — Detected-Emitter Clone Fault

This revision adds a new signal-domain chaos experiment: **DEMO: Clone detected emitters**.

## Added

- FFT peak detector operating on decoded VITA sample payloads.
- Median-spectrum thresholding with configurable dB margin.
- Selection of the strongest detected source emitters.
- Frequency-shifted ghost copies with configurable count, spacing, gain, and spectral width.
- Alternating above/below placement when targets remain in band.
- Real-sample Hermitian symmetry preservation.
- Complex-I/Q signed-frequency handling.
- Phase progression across VITA packet boundaries.
- Cached FFT/IFFT plans for live-rate use.
- `emitter_clones` engine counter.
- Rich `emitter_clone` timeline events containing detected source frequencies and copy counts.
- GUI controls and a one-click demonstration preset.
- Unit test asserting that a 20 kHz synthetic emitter produces a new 30 kHz ghost at the configured 10 kHz offset while VITA geometry is unchanged.

## Default preset

- up to 2 detected source emitters
- 2 clones per source
- 12.5 kHz nominal spacing
- 0.70× clone gain
- 10 dB above median spectral magnitude
- ±2 FFT-bin source slice
- alternating upper/lower clone placement

TEST multicast emission remains independently locked and disabled by default.
