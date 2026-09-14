# v0.4 Architecture

```text
                         RS-34 Bx01 / PCAP
                                |
                   +------------v------------+
                   | PacketSource            |
                   | TShark 2.6.2 canonical |
                   | or native libpcap       |
                   +------------+------------+
                                |
                         reassembled VITA
                                |
             +------------------+------------------+
             |                                     |
       CLEAN observer                         ChaosEngine
       VITA metrics                           transport
       decoded samples                        protocol
             |                                signal/RF
             |                                system delay
             |                                     |
             |                                scheduled packets
             |                                     |
             |                               CHAOS observer
             |                                     |
             +------------------+------------------+
                                |
                  +-------------+-------------+
                  |                           |
             AnalysisEngine              JSONL evidence
             dBFS FFT                    engine events
             source tracker              metrics
             comparison stats
                  |
             WorkerUpdate
                  |
      +-----------+------------+
      |                        |
 Operator Workstation     optional TEST multicast
      |                   (double opt-in safety)
      |
 Linux ProcessMonitor
 CPU / RSS / queue / freshness
```

## Thread boundary

The GUI never captures packets or performs packet mutation. A worker thread owns the source, parser/metrics, chaos engine, analytics, logger, and optional TEST emitter. Bounded worker snapshots are sent to egui roughly every 100 ms.

## Capture truth

RS5 live mode remains deliberately anchored to the proven TShark chain:

1. BPF selects source/destination IPs without filtering away non-initial IPv4 fragments.
2. TShark performs IPv4 reassembly.
3. Display filtering applies UDP source/destination ports after reassembly.
4. The raw reassembled UDP payload is read from `data.data` on TShark 2.6.2 or `udp.payload` where supported.
5. Rust parses VITA-49 and measures actual payload geometry.

## Clean and chaos truth

`ChaosEngine::ingest()` emits a byte-for-byte pass-through packet while inactive. Therefore the clean and chaos observers are fed by distinct, real paths even in pass-through mode. During an active fault, the chaos observer sees only what the engine actually emits after drop/delay/reorder/protocol/sample mutations.

## Analysis truth

`AnalysisEngine` consumes recent clean/chaos decoded sample buffers. It generates:

- dBFS-normalized FFT traces
- RMS/peak dBFS
- 99% occupied bandwidth
- spectral flatness
- clean/chaos waveform correlation
- RMS spectral difference
- persistent local-maximum spectral-source tracks

The source tracker makes no modulation or emitter-identity claim.

## Safety boundary

Mutated network output is disabled by default. It requires:

1. launching live mode with `--allow-emit`, and
2. explicitly enabling TEST output in the GUI.

The resolved configuration rejects a TEST output group+port equal to the real input group+port.
