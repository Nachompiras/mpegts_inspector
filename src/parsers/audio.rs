//! Audio codec parsers

use crate::types::AudioInfo;

/// Parse first ADTS header in the payload
pub fn parse_aac_adts(data: &[u8]) -> Option<AudioInfo> {
    for i in 0..data.len().saturating_sub(7) {
        if data[i] == 0xFF && (data[i + 1] & 0xF6) == 0xF0 {
            let sr_index = (data[i + 2] & 0x3C) >> 2;
            let channel_cfg = ((data[i + 2] & 0x01) << 2) | ((data[i + 3] & 0xC0) >> 6);
            let sample_rate = match sr_index {
                0 => 96000,
                1 => 88200,
                2 => 64000,
                3 => 48000,
                4 => 44100,
                5 => 32000,
                6 => 24000,
                7 => 22050,
                8 => 16000,
                9 => 12000,
                10 => 11025,
                11 => 8000,
                _ => 0,
            };
            return Some(AudioInfo {
                codec: "AAC".to_string(),
                profile: Some("LC".to_string()),
                sample_rate: Some(sample_rate),
                channels: Some(channel_cfg),
            });
        }
    }
    None
}

/// Parse MPEG Audio (MP1/MP2/MP3) frame header
pub fn parse_mp2(data: &[u8]) -> Option<AudioInfo> {
    // MPEG Audio frame starts with 0xFFF (12 bits sync word)
    for i in 0..data.len().saturating_sub(4) {
        if data[i] == 0xFF && (data[i + 1] & 0xE0) == 0xE0 {
            let header = &data[i..i + 4];
            if header.len() < 4 { continue; }

            // Parse MPEG Audio header
            let version = (header[1] >> 3) & 0x03;
            let layer = (header[1] >> 1) & 0x03;
            let _bitrate_index = (header[2] >> 4) & 0x0F;
            let sample_rate_index = (header[2] >> 2) & 0x03;
            let channel_mode = (header[3] >> 6) & 0x03;

            // We're specifically looking for Layer II (MP2)
            if layer != 0x02 { continue; }  // Layer II = 0x02

            let sample_rate = match (version, sample_rate_index) {
                (0x03, 0x00) => 44100,  // MPEG-1, 44.1kHz
                (0x03, 0x01) => 48000,  // MPEG-1, 48kHz
                (0x03, 0x02) => 32000,  // MPEG-1, 32kHz
                (0x02, 0x00) => 22050,  // MPEG-2, 22.05kHz
                (0x02, 0x01) => 24000,  // MPEG-2, 24kHz
                (0x02, 0x02) => 16000,  // MPEG-2, 16kHz
                (0x00, 0x00) => 11025,  // MPEG-2.5, 11.025kHz
                (0x00, 0x01) => 12000,  // MPEG-2.5, 12kHz
                (0x00, 0x02) => 8000,   // MPEG-2.5, 8kHz
                _ => continue,
            };

            let channels = match channel_mode {
                0x00 => 2, // Stereo
                0x01 => 2, // Joint stereo
                0x02 => 2, // Dual channel
                0x03 => 1, // Mono
                _ => 2,
            };

            // Layer II bitrates (kbps) for MPEG-1
            let codec_name = match version {
                0x03 => "MP2",      // MPEG-1 Layer II
                0x02 => "MP2",      // MPEG-2 Layer II
                0x00 => "MP2",      // MPEG-2.5 Layer II
                _ => continue,
            };

            return Some(AudioInfo {
                codec: codec_name.to_string(),
                profile: None,
                sample_rate: Some(sample_rate),
                channels: Some(channels),
            });
        }
    }
    None
}

/// Parse AC-3 sync frame header
pub fn parse_ac3(data: &[u8]) -> Option<AudioInfo> {
    // AC-3 sync frame starts with 0x0B77
    for i in 0..data.len().saturating_sub(5) {
        if data[i] == 0x0B && data[i + 1] == 0x77 {
            // Basic AC-3 frame found
            if i + 4 < data.len() {
                let fscod = (data[i + 4] >> 6) & 0x03;
                let acmod = (data[i + 6] >> 5) & 0x07;

                let sample_rate = match fscod {
                    0x00 => 48000,
                    0x01 => 44100,
                    0x02 => 32000,
                    _ => 0,
                };

                let channels = match acmod {
                    0x00 => 2, // 1+1 (dual mono)
                    0x01 => 1, // 1/0 (mono)
                    0x02 => 2, // 2/0 (stereo)
                    0x03 => 3, // 3/0
                    0x04 => 3, // 2/1
                    0x05 => 4, // 3/1
                    0x06 => 4, // 2/2
                    0x07 => 5, // 3/2
                    _ => 2,
                };

                // Check for LFE channel
                let lfe = if acmod == 0x01 { // mono doesn't use lfeon bit
                    false
                } else {
                    (data[i + 6] >> 4) & 0x01 == 1
                };

                return Some(AudioInfo {
                    codec: "AC-3".to_string(),
                    profile: None,
                    sample_rate: Some(sample_rate),
                    channels: Some(channels + if lfe { 1 } else { 0 }),
                });
            }
        }
    }
    None
}

/// Parse AAC LATM (Low-overhead MPEG-4 Audio Transport Multiplex) header
/// Used in stream_type 0x11 (LATM AAC)
pub fn parse_aac_latm(data: &[u8]) -> Option<AudioInfo> {
    // LATM sync pattern: 0x2B7 (11 bits) followed by length and config
    for i in 0..data.len().saturating_sub(3) {
        // Check for LATM sync word: 0x2B7 (11 bits)
        if ((data[i] as u16) << 3) | ((data[i + 1] as u16) >> 5) == 0x2B7 {
            // Found LATM sync, now parse the AudioMuxElement
            let mut bit_offset = 11; // Skip sync word
            let byte_offset = i;

            // Parse useSameStreamMux flag
            let use_same_mux = get_bit(data, byte_offset, bit_offset);
            bit_offset += 1;

            if !use_same_mux {
                // Parse StreamMuxConfig
                if let Some((sample_rate, channels, profile)) = parse_stream_mux_config(data, byte_offset, &mut bit_offset) {
                    return Some(AudioInfo {
                        codec: "AAC".to_string(),
                        profile: Some(profile),
                        sample_rate: Some(sample_rate),
                        channels: Some(channels),
                    });
                }
            } else {
                // Use previous config - return basic AAC info
                return Some(AudioInfo {
                    codec: "AAC".to_string(),
                    profile: Some("LC".to_string()),
                    sample_rate: None, // Would need to store previous config
                    channels: None,
                });
            }
        }
    }
    None
}

/// Parse StreamMuxConfig for LATM
fn parse_stream_mux_config(data: &[u8], byte_offset: usize, bit_offset: &mut usize) -> Option<(u32, u8, String)> {
    if byte_offset + 4 >= data.len() {
        return None;
    }

    // Parse audioMuxVersion (1 bit)
    let _audio_mux_version = get_bit(data, byte_offset, *bit_offset);
    *bit_offset += 1;

    // Parse allStreamsSameTimeFraming (1 bit)
    let _all_streams_same_time = get_bit(data, byte_offset, *bit_offset);
    *bit_offset += 1;

    // Parse numSubFrames (6 bits)
    let _num_sub_frames = get_bits(data, byte_offset, *bit_offset, 6);
    *bit_offset += 6;

    // Parse numProgram (4 bits)
    let num_program = get_bits(data, byte_offset, *bit_offset, 4);
    *bit_offset += 4;

    if num_program != 0 {
        return None; // We only handle single program for now
    }

    // Parse numLayer (3 bits)
    let num_layer = get_bits(data, byte_offset, *bit_offset, 3);
    *bit_offset += 3;

    if num_layer != 0 {
        return None; // We only handle single layer for now
    }

    // Parse AudioSpecificConfig (simplified)
    if let Some((sample_rate, channels, profile)) = parse_audio_specific_config_latm(data, byte_offset, bit_offset) {
        Some((sample_rate, channels, profile))
    } else {
        None
    }
}

/// Parse AudioSpecificConfig for LATM (simplified version)
fn parse_audio_specific_config_latm(data: &[u8], byte_offset: usize, bit_offset: &mut usize) -> Option<(u32, u8, String)> {
    if byte_offset + 2 >= data.len() {
        return None;
    }

    // Parse audioObjectType (5 bits)
    let audio_object_type = get_bits(data, byte_offset, *bit_offset, 5);
    *bit_offset += 5;

    // Parse samplingFrequencyIndex (4 bits)
    let sampling_freq_index = get_bits(data, byte_offset, *bit_offset, 4);
    *bit_offset += 4;

    let sample_rate = match sampling_freq_index {
        0 => 96000,
        1 => 88200,
        2 => 64000,
        3 => 48000,
        4 => 44100,
        5 => 32000,
        6 => 24000,
        7 => 22050,
        8 => 16000,
        9 => 12000,
        10 => 11025,
        11 => 8000,
        15 => {
            // Explicit frequency (24 bits) - skip for simplicity
            *bit_offset += 24;
            0
        },
        _ => 0,
    };

    // Parse channelConfiguration (4 bits)
    let channel_config = get_bits(data, byte_offset, *bit_offset, 4);
    *bit_offset += 4;

    let channels = match channel_config {
        0 => 0, // Defined in AOT Specific Config
        1 => 1, // 1 channel: front-center
        2 => 2, // 2 channels: front-left, front-right
        3 => 3, // 3 channels: front-center, front-left, front-right
        4 => 4, // 4 channels: front-center, front-left, front-right, back-center
        5 => 5, // 5 channels: front-center, front-left, front-right, back-left, back-right
        6 => 6, // 6 channels: front-center, front-left, front-right, back-left, back-right, LFE-channel
        7 => 8, // 8 channels: front-center, front-left, front-right, side-left, side-right, back-left, back-right, LFE-channel
        _ => 2, // Default to stereo
    };

    let profile = match audio_object_type {
        1 => "Main".to_string(),
        2 => "LC".to_string(),   // Low Complexity (most common)
        3 => "SSR".to_string(),  // Scalable Sampling Rate
        4 => "LTP".to_string(),  // Long Term Prediction
        5 => "SBR".to_string(),  // Spectral Band Replication
        _ => "LC".to_string(),   // Default to LC
    };

    if sample_rate > 0 && channels > 0 {
        Some((sample_rate, channels, profile))
    } else {
        None
    }
}

/// Extract a single bit from data at given byte and bit offset
fn get_bit(data: &[u8], byte_offset: usize, bit_offset: usize) -> bool {
    let byte_idx = byte_offset + (bit_offset / 8);
    let bit_idx = 7 - (bit_offset % 8);

    if byte_idx >= data.len() {
        return false;
    }

    (data[byte_idx] >> bit_idx) & 0x01 != 0
}

/// Extract multiple bits from data at given byte and bit offset
fn get_bits(data: &[u8], byte_offset: usize, bit_offset: usize, num_bits: usize) -> u32 {
    let mut result = 0u32;
    let mut current_bit = bit_offset;

    for _ in 0..num_bits {
        if get_bit(data, byte_offset, current_bit) {
            result = (result << 1) | 1;
        } else {
            result <<= 1;
        }
        current_bit += 1;
    }

    result
}