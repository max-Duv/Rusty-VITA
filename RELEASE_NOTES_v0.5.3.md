# Release Notes — v0.5.3

## Why this release exists

The v0.5.2 RS5 screenshot showed two remaining usability defects: the bottom emitter/comparison panes could extend beneath the visible desktop and their internal content was not practically scrollable, while the persistent use of the word “chaos” for the inactive output branch made clean pass-through observation look like an experiment was being assessed.

## Layout fixes

- reserves a small bottom safe inset for remote-desktop/taskbar overlays;
- constrains the lower row to the actual remaining viewport height;
- emitter/source and comparison panes now use both-axis scroll areas inside fixed-height lower panels;
- adds a visible scroll hint to the source table;
- adds SHOW INSPECTOR / HIDE INSPECTOR in the header so the right health rail can be collapsed when screen width is limited;
- keeps the right rail resizable but with a more stable minimum width.

## Pass-through / chaos semantics

- inactive output is labeled OUTPUT or PASS-THROUGH rather than CHAOS;
- spectrum and waveform legends say Pass-through until an experiment is active;
- comparison view becomes CLEAN ↔ PASS-THROUGH CONSISTENCY while inactive;
- Experiment Impact displays N/A and explicitly states that source/stream anomalies are baseline health until RUN CHAOS is active;
- experiment counters are hidden in passive mode;
- System Health exposes Output mode = EXACT PASS-THROUGH when inactive.

## Engine invariant fix

The inactive engine now returns each packet immediately and byte-for-byte without passing through the transport scheduler. This matters because the scheduler intentionally applies fixed delay, jitter and throttle. A previously run scenario can therefore no longer leak transport behavior into passive mode.

STOP is now a hard experiment boundary: it closes the experiment log, cancels pending delayed/reordered packets, restores the default configuration, resets engine state, and resumes exact pass-through.

A regression test verifies that an inactive engine remains exact pass-through even when its stored configuration contains blackout, delay, jitter, throttle and signal mutation settings.
