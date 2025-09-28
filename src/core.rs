//! Core inspection functionality using the new modular architecture

use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

use crate::types::{Options, InspectorReport, AnalysisMode, AnalysisCommand};
use crate::network::create_udp_socket;
use crate::processor::PacketProcessor;
use crate::report::Reporter;

/// Main entry point for UDP socket-based inspection
pub async fn run(opts: Options) -> anyhow::Result<()> {
    let socket = create_udp_socket(&opts.addr.to_string())?;
    let sock = UdpSocket::from_std(socket.into())?;

    let enable_tr101 = matches!(opts.analysis_mode, Some(AnalysisMode::Tr101) | Some(AnalysisMode::Tr101Priority1) | Some(AnalysisMode::Tr101Priority12));
    let mut processor = PacketProcessor::new(enable_tr101);
    let mut buf = [0u8; 2048];
    let mut last_print = Instant::now();

    loop {
        let n = sock.recv(&mut buf).await?;
        if n == 0 {
            continue;
        }

        // Process TS packets (188 B aligned)
        for chunk in buf[..n].chunks_exact(188) {
            if chunk[0] != 0x47 {
                continue; // bad sync
            }
            processor.process_packet(chunk, opts.analysis_mode);
        }

        // Generate periodic reports
        if last_print.elapsed() >= Duration::from_secs(opts.refresh_secs) {
            processor.cleanup_old_streams(30);

            let json = Reporter::generate_json_report(
                &processor,
                processor.get_tr101_metrics(),
                opts.analysis_mode,
            );
            println!("{json}");
            last_print = Instant::now();
        }
    }
}

/// Broadcast receiver-based inspection with structured data callback
pub async fn run_broadcast<F>(
    rx: &mut tokio::sync::broadcast::Receiver<Vec<u8>>,
    refresh_secs: u64,
    analysis: bool,
    callback: &mut F,
) -> anyhow::Result<()>
where
    F: FnMut(InspectorReport) + Send,
{
    let analysis_mode = if analysis { Some(AnalysisMode::Tr101Priority12) } else { Some(AnalysisMode::Mux) };
    let mut processor = PacketProcessor::new(analysis);
    let mut last_print = Instant::now();

    loop {
        let buf = rx.recv().await?;

        for chunk in buf.chunks_exact(188) {
            if chunk[0] != 0x47 {
                continue;
            }
            processor.process_packet(chunk, analysis_mode);
        }

        if last_print.elapsed() >= Duration::from_secs(refresh_secs) {
            processor.cleanup_old_streams(30);

            let report = Reporter::create_report(
                &processor,
                processor.get_tr101_metrics(),
                analysis_mode,
            );
            callback(report);
            last_print = Instant::now();
        }
    }
}

/// Advanced broadcast inspection with runtime analysis control
pub async fn run_broadcast_with_control(
    rx: &mut tokio::sync::broadcast::Receiver<Vec<u8>>,
    control_rx: &mut tokio::sync::broadcast::Receiver<AnalysisCommand>,
    refresh_secs: u64,
    initial_mode: Option<AnalysisMode>,
) -> anyhow::Result<()> {
    let mut processor = PacketProcessor::new(matches!(initial_mode, Some(AnalysisMode::Tr101)));
    let mut current_mode = initial_mode;
    let mut last_print = Instant::now();

    loop {
        tokio::select! {
            // Handle TS packet data
            buf_result = rx.recv() => {
                let buf = buf_result?;
                for chunk in buf.chunks_exact(188) {
                    if chunk[0] != 0x47 {
                        continue;
                    }

                    // Process packet based on current analysis mode
                    match current_mode {
                        Some(AnalysisMode::None) => {
                            // Skip all analysis except basic packet counting
                            continue;
                        },
                        Some(mode) => {
                            processor.process_packet(chunk, Some(mode));
                        },
                        None => {
                            // Analysis stopped, just consume packets
                            continue;
                        }
                    }
                }
            }

            // Handle analysis control commands
            cmd_result = control_rx.recv() => {
                match cmd_result? {
                    AnalysisCommand::Start(mode) => {
                        current_mode = Some(mode);
                        processor.set_analysis_mode(Some(mode));
                        eprintln!("Analysis mode changed to: {:?}", mode);
                    },
                    AnalysisCommand::Stop => {
                        current_mode = None;
                        eprintln!("Analysis stopped");
                    },
                    AnalysisCommand::GetStatus => {
                        let status = crate::types::AnalysisStatus {
                            current_mode,
                            is_running: current_mode.is_some(),
                        };
                        eprintln!("Analysis status: {:?}", status);
                    }
                }
            }
        }

        // Generate reports at specified intervals
        if current_mode.is_some() && last_print.elapsed() >= Duration::from_secs(refresh_secs) {
            processor.cleanup_old_streams(30);

            let json = Reporter::generate_json_report(
                &processor,
                processor.get_tr101_metrics(),
                current_mode,
            );
            println!("{json}");
            last_print = Instant::now();
        }
    }
}