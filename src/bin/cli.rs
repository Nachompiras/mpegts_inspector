use clap::Parser;
use mpegts_inspector::inspector::{Options, run};

#[derive(Parser)]
struct Opt {
    /// UDP socket to bind + listen (IPv4)
    #[clap(long, default_value = "239.1.1.2:1234")]
    addr: String,

    /// Refresh interval for the JSON snapshot
    #[clap(long, default_value_t = 2)]
    refresh: u64,

    /// Disable TR 101 290 analysis (faster, fewer counters)
    #[clap(long, default_value_t = false)]
    no_analysis: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    run(Options {
        addr: opt.addr.parse()?,
        refresh_secs: opt.refresh,
        analysis: !opt.no_analysis,     // <─ just flip the bool
    })
    .await
}