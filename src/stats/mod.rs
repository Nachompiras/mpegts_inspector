//! Statistics management for elementary streams

use std::collections::HashMap;
use std::time::{Duration, Instant};
use crate::types::{EsStats, CodecInfo};

/// Manages elementary stream statistics and cleanup
pub struct StatsManager {
    pub es_stats: HashMap<u16, EsStats>,
}

impl StatsManager {
    pub fn new() -> Self {
        Self {
            es_stats: HashMap::new(),
        }
    }

    /// Add a new elementary stream to track
    pub fn add_stream(&mut self, pid: u16, stream_type: u8) {
        self.es_stats.insert(
            pid,
            EsStats {
                stream_type,
                codec: None,
                bytes: 0,
                start: Instant::now(),
                last_pts: None,
                pts_samples: Vec::new(),
            },
        );
    }

    /// Update byte count for a PID
    pub fn update_bytes(&mut self, pid: u16, bytes: usize) {
        if let Some(stats) = self.es_stats.get_mut(&pid) {
            stats.bytes += bytes;
        }
    }

    /// Set codec information for a stream
    pub fn set_codec(&mut self, pid: u16, codec: CodecInfo) {
        if let Some(stats) = self.es_stats.get_mut(&pid) {
            stats.codec = Some(codec);
        }
    }

    /// Update PTS for a stream (used for FPS calculation)
    pub fn update_pts(&mut self, pid: u16, pts: u64) {
        if let Some(stats) = self.es_stats.get_mut(&pid) {
            stats.last_pts = Some(pts);
        }
    }

    /// Get mutable reference to stream stats
    pub fn get_mut(&mut self, pid: u16) -> Option<&mut EsStats> {
        self.es_stats.get_mut(&pid)
    }

    /// Get immutable reference to stream stats
    pub fn get(&self, pid: u16) -> Option<&EsStats> {
        self.es_stats.get(&pid)
    }

    /// Check if a PID is being tracked
    pub fn contains_pid(&self, pid: u16) -> bool {
        self.es_stats.contains_key(&pid)
    }

    /// Remove old/inactive streams (older than timeout)
    pub fn cleanup_old_streams(&mut self, timeout: Duration) {
        self.es_stats.retain(|_, stats| stats.start.elapsed() < timeout);
    }

    /// Calculate bitrate for a stream in kbps
    pub fn calculate_bitrate(&self, pid: u16) -> Option<f64> {
        let stats = self.es_stats.get(&pid)?;
        let seconds = stats.start.elapsed().as_secs_f64().max(0.1);
        Some((stats.bytes as f64 * 8.0 / 1000.0) / seconds)
    }

    /// Get all tracked PIDs
    pub fn get_all_pids(&self) -> Vec<u16> {
        self.es_stats.keys().copied().collect()
    }

    /// Get iterator over all stats
    pub fn iter(&self) -> impl Iterator<Item = (&u16, &EsStats)> {
        self.es_stats.iter()
    }
}

impl Default for StatsManager {
    fn default() -> Self {
        Self::new()
    }
}