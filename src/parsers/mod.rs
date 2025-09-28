//! Codec parsers for different elementary stream types
//!
//! This module contains parsers for various audio and video codecs
//! found in MPEG-TS streams.

mod video;
mod audio;
mod utils;

pub use video::{parse_mpeg2_seq_hdr, parse_h26x_sps};
pub use audio::{parse_aac_adts, parse_aac_latm, parse_mp2, parse_ac3};

use crate::types::{VideoInfo, AudioInfo};

/// Parse any video codec from elementary stream data
pub fn parse_video_codec(stream_type: u8, data: &[u8]) -> Option<VideoInfo> {
    match stream_type {
        0x02 => parse_mpeg2_seq_hdr(data),
        0x1B | 0x24 => parse_h26x_sps(data),
        _ => None,
    }
}

/// Parse any audio codec from elementary stream data
pub fn parse_audio_codec(stream_type: u8, data: &[u8]) -> Option<AudioInfo> {
    match stream_type {
        0x03 | 0x04 => parse_mp2(data),
        0x0F => parse_aac_adts(data),
        0x11 => parse_aac_latm(data),    // AAC LATM
        0x81 => parse_ac3(data),
        _ => None,
    }
}