// src/tr101.rs
use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Serialize;

/// 27 MHz clock: 27 000 000 ticks / second
const PCR_CLOCK_HZ: f64 = 27_000_000.0;
/// ±500 ns in PCR ticks  →  27 000 000 * 5e-7  ≈ 13.5
const PCR_ACCURACY_TICKS: u64 = 27;
/// Repetition threshold 40 ms
const PCR_REPETITION_MS: u64 = 100;

const NULL_RATE_THRESHOLD: f64 = 0.15;          // 10 %
const CAT_TIMEOUT_MS:  u64 = 2000;   // 2 s
const NIT_TIMEOUT_MS:  u64 = 2000;   // 2 s
const SDT_TIMEOUT_MS:  u64 = 2000;   // 2 s
const EIT_TIMEOUT_MS:  u64 = 2000;   // 2 s
const TDT_TIMEOUT_MS:  u64 = 2000;   // 2 s

#[derive(Default, Debug, Clone,Serialize)]
pub struct Tr101Metrics {
    // Priority-1 counters
    pub sync_byte_errors:            u64, // 1.1
    pub transport_error_indicator:   u64, // 1.2
    pub pat_crc_errors:              u64, // 1.3a
    pub pat_timeout:                 u64, // 1.3b
    pub continuity_counter_errors:   u64, // 1.4
    pub pmt_crc_errors:              u64, // 1.5a
    pub pmt_timeout:                 u64, // 1.5b

    /* ───────── Priority-2 (new) ───────── */
    pub pcr_repetition_errors:       u64, // 2.4
    pub pcr_accuracy_errors:         u64, // 2.5     
    pub null_packet_rate_errors:    u64, // 2.6
    pub cat_crc_errors:             u64, // 2.7a
    pub cat_timeout:                u64, // 2.7b
 
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
}

impl Tr101Metrics {
    pub fn new() -> Self {
        Self {
            last_rate_check: None,
            ..Self::default()
        }
    }

    /// Get a filtered version with only Priority 1 errors
    pub fn priority_1_only(&self) -> Self {
        Self {
            // Priority 1 errors
            sync_byte_errors: self.sync_byte_errors,
            transport_error_indicator: self.transport_error_indicator,
            pat_crc_errors: self.pat_crc_errors,
            pat_timeout: self.pat_timeout,
            continuity_counter_errors: self.continuity_counter_errors,
            pmt_crc_errors: self.pmt_crc_errors,
            pmt_timeout: self.pmt_timeout,

            // Zero out Priority 2 and 3
            pcr_repetition_errors: 0,
            pcr_accuracy_errors: 0,
            null_packet_rate_errors: 0,
            cat_crc_errors: 0,
            cat_timeout: 0,
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
        }
    }

    /// Get a filtered version with Priority 1+2 errors only
    pub fn priority_1_and_2_only(&self) -> Self {
        Self {
            // Priority 1 errors
            sync_byte_errors: self.sync_byte_errors,
            transport_error_indicator: self.transport_error_indicator,
            pat_crc_errors: self.pat_crc_errors,
            pat_timeout: self.pat_timeout,
            continuity_counter_errors: self.continuity_counter_errors,
            pmt_crc_errors: self.pmt_crc_errors,
            pmt_timeout: self.pmt_timeout,

            // Priority 2 errors
            pcr_repetition_errors: self.pcr_repetition_errors,
            pcr_accuracy_errors: self.pcr_accuracy_errors,
            null_packet_rate_errors: self.null_packet_rate_errors,
            cat_crc_errors: self.cat_crc_errors,
            cat_timeout: self.cat_timeout,

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
        }
    }

    pub fn on_packet(
        &mut self,
        chunk: &[u8],
        pid: u16,
        payload_unit_start: bool,
        pat_pid: u16,
        is_pat_crc_ok: Option<bool>,
        is_pmt_crc_ok: Option<bool>,
        /* PCR tuple for priority-2 checks */
        pcr_opt: Option<(u64, u16)>,          // (base, extension)

        /* priority-2/3 table flags                    */
        cat_crc_ok: Option<bool>,
        nit_crc_ok: Option<bool>,
        sdt_crc_ok: Option<bool>,
        eit_crc_ok: Option<bool>,

        /* last parsed table_id (needed to know SDT vs EIT vs TDT) */
        table_id: u8,

        /* priority level for selective error reporting */
        priority_level: crate::types::AnalysisMode,
    ) {
        /* ───── 1.1 sync byte ───── */
        if chunk[0] != 0x47 {
            self.sync_byte_errors += 1;
            return;
        }

        /* ───── 1.2 TEI flag ───── */
        if chunk[1] & 0x80 != 0 {
            self.transport_error_indicator += 1;
        }

        /* ───── 1.4 continuity-counter ───── */
        let cc = chunk[3] & 0x0F;
        if let Some(prev) = self.last_cc.insert(pid, cc) {
            let has_payload = (chunk[3] & 0x10) != 0;
            if has_payload && ((prev + 1) & 0x0F) != cc {
                self.continuity_counter_errors += 1;
            }
        }       

        /* ───── PAT / PMT handling ───── */
        let now = Instant::now();
        if pid == pat_pid {
            if let Some(ok) = is_pat_crc_ok {
                if !ok { self.pat_crc_errors += 1; }
            }
            self.last_pat_seen = Some(now);
        } else if let Some(ok) = is_pmt_crc_ok {
            if !ok { self.pmt_crc_errors += 1; }
            self.last_pmt_seen.insert(pid, now);
        }

        /* time-outs */
        if let Some(last) = self.last_pat_seen {
            if last.elapsed() > Duration::from_millis(500) {
                self.pat_timeout += 1;
                self.last_pat_seen = Some(now);
            }
        }
        for last in self.last_pmt_seen.values_mut() {
            if last.elapsed() > Duration::from_secs(1) {
                self.pmt_timeout += 1;
                *last = now;
            }
        }

        /* ───── PCR checks (2.4 / 2.5) - Priority 2 ───── */
        if matches!(priority_level, crate::types::AnalysisMode::Tr101 | crate::types::AnalysisMode::Tr101Priority12) {
            if let Some((base, ext)) = pcr_opt {
                let pcr_ticks = base * 300 + ext as u64; // full 27 MHz ticks
                match self.last_pcr_info.get_mut(&pid) {
                    None => {
                        self.last_pcr_info.insert(pid, (pcr_ticks, now));
                    }
                    Some((prev_ticks, prev_time)) => {
                        let wall_delta = prev_time.elapsed();
                        let ticks_delta = pcr_ticks.wrapping_sub(*prev_ticks); // handle wrap

                        /* 2.4 repetition */
                        if wall_delta.as_millis() as u64 > PCR_REPETITION_MS {
                            self.pcr_repetition_errors += 1;
                        }

                        /* 2.5 accuracy */
                        let expected_ticks =
                            (wall_delta.as_secs_f64() * PCR_CLOCK_HZ).round() as i64;
                        let error = ticks_delta as i64 - expected_ticks;
                        if error.unsigned_abs() > PCR_ACCURACY_TICKS {
                            self.pcr_accuracy_errors += 1;
                        }

                        /* update state */
                        *prev_ticks = pcr_ticks;
                        *prev_time = now;
                    }
                }
            }
        }

        /* ───── null-packet rate counting - Priority 2 ───── */
        if matches!(priority_level, crate::types::AnalysisMode::Tr101 | crate::types::AnalysisMode::Tr101Priority12) {
            self.bytes_in_1s += 188;
            if pid == 0x1FFF { self.null_bytes_in_1s += 188; }

            let now = Instant::now();
            if let Some(last_check) = self.last_rate_check {
                if now.duration_since(last_check) >= Duration::from_secs(1) {
                if self.bytes_in_1s > 0 {
                    let rate = self.null_bytes_in_1s as f64 / self.bytes_in_1s as f64;
                    if rate > NULL_RATE_THRESHOLD {
                        self.null_packet_rate_errors += 1;
                    }
                }
                self.bytes_in_1s = 0;
                    self.last_rate_check = Some(now);
                }
                self.last_rate_check = Some(now);
            }
        }

        /* ───── CAT detection - Priority 2 ───── */
        if matches!(priority_level, crate::types::AnalysisMode::Tr101 | crate::types::AnalysisMode::Tr101Priority12) {
            if pid == 0x0001 {          // CAT
                if let Some(ok) = cat_crc_ok {
                    if !ok { self.cat_crc_errors += 1; }
                }
                self.last_cat_seen = Some(now);
            }
        }

        /* ───── NIT / SDT / EIT / TDT detection - Priority 3 ───── */
        if matches!(priority_level, crate::types::AnalysisMode::Tr101) {
            match pid {
                0x0010 => {          // NIT
                    if let Some(ok) = nit_crc_ok { if !ok { self.nit_crc_errors += 1; } }
                    self.last_nit_seen = Some(now);
                }
                0x0011 => {          // SDT, BAT, EIT, RST, TDT/TOT share 0x11
                    if table_id == 0x42 || table_id == 0x46 {      // SDT actual/other
                        if let Some(ok) = sdt_crc_ok { if !ok { self.sdt_crc_errors += 1; } }
                        self.last_sdt_seen = Some(now);
                    } else if table_id == 0x4E || table_id == 0x4F { // EIT p/f
                        if let Some(ok) = eit_crc_ok { if !ok { self.eit_crc_errors += 1; } }
                        self.last_eit_seen = Some(now);
                    } else if table_id == 0x70 || table_id == 0x73 { // TDT/TOT
                        self.last_tdt_seen = Some(now);
                    }
                }
                _ => {}
            }
        }

        /* ───── CAT timeout - Priority 2 ───── */
        if matches!(priority_level, crate::types::AnalysisMode::Tr101 | crate::types::AnalysisMode::Tr101Priority12) {
            if self.last_cat_seen.map_or(true, |t| t.elapsed()
                    > Duration::from_millis(CAT_TIMEOUT_MS)) {
                self.cat_timeout += 1;
                self.last_cat_seen = Some(now);
            }
        }

        /* ───── NIT/SDT/EIT/TDT timeouts - Priority 3 ───── */
        if matches!(priority_level, crate::types::AnalysisMode::Tr101) {
            if self.last_nit_seen.map_or(true, |t| t.elapsed()
                    > Duration::from_millis(NIT_TIMEOUT_MS)) {
                self.nit_timeout += 1;
                self.last_nit_seen = Some(now);
            }
            if self.last_sdt_seen.map_or(true, |t| t.elapsed()
                    > Duration::from_millis(SDT_TIMEOUT_MS)) {
                self.sdt_timeout += 1;
                self.last_sdt_seen = Some(now);
            }
            if self.last_eit_seen.map_or(true, |t| t.elapsed()
                    > Duration::from_millis(EIT_TIMEOUT_MS)) {
                self.eit_timeout += 1;
                self.last_eit_seen = Some(now);
            }
            if self.last_tdt_seen.map_or(true, |t| t.elapsed()
                    > Duration::from_millis(TDT_TIMEOUT_MS)) {
                self.tdt_timeout += 1;
                self.last_tdt_seen = Some(now);
            }
        }
    }
}