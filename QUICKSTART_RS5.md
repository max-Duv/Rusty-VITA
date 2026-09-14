# RS5 Quick Start — v0.5.3

## 1. Enter the project

```bash
cd vita49_chaos_rust_v0.5.3
```

## 2. Host preflight

```bash
./scripts/doctor_rs5.sh
cargo --version
rustc --version
```

## 3. Compile and test

```bash
./scripts/build_rhel8.sh
```

## 4. Verify the real Bx01 stream

```bash
sudo -v
./scripts/probe_bx01.sh 30
```

The v0.5.3 probe reports both wall-clock delivery rate and capture-time wire rate. Use the **wire rate** (`frame.time_epoch`) for source-rate validation; wall rate includes TShark startup/drain overhead. Review the geometry histogram before fault experiments.

## 5. Launch the Operator Workstation

```bash
sudo -v
./scripts/run_bx01.sh
```

Equivalent:

```bash
./target/release/vita49-chaos --profile bx01 live --capture-backend tshark
```

The dashboard initially stays in PASS-THROUGH / CLEAN OBSERVATION. It populates only from received/replayed VITA data.

## 6. Run a first fault

Keep TEST multicast disabled. Select a preset, ARM, then RUN CHAOS. Clean and chaos branches are measured independently. STOP returns the engine to pass-through and allows recovery timing to be measured.
