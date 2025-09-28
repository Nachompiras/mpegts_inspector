//! Main packet processing logic

use std::collections::HashMap;
use crate::types::{CodecInfo, SubtitleInfo, AnalysisMode, SiTableContext, PacketContext, CrcValidation};
use crate::constants::*;
use crate::stats::StatsManager;
use crate::parsers::{parse_video_codec, parse_audio_codec};
use crate::psi::{parse_pat, parse_pmt, parse_cat, parse_nit, parse_sdt, parse_eit_pf, parse_tdt_tot, PatSection, PmtSection};
use crate::si_cache::SiCache;
use crate::tr101::Tr101Metrics;

pub struct PacketProcessor {
    pub pat_map: HashMap<u16, PatSection>,
    pub pmt_map: HashMap<u16, PmtSection>,
    pub pcr_pid_map: HashMap<u16, u16>, // program_number -> pcr_pid
    pub pat_versions: HashMap<u16, u8>, // program_number -> version
    pub pmt_versions: HashMap<u16, u8>, // pmt_pid -> version
    pub stats_manager: StatsManager,
    pub si_cache: SiCache,
    pub tr101: Option<Tr101Metrics>,
}

impl PacketProcessor {
    pub fn new(enable_tr101: bool) -> Self {
        Self {
            pat_map: HashMap::new(),
            pmt_map: HashMap::new(),
            pcr_pid_map: HashMap::new(),
            pat_versions: HashMap::new(),
            pmt_versions: HashMap::new(),
            stats_manager: StatsManager::new(),
            si_cache: SiCache::default(),
            tr101: if enable_tr101 { Some(Tr101Metrics::new()) } else { None },
        }
    }

    pub fn set_analysis_mode(&mut self, mode: Option<AnalysisMode>) {
        match mode {
            Some(AnalysisMode::Tr101) | Some(AnalysisMode::Tr101Priority1) | Some(AnalysisMode::Tr101Priority12) => {
                if self.tr101.is_none() {
                    self.tr101 = Some(Tr101Metrics::new());
                }
            }
            Some(AnalysisMode::Mux) => {
                // Keep existing tr101 instance but don't use it actively
            }
            Some(AnalysisMode::None) | None => {
                // Keep all data structures but minimal processing
            }
        }
    }

    /// Process a single TS packet
    pub fn process_packet(&mut self, chunk: &[u8], analysis_mode: Option<AnalysisMode>) {
        // Check packet length
        if chunk.len() < TS_PACKET_SIZE {
            return; // Invalid packet
        }

        // Check sync byte and detect sync loss
        let sync_byte_valid = chunk[0] == TS_SYNC_BYTE;
        if let Some(ref mut tr101) = self.tr101 {
            tr101.check_ts_sync_loss(sync_byte_valid, analysis_mode.unwrap_or(AnalysisMode::None));
        }

        if !sync_byte_valid {
            return; // Invalid sync byte
        }

        let pid = (((chunk[1] & 0x1F) as u16) << 8) | (chunk[2] as u16);
        let payload_unit_start = chunk[1] & 0x40 != 0;
        let adaption_field_ctrl = (chunk[3] & 0x30) >> 4;
        let mut payload_offset = 4usize;

        // Check for PID errors (unexpected/undeclared PIDs)
        if let Some(ref mut tr101) = self.tr101 {
            tr101.check_pid_error(pid, analysis_mode.unwrap_or(AnalysisMode::None));
        }

        // State variables for TR-101 reporting
        let mut si_context = SiTableContext {
            table_id: 0xFF,
            ..Default::default()
        };

        // Skip packets with no payload or adaptation field only
        if adaption_field_ctrl == 2 || adaption_field_ctrl == 0 {
            return;
        }

        // Handle adaptation field
        if adaption_field_ctrl == 3 {
            let adap_len = chunk[4] as usize;
            payload_offset += 1 + adap_len;
            if payload_offset >= 188 {
                return;
            }
        }

        // Extract PCR if present and this PID is a designated PCR PID
        let mut pcr_found: Option<(u64, u16)> = None;
        let is_pcr_pid = self.pcr_pid_map.values().any(|&pcr_pid| pcr_pid == pid);

        if adaption_field_ctrl & 0x02 != 0 && payload_offset > 4 && is_pcr_pid {
            let ad_len = chunk[4] as usize;
            if ad_len >= 7 && chunk[5] & 0x10 != 0 { // PCR_flag
                let p = &chunk[6..12];
                let base = ((p[0] as u64) << 25)
                        | ((p[1] as u64) << 17)
                        | ((p[2] as u64) << 9)
                        | ((p[3] as u64) << 1)
                        | ((p[4] as u64) >> 7);
                let ext = (((p[4] & 0x01) as u16) << 8) | (p[5] as u16);
                pcr_found = Some((base, ext));
            }
        }

        let payload = &chunk[payload_offset..];

        // Only process SI tables if in analysis mode (any TR-101 level or Mux)
        if matches!(analysis_mode, Some(AnalysisMode::Mux) | Some(AnalysisMode::Tr101) | Some(AnalysisMode::Tr101Priority1) | Some(AnalysisMode::Tr101Priority12)) {
            self.process_si_tables(pid, payload_unit_start, payload, &mut si_context, analysis_mode);
            self.process_elementary_streams(pid, payload_unit_start, payload, analysis_mode);
        }

        // TR-101 analysis if enabled
        if matches!(analysis_mode, Some(AnalysisMode::Tr101) | Some(AnalysisMode::Tr101Priority1) | Some(AnalysisMode::Tr101Priority12)) {
            if let Some(ref mut tr101) = self.tr101 {
                // Check for service ID mismatch - Priority 3
                if matches!(analysis_mode, Some(AnalysisMode::Tr101)) && self.si_cache.check_service_id_mismatch() {
                    tr101.service_id_mismatch += 1;
                }

                // Handle splice_countdown in adaptation field - Priority 3
                if matches!(analysis_mode, Some(AnalysisMode::Tr101)) && adaption_field_ctrl & 0x02 != 0 && payload_offset > 4 {
                    let ad_len = chunk[4] as usize;
                    if ad_len >= 1 {
                        let flags = chunk[5];
                        if flags & 0x04 != 0 {
                            // splice_countdown present
                            let sc_pos = 6 + ad_len - 1;
                            if sc_pos < chunk.len() {
                                let val = chunk[sc_pos] as i8;
                                match tr101.last_splice_value {
                                    None => tr101.last_splice_value = Some(val),
                                    Some(prev) => {
                                        // Legal: same value, decrement by 1, or wrap -1→0
                                        if !(val == prev || val == prev - 1 || (prev == -1 && val == 0)) {
                                            tr101.splice_count_errors += 1;
                                        }
                                        tr101.last_splice_value = Some(val);
                                    }
                                }
                            }
                        }
                    }
                }

                // Call optimized TR-101 packet handler
                let packet_ctx = PacketContext {
                    chunk,
                    pid,
                    payload_unit_start,
                    pat_pid: 0x0000,
                    pcr_opt: pcr_found,
                    table_id: si_context.table_id,
                    priority_level: analysis_mode.unwrap_or(AnalysisMode::None),
                };

                let crc_validation = CrcValidation {
                    pat_crc_ok: si_context.pat_crc_ok,
                    pmt_crc_ok: si_context.pmt_crc_ok,
                    cat_crc_ok: si_context.cat_crc_ok,
                    nit_crc_ok: si_context.nit_crc_ok,
                    sdt_crc_ok: si_context.sdt_crc_ok,
                    eit_crc_ok: si_context.eit_crc_ok,
                };

                tr101.on_packet_with_context(packet_ctx, crc_validation);
            }
        }
    }

    fn process_si_tables(
        &mut self,
        pid: u16,
        payload_unit_start: bool,
        payload: &[u8],
        context: &mut SiTableContext,
        analysis_mode: Option<AnalysisMode>,
    ) {
        // PAT (PID 0x0000)
        if pid == 0x0000 && payload_unit_start {
            match parse_pat(payload) {
                Ok(pat) => {
                    context.pat_crc_ok = Some(true);

                    // Check for PAT version changes (Priority 2)
                    if let Some(ref mut tr101) = self.tr101 {
                        for entry in &pat.programs {
                            tr101.check_pat_version_change(entry.program_number, pat.version, analysis_mode.unwrap_or(AnalysisMode::None));
                        }
                    }

                    // Store PAT efficiently - avoid multiple clones
                    self.si_cache.update_pat(pat.clone());
                    for entry in &pat.programs {
                        self.pat_map.insert(entry.program_number, pat.clone());
                    }
                }
                Err(_) => { context.pat_crc_ok = Some(false); }
            }
        }

        // CAT (PID 0x0001)
        if pid == 0x0001 && payload_unit_start {
            match parse_cat(payload) {
                Ok((_table_id, _cat)) => {
                    context.cat_crc_ok = Some(true);
                    context.table_id = _table_id;
                }
                Err(_) => { context.cat_crc_ok = Some(false); }
            }
        }

        // NIT (PID 0x0010)
        if pid == 0x0010 && payload_unit_start {
            match parse_nit(payload) {
                Ok((tid, nit)) => {
                    context.nit_crc_ok = Some(true);
                    context.table_id = tid;
                    self.si_cache.update_nit(nit);
                }
                Err(_) => {
                    context.nit_crc_ok = Some(false);
                }
            }
        }

        // PMT
        if let Some((_prog_num, _pat)) =
            self.pat_map.iter().find(|(_, p)| p.programs.iter().any(|e| e.pmt_pid == pid))
        {
            if payload_unit_start {
                match parse_pmt(payload) {
                    Ok(pmt) => {
                        context.pmt_crc_ok = Some(true);

                        // Check for PMT version changes (Priority 2)
                        if let Some(ref mut tr101) = self.tr101 {
                            tr101.check_pmt_version_change(pid, pmt.version, analysis_mode.unwrap_or(AnalysisMode::None));

                            // Register all PIDs in this PMT as known/authorized
                            tr101.register_known_pid(pmt.pcr_pid); // Register PCR PID
                            for stream in &pmt.streams {
                                tr101.register_known_pid(stream.elementary_pid); // Register elementary stream PIDs
                            }
                        }

                        // Extract and store PCR PID for this program
                        if let Some((_prog_num, _pat)) = self.pat_map.iter().find(|(_, p)| p.programs.iter().any(|e| e.pmt_pid == pid)) {
                            if let Some(pat_entry) = _pat.programs.iter().find(|e| e.pmt_pid == pid) {
                                self.pcr_pid_map.insert(pat_entry.program_number, pmt.pcr_pid);
                            }
                        }

                        self.si_cache.update_pmt(pid, pmt.clone());
                        self.pmt_map.insert(pid, pmt.clone());
                    }
                    Err(_) => { context.pmt_crc_ok = Some(false); }
                }
            }
        }

        // SDT/EIT (PID 0x0011)
        if pid == 0x0011 && payload_unit_start {
            let mut handled = false;
            if context.sdt_crc_ok.is_none() {
                if let Ok((tid, sdt)) = parse_sdt(payload) {
                    context.sdt_crc_ok = Some(true);
                    context.table_id = tid;
                    self.si_cache.update_sdt(sdt);
                    handled = true;
                }
            }

            if !handled {
                match parse_eit_pf(payload) {
                    Ok((tid, _eit)) => {
                        context.eit_crc_ok = Some(true);
                        context.table_id = tid;
                    }
                    Err(_) => { /* may be TOT/TDT or CRC error → ignore */ }
                }
            }
        }

        // TDT/TOT (PID 0x0014)
        if pid == 0x0014 && payload_unit_start {
            match parse_tdt_tot(payload) {
                Ok((tid, _tdt_tot)) => {
                    context.table_id = tid;
                    // TDT (0x70) has no CRC, TOT (0x73) has CRC
                    if tid == 0x73 {
                        context.tdt_crc_ok = Some(true);  // TOT CRC was validated successfully
                    }
                    // For TDT, we don't set tdt_crc_ok since it has no CRC
                }
                Err(_) => {
                    // If it's a TOT (should have CRC), mark as CRC error
                    // We can't easily determine if it was supposed to be TOT vs TDT here,
                    // so we conservatively assume CRC error only if parse failed
                    context.tdt_crc_ok = Some(false);
                }
            }
        }
    }

    fn process_elementary_streams(&mut self, pid: u16, payload_unit_start: bool, payload: &[u8], analysis_mode: Option<AnalysisMode>) {
        // Update byte counts for existing streams
        if self.stats_manager.contains_pid(pid) {
            self.stats_manager.update_bytes(pid, TS_PACKET_SIZE);
            self.parse_codec_info(pid, payload_unit_start, payload, analysis_mode);
        } else if payload_unit_start {
            // Check if this PID is an elementary stream from any PMT
            if let Some((_, pmt)) = self.pmt_map
                .iter()
                .find(|(_, p)| p.streams.iter().any(|s| s.elementary_pid == pid))
            {
                if let Some(stream) = pmt
                    .streams
                    .iter()
                    .find(|s| s.elementary_pid == pid)
                {

                    self.stats_manager.add_stream(pid, stream.stream_type);
                    self.stats_manager.update_bytes(pid, TS_PACKET_SIZE);
                }
            }
        }
    }

    fn parse_codec_info(&mut self, pid: u16, payload_unit_start: bool, payload: &[u8], analysis_mode: Option<AnalysisMode>) {
        let Some(stats) = self.stats_manager.get(pid) else { return };

        if stats.codec.is_some() {
            return; // Already parsed
        }

        let stream_type = stats.stream_type;

        // Handle stream types that don't require PES header parsing
        match stream_type {
            0x06 => {
                // DVB Subtitle - no ES parsing needed
                let codec = CodecInfo::Subtitle(SubtitleInfo {
                    codec: "DVB Subtitle".to_string(),
                });
                self.stats_manager.set_codec(pid, codec);
            }
            0x03 | 0x04 => {
                // MPEG-1 Audio Layer II - can be found directly in payload
                if let Some(mp2) = parse_audio_codec(stream_type, payload) {
                    let codec = CodecInfo::Audio(mp2);
                    self.stats_manager.set_codec(pid, codec);
                }
            }
            0x11 => {
                // AAC LATM - can be found directly in payload
                if let Some(latm) = parse_audio_codec(stream_type, payload) {
                    let codec = CodecInfo::Audio(latm);
                    self.stats_manager.set_codec(pid, codec);
                }
            }
            0x81 => {
                // AC-3 - can be found directly in payload
                if let Some(ac3) = parse_audio_codec(stream_type, payload) {
                    let codec = CodecInfo::Audio(ac3);
                    self.stats_manager.set_codec(pid, codec);
                }
            }
            _ => {}
        }

        // Handle PES-based parsing for video and AAC
        if payload_unit_start && payload.len() >= 6 && payload.starts_with(&[0x00, 0x00, 0x01]) {
            let pes_hdr_len = 9 + payload[8] as usize;
            if pes_hdr_len < payload.len() {
                let es_payload = &payload[pes_hdr_len..];

                // Try video parsing
                if let Some(video_info) = parse_video_codec(stream_type, es_payload) {
                    let codec = CodecInfo::Video(video_info);
                    self.stats_manager.set_codec(pid, codec);
                }
                // Try audio parsing
                else if let Some(audio_info) = parse_audio_codec(stream_type, es_payload) {
                    let codec = CodecInfo::Audio(audio_info);
                    self.stats_manager.set_codec(pid, codec);
                }
            }
        }

        // FPS calculation by PTS for video streams
        self.calculate_fps_from_pts(pid, payload_unit_start, payload, analysis_mode);
    }

    fn calculate_fps_from_pts(&mut self, pid: u16, payload_unit_start: bool, payload: &[u8], analysis_mode: Option<AnalysisMode>) {
        if !payload_unit_start || payload.len() <= 14 || !payload.starts_with(&[0x00, 0x00, 0x01]) {
            return;
        }

        let stream_id = payload[3];
        if stream_id & 0xF0 != 0xE0 { // Not video stream
            return;
        }

        let pts_dts_flags = (payload[7] & 0xC0) >> 6;
        if pts_dts_flags & 0b10 == 0 { // No PTS
            return;
        }

        let p = &payload[9..14];
        let pts: u64 = ((p[0] as u64 & 0x0E) << 29)
            | ((p[1] as u64) << 22)
            | (((p[2] as u64 & 0xFE) >> 1) << 15)
            | ((p[3] as u64) << 7)
            | ((p[4] as u64) >> 1);

        if let Some(stats) = self.stats_manager.get_mut(pid) {
            // Store PTS sample for FPS calculation
            stats.pts_samples.push(pts);

            // Keep only recent samples (last 10 frames)
            if stats.pts_samples.len() > 10 {
                stats.pts_samples.remove(0);
            }

            if let Some(CodecInfo::Video(ref mut vinfo)) = stats.codec {
                // Calculate FPS from multiple PTS samples if we have enough
                if stats.pts_samples.len() >= 3 {
                    // Calculate deltas efficiently without unnecessary clones
                    let mut deltas: Vec<u64> = {
                        let mut sorted_indices: Vec<usize> = (0..stats.pts_samples.len()).collect();
                        sorted_indices.sort_unstable_by_key(|&i| stats.pts_samples[i]);

                        sorted_indices.windows(2)
                            .filter_map(|window| {
                                let delta = stats.pts_samples[window[1]].saturating_sub(stats.pts_samples[window[0]]);
                                if delta > 0 && delta < MAX_PTS_DELTA_TICKS { // Sanity check: delta should be less than 1 second
                                    Some(delta)
                                } else {
                                    None
                                }
                            })
                            .collect()
                    };

                    if !deltas.is_empty() {
                        // Use median delta to avoid outliers from B-frames
                        deltas.sort_unstable();
                        let median_delta = deltas[deltas.len() / 2];
                        let fps_est = 90000.0 / median_delta as f32;


                        // Only update FPS if:
                        // 1. We don't have FPS from SPS (fps == 0.0), OR
                        // 2. The FPS from SPS seems wrong (too different from PTS calculation)
                        if vinfo.fps == 0.0 || (vinfo.fps - fps_est).abs() > 2.0 {
                            vinfo.fps = round_to_common_fps(fps_est);
                        }
                    }
                }
            }
            // Check for PTS errors (Priority 2)
            if let Some(ref mut tr101) = self.tr101 {
                tr101.check_pts_error(pid, pts, analysis_mode.unwrap_or(AnalysisMode::None));
            }

            stats.last_pts = Some(pts);
        }
    }

    /// Clean up old/inactive streams
    pub fn cleanup_old_streams(&mut self, timeout_secs: u64) {
        self.stats_manager.cleanup_old_streams(std::time::Duration::from_secs(timeout_secs));
    }

    /// Get PCR PID for a specific program number
    pub fn get_pcr_pid(&self, program_number: u16) -> Option<u16> {
        self.pcr_pid_map.get(&program_number).copied()
    }

    /// Get PMT version for a specific PMT PID
    pub fn get_pmt_version(&self, pmt_pid: u16) -> Option<u8> {
        self.pmt_map.get(&pmt_pid).map(|pmt| pmt.version)
    }

    /// Get TR-101 metrics reference
    pub fn get_tr101_metrics(&self) -> Tr101Metrics {
        self.tr101.as_ref().cloned().unwrap_or_default()
    }
}

/// Round estimated FPS to common frame rates for better accuracy
/// Also handles interlaced video detection (field rate -> frame rate)
fn round_to_common_fps(fps_est: f32) -> f32 {
    let frame_rates = [
        23.976, 24.0, 25.0, 29.97, 30.0, 48.0, 50.0, 60.0, 120.0
    ];
    let field_rates = [
        47.952, 48.0, 50.0, 59.94, 60.0, 96.0, 100.0, 120.0, 240.0
    ];

    // First, check if it matches common frame rates directly
    for &rate in &frame_rates {
        if (fps_est - rate).abs() < 0.5 {
            return rate;
        }
    }

    // Check if it matches common field rates (interlaced video)
    // If so, divide by 2 to get frame rate
    for &field_rate in &field_rates {
        if (fps_est - field_rate).abs() < 0.5 {
            let frame_rate = field_rate / 2.0;
            // Verify it's a sensible frame rate
            for &rate in &frame_rates {
                if (frame_rate - rate).abs() < 0.1 {
                    return rate;
                }
            }
            return (frame_rate * 100.0).round() / 100.0;
        }
    }

    // Round to 2 decimal places if no common rate found
    (fps_est * 100.0).round() / 100.0
}