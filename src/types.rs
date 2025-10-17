use serde::Serialize;
use std::time::Instant;

/// Context for SI table processing to reduce function parameters
#[derive(Default)]
pub struct SiTableContext {
    pub pat_crc_ok: Option<bool>,
    pub pmt_crc_ok: Option<bool>,
    pub cat_crc_ok: Option<bool>,
    pub nit_crc_ok: Option<bool>,
    pub sdt_crc_ok: Option<bool>,
    pub eit_crc_ok: Option<bool>,
    pub tdt_crc_ok: Option<bool>,
    pub table_id: u8,
}

/// Context for packet processing in TR-101 analysis
pub struct PacketContext<'a> {
    pub chunk: &'a [u8],
    pub pid: u16,
    pub payload_unit_start: bool,
    pub pat_pid: u16,
    pub pcr_opt: Option<(u64, u16)>,
    pub table_id: u8,
    pub priority_level: AnalysisMode,
    pub total_bytes_processed: u64,  // Total bytes processed since start
}

/// CRC validation results for all table types
#[derive(Default)]
pub struct CrcValidation {
    pub pat_crc_ok: Option<bool>,
    pub pmt_crc_ok: Option<bool>,
    pub cat_crc_ok: Option<bool>,
    pub nit_crc_ok: Option<bool>,
    pub sdt_crc_ok: Option<bool>,
    pub eit_crc_ok: Option<bool>,
}

/// Video codec information
#[derive(Debug, Clone, Serialize)]
pub struct VideoInfo {
    pub codec: String,
    pub width: u16,
    pub height: u16,
    pub fps: f32,
    pub chroma: String,
    pub interlaced: bool,
}

/// Audio codec information
#[derive(Debug, Clone, Serialize)]
pub struct AudioInfo {
    pub codec: String,
    pub profile: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
}

/// Subtitle codec information
#[derive(Debug, Clone, Serialize)]
pub struct SubtitleInfo {
    pub codec: String,
}

/// Codec information for different stream types
#[derive(Debug, Clone, Serialize)]
pub enum CodecInfo {
    Video(VideoInfo),
    Audio(AudioInfo),
    Subtitle(SubtitleInfo),
}

/// Elementary stream information (public API)
#[derive(Debug, Clone, Serialize)]
pub struct StreamInfo {
    pub pid: u16,
    pub stream_type: u8,
    pub codec: Option<CodecInfo>,
    pub bitrate_kbps: f64,
}

/// Program information containing all its streams (public API)
#[derive(Debug, Clone, Serialize)]
pub struct ProgramInfo {
    pub program_number: u16,
    pub streams: Vec<StreamInfo>,
    /// PCR PID for this program (from PMT)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pcr_pid: Option<u16>,
    /// PMT version for change tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pmt_version: Option<u8>,
}

/// Complete inspection report with all discovered programs and TR-101 metrics
#[derive(Debug, Clone, Serialize)]
pub struct InspectorReport {
    pub timestamp: String,
    pub programs: Vec<ProgramInfo>,
    pub tr101_metrics: crate::tr101::Tr101Metrics,
}

/// Internal elementary stream statistics
pub struct EsStats {
    pub stream_type: u8,
    pub codec: Option<CodecInfo>,
    pub bytes: usize,
    pub start: Instant,
    pub last_pts: Option<u64>,
    pub pts_samples: Vec<u64>,  // Store recent PTS values for better FPS calculation
}

/// Analysis modes for different levels of processing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisMode {
    /// Basic stream detection only (codec, bitrate, basic metadata)
    Mux,
    /// Full TR 101 290 compliance analysis (all priorities)
    Tr101,
    /// TR 101 290 Priority 1 errors only (critical transport errors)
    Tr101Priority1,
    /// TR 101 290 Priority 1+2 errors (critical + recommended)
    Tr101Priority12,
    /// No analysis, raw stream detection only
    None,
}

/// Control commands for runtime analysis mode switching
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisCommand {
    Start(AnalysisMode),
    Stop,
    GetStatus,
}

/// Response from analysis control commands
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisStatus {
    pub current_mode: Option<AnalysisMode>,
    pub is_running: bool,
}

/// Configuration options for the inspector
pub struct Options {
    pub addr: std::net::SocketAddr,
    pub refresh_secs: u64,
    pub analysis_mode: Option<AnalysisMode>,
}