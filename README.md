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
- **AAC LATM** (stream_type 0x11): Low-overhead MPEG-4 Audio Transport Multiplex parsing
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
- **TR 101 290** compliance monitoring with configurable priority levels
- **Priority 1**: Critical transport errors (sync, TEI, PAT/PMT, continuity)
- **Priority 2**: PCR timing, null packet rate, CAT presence
- **Priority 3**: Service information validation (NIT/SDT/EIT/TDT)
- **PSI table validation** with CRC checking
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
| `--tr101-priority`   | `12`             | TR 101 290 priority level: `1`, `12`, or `all`|

#### TR 101 290 Priority Examples
```bash
# Priority 1 only (critical errors)
cargo run --release -- --addr 239.1.1.2:1234 --tr101-priority 1

# Priority 1+2 (critical + recommended, default)
cargo run --release -- --addr 239.1.1.2:1234 --tr101-priority 12

# All priorities (complete monitoring)
cargo run --release -- --addr 239.1.1.2:1234 --tr101-priority all
```

---

## 🔧 Advanced Features

### **Broadcast Integration API**

#### **Basic Integration with Structured Data**
```rust
use mpegts_inspector::inspector::{self, InspectorReport, CodecInfo};

// Create broadcast channel for TS packets
let (tx, rx) = tokio::sync::broadcast::channel(1000);

// Process inspection results with callback
// Note: uses Priority 1+2 analysis by default for broadcast integration
inspector::run_from_broadcast(
    rx,
    2,    // Generate report every 2 seconds
    true, // Enable TR-101 analysis (Priority 1+2 by default)
    |report: InspectorReport| {
        // Process structured data directly (no JSON parsing needed)
        for program in &report.programs {
            println!("Program {}", program.program_number);
            for stream in &program.streams {
                match &stream.codec {
                    Some(CodecInfo::Video(v)) => println!("  Video: {} {}x{} @ {:.1}fps",
                        v.codec, v.width, v.height, v.fps),
                    Some(CodecInfo::Audio(a)) => println!("  Audio: {} {}Hz {}ch",
                        a.codec, a.sample_rate.unwrap_or(0), a.channels.unwrap_or(0)),
                    Some(CodecInfo::Subtitle(s)) => println!("  Subtitle: {}", s.codec),
                    None => println!("  Unknown stream type {}", stream.stream_type),
                }
            }
        }

        // Access TR-101 metrics (filtered by priority level)
        if report.tr101_metrics.sync_byte_errors > 0 {
            println!("⚠️ Critical: Sync byte errors: {}", report.tr101_metrics.sync_byte_errors);
        }
        if report.tr101_metrics.pcr_accuracy_errors > 0 {
            println!("⚠️ Timing: PCR accuracy errors: {}", report.tr101_metrics.pcr_accuracy_errors);
        }
        // Priority 3 errors (like service_id_mismatch) are automatically filtered out
    }
).await?;

// Send TS data to the channel
let ts_data = vec![0x47, 0x00, 0x00, /* ... 188 bytes ... */];
tx.send(ts_data)?;
```

#### **Advanced Integration with Runtime Control**
```rust
use mpegts_inspector::inspector::{self, AnalysisMode, AnalysisCommand};

// Set up data and control channels
let (data_tx, data_rx) = tokio::sync::broadcast::channel(1000);
let (control_tx, control_rx) = tokio::sync::broadcast::channel(100);

// Start with Priority 1+2 analysis (recommended for production)
let inspector_task = tokio::spawn(async move {
    inspector::run_from_broadcast_with_control(
        data_rx,
        control_rx,
        2,                                    // 2-second reports
        Some(AnalysisMode::Tr101Priority12)   // Start with Priority 1+2
    ).await
});

// Runtime control examples with priority levels
control_tx.send(AnalysisCommand::Start(AnalysisMode::Tr101Priority1))?;  // Critical only
control_tx.send(AnalysisCommand::Start(AnalysisMode::Tr101Priority12))?; // Critical + recommended
control_tx.send(AnalysisCommand::Start(AnalysisMode::Tr101))?;           // All priorities
control_tx.send(AnalysisCommand::Start(AnalysisMode::Mux))?;             // Codec detection only
control_tx.send(AnalysisCommand::Stop)?;                                 // Stop analysis
control_tx.send(AnalysisCommand::GetStatus)?;                            // Query status

// Feed TS data (188-byte aligned chunks)
let ts_packet_buffer = vec![/* ... TS packets ... */];
data_tx.send(ts_packet_buffer)?;
```

#### **Priority-Aware Processing Example**
```rust
use mpegts_inspector::inspector::{self, InspectorReport, AnalysisMode};

// Production monitoring with Priority 1+2 only
let (tx, rx) = tokio::sync::broadcast::channel(1000);

inspector::run_from_broadcast(
    rx,
    5,    // 5-second reports
    true, // Enable analysis
    |report: InspectorReport| {
        // Only Priority 1+2 errors will be included in tr101_metrics
        let metrics = &report.tr101_metrics;

        // Priority 1 (critical) - these affect stream decodability
        if metrics.sync_byte_errors > 0 || metrics.continuity_counter_errors > 0 {
            eprintln!("🚨 CRITICAL: Transport stream corruption detected!");
        }

        // Priority 2 (recommended) - these affect quality/compliance
        if metrics.pcr_accuracy_errors > 0 {
            eprintln!("⚠️  TIMING: PCR accuracy issues detected");
        }

        // Priority 3 errors (like service_id_mismatch) are NOT included
        // This reduces noise and focuses on actual streaming problems
    }
).await?;
```

#### **Data Structures**

When using `run_from_broadcast()`, you get direct access to structured data instead of JSON:

```rust
pub struct InspectorReport {
    pub timestamp: String,
    pub programs: Vec<ProgramInfo>,
    pub tr101_metrics: Tr101Metrics,
}

pub struct ProgramInfo {
    pub program_number: u16,
    pub streams: Vec<StreamInfo>,
}

pub struct StreamInfo {
    pub pid: u16,
    pub stream_type: u8,
    pub codec: Option<CodecInfo>,
    pub bitrate_kbps: f64,
}

pub enum CodecInfo {
    Video(VideoInfo),    // width, height, fps, chroma
    Audio(AudioInfo),    // codec, sample_rate, channels, profile
    Subtitle(SubtitleInfo), // codec
}
```

#### **Production Use Case: Broadcast Monitoring System**
```rust
use mpegts_inspector::inspector::{self, InspectorReport, AnalysisMode};
use tokio::sync::broadcast;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Set up broadcast channel for TS packets
    let (ts_tx, ts_rx) = broadcast::channel(2000);  // Buffer for ~2000 TS packets

    // Set up control channel for runtime configuration
    let (control_tx, control_rx) = broadcast::channel(100);

    // Start inspector with Priority 1+2 monitoring (recommended for production)
    let inspector_handle = tokio::spawn(async move {
        inspector::run_from_broadcast_with_control(
            ts_rx,
            control_rx,
            10,                                   // 10-second reports for production
            Some(AnalysisMode::Tr101Priority12),  // Critical + recommended errors
        ).await
    });

    // Start your TS data source (UDP receiver, file reader, etc.)
    let data_source_handle = tokio::spawn(async move {
        // Example: UDP receiver that feeds the broadcast channel
        let socket = tokio::net::UdpSocket::bind("0.0.0.0:1234").await?;
        let mut buf = [0u8; 1316]; // 7 TS packets per UDP packet

        loop {
            let len = socket.recv(&mut buf).await?;
            if len > 0 {
                // Send TS data to inspector
                if let Err(_) = ts_tx.send(buf[..len].to_vec()) {
                    eprintln!("Inspector channel full, dropping packets");
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    });

    // Runtime control examples
    tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

    // Switch to critical-only mode during maintenance
    control_tx.send(AnalysisCommand::Start(AnalysisMode::Tr101Priority1))?;
    println!("Switched to Priority 1 (critical only) mode");

    // Wait for completion
    tokio::try_join!(inspector_handle, data_source_handle)?;
    Ok(())
}
```

#### **Analysis Modes for Broadcast Integration**
- **`AnalysisMode::Mux`**: Stream detection, codec analysis, bitrate calculation (low CPU)
- **`AnalysisMode::Tr101Priority1`**: Critical transport errors only (sync, TEI, PAT/PMT, continuity)
- **`AnalysisMode::Tr101Priority12`**: Critical + recommended errors (includes PCR, CAT monitoring)
- **`AnalysisMode::Tr101`**: Full TR 101 290 compliance monitoring (all priorities, higher CPU)
- **`AnalysisMode::None`**: Minimal processing, packet consumption only

#### **Choosing the Right Priority Level**
- **Priority 1**: Use when you only care about stream decodability and critical transport errors
- **Priority 1+2**: **Recommended for production** - covers critical and timing/quality issues
- **All priorities**: Use for complete broadcast compliance testing and certification

### **Stream Type Detection Matrix**

| Stream Type | Format           | Detection Method                    | Metadata Extracted                |
|-------------|------------------|-------------------------------------|-----------------------------------|
| 0x02        | MPEG-2 Video     | Sequence header parsing             | Resolution, FPS, aspect ratio     |
| 0x03/0x04   | MP2 Audio        | Frame header analysis               | Sample rate, channels, version    |
| 0x06        | DVB Subtitles    | Stream identification               | Bitrate monitoring                |
| 0x0F        | AAC Audio        | ADTS header parsing                 | Profile, sample rate, channels    |
| 0x11        | AAC LATM         | LATM sync + config parsing          | Profile, sample rate, channels    |
| 0x1B        | H.264/AVC        | SPS NAL unit parsing                | Resolution, FPS, chroma format    |
| 0x24        | HEVC/H.265       | SPS NAL unit parsing                | Resolution, basic parameters      |
| 0x81        | AC-3/Dolby       | Sync frame analysis                 | Sample rate, channels, LFE        |

### **TR 101 290 Compliance Monitoring**

The inspector implements comprehensive broadcast quality monitoring with configurable priority levels:

#### **Priority 1 (Critical Transport Errors)**
- `sync_byte_errors`: Missing or corrupted 0x47 sync bytes
- `transport_error_indicator`: TEI flag set in TS header
- `pat_crc_errors`: PAT table CRC validation failures
- `pat_timeout`: PAT not received within 500ms
- `continuity_counter_errors`: Missing or duplicate packets
- `pmt_crc_errors`: PMT table CRC validation failures
- `pmt_timeout`: PMT not received within 1 second

#### **Priority 2 (Recommended Quality Checks)**
- `pcr_repetition_errors`: PCR not repeated within 100ms
- `pcr_accuracy_errors`: PCR drift beyond ±500ns tolerance
- `null_packet_rate_errors`: Null packet rate exceeds 15%
- `cat_crc_errors`: CAT table CRC validation failures
- `cat_timeout`: CAT not received within 2 seconds

#### **Priority 3 (Optional SI Validation)**
- `service_id_mismatch`: Service ID inconsistency between SDT and PAT
- `nit_crc_errors`, `nit_timeout`: NIT table validation
- `sdt_crc_errors`, `sdt_timeout`: SDT table validation
- `eit_crc_errors`, `eit_timeout`: EIT table validation
- `tdt_timeout`: TDT/TOT table presence monitoring
- `splice_count_errors`: SCTE-35 splice countdown validation

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
- **Dynamic analysis control** - switch between MUX/TR101 modes at runtime
- **Automatic multicast join** for broadcast monitoring
- **Robust error handling** with graceful degradation

**PSI Table Support**: PAT, PMT, CAT, NIT, SDT, EIT parsing with full CRC validation

---

## 📊 Use Cases & Best Practices

### **CLI Usage (Quick Analysis)**
```bash
# Basic stream analysis with Priority 1+2 monitoring (recommended)
cargo run --release -- --addr 239.1.1.2:1234

# Critical errors only (minimal overhead)
cargo run --release -- --addr 239.1.1.2:1234 --tr101-priority 1

# Complete compliance testing
cargo run --release -- --addr 239.1.1.2:1234 --tr101-priority all
```

### **Broadcast Integration (Production Systems)**
- **Broadcast Quality Monitoring**: TR 101 290 compliance checking with priority filtering
- **Stream Analysis**: Real-time codec detection and metadata extraction
- **Network Debugging**: Transport stream validation with detailed error reporting
- **Content Verification**: Automated stream parameter validation
- **Integration Testing**: Programmatic TS analysis via structured data API

### **Performance Recommendations**
- **Development/Testing**: Use `AnalysisMode::Tr101` (all priorities) for complete visibility
- **Production Monitoring**: Use `AnalysisMode::Tr101Priority12` for essential error detection
- **High-throughput Systems**: Use `AnalysisMode::Tr101Priority1` for minimal overhead
- **Codec Detection Only**: Use `AnalysisMode::Mux` when TR-101 compliance is not needed

---

## 📜 License

MIT © 2025 Ignacio Opazo
Contributions welcome! Feel free to fork & submit PRs.
