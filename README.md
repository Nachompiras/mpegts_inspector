# MPEG-TS Inspector

A comprehensive, async **MPEG‑TS packet inspector and analyzer** written in Rust.
Listens to unicast or multicast UDP transport streams, parses PSI tables on‑the‑fly,
and provides detailed live statistics with **broadcast-grade quality monitoring**.

## 🚀 Key Features

### 📺 **Video Codec Support**
- **MPEG-2** (stream_type 0x02): Resolution, frame rate, aspect ratio from sequence headers
- **H.264/AVC** (stream_type 0x1B): Full SPS parsing for resolution, FPS, chroma format
- **HEVC/H.265** (stream_type 0x24): Resolution extraction from SPS

### 🎵 **Audio Codec Support**
- **MP2** (stream_type 0x03/0x04): MPEG-1 Audio Layer II with sample rate and channel detection
- **AAC** (stream_type 0x0F): ADTS header parsing for sample rate, channels, profile
- **AC-3/Dolby Digital** (stream_type 0x81): Complete frame analysis including LFE detection

### 📄 **Subtitle Support**
- **DVB Subtitles** (stream_type 0x06): Detection and bitrate monitoring

### 📊 **Live Monitoring**
- **Real-time bitrate calculation** with rolling averages
- **Frame-accurate timing** from PTS deltas and codec headers
- **Automatic PAT/PMT change detection** and refresh
- **JSON reports** every N seconds (configurable)
- **Multicast/Unicast UDP** input support

### 🔍 **Broadcast Compliance**
- **TR 101 290** compliance monitoring with error counters
- **PSI table validation** with CRC checking
- **Continuity counter** error detection
- **PCR analysis** for timing accuracy
- **Service information caching** for semantic validation

---

## ✨ Example Output

```bash
$ cargo run --release -- --addr 239.1.1.2:1234
```

```json
{
  "ts_time": "2025-09-23T18:46:54Z",
  "programs": [
    {
      "program": 26,
      "streams": [
        {
          "pid": 2210,
          "stream_type": 27,
          "codec": "H.264",
          "bitrate_kbps": 5468.2,
          "width": 1920,
          "height": 1080,
          "fps": 29.97,
          "chroma": "4:2:0"
        },
        {
          "pid": 2211,
          "stream_type": 129,
          "codec": "AC-3",
          "bitrate_kbps": 384.0,
          "channels": 6,
          "sample_rate": 48000
        },
        {
          "pid": 2212,
          "stream_type": 129,
          "codec": "AC-3",
          "bitrate_kbps": 128.0,
          "channels": 2,
          "sample_rate": 48000
        },
        {
          "pid": 2215,
          "stream_type": 6,
          "codec": "DVB Subtitle",
          "bitrate_kbps": 2.1
        }
      ]
    }
  ],
  "tr101": {
    "sync_byte_errors": 0,
    "transport_error_indicator": 0,
    "pat_crc_errors": 0,
    "continuity_counter_errors": 1249,
    "pmt_crc_errors": 0,
    "pcr_accuracy_errors": 480,
    "service_id_mismatch": 129083
  }
}
```

---

## 🚀 Quick Start

```bash
# 1. Clone
git clone https://github.com/your-user/mpegts_inspector.git
cd mpegts_inspector

# 2. Build (requires Rust 1.75+)
cargo build --release

# 3. Run Examples
cargo run --release -- --addr 239.1.1.2:1234   # Multicast monitoring
cargo run --release -- --addr 0.0.0.0:5000     # Unicast any-source
cargo run --release -- --addr 127.0.0.1:8080 --refresh 5  # Local with 5s reports
```

### CLI Options

| Flag                 | Default          | Description                                    |
|----------------------|------------------|------------------------------------------------|
| `--addr <ip:port>`   | `239.1.1.2:1234` | Socket to bind & listen (IPv4)                |
| `--refresh <sec>`    | `2`              | JSON report interval in seconds                |
| `--no-analysis`      | `false`          | Disable TR 101 290 analysis for performance   |

---

## 🔧 Advanced Features

### **Broadcast Integration API**
```rust
use mpegts_inspector::inspector;

// For integration with existing streaming pipelines
let (tx, rx) = tokio::sync::broadcast::channel(1000);
inspector::run_from_broadcast(rx, 2, true).await?;
```

### **Stream Type Detection Matrix**

| Stream Type | Format           | Detection Method                    | Metadata Extracted                |
|-------------|------------------|-------------------------------------|-----------------------------------|
| 0x02        | MPEG-2 Video     | Sequence header parsing             | Resolution, FPS, aspect ratio     |
| 0x03/0x04   | MP2 Audio        | Frame header analysis               | Sample rate, channels, version    |
| 0x06        | DVB Subtitles    | Stream identification               | Bitrate monitoring                |
| 0x0F        | AAC Audio        | ADTS header parsing                 | Profile, sample rate, channels    |
| 0x1B        | H.264/AVC        | SPS NAL unit parsing                | Resolution, FPS, chroma format    |
| 0x24        | HEVC/H.265       | SPS NAL unit parsing                | Resolution, basic parameters      |
| 0x81        | AC-3/Dolby       | Sync frame analysis                 | Sample rate, channels, LFE        |

### **TR 101 290 Compliance Monitoring**

The inspector implements comprehensive broadcast quality monitoring:

- **Priority 1 Errors**: Sync byte, TEI, continuity counters, PAT/PMT presence
- **Priority 2 Errors**: PCR accuracy, repetition intervals, PSI structure
- **Custom Metrics**: Service ID validation, table versioning, error rates

---

## 📝 JSON Schema Reference

```typescript
interface Report {
  ts_time: string;          // ISO-8601 UTC timestamp
  programs: Program[];
  tr101: TR101Metrics;      // Broadcast compliance counters
}

interface Program {
  program: number;          // Program number from PAT
  streams: ElementaryStream[];
}

interface ElementaryStream {
  pid: number;              // Packet ID
  stream_type: number;      // ISO 13818-1 stream type
  codec: string;            // Human-readable codec name
  bitrate_kbps: number;     // Rolling average bitrate

  // Video-specific (when applicable)
  width?: number;
  height?: number;
  fps?: number;
  chroma?: string;          // "4:2:0", "4:2:2", etc.

  // Audio-specific (when applicable)
  channels?: number;
  sample_rate?: number;
}
```

---

## 🏗️ Architecture

**Built for Performance & Accuracy**
- **Zero-copy parsing** with `bitstream-io`
- **No FFmpeg dependencies** - pure Rust implementation
- **Async/await** architecture for high throughput
- **Memory-efficient** stream processing
- **Automatic multicast join** for broadcast monitoring
- **Robust error handling** with graceful degradation

**PSI Table Support**: PAT, PMT, CAT, NIT, SDT, EIT parsing with full CRC validation

---

## 📊 Use Cases

- **Broadcast Quality Monitoring**: TR 101 290 compliance checking
- **Stream Analysis**: Detailed codec and metadata inspection
- **Network Debugging**: Real-time transport stream validation
- **Content Verification**: Automated stream parameter validation
- **Integration Testing**: Programmatic TS analysis via Rust API

---

## 📜 License

MIT © 2025 Ignacio Opazo
Contributions welcome! Feel free to fork & submit PRs.