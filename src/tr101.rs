// src/tr101.rs
use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Serialize;
use crate::types::{PacketContext, CrcValidation};
use crate::constants::*;

// Local constants specific to TR-101 implementation
/// PCR accuracy tolerance in PCR ticks (27 MHz)
/// TR 101 290 specifies ±500ns, but we use a more permissive threshold
/// to avoid false positives from network jitter and OS scheduling
/// 500 µs = 27,000,000 * 500e-6 = 13,500 ticks
const PCR_ACCURACY_TICKS: u64 = 13_500;

#[derive(Default, Debug, Clone,Serialize)]
pub struct Tr101Metrics {
    // Priority-1 counters
    pub sync_byte_errors:            u64, // 1.1
    pub ts_sync_loss:                u64, // 1.1b (TS synchronization loss)
    pub transport_error_indicator:   u64, // 1.2
    pub pat_crc_errors:              u64, // 1.3a
    pub pat_timeout:                 u64, // 1.3b
    pub continuity_counter_errors:   u64, // 1.4
    pub pmt_crc_errors:              u64, // 1.5a
    pub pmt_timeout:                 u64, // 1.5b
    pub pid_errors:                  u64, // 1.6 (unreferenced/unexpected PIDs)

    /* ───────── Priority-2 (new) ───────── */
    pub pcr_repetition_errors:       u64, // 2.4
    pub pcr_accuracy_errors:         u64, // 2.5
    pub null_packet_rate_errors:    u64, // 2.6
    pub cat_crc_errors:             u64, // 2.7a
    pub cat_timeout:                u64, // 2.7b
    pub pat_version_changes:         u64, // 2.8 (version change detection)
    pub pmt_version_changes:         u64, // 2.9 (version change detection)
    pub pts_errors:                  u64, // 2.10 (PTS discontinuity/errors)
 
     /* Priority-3 */
     pub service_id_mismatch:        u64, // 3.2-d
     pub nit_crc_errors:             u64, // 3.1a
     pub nit_timeout:                u64, // 3.1b
     pub sdt_crc_errors:             u64, // 3.2a
     pub sdt_timeout:                u64, // 3.2b
     pub eit_crc_errors:             u64, // 3.3a
     pub eit_timeout:                u64, // 3.3b
     pub tdt_timeout:                u64, // 3.4   (TDT/TOT presence)
     pub splice_count_errors: u64, // 3.5

    // internal state
    #[serde(skip)]
    last_pat_seen: Option<Instant>,
    #[serde(skip)]
    last_pmt_seen: HashMap<u16, Instant>, // pmt_pid → last time seen
    last_cc: HashMap<u16, u8>,            // pid → last continuity counter
    #[serde(skip)]
    pat_versions: HashMap<u16, u8>,       // program_number → last version
    #[serde(skip)]
    pmt_versions: HashMap<u16, u8>,       // pmt_pid → last version
    #[serde(skip)]
    last_pcr_info: HashMap<u16, (u64, Instant)>, // pid → (pcr_ticks, wallclock)
    #[serde(skip)]
    bytes_in_1s:           u64,
    #[serde(skip)]
    null_bytes_in_1s:      u64,
    #[serde(skip)]
    last_rate_check:       Option<Instant>,
    #[serde(skip)]
    last_cat_seen:         Option<Instant>,
    #[serde(skip)]
    last_nit_seen:         Option<Instant>,
    #[serde(skip)]
    last_sdt_seen:         Option<Instant>,
    #[serde(skip)]
    last_eit_seen:         Option<Instant>,
    #[serde(skip)]
    last_tdt_seen:         Option<Instant>,
    #[serde(skip)]
    pub last_splice_value: Option<i8>,
    #[serde(skip)]
    startup_time: Option<Instant>,
    #[serde(skip)]
    pat_timeout_state: bool,  // Track if PAT is currently in timeout state
    #[serde(skip)]
    pmt_timeout_state: HashMap<u16, bool>,  // Track PMT timeout state per PID
    #[serde(skip)]
    cat_timeout_state: bool,  // Track if CAT is currently in timeout state
    #[serde(skip)]
    known_pids: std::collections::HashSet<u16>,  // PIDs that are authorized/expected
    #[serde(skip)]
    last_pts_per_pid: HashMap<u16, u64>,  // Track last PTS per PID for discontinuity detection
    #[serde(skip)]
    sync_loss_counter: u64,  // Track consecutive sync loss occurrences
}

impl Tr101Metrics {
    pub fn new() -> Self {
        Self {
            last_rate_check: None,
            startup_time: Some(Instant::now()),
            pat_timeout_state: false,
            pmt_timeout_state: HashMap::new(),
            cat_timeout_state: false,
            ..Self::default()
        }
    }

    /// Get a filtered version with only Priority 1 errors
    pub fn priority_1_only(&self) -> Self {
        Self {
            // Priority 1 errors
            sync_byte_errors: self.sync_byte_errors,
            ts_sync_loss: self.ts_sync_loss,
            transport_error_indicator: self.transport_error_indicator,
            pat_crc_errors: self.pat_crc_errors,
            pat_timeout: self.pat_timeout,
            continuity_counter_errors: self.continuity_counter_errors,
            pmt_crc_errors: self.pmt_crc_errors,
            pmt_timeout: self.pmt_timeout,
            pid_errors: self.pid_errors,

            // Zero out Priority 2 and 3
            pcr_repetition_errors: 0,
            pcr_accuracy_errors: 0,
            null_packet_rate_errors: 0,
            cat_crc_errors: 0,
            cat_timeout: 0,
            pat_version_changes: 0,
            pmt_version_changes: 0,
            pts_errors: 0,
            service_id_mismatch: 0,
            nit_crc_errors: 0,
            nit_timeout: 0,
            sdt_crc_errors: 0,
            sdt_timeout: 0,
            eit_crc_errors: 0,
            eit_timeout: 0,
            tdt_timeout: 0,
            splice_count_errors: 0,

            // Keep internal state
            last_pat_seen: self.last_pat_seen,
            last_pmt_seen: self.last_pmt_seen.clone(),
            last_cc: self.last_cc.clone(),
            pat_versions: self.pat_versions.clone(),
            pmt_versions: self.pmt_versions.clone(),
            last_pcr_info: self.last_pcr_info.clone(),
            bytes_in_1s: self.bytes_in_1s,
            null_bytes_in_1s: self.null_bytes_in_1s,
            last_rate_check: self.last_rate_check,
            last_cat_seen: self.last_cat_seen,
            last_nit_seen: self.last_nit_seen,
            last_sdt_seen: self.last_sdt_seen,
            last_eit_seen: self.last_eit_seen,
            last_tdt_seen: self.last_tdt_seen,
            last_splice_value: self.last_splice_value,
            startup_time: self.startup_time,
            pat_timeout_state: self.pat_timeout_state,
            pmt_timeout_state: self.pmt_timeout_state.clone(),
            cat_timeout_state: self.cat_timeout_state,
            known_pids: self.known_pids.clone(),
            last_pts_per_pid: self.last_pts_per_pid.clone(),
            sync_loss_counter: self.sync_loss_counter,
        }
    }

    /// Get a filtered version with Priority 1+2 errors only
    pub fn priority_1_and_2_only(&self) -> Self {
        Self {
            // Priority 1 errors
            sync_byte_errors: self.sync_byte_errors,
            ts_sync_loss: self.ts_sync_loss,
            transport_error_indicator: self.transport_error_indicator,
            pat_crc_errors: self.pat_crc_errors,
            pat_timeout: self.pat_timeout,
            continuity_counter_errors: self.continuity_counter_errors,
            pmt_crc_errors: self.pmt_crc_errors,
            pmt_timeout: self.pmt_timeout,
            pid_errors: self.pid_errors,

            // Priority 2 errors
            pcr_repetition_errors: self.pcr_repetition_errors,
            pcr_accuracy_errors: self.pcr_accuracy_errors,
            null_packet_rate_errors: self.null_packet_rate_errors,
            cat_crc_errors: self.cat_crc_errors,
            cat_timeout: self.cat_timeout,
            pat_version_changes: self.pat_version_changes,
            pmt_version_changes: self.pmt_version_changes,
            pts_errors: self.pts_errors,

            // Zero out Priority 3
            service_id_mismatch: 0,
            nit_crc_errors: 0,
            nit_timeout: 0,
            sdt_crc_errors: 0,
            sdt_timeout: 0,
            eit_crc_errors: 0,
            eit_timeout: 0,
            tdt_timeout: 0,
            splice_count_errors: 0,

            // Keep internal state
            last_pat_seen: self.last_pat_seen,
            last_pmt_seen: self.last_pmt_seen.clone(),
            last_cc: self.last_cc.clone(),
            pat_versions: self.pat_versions.clone(),
            pmt_versions: self.pmt_versions.clone(),
            last_pcr_info: self.last_pcr_info.clone(),
            bytes_in_1s: self.bytes_in_1s,
            null_bytes_in_1s: self.null_bytes_in_1s,
            last_rate_check: self.last_rate_check,
            last_cat_seen: self.last_cat_seen,
            last_nit_seen: self.last_nit_seen,
            last_sdt_seen: self.last_sdt_seen,
            last_eit_seen: self.last_eit_seen,
            last_tdt_seen: self.last_tdt_seen,
            last_splice_value: self.last_splice_value,
            startup_time: self.startup_time,
            pat_timeout_state: self.pat_timeout_state,
            pmt_timeout_state: self.pmt_timeout_state.clone(),
            cat_timeout_state: self.cat_timeout_state,
            known_pids: self.known_pids.clone(),
            last_pts_per_pid: self.last_pts_per_pid.clone(),
            sync_loss_counter: self.sync_loss_counter,
        }
    }

    /// Check for PAT version change (Priority 2)
    pub fn check_pat_version_change(&mut self, program_number: u16, new_version: u8, priority_level: crate::types::AnalysisMode) -> bool {
        if !matches!(priority_level, crate::types::AnalysisMode::Tr101 | crate::types::AnalysisMode::Tr101Priority12) {
            return false;
        }

        match self.pat_versions.get(&program_number) {
            Some(&old_version) => {
                if old_version != new_version {
                    self.pat_version_changes = self.pat_version_changes.saturating_add(1);
                    self.pat_versions.insert(program_number, new_version);
                    true
                } else {
                    false
                }
            }
            None => {
                // First time seeing this program, store version
                self.pat_versions.insert(program_number, new_version);
                false
            }
        }
    }

    /// Check for PMT version change (Priority 2)
    pub fn check_pmt_version_change(&mut self, pmt_pid: u16, new_version: u8, priority_level: crate::types::AnalysisMode) -> bool {
        if !matches!(priority_level, crate::types::AnalysisMode::Tr101 | crate::types::AnalysisMode::Tr101Priority12) {
            return false;
        }

        match self.pmt_versions.get(&pmt_pid) {
            Some(&old_version) => {
                if old_version != new_version {
                    self.pmt_version_changes = self.pmt_version_changes.saturating_add(1);
                    self.pmt_versions.insert(pmt_pid, new_version);
                    true
                } else {
                    false
                }
            }
            None => {
                // First time seeing this PMT, store version
                self.pmt_versions.insert(pmt_pid, new_version);
                false
            }
        }
    }

    /// Check for TS sync loss (Priority 1)
    pub fn check_ts_sync_loss(&mut self, sync_byte_valid: bool, priority_level: crate::types::AnalysisMode) {
        if !matches!(priority_level, crate::types::AnalysisMode::Tr101 | crate::types::AnalysisMode::Tr101Priority12 | crate::types::AnalysisMode::Tr101Priority1) {
            return;
        }

        if sync_byte_valid {
            // Reset sync loss counter on valid sync
            self.sync_loss_counter = 0;
        } else {
            // Increment sync loss counter
            self.sync_loss_counter = self.sync_loss_counter.saturating_add(1);

            // After consecutive sync losses, count as TS sync loss
            if self.sync_loss_counter >= SYNC_LOSS_THRESHOLD {
                self.ts_sync_loss = self.ts_sync_loss.saturating_add(1);
            }
        }
    }

    /// Check for PID errors (Priority 1)
    /// Only flags truly invalid PIDs per TR 101 290 spec, not undeclared PIDs
    pub fn check_pid_error(&mut self, pid: u16, priority_level: crate::types::AnalysisMode) {
        if !matches!(priority_level, crate::types::AnalysisMode::Tr101 | crate::types::AnalysisMode::Tr101Priority12 | crate::types::AnalysisMode::Tr101Priority1) {
            return;
        }

        // Allow system PIDs (PAT, CAT, NIT, SDT, EIT, TDT, etc.)
        if SYSTEM_PIDS.contains(&pid) {
            self.known_pids.insert(pid);
            return;
        }

        // Allow null packets
        if pid == 0x1FFF {
            return;
        }

        // Allow PIDs declared in PMT
        if self.known_pids.contains(&pid) {
            return;
        }

        // Per TR 101 290, we should only flag PIDs that are:
        // 1. Reserved (0x0002-0x000F except those in SYSTEM_PIDS)
        // 2. Invalid range (> 0x1FFE)
        // We don't flag undeclared PIDs as errors since they may be legitimate
        // private data, stuffing, or services we haven't parsed yet

        // Flag reserved PIDs that shouldn't be used
        if (0x0002..=0x000F).contains(&pid) && !SYSTEM_PIDS.contains(&pid) {
            self.pid_errors = self.pid_errors.saturating_add(1);
            return;
        }

        // Flag invalid PID range (should never happen with 13-bit PID, but check anyway)
        if pid > 0x1FFE {
            self.pid_errors = self.pid_errors.saturating_add(1);
        }
    }

    /// Register a PID as known/authorized (called from PMT processing)
    pub fn register_known_pid(&mut self, pid: u16) {
        self.known_pids.insert(pid);
    }

    /// Check for PTS errors (Priority 2)
    pub fn check_pts_error(&mut self, pid: u16, pts: u64, priority_level: crate::types::AnalysisMode) {
        if !matches!(priority_level, crate::types::AnalysisMode::Tr101 | crate::types::AnalysisMode::Tr101Priority12) {
            return;
        }

        if let Some(&last_pts) = self.last_pts_per_pid.get(&pid) {
            // Check for PTS discontinuity (backward jump or too large forward jump)
            use crate::constants::MAX_PTS_JUMP;

            if pts < last_pts {
                // Backward PTS jump (unless it's a wrap-around)
                use crate::constants::PTS_WRAP_THRESHOLD;
                let pts_diff = last_pts - pts;

                // If the difference is large, it might be a wrap-around
                if pts_diff < PTS_WRAP_THRESHOLD / 2 {
                    self.pts_errors = self.pts_errors.saturating_add(1);
                }
            } else {
                let pts_diff = pts - last_pts;
                // Check for too large forward jump
                if pts_diff > MAX_PTS_JUMP {
                    self.pts_errors = self.pts_errors.saturating_add(1);
                }
            }
        }

        self.last_pts_per_pid.insert(pid, pts);
    }

    /// Optimized packet handler with context structs (eliminates 'too many arguments' warning)
    pub fn on_packet_with_context(
        &mut self,
        packet_ctx: PacketContext,
        crc_validation: CrcValidation,
    ) {
        use crate::constants::*;

        // Basic packet validation
        if packet_ctx.chunk.len() != TS_PACKET_SIZE {
            return; // Invalid packet size
        }

        /* ───── 1.1 sync byte ───── */
        if packet_ctx.chunk[0] != TS_SYNC_BYTE {
            self.sync_byte_errors = self.sync_byte_errors.saturating_add(1);
            return;
        }

        /* ───── 1.2 TEI flag ───── */
        if packet_ctx.chunk[1] & 0x80 != 0 {
            self.transport_error_indicator = self.transport_error_indicator.saturating_add(1);
        }

        /* ───── 1.4 continuity-counter ───── */
        // Skip continuity counter check for null packets (PID 0x1FFF)
        if packet_ctx.pid != 0x1FFF {
            let cc = packet_ctx.chunk[3] & 0x0F;
            let adaptation_field_control = (packet_ctx.chunk[3] & 0x30) >> 4;

            // CC should increment for packets with payload or adaptation field
            // Only skip CC check for adaptation field only packets (0b10)
            let should_increment_cc = adaptation_field_control != 0b10;

            if let Some(prev) = self.last_cc.insert(packet_ctx.pid, cc) {
                if should_increment_cc && ((prev + 1) & 0x0F) != cc {
                    self.continuity_counter_errors = self.continuity_counter_errors.saturating_add(1);
                }
            }
        }

        /* ───── PAT / PMT handling ───── */
        let now = Instant::now();
        if packet_ctx.pid == packet_ctx.pat_pid {
            if let Some(ok) = crc_validation.pat_crc_ok {
                if !ok {
                    self.pat_crc_errors = self.pat_crc_errors.saturating_add(1);
                }
            }
            self.last_pat_seen = Some(now);
        } else if let Some(ok) = crc_validation.pmt_crc_ok {
            if !ok {
                self.pmt_crc_errors = self.pmt_crc_errors.saturating_add(1);
            }
            self.last_pmt_seen.insert(packet_ctx.pid, now);
        }

        /* time-outs - increment only on state transitions */
        if let Some(start_time) = self.startup_time {
            if start_time.elapsed() > Duration::from_millis(1000) {
                // Check PAT timeout
                let was_timeout = self.pat_timeout_state;
                let is_timeout = self.last_pat_seen.is_none_or(|last|
                    last.elapsed() > Duration::from_millis(PAT_TIMEOUT_MS)
                );
                if is_timeout && !was_timeout {
                    self.pat_timeout = self.pat_timeout.saturating_add(1);
                }
                self.pat_timeout_state = is_timeout;

                // Check PMT timeouts for all known PMT PIDs
                for (&pmt_pid, &last_seen) in &self.last_pmt_seen {
                    let was_timeout = self.pmt_timeout_state.get(&pmt_pid).unwrap_or(&false);
                    let is_timeout = last_seen.elapsed() > Duration::from_millis(PMT_TIMEOUT_MS);
                    if is_timeout && !was_timeout {
                        self.pmt_timeout = self.pmt_timeout.saturating_add(1);
                    }
                    self.pmt_timeout_state.insert(pmt_pid, is_timeout);
                }
            }
        }

        /* ───── PCR checks (2.4 / 2.5) - Priority 2 ───── */
        if matches!(packet_ctx.priority_level, crate::types::AnalysisMode::Tr101 | crate::types::AnalysisMode::Tr101Priority12) {
            if let Some((base, ext)) = packet_ctx.pcr_opt {
                // Validate PCR values are within spec
                if base > (1u64 << 33) || ext > 299 {
                    // Invalid PCR values, skip processing
                    return;
                }

                // PCR base is in 90kHz units, extension in 27MHz units
                // Convert to full 27MHz ticks: base * 300 + extension
                let pcr_ticks = base.saturating_mul(300).saturating_add(ext as u64);

                match self.last_pcr_info.get_mut(&packet_ctx.pid) {
                    None => {
                        self.last_pcr_info.insert(packet_ctx.pid, (pcr_ticks, now));
                    }
                    Some((prev_ticks, prev_time)) => {
                        let wall_delta = prev_time.elapsed();

                        // Handle PCR wrap-around (33-bit counter wraps every ~26.5 hours)
                        const PCR_WRAP: u64 = (1u64 << 33) * 300; // PCR wraps at 2^33 in 90kHz units
                        let ticks_delta = if pcr_ticks >= *prev_ticks {
                            pcr_ticks - *prev_ticks
                        } else {
                            // Handle wrap-around
                            (PCR_WRAP - *prev_ticks) + pcr_ticks
                        };

                        /* 2.4 repetition check */
                        if wall_delta.as_millis() as u64 > PCR_REPETITION_MS {
                            self.pcr_repetition_errors = self.pcr_repetition_errors.saturating_add(1);
                        }

                        /* 2.5 accuracy check */
                        // Only check accuracy if wall_delta is reasonable (100ms to 1000ms)
                        // Using larger windows reduces false positives from network jitter
                        let wall_ms = wall_delta.as_millis() as u64;
                        if (100..=1000).contains(&wall_ms) {
                            let expected_ticks = (wall_delta.as_secs_f64() * PCR_CLOCK_HZ).round() as u64;

                            // Only check accuracy if ticks_delta is reasonable (avoid wrap-around issues)
                            if ticks_delta < expected_ticks * 2 {
                                let error = if ticks_delta > expected_ticks {
                                    ticks_delta - expected_ticks
                                } else {
                                    expected_ticks - ticks_delta
                                };

                                // Calculate error rate (ppm - parts per million)
                                let error_ppm = (error as f64 / expected_ticks as f64) * 1_000_000.0;

                                // Flag only if error exceeds threshold AND error rate is significant
                                // This prevents false positives from small timing variations
                                if error > PCR_ACCURACY_TICKS && error_ppm > 100.0 {
                                    self.pcr_accuracy_errors = self.pcr_accuracy_errors.saturating_add(1);
                                }
                            }
                        }

                        *prev_ticks = pcr_ticks;
                        *prev_time = now;
                    }
                }
            }
        }

        /* ───── byte rate / null packet rate check (2.6) ───── */
        self.bytes_in_1s += packet_ctx.chunk.len() as u64;
        if packet_ctx.pid == 0x1FFF {
            self.null_bytes_in_1s += packet_ctx.chunk.len() as u64;
        }

        if let Some(last_check) = self.last_rate_check {
            if last_check.elapsed() >= Duration::from_secs(1) {
                let total = self.bytes_in_1s;
                let null_bytes = self.null_bytes_in_1s;
                if total > 0 {
                    let rate = null_bytes as f64 / total as f64;

                    // Only increment error counter if we're monitoring Priority 2+
                    if matches!(packet_ctx.priority_level, crate::types::AnalysisMode::Tr101 | crate::types::AnalysisMode::Tr101Priority12) && rate > NULL_RATE_THRESHOLD {
                        self.null_packet_rate_errors = self.null_packet_rate_errors.saturating_add(1);
                    }
                }

                // Reset counters and update timestamp
                self.bytes_in_1s = 0;
                self.null_bytes_in_1s = 0;
                self.last_rate_check = Some(now);
            }
        } else {
            self.last_rate_check = Some(now);
        }

        /* ───── CAT / NIT / SDT / EIT timeout and CRC errors ───── */
        if matches!(packet_ctx.priority_level, crate::types::AnalysisMode::Tr101 | crate::types::AnalysisMode::Tr101Priority12) && packet_ctx.pid == 0x0001 {          // CAT
            if let Some(ok) = crc_validation.cat_crc_ok {
                if !ok {
                    self.cat_crc_errors = self.cat_crc_errors.saturating_add(1);
                }
            }
            self.last_cat_seen = Some(now);
        }

        /* ───── NIT / SDT / EIT / TDT detection - Priority 3 ───── */
        if matches!(packet_ctx.priority_level, crate::types::AnalysisMode::Tr101) {
            match packet_ctx.pid {
                0x0010 => {          // NIT
                    if let Some(ok) = crc_validation.nit_crc_ok { if !ok { self.nit_crc_errors += 1; } }
                    self.last_nit_seen = Some(now);
                }
                0x0011 => {          // SDT / EIT
                    if packet_ctx.table_id == 0x42 || packet_ctx.table_id == 0x46 { // SDT
                        if let Some(ok) = crc_validation.sdt_crc_ok { if !ok { self.sdt_crc_errors += 1; } }
                        self.last_sdt_seen = Some(now);
                    } else if packet_ctx.table_id == 0x4E || packet_ctx.table_id == 0x4F { // EIT p/f
                        if let Some(ok) = crc_validation.eit_crc_ok { if !ok { self.eit_crc_errors += 1; } }
                        self.last_eit_seen = Some(now);
                    } else if packet_ctx.table_id == 0x70 || packet_ctx.table_id == 0x73 { // TDT/TOT
                        self.last_tdt_seen = Some(now);
                    }
                }
                _ => {}
            }
        }

        /* ───── NIT/SDT/EIT/TDT timeouts - Priority 3 ───── */
        if matches!(packet_ctx.priority_level, crate::types::AnalysisMode::Tr101) {
            if self.last_nit_seen.is_none_or(|t| t.elapsed()
                    > Duration::from_millis(NIT_TIMEOUT_MS)) {
                self.nit_timeout += 1;
                self.last_nit_seen = Some(now);
            }
            if self.last_sdt_seen.is_none_or(|t| t.elapsed()
                    > Duration::from_millis(SDT_TIMEOUT_MS)) {
                self.sdt_timeout += 1;
                self.last_sdt_seen = Some(now);
            }
            if self.last_eit_seen.is_none_or(|t| t.elapsed()
                    > Duration::from_millis(EIT_TIMEOUT_MS)) {
                self.eit_timeout += 1;
                self.last_eit_seen = Some(now);
            }
            if self.last_tdt_seen.is_none_or(|t| t.elapsed()
                    > Duration::from_millis(TDT_TIMEOUT_MS)) {
                self.tdt_timeout += 1;
                self.last_tdt_seen = Some(now);
            }
        }
    }
}