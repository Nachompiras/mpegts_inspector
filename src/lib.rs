//! MPEG-TS Inspector Library
//!
//! This library provides async MPEG-TS packet inspection capabilities
//! for UDP transport streams (unicast or multicast). It parses PAT/PMT
//! tables on-the-fly and provides live statistics for elementary streams.

// Internal modules
mod types;
mod network;
mod parsers;
mod stats;
mod report;
mod processor;
mod psi;
mod tr101;
mod si_cache;

// Public API module
pub mod inspector {
    // Re-export public types
    pub use crate::types::{
        VideoInfo, AudioInfo, SubtitleInfo, CodecInfo, StreamInfo,
        ProgramInfo, InspectorReport, AnalysisMode, AnalysisCommand,
        AnalysisStatus, Options
    };

    /// Async entry-point; returns when stopped (Ctrl-C or socket error)
    pub async fn run(opts: Options) -> anyhow::Result<()> {
        crate::core::run(opts).await
    }

    /// Entry-point that reads TS packets from a broadcast channel and provides structured data via callback.
    pub async fn run_from_broadcast<F>(
        mut rx: tokio::sync::broadcast::Receiver<Vec<u8>>,
        refresh_secs: u64,
        analysis: bool,
        mut callback: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(InspectorReport) + Send,
    {
        crate::core::run_broadcast(&mut rx, refresh_secs, analysis, &mut callback).await
    }

    /// Advanced broadcast entry-point with runtime analysis control
    pub async fn run_from_broadcast_with_control(
        mut rx: tokio::sync::broadcast::Receiver<Vec<u8>>,
        mut control_rx: tokio::sync::broadcast::Receiver<AnalysisCommand>,
        refresh_secs: u64,
        initial_mode: Option<AnalysisMode>,
    ) -> anyhow::Result<()> {
        crate::core::run_broadcast_with_control(&mut rx, &mut control_rx, refresh_secs, initial_mode).await
    }
}

// Compatibility module - will be refactored
mod core;

// Re-export TR101 for backwards compatibility
pub use tr101::Tr101Metrics;
