use clap::Parser;
use mpegts_inspector::inspector::{Options, run, AnalysisMode};

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

    /// TR 101 290 priority level (1, 12, or all). Only used when analysis is enabled.
    #[clap(long, default_value = "12")]
    tr101_priority: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    let analysis_mode = if opt.no_analysis {
        None
    } else {
        match opt.tr101_priority.as_str() {
            "1" => Some(AnalysisMode::Tr101Priority1),
            "12" => Some(AnalysisMode::Tr101Priority12),
            "all" => Some(AnalysisMode::Tr101),
            _ => {
                eprintln!("Invalid TR 101 priority level: '{}'. Use '1', '12', or 'all'", opt.tr101_priority);
                std::process::exit(1);
            }
        }
    };

    run(Options {
        addr: opt.addr.parse()?,
        refresh_secs: opt.refresh,
        analysis_mode,
    })
    .await
}