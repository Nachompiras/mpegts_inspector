// src/lib.rs
pub mod inspector {
    use std::net::SocketAddr;

    pub struct Options {
        pub addr: SocketAddr,
        pub refresh_secs: u64,
    }

    /// Async entry-point; returns when stopped (Ctrl-C or socket error)
    pub async fn run(opts: Options) -> anyhow::Result<()> {
        crate::core::run(opts)
    }
}

mod psi;
mod es;
mod core;         // your former main.rs logic split into functions
