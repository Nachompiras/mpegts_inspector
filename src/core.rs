use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::{Duration, Instant},
};
use serde::Serialize;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::{net::UdpSocket};

use crate::{es::{parse_aac_adts, parse_h26x_sps}, psi::{parse_cat, parse_eit_pf, parse_nit, parse_pat, parse_pmt, parse_sdt, PatSection, PmtSection}, si_cache};
use crate::tr101;

pub async fn run(opts: crate::inspector::Options) -> anyhow::Result<()> {
    let socket = create_udp_socket(&opts.addr.to_string())?;
    let sock = UdpSocket::from_std(socket.into())?;
    let mut si_cache = si_cache::SiCache::default();
    let mut tr101 = if true { Some(tr101::Tr101Metrics::new()) } else { None };
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
            process_ts_packet(chunk, &mut pat_map, &mut pmt_map, &mut es_stats, &mut si_cache, tr101.as_mut());
        }

        if last_print.elapsed() >= Duration::from_secs(opts.refresh_secs) {
            // Limpia stats viejos antes de generar JSON
            es_stats.retain(|_, s| s.start.elapsed() < Duration::from_secs(30));
        
            let json = report_json(&pat_map, &pmt_map, &es_stats, tr101.as_ref().unwrap_or(&tr101::Tr101Metrics::default()));
            println!("{json}");
            last_print = Instant::now();
        }
    }
}

/// Same inspection loop but reading from a `broadcast::Receiver` instead of UDP.
///
/// A sender must push `Vec<u8>` buffers that are 188‑byte‑aligned.
pub async fn run_broadcast(
    rx: &mut tokio::sync::broadcast::Receiver<Vec<u8>>,
    refresh_secs: u64,
    analysis: bool, 
) -> anyhow::Result<()> {

    let mut si_cache = si_cache::SiCache::default();
    let mut tr101 = if analysis { Some(tr101::Tr101Metrics::new()) } else { None };
    let mut pat_map = HashMap::<u16, PatSection>::new();
    let mut pmt_map = HashMap::<u16, PmtSection>::new();
    let mut es_stats = HashMap::<u16, EsStats>::new();
    let mut last_print = Instant::now();

    loop {
        let buf = rx.recv().await?;          // waits for next TS chunk
        for chunk in buf.chunks_exact(188) {
            if chunk[0] != 0x47 { continue; }
            process_ts_packet(chunk, &mut pat_map, &mut pmt_map, &mut es_stats, &mut si_cache,tr101.as_mut());
        }

        if last_print.elapsed() >= Duration::from_secs(refresh_secs) {
            es_stats.retain(|_, s| s.start.elapsed() < Duration::from_secs(30));
            let json = report_json(&pat_map, &pmt_map, &es_stats, tr101.as_ref().unwrap_or(&tr101::Tr101Metrics::default()));
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
    tr101:    &'a tr101::Tr101Metrics, 
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
    tr101:     &'a tr101::Tr101Metrics,
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
        tr101: tr101,
    };
    serde_json::to_string_pretty(&rep).unwrap()
}

fn process_ts_packet(
    chunk: &[u8],
    pat_map: &mut HashMap<u16, PatSection>,
    pmt_map: &mut HashMap<u16, PmtSection>,
    es_stats: &mut HashMap<u16, EsStats>,
    si_cache:  &mut si_cache::SiCache,         
    tr101: Option<&mut tr101::Tr101Metrics>,
) {
    let pid = ((chunk[1] & 0x1F) as u16) << 8 | chunk[2] as u16;
    let payload_unit_start = chunk[1] & 0x40 != 0;
    let adaption_field_ctrl = (chunk[3] & 0x30) >> 4;
    let mut payload_offset = 4usize;
    let mut pat_crc_ok: Option<bool> = None;
    let mut pmt_crc_ok: Option<bool> = None;
    let mut cat_crc_ok: Option<bool> = None;
    let mut nit_crc_ok: Option<bool> = None;
    let mut sdt_crc_ok: Option<bool> = None;
    let mut eit_crc_ok: Option<bool> = None;
    let mut table_id: u8 = 0xFF; 

    if adaption_field_ctrl == 2 || adaption_field_ctrl == 0 {
        return;
    }
    if adaption_field_ctrl == 3 {
        let adap_len = chunk[4] as usize;
        payload_offset += 1 + adap_len;
        if payload_offset >= 188 {
            return;
        }
    }

    let mut pcr_found: Option<(u64,u16)> = None;
    if adaption_field_ctrl & 0x02 != 0 && payload_offset > 4 {
        /* adaptation field starts at chunk[4] */
        let ad_len = chunk[4] as usize;
        if ad_len >= 7 && chunk[5] & 0x10 != 0 { // PCR_flag
            let p = &chunk[6..12];
            let base = ((p[0] as u64) << 25)
                    | ((p[1] as u64) << 17)
                    | ((p[2] as u64) << 9)
                    | ((p[3] as u64) << 1)
                    | ((p[4] as u64) >> 7);
            let ext  = ((p[4] & 0x01) as u16) << 8 | p[5] as u16;
            pcr_found = Some((base, ext));
        }
    }

    let payload = &chunk[payload_offset..];

    // PAT
    if pid == 0x0000 && payload_unit_start {
        match parse_pat(payload) {
            Ok(pat) => { 
                pat_crc_ok = Some(true); 
                si_cache.update_pat(pat.clone());
                for entry in &pat.programs {
                    pat_map.insert(entry.program_number, pat.clone());
                }
            }
            Err(_)  => { pat_crc_ok = Some(false); }
        }        
    }
    //CAT
    if pid == 0x0001 && payload_unit_start {
        match parse_cat(payload) {
            Ok((_table_id, _cat)) => {
                cat_crc_ok = Some(true);
                table_id   = _table_id;
            }
            Err(_) => { cat_crc_ok = Some(false); }
        }
    }

    // ── NIT  (PID 0x0010) ───────────────────────────────────────────
    if pid == 0x0010 && payload_unit_start {
        match parse_nit(payload) {
            Ok((tid, nit)) => {
                nit_crc_ok = Some(true);      // CRC passed
                table_id   = tid;             // 0x40 or 0x41
                si_cache.update_nit(nit);     // store for semantic checks
            }
            Err(_) => {
                nit_crc_ok = Some(false);     // CRC fail or malformed
            }
        }
    }

    // PMT
    if let Some((_prog_num, pat)) =
        pat_map.iter().find(|(_, p)| p.programs.iter().any(|e| e.pmt_pid == pid))
    {
        if payload_unit_start {
            match parse_pmt(payload) {
                Ok(pmt) => { 
                    pmt_crc_ok = Some(true); 
                    si_cache.update_pmt(pid, pmt.clone());
                    pmt_map.insert(pid, pmt.clone());
                }
                Err(_)  => { pmt_crc_ok = Some(false); }
            }            
        }
    }

    // ── SDT arrived (PID 0x0011) ───────────────────────────────
    if pid == 0x0011 && payload_unit_start {
        let mut handled = false;
        if sdt_crc_ok.is_none() {             // only parse if not already detected
            if let Ok((tid, sdt)) = crate::psi::parse_sdt(payload) {
                sdt_crc_ok = Some(true);
                table_id   = tid;             // 0x42 / 0x46
                si_cache.update_sdt(sdt);
                handled = true;
            }
        }
    
        // if not SDT, try EIT present/following
        if !handled {
            match parse_eit_pf(payload) {
                Ok((tid, _eit)) => {
                    eit_crc_ok = Some(true);
                    table_id   = tid;         // 0x4E / 0x4F
                }
                Err(_) => { /* may be TOT/TDT or CRC error → ignore */ }
            }
        }
    }

    // ES parsing / bitrate
    if let Some(stats) = es_stats.get_mut(&pid) {
        stats.bytes += 188;
        if stats.codec.is_none() && payload_unit_start && payload.len() >= 6
            && payload.starts_with(&[0x00, 0x00, 0x01])
        {
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
        // FPS by PTS (reuse existing code)
        if let Some(CodecInfo::Video(ref mut vinfo)) = stats.codec {
            if payload_unit_start && payload.len() > 14
                && payload.starts_with(&[0x00, 0x00, 0x01])
            {
                let stream_id = payload[3];
                if stream_id & 0xF0 == 0xE0 {
                    let pts_dts_flags = (payload[7] & 0xC0) >> 6;
                    if pts_dts_flags & 0b10 != 0 {
                        let p = &payload[9..14];
                        let pts: u64 = (((p[0] as u64 & 0x0E) << 29)
                            | ((p[1] as u64) << 22)
                            | (((p[2] as u64 & 0xFE) >> 1) << 15)
                            | ((p[3] as u64) << 7)
                            | ((p[4] as u64) >> 1));
                        if let Some(prev) = stats.last_pts {
                            if pts > prev {
                                let delta = pts - prev;
                                if delta != 0 {
                                    let fps_est = 90000.0 / delta as f32;
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
     /* … inside the function … */
     if let Some(metrics) = tr101 {

        if si_cache.check_service_id_mismatch() {
            metrics.service_id_mismatch += 1;
        }

         /* ───── 3.5 splice_countdown ───── */
         if adaption_field_ctrl & 0x02 != 0 && payload_offset > 4 {
            let ad_len = chunk[4] as usize;
            if ad_len >= 1 {
                let flags = chunk[5];
                if flags & 0x04 != 0 {
                    // splice_countdown present
                    let sc_pos = 6 + ad_len - 1;           // last byte in AF
                    let val = chunk[sc_pos] as i8;
                    match metrics.last_splice_value {
                        None => metrics.last_splice_value = Some(val),
                        Some(prev) => {
                            // legal: same value, decrement by 1, or wrap -1→0
                            if !(val == prev || val == prev - 1 || (prev == -1 && val == 0)) {
                                metrics.splice_count_errors += 1; // increment TR-101 counter
                            }
                            metrics.last_splice_value = Some(val);
                        }
                    }
                }
            }
        }

        metrics.on_packet(
            chunk,
            pid,
            payload_unit_start,
            0x0000,
            pat_crc_ok,
            pmt_crc_ok,
            pcr_found,
            cat_crc_ok,
            nit_crc_ok,
            sdt_crc_ok,
            eit_crc_ok,
            table_id,            
        );
    }
}

fn stream_type_name(st: u8) -> &'static str {
    match st {
        0x1B => "H.264",
        0x24 => "HEVC",
        0x0F => "AAC",
        _ => "unk",
    }
}