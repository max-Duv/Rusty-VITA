# VITA-49 Chaos Workbench v0.3 — RF Scene Console

v0.3 expands both the chaos engine and the operations-console GUI.

## Chaos engine

Detected-emitter cloning now supports:

- per-copy gain decay so outer ghosts can taper naturally;
- deterministic frequency dither for less mechanically-spaced synthetic scenes;
- occupancy guarding that skips already-strong destination bins;
- optional RMS preservation so spectral topology can change without increasing packet power;
- target-frequency reporting in `emitter_clone` events;
- a counter for clone placements skipped by the occupancy guard;
- an additional unit test for power-neutral cloning.

New presets:

- **DEMO: Emitter swarm** — up to three detected sources, four decaying/dithered clones per source.
- **DEMO: Power-neutral ghosts** — bidirectional clones with post-mutation RMS restored to the original packet power.

## GUI fidelity

The GUI was reorganized as an RF operations console rather than a collection of default egui widgets:

- two-tier command/status header with source, RX, TX, ARM/RUN/STOP state;
- richer metric cards and domain badges;
- card-based fault controls with separate transport, VITA, RF-scene and system domains;
- persistent network safety card;
- configurable waterfall history, level range and freeze control;
- spectrum overlay with detected-source and planned-ghost frequency markers;
- live emitter-scene table;
- experiment-impact gauges for transport loss, protocol integrity, spectral divergence and waveform correlation;
- live RMS delta, normalized waveform correlation and spectral-difference metrics;
- cleaner waveform-window control;
- expanded health scorecard and engine counters;
- searchable event log;
- bottom capture/logging status rail;
- improved dark palette, borders, spacing and visual hierarchy for remote-desktop use;
- independently scrollable central workspace so 1180×720/remote sessions no longer clip the lower scene, impact, or timeline cards;
- convenience launchers for demo/live emitter-swarm experiments.

## Safety posture

No safety boundary changed in v0.3. Mutations remain internal unless the application is launched with `--allow-emit` and TEST output is explicitly enabled in the GUI. TEST multicast is still required to differ from the input group+port.
