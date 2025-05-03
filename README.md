# mpegts_inspector

A tiny, async **MPEG‑TS packet inspector** written in Rust.  
It listens to a unicast *or* multicast UDP transport stream, parses the PAT/PMT
tables on‑the‑fly and keeps live statistics for every elementary stream:

* **Codec & friendly name** (H.264 / HEVC, AAC, AC‑3, MPEG‑2 V/A …)  
* **Bit‑rate** in kb/s (rolling average)  
* **Video**: width × height, frame‑rate (from SPS or PTS‑Δ), chroma format  
* **Audio**: sample‑rate, channel layout  
* Automatic refresh when PAT/PMT version changes  
* Prints a clean **JSON report** every _N_ seconds (default 2 s)

No FFmpeg, `libav` or heavy deps required – everything is parsed by hand with
`bitstream‑io`.

---

## ✨ Demo

```bash
$ cargo run --release -- --addr 239.1.1.2:1234
{
  "ts_time": "2025-05-03T14:12:20Z",
  "programs": [
    {
      "program": 1,
      "streams": [
        {
          "pid": 256,
          "stream_type": 27,
          "codec": "H.264",
          "bitrate_kbps": 3950.8,
          "width": 1440,
          "height": 1080,
          "fps": 29.97,
          "chroma": "4:2:0"
        },
        {
          "pid": 257,
          "stream_type": 15,
          "codec": "AAC",
          "bitrate_kbps": 112.3,
          "channels": 2,
          "sample_rate": 48000
        }
      ]
    }
  ]
}
```

---

## 🚀 Quick Start

```bash
# 1. Clone
git clone https://github.com/your-user/mpegts_inspector.git
cd mpegts_inspector

# 2. Build
cargo build --release   # Rust 1.75+ recommended

# 3. Run
cargo run --release -- --addr 239.1.1.2:1234   # multicast example
# or
cargo run --release -- --addr 0.0.0.0:5000     # any-source unicast
```

### CLI flags

| Flag               | Default          | Description                                  |
|--------------------|------------------|----------------------------------------------|
| `--addr <ip:port>` | `239.1.1.2:1234` | Socket to **bind & listen** (IPv4)           |
| `--refresh <sec>`  | `2`              | Interval to emit the JSON snapshot           |

---

## 📝  JSON Schema (high‑level)

```text
Report {
  ts_time: ISO‑8601 UTC,
  programs: [
    Program {
      program: u16,
      streams: [
        ES {
          pid: u16,
          stream_type: u8,
          codec: str,
          bitrate_kbps: f64,
          // video‑only
          width?: u16, height?: u16, fps?: f32, chroma?: str,
          // audio‑only
          channels?: u8, sample_rate?: u32
        }
      ]
    }
  ]
}
```

*Fields marked `?` appear only when relevant.*


## 📜  License

MIT © 2025 Ignacio Opazo  
Feel free to fork & PR!