# RS5 Quick Start — v0.4

## 1. Enter the project

```bash
cd vita49_chaos_rust_v0.4
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
./scripts/probe_bx01.sh 5
```

Do not proceed to fault experiments until the probe shows valid Bx01 geometry, a rate near 500 packets/s, and zero parser errors.

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
