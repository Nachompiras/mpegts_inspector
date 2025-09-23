// src/lib.rs
pub mod inspector {
    use std::net::SocketAddr;

    pub struct Options {
        pub addr: SocketAddr,
        pub refresh_secs: u64,
        pub analysis: bool
    }

    /// Analysis modes for different levels of processing
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum AnalysisMode {
        /// Basic stream detection only (codec, bitrate, basic metadata)
        Mux,
        /// Full TR 101 290 compliance analysis
        Tr101,
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

    /// Async entry-point; returns when stopped (Ctrl-C or socket error)
    pub async fn run(opts: Options) -> anyhow::Result<()> {
        crate::core::run(opts).await
    }

    /// Entry-point that reads TS packets from a `tokio::broadcast` channel.
    pub async fn run_from_broadcast(
        mut rx: tokio::sync::broadcast::Receiver<Vec<u8>>,
        refresh_secs: u64,
        analysis: bool,                 // ← legacy flag for compatibility
    ) -> anyhow::Result<()> {
        crate::core::run_broadcast(&mut rx, refresh_secs, analysis).await
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

mod psi;
mod es;
mod core;
mod tr101;
mod si_cache;
