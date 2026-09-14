# VITA-49 Chaos Workbench v0.4.1

## RS5 compiler/test hotfix

- Fixes `emitter_clone_creates_frequency_shifted_ghost` failing when the default occupancy guard interpreted the Hann-window spectral skirt of the source as an occupied destination.
- Occupancy blocking now requires a local spectral maximum inside the destination slice, so a real emitter still blocks cloning while monotonic leakage does not.
- Adds a regression test for the leakage-skirt case.
- Makes egui `Stroke` widths explicitly `f32` for Rust 1.98+ future compatibility.
- Removes one unused GUI import.
- Bumps crate version to 0.4.1.

The Bx01 capture path, VITA parser, safety boundary, and test multicast behavior are unchanged.
