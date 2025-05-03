use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::{Duration, Instant},
};
use serde::Serialize;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::{net::UdpSocket};


use crate::psi::{parse_pat, parse_pmt, PatSection, PmtSection};
use crate::es::{parse_aac_adts, parse_h26x_sps};


#[tokio::main(flavor = "multi_thread")]
pub async fn run(opts: crate::inspector::Options) -> anyhow::Result<()> {
    let socket = create_udp_socket(&opts.addr.to_string())?;
    let sock = UdpSocket::from_std(socket.into())?;

    let mut buf = [0u8; 2048];
    let mut pat_map = HashMap::<u16, PatSection>::new(); // program_number -> PAT info
    let mut pmt_map = HashMap::<u16, PmtSection>::new(); // pmt_pid -> parsed PMT
    let mut es_stats = HashMap::<u16, EsStats>::new(); // pid -> rolling stats

    let mut last_print = Instant::now();

    loop {
        let n = sock.recv(&mut buf).await?;
        if n == 0 {
            continue;
        }

        // iterate TS packets (188 B aligned)
        for chunk in buf[..n].chunks_exact(188) {
            if chunk[0] != 0x47 {
                continue; // bad sync
            }
            let pid = ((chunk[1] & 0x1F) as u16) << 8 | chunk[2] as u16;
            let payload_unit_start = chunk[1] & 0x40 != 0;
            let adaption_field_ctrl = (chunk[3] & 0x30) >> 4;

            let mut payload_offset = 4usize;
            if adaption_field_ctrl == 2 || adaption_field_ctrl == 0 {
                continue; // no payload
            }
            if adaption_field_ctrl == 3 {
                // skip adaptation field
                let adap_len = chunk[4] as usize;
                payload_offset += 1 + adap_len;
                if payload_offset >= 188 {
                    continue;
                }
            }
            let payload = &chunk[payload_offset..];

            // PAT
            if pid == 0x0000 && payload_unit_start {
                if let Ok(pat) = parse_pat(payload) {
                    for entry in &pat.programs {
                        pat_map.insert(entry.program_number, pat.clone());
                    }
                }
            }

            // PMT
            if let Some((_prog_num, pat)) = pat_map.iter().find(|(_, p)| p.programs.iter().any(|e| e.pmt_pid == pid)) {
                if payload_unit_start {
                    if let Ok(pmt) = parse_pmt(payload) {
                        pmt_map.insert(pid, pmt);
                    }
                }
            }

            // ES parsing / bitrate
            if let Some(stats) = es_stats.get_mut(&pid) {
                stats.bytes += 188;
                if stats.codec.is_none() {
                    // first PES packet generally starts with PES start code 0x000001
                    if payload_unit_start && payload.len() >= 6 && payload[0] == 0x00 && payload[1] == 0x00 && payload[2] == 0x01 {
                        let stream_id = payload[3];
                        let pes_hdr_len = 9 + payload[8] as usize;
                        if pes_hdr_len < payload.len() {
                            let es_payload = &payload[pes_hdr_len..];
                            match stats.stream_type {
                                0x1B | 0x24 => {
                                    if let Some(v) = parse_h26x_sps(es_payload) {
                                        stats.codec = Some(CodecInfo::Video(v));
                                    }
                                }
                                0x0F => {
                                    if let Some(aac) = parse_aac_adts(es_payload) {
                                        stats.codec = Some(CodecInfo::Audio(aac));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                // ---------- FPS estimation using PTS ----------
                if let Some(CodecInfo::Video(ref mut vinfo)) = stats.codec {
                    // We only evaluate on PES start for video streams (stream_id 0xE0–0xEF)
                    if payload_unit_start && payload.len() > 14 && payload.starts_with(&[0x00, 0x00, 0x01]) {
                        let stream_id = payload[3];
                        if stream_id & 0xF0 == 0xE0 {
                            // PTS flag present?
                            let pts_dts_flags = (payload[7] & 0xC0) >> 6;
                            if pts_dts_flags & 0b10 != 0 {
                                // PTS is stored in 5 bytes starting at index 9
                                let p = &payload[9..14];
                                let pts: u64 = (((p[0] as u64 & 0x0E) << 29)
                                    |  ((p[1] as u64) << 22)
                                    | (((p[2] as u64 & 0xFE) >> 1) << 15)
                                    |  ((p[3] as u64) << 7)
                                    |  ((p[4] as u64) >> 1));

                                if let Some(prev) = stats.last_pts {
                                    if pts > prev {
                                        let delta = pts - prev; // units: 1/90000 s
                                        if delta != 0 {
                                            let fps_est = 90000.0 / delta as f32;
                                            // only overwrite if SPS didn't carry fps info
                                            if vinfo.fps == 0.0 {
                                                vinfo.fps = (fps_est * 100.0).round() / 100.0;
                                            }
                                        }
                                    }
                                }
                                stats.last_pts = Some(pts);
                            }
                        }
                    }
                }
            } else if payload_unit_start {
                // maybe new ES PID
                if let Some((_pmt_pid, pmt)) = pmt_map
                    .iter()
                    .find(|(_, p)| p.streams.iter().any(|s| s.elementary_pid == pid))
                {
                    let stream = pmt
                        .streams
                        .iter()
                        .find(|s| s.elementary_pid == pid)
                        .unwrap();
                    es_stats.insert(
                        pid,
                        EsStats {
                            stream_type: stream.stream_type,
                            codec: None,
                            bytes: 188,
                            start: Instant::now(),
                            last_pts: None,
                        },
                    );
                }
            }
        }

        if last_print.elapsed() >= Duration::from_secs(opts.refresh_secs) {
            // Limpia stats viejos antes de generar JSON
            es_stats.retain(|_, s| s.start.elapsed() < Duration::from_secs(30));
        
            let json = report_json(&pat_map, &pmt_map, &es_stats);
            println!("{json}");
            last_print = Instant::now();
        }
    }
}

/// Join multicast / bind unicast socket helper
fn create_udp_socket(addr: &str) -> anyhow::Result<Socket> {
    let sock_addr: SocketAddr = addr.parse()?;
    let ip = match sock_addr.ip() {
        IpAddr::V4(v4) => v4,
        _ => anyhow::bail!("only IPv4 is supported"),
    };

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.bind(&sock_addr.into())?;

    if ip.is_multicast() {
        let iface = Ipv4Addr::UNSPECIFIED; // default interface
        socket.join_multicast_v4(&ip, &iface)?;
    }
    socket.set_nonblocking(true)?;
    Ok(socket)
}

/// Stores rolling statistics for an ES
struct EsStats {
    stream_type: u8,
    codec: Option<CodecInfo>,
    bytes: usize,
    start: Instant,
    last_pts: Option<u64>,
}

#[derive(Clone)]
pub enum CodecInfo {
    Video(VideoInfo),
    Audio(AacInfo),
}

#[derive(Clone)]
pub struct VideoInfo {
    pub codec: &'static str,
    pub width: u16,
    pub height: u16,
    pub fps: f32,
    pub chroma: String,
}

#[derive(Clone)]
pub struct AacInfo {
    pub profile: &'static str,
    pub sr: u32,
    pub channels: u8,
}

#[derive(Serialize)]
struct EsJson<'a> {
    pid:        u16,
    stream_type:u8,
    codec:      &'a str,
    bitrate_kbps:f64,
    // campos opcionales que no siempre existen:
    #[serde(skip_serializing_if = "Option::is_none")]
    width:      Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    height:     Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fps:        Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chroma:     Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    channels:   Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sample_rate:Option<u32>,
}

#[derive(Serialize)]
struct ProgramJson<'a> {
    program: u16,
    streams: Vec<EsJson<'a>>,
}

#[derive(Serialize)]
struct ReportJson<'a> {
    ts_time: String,
    programs: Vec<ProgramJson<'a>>,
}

fn print_report(
    pat_map: &HashMap<u16, PatSection>,
    pmt_map: &HashMap<u16, PmtSection>,
    es_stats: &mut HashMap<u16, EsStats>,
) {
    println!("================ MPEG-TS Inspector =================");
    for (prog_num, pat) in pat_map {
        println!("Program #{prog_num}");
        // find PMT
        if let Some(pmt_pid) = pat.programs.iter().find(|p| p.program_number == *prog_num).map(|p| p.pmt_pid) {
            if let Some(pmt) = pmt_map.get(&pmt_pid) {
                for s in &pmt.streams {
                    let pid = s.elementary_pid;
                    if let Some(stat) = es_stats.get(&pid) {
                        let seconds = stat.start.elapsed().as_secs_f64();
                        let bitrate_kbps = (stat.bytes as f64 * 8.0 / 1000.0) / seconds.max(0.1);
                        let (codec_name, extra) = match &stat.codec {
                            Some(CodecInfo::Video(v)) => (
                                v.codec,
                                format!("{:}×{:} {:.2} fps {}", v.width, v.height, v.fps, v.chroma),
                            ),
                            Some(CodecInfo::Audio(a)) => (
                                "AAC",
                                format!("{}ch {} Hz ({})", a.channels, a.sr, a.profile),
                            ),
                            None => ("?", String::new()),
                        };
                        println!(
                            "  PID 0x{pid:04X} | {: <4} {: <9} | {:>6.1} kb/s {}",
                            stream_type_name(s.stream_type),
                            codec_name,
                            bitrate_kbps,
                            extra
                        );
                    }
                }
            }
        }
    }
    // prune old pids
    es_stats.retain(|_, s| s.start.elapsed() < Duration::from_secs(30));
}

fn report_json<'a>(
    pat_map: &'a HashMap<u16, PatSection>,
    pmt_map: &'a HashMap<u16, PmtSection>,
    es_stats: &'a HashMap<u16, EsStats>,
) -> String {
    let mut programs_out = Vec::new();

    for (prog_num, pat) in pat_map {
        if let Some(pmt_pid) = pat.programs
                                  .iter()
                                  .find(|p| p.program_number == *prog_num)
                                  .map(|p| p.pmt_pid)
        {
            if let Some(pmt) = pmt_map.get(&pmt_pid) {
                let mut es_vec = Vec::new();
                for s in &pmt.streams {
                    if let Some(stat) = es_stats.get(&s.elementary_pid) {
                        let secs = stat.start.elapsed().as_secs_f64().max(0.1);
                        let br   = (stat.bytes as f64 * 8.0 / 1000.0) / secs;

                        match &stat.codec {
                            Some(CodecInfo::Video(v)) => es_vec.push(EsJson {
                                pid: s.elementary_pid,
                                stream_type: s.stream_type,
                                codec: v.codec,
                                bitrate_kbps: br,
                                width: Some(v.width),
                                height: Some(v.height),
                                fps: if v.fps > 0.0 { Some(v.fps) } else { None },
                                chroma: Some(&v.chroma),
                                channels: None,
                                sample_rate: None,
                            }),
                            Some(CodecInfo::Audio(a)) => es_vec.push(EsJson {
                                pid: s.elementary_pid,
                                stream_type: s.stream_type,
                                codec: "AAC",
                                bitrate_kbps: br,
                                width: None,
                                height: None,
                                fps: None,
                                chroma: None,
                                channels: Some(a.channels),
                                sample_rate: Some(a.sr),
                            }),
                            None => {}
                        }
                    }
                }
                programs_out.push(ProgramJson {
                    program: *prog_num,
                    streams: es_vec,
                });
            }
        }
    }

    let rep = ReportJson {
        ts_time: chrono::Utc::now().to_rfc3339(),
        programs: programs_out,
    };
    serde_json::to_string_pretty(&rep).unwrap()
}

fn stream_type_name(st: u8) -> &'static str {
    match st {
        0x1B => "H.264",
        0x24 => "HEVC",
        0x0F => "AAC",
        _ => "unk",
    }
}