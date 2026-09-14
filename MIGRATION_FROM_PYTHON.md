# Migration from the Python workbench

v0.4 is a native Rust application; the Python virtual environment is not used.

## Python-era command

The Python tool was launched with parameters such as:

```bash
python3 app.py live \
  --group 239.254.253.252 \
  --port 52101 \
  --interface bridge0 \
  --dtype be-i32 \
  --fs 250000 \
  --capture-backend tshark
```

## Rust v0.4 equivalent

The Bx01 profile contains those confirmed values:

```bash
sudo -v
./target/release/vita49-chaos --profile bx01 live --capture-backend tshark
```

The explicit Rust form remains available for other streams.

## What changed

- PyQt/pyqtgraph were replaced with native eframe/egui.
- TShark 2.6.2 capture compatibility is retained because it is the proven RS5 path.
- Python virtual environments and pip/PyQt wheel compatibility are gone.
- Packet parsing, chaos injection, DSP, analytics, logging, and the GUI are native Rust.
- v0.4 removes the production synthetic packet source. Use the real live stream or a real captured PCAP.
- The Operator Workstation UI uses actual packet/sample/process data rather than seeded dashboard placeholders.

## First launch after migration

```bash
./scripts/doctor_rs5.sh
./scripts/build_rhel8.sh
sudo -v
./scripts/probe_bx01.sh 5
./scripts/run_bx01.sh
```
