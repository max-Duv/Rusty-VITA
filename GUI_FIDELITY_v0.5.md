# GUI Fidelity v0.5.2 — On-box RS5 remediation

The v0.5.2 GUI specifically addresses the RS5 screenshot where the previous build looked underscaled, flat, and visually empty despite live data.

## Root causes fixed

1. **Hard-coded display range**: real Bx01 spectrum was around the -200 dBFS region while the waterfall was fixed at -120..0 dBFS, so valid data rendered almost black. v0.5.2 derives a robust display range from actual spectrum percentiles.
2. **Waveform scale mismatch**: samples were normalized against i32 full scale and then forced into ±1. For a low-level stream this visually collapsed to zero. v0.5.2 plots raw decoded sample units and auto-ranges the Y axis.
3. **Linux/RDP typography**: egui compact defaults and resizable/persisted side-panel widths made the real application much smaller than the selected design. v0.5.2 uses explicit larger text styles and fixed operator-rail widths.
4. **CPU semantics**: Linux process CPU can exceed 100% on multithreaded workloads. v0.5.2 reports percent of total logical host capacity and also shows core equivalents.
5. **Source-table clutter**: local FFT texture created too many tracked rows. v0.5.2 adds local-prominence rejection and bounds the visible tracker set.

All scaling changes are display transforms only. No synthetic RF values are inserted.
