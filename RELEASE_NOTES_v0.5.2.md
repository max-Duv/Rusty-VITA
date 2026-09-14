# Release Notes — v0.5.2

## Responsive fullscreen / DPI remediation

v0.5.2 keeps the v0.5.1 packet-class separation and real-data pipeline, while rebuilding the operator layout so it scales correctly on the actual RS5 GNOME/RDP desktop.

### What changed

- Reduced the native viewport minimum from 1360×820 logical points to 980×680. The old minimum could exceed the usable logical desktop when X11/RDP scaling was greater than 1.0, which caused clipping even on a 1920×1080 physical display.
- Side rails now choose widths from the current egui logical viewport and remain user-resizable inside bounded ranges.
- The six KPI cards stay on one row when space permits and automatically become a 3×2 arrangement when the center workspace becomes narrow. No KPI card is allowed to force the right rail off screen.
- The primary Monitor workspace no longer lives inside a vertical scroll area. Waterfall, spectrum/waveform, and source/comparison rows derive their heights from the live viewport and expand/shrink to fill it.
- Waterfall, spectrum, waveform, source table, and comparison panes now receive viewport-derived heights rather than fixed 248/238/185 point dimensions.
- The lower source/comparison row uses bounded internal scrolling at compact heights instead of expanding the whole central canvas below the visible desktop.
- Header/footer height also adapts to compact logical-height sessions.

### Why this matters on RS5

The recorded RS5 session showed a ~1916×1002 physical desktop, but egui lays out widgets in logical points. A high-DPI or remote-session scale can therefore make a physically large screen substantially smaller to the application. v0.5 used fixed 292/318 point side rails, six minimum-width KPI cards, and a 1360 point minimum viewport, so the right rail and bottom analysis row could fall outside the visible region. v0.5.2 removes those fixed assumptions.

### Data behavior

No synthetic display data was added. All RF plots and operational counters remain bound to the live/replayed VITA stream and the v0.5.1 type-aware data/context split.
