# Real Data Contract — v0.4

The Operator Workstation follows one rule: **unknown data is displayed as unknown; it is never replaced with a plausible-looking value.**

| UI field / panel | Runtime source |
|---|---|
| RX group / port / interface / configured Fs | resolved launch/profile configuration |
| Raw packet count / bytes | `PacketSource::SourceStatus` |
| Capture backend / payload field | actual TShark/native-pcap source status |
| Source freshness | last received packet timestamp |
| Queue depth | actual bounded capture-reader channel depth |
| Clean PPS / Sa/s / Mbps | `StreamMetrics` on received packets |
| Chaos PPS / Sa/s / Mbps | `StreamMetrics` on packets actually emitted by `ChaosEngine` |
| Sequence gaps / reorders / timestamp drops / glitches | VITA metadata observed by `StreamMetrics` |
| Parse errors | actual VITA parser failures |
| Waterfall | FFT of decoded chaos-output samples; explicit floor column only when the clean branch advances and chaos emits no packet |
| Spectrum | FFT of decoded clean/chaos samples, normalized to configured sample-format full scale (dBFS) |
| Noise-floor line | median of the current clean spectrum |
| Waveform | decoded recent sample buffers, normalized by real sample-format full scale |
| Emitter/source table | local maxima in the current real clean spectrum above a robust median threshold, tracked over time |
| Planned ghost targets | configured clone offsets applied to real detected-source frequencies; never labeled as measured |
| Total power | sample RMS relative to full scale |
| Peak | peak sample magnitude relative to full scale |
| Occupied bandwidth | 99% integrated spectral-power bandwidth from current FFT |
| Spectral flatness | geometric / arithmetic mean of current spectral power |
| Waveform correlation | normalized complex correlation between current clean and chaos sample windows |
| Spectral delta | RMS dB difference between current clean and chaos spectra |
| Fault counters | `ChaosEngine::EngineStats` |
| Event log | actual source/worker/engine events and timestamps |
| CPU | `/proc/self/stat` delta using host clock-tick rate |
| Memory | `/proc/self/status` `VmRSS` |
| Health state | worker error + live-source freshness + parse state + queue depth |
| Log path | actual active JSONL experiment logger path |

## Allowed model-derived values

The tool necessarily computes engineering quantities from measured data. These are **derived**, not fabricated: FFT spectra, RMS, dBFS normalization, occupied bandwidth, correlation, spectral flatness, source detection, recovery time, and comparison deltas.

The source detector is intentionally labeled as spectral-source detection. It does **not** assert modulation, platform identity, emitter identity, or intent.

## Deliberately absent production data sources

v0.4 has no production `demo` command and no GUI source that synthesizes a VITA stream simply to populate charts. Unit tests may construct deterministic packets and waveforms as test fixtures; those fixtures never enter the release GUI unless a developer writes a test harness around them.
