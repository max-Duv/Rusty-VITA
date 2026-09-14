# Build Validation — v0.4

The artifact-generation container does not contain `cargo` or `rustc`; no claim is made that this package was compiled there.

## Authoritative RS5 gate

```bash
./scripts/check.sh
./scripts/build_rhel8.sh
```

The release helper runs Rust tests before producing `target/release/vita49-chaos`.

## v0.4 checks that should pass on RS5

- CLI contains only `live`, `pcap`, and `probe` production sources.
- TShark 2.6.2 `data.data` detection succeeds.
- Bx01 probe parses 2032-byte VITA packets with 2000-byte payloads.
- DSP dBFS unit test detects a full-scale bin-centered sine near 0 dBFS.
- analysis tracker test finds a known FFT peak and reports clean/chaos correlation near 1.
- existing VITA parser and chaos-engine tests remain green.

## Runtime validation order

```bash
sudo -v
./scripts/probe_bx01.sh 5
./scripts/run_bx01.sh
```

Before injecting a fault, verify the GUI shows measured packet/sample rates and that the source freshness/health rail is nominal. Then run a short internal-only transport fault with TEST emission disabled and verify the clean branch remains stable while chaos counters and plots change.
