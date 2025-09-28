//! Report generation for MPEG-TS inspection results

use std::collections::HashMap;
use serde::Serialize;
use crate::types::{InspectorReport, ProgramInfo, StreamInfo, CodecInfo};
use crate::stats::StatsManager;
use crate::psi::{PatSection, PmtSection};
use crate::tr101::Tr101Metrics;

/// JSON structure for elementary streams (internal serialization)
#[derive(Serialize)]
struct EsJson<'a> {
    pid: u16,
    stream_type: u8,
    codec: &'a str,
    bitrate_kbps: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    width: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    height: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fps: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chroma: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    channels: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sample_rate: Option<u32>,
}

/// JSON structure for programs (internal serialization)
#[derive(Serialize)]
struct ProgramJson<'a> {
    program: u16,
    streams: Vec<EsJson<'a>>,
}

/// JSON structure for complete report (internal serialization)
#[derive(Serialize)]
struct ReportJson<'a> {
    ts_time: String,
    programs: Vec<ProgramJson<'a>>,
    tr101: &'a Tr101Metrics,
}

/// Report generator for MPEG-TS inspection results
pub struct Reporter;

impl Reporter {
    /// Generate a structured InspectorReport for API consumers
    pub fn create_report(
        pat_map: &HashMap<u16, PatSection>,
        pmt_map: &HashMap<u16, PmtSection>,
        stats_manager: &StatsManager,
        tr101: Tr101Metrics,
        analysis_mode: Option<crate::types::AnalysisMode>,
    ) -> InspectorReport {
        let mut programs = Vec::new();

        for (prog_num, pat) in pat_map {
            if let Some(pmt_pid) = pat.programs
                .iter()
                .find(|p| p.program_number == *prog_num)
                .map(|p| p.pmt_pid)
            {
                if let Some(pmt) = pmt_map.get(&pmt_pid) {
                    let mut streams = Vec::new();
                    for s in &pmt.streams {
                        if let Some(stats) = stats_manager.get(s.elementary_pid) {
                            if let Some(bitrate_kbps) = stats_manager.calculate_bitrate(s.elementary_pid) {
                                streams.push(StreamInfo {
                                    pid: s.elementary_pid,
                                    stream_type: s.stream_type,
                                    codec: stats.codec.clone(),
                                    bitrate_kbps,
                                });
                            }
                        }
                    }
                    programs.push(ProgramInfo {
                        program_number: *prog_num,
                        streams,
                    });
                }
            }
        }

        // Filter TR-101 metrics based on analysis mode
        let filtered_tr101 = match analysis_mode {
            Some(crate::types::AnalysisMode::Tr101Priority1) => tr101.priority_1_only(),
            Some(crate::types::AnalysisMode::Tr101Priority12) => tr101.priority_1_and_2_only(),
            _ => tr101,
        };

        InspectorReport {
            timestamp: chrono::Utc::now().to_rfc3339(),
            programs,
            tr101_metrics: filtered_tr101,
        }
    }

    /// Generate pretty-printed JSON string for CLI output
    pub fn generate_json_report(
        pat_map: &HashMap<u16, PatSection>,
        pmt_map: &HashMap<u16, PmtSection>,
        stats_manager: &StatsManager,
        tr101: Tr101Metrics,
        analysis_mode: Option<crate::types::AnalysisMode>,
    ) -> String {
        let mut programs_out = Vec::new();

        for (prog_num, pat) in pat_map {
            if let Some(pmt_pid) = pat.programs
                .iter()
                .find(|p| p.program_number == *prog_num)
                .map(|p| p.pmt_pid)
            {
                if let Some(pmt) = pmt_map.get(&pmt_pid) {
                    let mut es_vec = Vec::new();
                    for s in &pmt.streams {
                        if let Some(stats) = stats_manager.get(s.elementary_pid) {
                            if let Some(bitrate_kbps) = stats_manager.calculate_bitrate(s.elementary_pid) {
                                match &stats.codec {
                                    Some(CodecInfo::Video(v)) => es_vec.push(EsJson {
                                        pid: s.elementary_pid,
                                        stream_type: s.stream_type,
                                        codec: &v.codec,
                                        bitrate_kbps,
                                        width: Some(v.width),
                                        height: Some(v.height),
                                        fps: if v.fps > 0.0 { Some(v.fps) } else { None },
                                        chroma: Some(&v.chroma),
                                        channels: None,
                                        sample_rate: None,
                                    }),
                                    Some(CodecInfo::Audio(a)) => es_vec.push(EsJson {
                                        pid: s.elementary_pid,
                                        stream_type: s.stream_type,
                                        codec: &a.codec,
                                        bitrate_kbps,
                                        width: None,
                                        height: None,
                                        fps: None,
                                        chroma: None,
                                        channels: a.channels,
                                        sample_rate: a.sample_rate,
                                    }),
                                    Some(CodecInfo::Subtitle(sub)) => es_vec.push(EsJson {
                                        pid: s.elementary_pid,
                                        stream_type: s.stream_type,
                                        codec: &sub.codec,
                                        bitrate_kbps,
                                        width: None,
                                        height: None,
                                        fps: None,
                                        chroma: None,
                                        channels: None,
                                        sample_rate: None,
                                    }),
                                    None => {
                                        // Skip streams without codec info
                                    }
                                }
                            }
                        }
                    }
                    programs_out.push(ProgramJson {
                        program: *prog_num,
                        streams: es_vec,
                    });
                }
            }
        }

        // Filter TR-101 metrics based on analysis mode
        let filtered_tr101 = match analysis_mode {
            Some(crate::types::AnalysisMode::Tr101Priority1) => tr101.priority_1_only(),
            Some(crate::types::AnalysisMode::Tr101Priority12) => tr101.priority_1_and_2_only(),
            _ => tr101,
        };

        let rep = ReportJson {
            ts_time: chrono::Utc::now().to_rfc3339(),
            programs: programs_out,
            tr101: &filtered_tr101,
        };
        serde_json::to_string_pretty(&rep).unwrap()
    }

    /// Generate console output report (for debugging)
    pub fn print_console_report(
        pat_map: &HashMap<u16, PatSection>,
        pmt_map: &HashMap<u16, PmtSection>,
        stats_manager: &StatsManager,
    ) {
        println!("================ MPEG-TS Inspector =================");
        for (prog_num, pat) in pat_map {
            println!("Program #{prog_num}");
            // find PMT
            if let Some(pmt_pid) = pat.programs
                .iter()
                .find(|p| p.program_number == *prog_num)
                .map(|p| p.pmt_pid)
            {
                if let Some(pmt) = pmt_map.get(&pmt_pid) {
                    for s in &pmt.streams {
                        let pid = s.elementary_pid;
                        if let Some(stats) = stats_manager.get(pid) {
                            if let Some(bitrate_kbps) = stats_manager.calculate_bitrate(pid) {
                                let (codec_name, extra) = match &stats.codec {
                                    Some(CodecInfo::Video(v)) => (
                                        v.codec.as_str(),
                                        format!("{}×{} {:.2} fps {}", v.width, v.height, v.fps, v.chroma),
                                    ),
                                    Some(CodecInfo::Audio(a)) => (
                                        a.codec.as_str(),
                                        format!("{}ch {} Hz",
                                            a.channels.map_or("?".to_string(), |c| c.to_string()),
                                            a.sample_rate.map_or("?".to_string(), |sr| sr.to_string())
                                        ),
                                    ),
                                    Some(CodecInfo::Subtitle(sub)) => (
                                        sub.codec.as_str(),
                                        String::new(),
                                    ),
                                    None => ("Unknown", String::new()),
                                };
                                println!(
                                    "  PID 0x{pid:04X} | {: <4} {: <9} | {:>6.1} kb/s {}",
                                    Self::stream_type_name(s.stream_type),
                                    codec_name,
                                    bitrate_kbps,
                                    extra
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn stream_type_name(st: u8) -> &'static str {
        match st {
            0x1B => "H.264",
            0x24 => "HEVC",
            0x0F => "AAC",
            0x11 => "LATM",    // AAC LATM
            0x02 => "MPEG2",
            0x03 | 0x04 => "MP2",
            0x81 => "AC-3",
            0x06 => "DVB-Sub",
            _ => "unk",
        }
    }
}