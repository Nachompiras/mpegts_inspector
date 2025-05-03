// src/lib.rs
pub mod inspector {
    use std::net::SocketAddr;

    pub struct Options {
        pub addr: SocketAddr,
        pub refresh_secs: u64,
        pub analysis: bool
    }

    /// Async entry-point; returns when stopped (Ctrl-C or socket error)
    pub async fn run(opts: Options) -> anyhow::Result<()> {
        crate::core::run(opts).await
    }
     /// Entry-point that reads TS packets from a `tokio::broadcast` channel.
     pub async fn run_from_broadcast(
        mut rx: tokio::sync::broadcast::Receiver<Vec<u8>>,
        refresh_secs: u64,
        analysis: bool,                 // ← new flag
    ) -> anyhow::Result<()> {
        crate::core::run_broadcast(&mut rx, refresh_secs, analysis).await
    }   
}

mod psi;
mod es;
mod core;
mod tr101;
mod si_cache;
