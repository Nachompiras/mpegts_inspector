//! Video codec parsers

use bitstream_io::{BitRead, BitReader, BigEndian};
use crate::types::VideoInfo;
use super::utils::{ue, se, remove_ep, remove_emulation_prevention};

/// Parse MPEG-2 sequence header for video parameters
pub fn parse_mpeg2_seq_hdr(data: &[u8]) -> Option<VideoInfo> {
    // MPEG-2 sequence header starts with 0x000001B3
    for i in 0..data.len().saturating_sub(8) {
        if data[i] == 0x00 && data[i + 1] == 0x00 && data[i + 2] == 0x01 && data[i + 3] == 0xB3 {
            let seq_hdr = &data[i + 4..];
            if seq_hdr.len() >= 8 {
                // Parse sequence header
                let horizontal_size = ((seq_hdr[0] as u16) << 4) | ((seq_hdr[1] as u16) >> 4);
                let vertical_size = ((seq_hdr[1] as u16 & 0x0F) << 8) | (seq_hdr[2] as u16);
                let aspect_ratio_info = (seq_hdr[3] >> 4) & 0x0F;
                let frame_rate_code = seq_hdr[3] & 0x0F;

                let fps = match frame_rate_code {
                    1 => 23.976,
                    2 => 24.0,
                    3 => 25.0,
                    4 => 29.97,
                    5 => 30.0,
                    6 => 50.0,
                    7 => 59.94,
                    8 => 60.0,
                    _ => 0.0,
                };

                let _aspect_ratio = match aspect_ratio_info {
                    1 => "1:1",     // Square pixels
                    2 => "4:3",     // 4:3 display
                    3 => "16:9",    // 16:9 display
                    4 => "2.21:1",  // 2.21:1 display
                    _ => "?",
                };

                return Some(VideoInfo {
                    codec: "MPEG-2".to_string(),
                    width: horizontal_size,
                    height: vertical_size,
                    fps: fps as f32,
                    chroma: "4:2:0".to_string(), // MPEG-2 is typically 4:2:0
                });
            }
        }
    }
    None
}

/// Tries to find the first SPS in a H.264 or HEVC ES payload and returns parsed info
pub fn parse_h26x_sps(data: &[u8]) -> Option<VideoInfo> {
    // Find NAL start 0x000001 / 0x00000001 and check nal_unit_type
    let mut i = 0;
    while i + 4 < data.len() {
        if data[i] == 0x00 && data[i + 1] == 0x00 && data[i + 2] == 0x01 {
            let nal_start = i + 3;
            let nal_type = data[nal_start] & 0x1F; // H264
            if nal_type == 7 {
                return parse_avc_sps(&data[nal_start + 1..]);
            }
            // HEVC (0x000001 0x40..0x4F types 33 = SPS)
            let nal_type265 = (data[nal_start] >> 1) & 0x3F;
            if nal_type265 == 33 {
                return parse_hevc_sps(&data[nal_start + 2..]);
            }
        }
        i += 1;
    }
    None
}

fn parse_avc_sps(raw: &[u8]) -> Option<VideoInfo> {
    let rbsp = remove_ep(raw);
    let mut br = BitReader::endian(&rbsp[..], BigEndian);

    // Header
    let profile_idc = br.read::<8, u8>().ok()?;
    br.skip(16).ok()?;                          // constraint flags + level_idc
    ue(&mut br)?;                                   // seq_parameter_set_id

    // High profiles
    let mut chroma_format_idc = 1;
    if matches!(
        profile_idc,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 144
    ) {
        chroma_format_idc = ue(&mut br)?;
        if chroma_format_idc == 3 {
            br.skip(1).ok()?; // separate_colour_plane_flag
        }
        ue(&mut br)?; // bit_depth_luma_minus8
        ue(&mut br)?; // bit_depth_chroma_minus8
        br.skip(1).ok()?; // qpprime_y_zero_transform_bypass_flag

        if br.read::<1, u8>().ok()? != 0 {
            let lists = if chroma_format_idc == 3 { 12 } else { 8 };
            for idx in 0..lists {
                if br.read::<1, u8>().ok()? != 0 {
                    // scaling_list_present_flag[i] ⇒ consume list
                    let size = if idx < 6 { 16 } else { 64 };
                    let mut last = 8i32;
                    for _ in 0..size {
                        let delta = se(&mut br).unwrap_or(0);
                        last = (last + delta + 256) % 256;
                    }
                }
            }
        }
    }

    // Required fields before size
    ue(&mut br)?; // log2_max_frame_num_minus4
    let pic_order_cnt_type = ue(&mut br)?;
    if pic_order_cnt_type == 0 {
        ue(&mut br)?; // log2_max_pic_order_cnt_lsb_minus4
    } else if pic_order_cnt_type == 1 {
        br.skip(1).ok()?; // delta_pic_order_always_zero_flag
        se(&mut br)?;         // offset_for_non_ref_pic
        se(&mut br)?;         // offset_for_top_to_bottom_field
        let n = ue(&mut br)?;
        for _ in 0..n {
            se(&mut br)?;
        }
    }
    ue(&mut br)?; // max_num_ref_frames
    br.skip(1).ok()?; // gaps_in_frame_num_value_allowed_flag

    // Size
    let pic_width_in_mbs_minus1 = ue(&mut br)? as u32;
    let pic_height_in_map_units_minus1 = ue(&mut br)? as u32;
    let frame_mbs_only_flag = br.read::<1, u8>().ok()? != 0;
    if !frame_mbs_only_flag {
        br.skip(1).ok()?; // mb_adaptive_frame_field_flag
    }
    br.skip(1).ok()?; // direct_8x8_inference_flag

    // Cropping
    let cropping_flag = br.read::<1, u8>().ok()? != 0;
    let (crop_l, crop_r, crop_t, crop_b) = if cropping_flag {
        (
            ue(&mut br)?,
            ue(&mut br)?,
            ue(&mut br)?,
            ue(&mut br)?,
        )
    } else {
        (0, 0, 0, 0)
    };

    // VUI → fps
    let mut fps = 0.0_f32;
    if br.read::<1, u8>().ok()? != 0 {
        // vui_parameters_present_flag
        if br.read::<1, u8>().ok()? != 0 {
            // aspect_ratio_info_present_flag
            let idc = br.read::<8, u8>().ok()?;
            if idc == 255 {
                br.skip(16).ok()?; // sar_width/height
            }
        }
        if br.read::<1, u8>().ok()? != 0 {
            // overscan_info_present_flag
            br.skip(1).ok()?;
        }
        if br.read::<1, u8>().ok()? != 0 {
            // video_signal_type_present_flag
            br.skip(3).ok()?;
            if br.read::<1, u8>().ok()? != 0 {
                br.skip(24).ok()?;
            }
        }
        if br.read::<1, u8>().ok()? != 0 {
            // chroma_loc_info_present_flag
            ue(&mut br)?; ue(&mut br)?;
        }
        if br.read::<1, u8>().ok()? != 0 {
            // timing_info_present_flag
            let num_units_in_tick = br.read::<32, u32>().ok()?;
            let time_scale = br.read::<32, u32>().ok()?;
            let fixed_frame_rate_flag = br.read::<1, u8>().ok()? != 0;

            if num_units_in_tick > 0 && time_scale > 0 {
                // For progressive video, divide by 2
                // For interlaced video (field-based), don't divide by 2
                let divisor = if fixed_frame_rate_flag { 2.0 } else { 1.0 };
                fps = (time_scale as f32) / (num_units_in_tick as f32 * divisor);

                // Sanity check: FPS should be reasonable (1-120 fps)
                if fps < 1.0 || fps > 120.0 {
                    fps = 0.0; // Invalid, will be calculated from PTS
                }
            }
        }
        // ignore rest (HRD, pic_struct, etc.)
    }

    // Final width/height calculation
    let crop_unit_x = match chroma_format_idc {
        0 | 3 => 1,
        _ => 2,
    };
    let crop_unit_y = match chroma_format_idc {
        0 => 2 - frame_mbs_only_flag as u32,
        1 | 2 => 2 * (2 - frame_mbs_only_flag as u32),
        3 => 1 * (2 - frame_mbs_only_flag as u32),
        _ => 2,
    };

    let width =
        ((pic_width_in_mbs_minus1 + 1) * 16) - (crop_l + crop_r) * crop_unit_x;
    let height_map_units =
        (pic_height_in_map_units_minus1 + 1) * if frame_mbs_only_flag { 1 } else { 2 };
    let height =
        (height_map_units * 16) - (crop_t + crop_b) * crop_unit_y;

    Some(VideoInfo {
        codec: "H.264".to_string(),
        width: width as u16,
        height: height as u16,
        fps,
        chroma: match chroma_format_idc {
            0 => "4:0:0",
            1 => "4:2:0",
            2 => "4:2:2",
            3 => "4:4:4",
            _ => "?",
        }
        .to_string(),
    })
}

fn parse_hevc_sps(raw: &[u8]) -> Option<VideoInfo> {
    let rbsp = remove_emulation_prevention(raw);
    let mut rdr = BitReader::endian(&rbsp[..], bitstream_io::BigEndian);
    rdr.skip(4 * 8).ok()?; // skip sps_video_parameter_set_id .. etc
    let width = ue(&mut rdr)? as u16; // misleading – real parsing needs more, simplified
    let height = ue(&mut rdr)? as u16;
    Some(VideoInfo {
        codec: "HEVC".to_string(),
        width,
        height,
        fps: 0.0,
        chroma: String::new(),
    })
}