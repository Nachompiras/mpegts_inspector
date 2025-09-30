//! Constants for MPEG-TS processing and TR 101 290 compliance

/// MPEG-TS packet constants
pub const TS_PACKET_SIZE: usize = 188;
pub const TS_SYNC_BYTE: u8 = 0x47;

/// PES packet constants
pub const PES_START_CODE: [u8; 3] = [0x00, 0x00, 0x01];

/// PCR constants
pub const PCR_CLOCK_HZ: f64 = 27_000_000.0; // 27 MHz
pub const PCR_REPETITION_MS: u64 = 100; // TR 101 290: Maximum 100ms between PCR
pub const PCR_WRAP_THRESHOLD: u64 = (1u64 << 33) * 300; // PCR wrap-around point

/// PTS constants
pub const PTS_CLOCK_HZ: u64 = 90_000; // 90 kHz
pub const PTS_WRAP_THRESHOLD: u64 = 1u64 << 33; // 33-bit PTS counter
pub const MAX_PTS_JUMP_SECONDS: u64 = 60; // Maximum allowed PTS jump in seconds (increased for ad insertion/splicing)
pub const MAX_PTS_JUMP: u64 = PTS_CLOCK_HZ * MAX_PTS_JUMP_SECONDS;

/// TR 101 290 timeout constants (in milliseconds)
pub const PAT_TIMEOUT_MS: u64 = 500;   // 500ms
pub const PMT_TIMEOUT_MS: u64 = 500;   // 500ms
pub const CAT_TIMEOUT_MS: u64 = 2000;  // 2s
pub const NIT_TIMEOUT_MS: u64 = 2000;  // 2s
pub const SDT_TIMEOUT_MS: u64 = 2000;  // 2s
pub const EIT_TIMEOUT_MS: u64 = 2000;  // 2s
pub const TDT_TIMEOUT_MS: u64 = 2000;  // 2s

/// TR 101 290 thresholds
pub const NULL_RATE_THRESHOLD: f64 = 0.2; // 20% null packet rate threshold
pub const SYNC_LOSS_THRESHOLD: u64 = 5;   // Consecutive sync losses before error
pub const STREAM_TIMEOUT_SECONDS: u64 = 30; // Stream inactivity timeout

/// System PIDs that are always allowed
pub const SYSTEM_PIDS: &[u16] = &[
    0x0000, // PAT
    0x0001, // CAT
    0x0010, // NIT
    0x0011, // SDT/BAT/EIT
    0x0012, // EIT
    0x0013, // RST/ST
    0x0014, // TDT/TOT
    0x1FFF, // Null packets
];

/// FPS calculation constants
pub const MIN_PTS_SAMPLES_FOR_FPS: usize = 3;
pub const MAX_PTS_DELTA_SECONDS: u64 = 1; // Maximum delta between PTS samples
pub const MAX_PTS_DELTA_TICKS: u64 = PTS_CLOCK_HZ * MAX_PTS_DELTA_SECONDS;