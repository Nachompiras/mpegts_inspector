//! Report generation for MPEG-TS inspection results

use serde::Serialize;
use crate::types::{InspectorReport, ProgramInfo, StreamInfo, CodecInfo};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pcr_pid: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pmt_version: Option<u8>,
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
        processor: &crate::processor::PacketProcessor,
        tr101: Tr101Metrics,
        analysis_mode: Option<crate::types::AnalysisMode>,
    ) -> InspectorReport {
        let mut programs = Vec::new();

        for (prog_num, pat) in &processor.pat_map {
            if let Some(pmt_pid) = pat.programs
                .iter()
                .find(|p| p.program_number == *prog_num)
                .map(|p| p.pmt_pid)
            {
                if let Some(pmt) = processor.pmt_map.get(&pmt_pid) {
                    let mut streams = Vec::new();
                    for s in &pmt.streams {
                        if let Some(stats) = processor.stats_manager.get(s.elementary_pid) {
                            if let Some(bitrate_kbps) = processor.stats_manager.calculate_bitrate(s.elementary_pid) {
                                streams.push(StreamInfo {
                                    pid: s.elementary_pid,
                                    stream_type: s.stream_type,
                                    codec: stats.codec.clone(),
                                    bitrate_kbps,
                                });
                            }
                        }
                    }
                    // Get PCR PID and PMT version for this program
                    let pcr_pid = processor.get_pcr_pid(*prog_num);
                    let pmt_version = processor.get_pmt_version(pmt_pid);

                    programs.push(ProgramInfo {
                        program_number: *prog_num,
                        streams,
                        pcr_pid,
                        pmt_version,
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
        processor: &crate::processor::PacketProcessor,
        tr101: Tr101Metrics,
        analysis_mode: Option<crate::types::AnalysisMode>,
    ) -> String {
        let mut programs_out = Vec::new();

        for (prog_num, pat) in &processor.pat_map {
            if let Some(pmt_pid) = pat.programs
                .iter()
                .find(|p| p.program_number == *prog_num)
                .map(|p| p.pmt_pid)
            {
                if let Some(pmt) = processor.pmt_map.get(&pmt_pid) {
                    let mut es_vec = Vec::new();
                    for s in &pmt.streams {
                        if let Some(stats) = processor.stats_manager.get(s.elementary_pid) {
                            if let Some(bitrate_kbps) = processor.stats_manager.calculate_bitrate(s.elementary_pid) {
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
                    // Get PCR PID and PMT version for this program
                    let pcr_pid = processor.get_pcr_pid(*prog_num);
                    let pmt_version = processor.get_pmt_version(pmt_pid);

                    programs_out.push(ProgramJson {
                        program: *prog_num,
                        streams: es_vec,
                        pcr_pid,
                        pmt_version,
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
        serde_json::to_string_pretty(&rep).unwrap_or_else(|_| "{\"error\": \"JSON serialization failed\"}".to_string())
    }            
}