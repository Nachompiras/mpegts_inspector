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
                    interlaced: false, // MPEG-2 sequence header doesn't provide interlaced info reliably
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
            let _fixed_frame_rate_flag = br.read::<1, u8>().ok()? != 0;

            if num_units_in_tick > 0 && time_scale > 0 {
                // Per ISO/IEC 14496-10: time_scale / (2 * num_units_in_tick) gives
                // the frame rate. The division by 2 is always needed because
                // num_units_in_tick represents clock ticks per field (2 fields per frame).
                // fixed_frame_rate_flag only indicates whether the rate is constant,
                // NOT whether to divide by 2.
                fps = (time_scale as f32) / (num_units_in_tick as f32 * 2.0);

                // Sanity check: FPS should be reasonable (1-120 fps)
                if !(1.0..=120.0).contains(&fps) {
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
        3 => 2 - frame_mbs_only_flag as u32,
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
        interlaced: !frame_mbs_only_flag,
    })
}

fn parse_hevc_sps(raw: &[u8]) -> Option<VideoInfo> {
    let rbsp = remove_emulation_prevention(raw);
    let mut br = BitReader::endian(&rbsp[..], bitstream_io::BigEndian);

    // sps_video_parameter_set_id  u(4)
    br.skip(4).ok()?;
    // sps_max_sub_layers_minus1  u(3)
    let max_sub_layers_minus1 = br.read::<3, u8>().ok()? as u32;
    // sps_temporal_id_nesting_flag  u(1)
    br.skip(1).ok()?;

    // ── profile_tier_level( 1, sps_max_sub_layers_minus1 ) ──
    // General profile: space(2)+tier(1)+idc(5)+compat(32)+constraints(48) = 88 bits
    br.skip(88).ok()?;
    // general_level_idc  u(8)
    br.skip(8).ok()?;

    // Sub-layer present flags
    let mut sub_layer_profile_present = [false; 8];
    let mut sub_layer_level_present = [false; 8];
    for i in 0..max_sub_layers_minus1 as usize {
        sub_layer_profile_present[i] = br.read::<1, u8>().ok()? != 0;
        sub_layer_level_present[i] = br.read::<1, u8>().ok()? != 0;
    }
    if max_sub_layers_minus1 > 0 {
        for _ in max_sub_layers_minus1..8 {
            br.skip(2).ok()?; // reserved_zero_2bits
        }
    }
    for i in 0..max_sub_layers_minus1 as usize {
        if sub_layer_profile_present[i] {
            br.skip(88).ok()?; // sub_layer profile (same structure as general)
        }
        if sub_layer_level_present[i] {
            br.skip(8).ok()?; // sub_layer_level_idc
        }
    }
    // ── end profile_tier_level ──

    // sps_seq_parameter_set_id
    ue(&mut br)?;

    // chroma_format_idc
    let chroma_format_idc = ue(&mut br)?;
    if chroma_format_idc == 3 {
        br.skip(1).ok()?; // separate_colour_plane_flag
    }

    // pic_width_in_luma_samples, pic_height_in_luma_samples
    let pic_w = ue(&mut br)?;
    let pic_h = ue(&mut br)?;

    // conformance_window_flag
    let (width, height) = if br.read::<1, u8>().ok()? != 0 {
        let l = ue(&mut br)?;
        let r = ue(&mut br)?;
        let t = ue(&mut br)?;
        let b = ue(&mut br)?;
        let (sub_w, sub_h) = match chroma_format_idc {
            1 => (2u32, 2u32),
            2 => (2, 1),
            _ => (1, 1),
        };
        (pic_w.saturating_sub(sub_w * (l + r)), pic_h.saturating_sub(sub_h * (t + b)))
    } else {
        (pic_w, pic_h)
    };

    let chroma = match chroma_format_idc {
        0 => "4:0:0", 1 => "4:2:0", 2 => "4:2:2", 3 => "4:4:4", _ => "?",
    }.to_string();

    // Best-effort: parse remaining SPS fields to reach VUI for FPS
    let fps = hevc_parse_to_vui_fps(&mut br, max_sub_layers_minus1).unwrap_or(0.0);

    Some(VideoInfo {
        codec: "HEVC".to_string(),
        width: width as u16,
        height: height as u16,
        fps,
        chroma,
        interlaced: false,
    })
}

/// Best-effort parsing of remaining HEVC SPS fields to extract FPS from VUI.
/// Returns None if parsing fails at any point (PTS-based FPS will be used instead).
fn hevc_parse_to_vui_fps<R: std::io::Read>(
    br: &mut BitReader<R, BigEndian>,
    max_sub_layers_minus1: u32,
) -> Option<f32> {
    // bit_depth_luma_minus8, bit_depth_chroma_minus8
    ue(br)?;
    ue(br)?;
    // log2_max_pic_order_cnt_lsb_minus4
    let log2_max_poc_lsb_minus4 = ue(br)?;

    // sps_sub_layer_ordering_info_present_flag
    let ordering_present = br.read::<1, u8>().ok()? != 0;
    let start = if ordering_present { 0 } else { max_sub_layers_minus1 };
    for _ in start..=max_sub_layers_minus1 {
        ue(br)?; // max_dec_pic_buffering_minus1
        ue(br)?; // max_num_reorder_pics
        ue(br)?; // max_latency_increase_plus1
    }

    // 6 ue values: coding/transform block sizes + transform hierarchy depths
    for _ in 0..6 { ue(br)?; }

    // scaling_list_enabled_flag
    if br.read::<1, u8>().ok()? != 0
        && br.read::<1, u8>().ok()? != 0
    {
        hevc_skip_scaling_list_data(br)?;
    }

    // amp_enabled_flag + sample_adaptive_offset_enabled_flag
    br.skip(2).ok()?;

    // pcm_enabled_flag
    if br.read::<1, u8>().ok()? != 0 {
        br.skip(8).ok()?; // pcm bit depths (4+4)
        ue(br)?; // log2_min_pcm_luma_coding_block_size_minus3
        ue(br)?; // log2_diff_max_min_pcm_luma_coding_block_size
        br.skip(1).ok()?; // pcm_loop_filter_disabled_flag
    }

    // num_short_term_ref_pic_sets
    let num_st_rps = ue(br)? as usize;
    let mut num_delta_pocs = vec![0u32; num_st_rps];
    for idx in 0..num_st_rps {
        hevc_skip_st_ref_pic_set(br, idx, &mut num_delta_pocs)?;
    }

    // long_term_ref_pics_present_flag
    if br.read::<1, u8>().ok()? != 0 {
        let num_lt = ue(br)?;
        let lt_bits = log2_max_poc_lsb_minus4 + 4;
        for _ in 0..num_lt {
            br.skip(lt_bits).ok()?; // lt_ref_pic_poc_lsb_sps
            br.skip(1).ok()?; // used_by_curr_pic_lt_sps_flag
        }
    }

    // sps_temporal_mvp_enabled_flag + strong_intra_smoothing_enabled_flag
    br.skip(2).ok()?;

    // vui_parameters_present_flag
    if br.read::<1, u8>().ok()? == 0 {
        return None;
    }

    // ── VUI parameters: skip to timing info ──
    if br.read::<1, u8>().ok()? != 0 { // aspect_ratio_info_present_flag
        if br.read::<8, u8>().ok()? == 255 { // Extended_SAR
            br.skip(32).ok()?; // sar_width + sar_height
        }
    }
    if br.read::<1, u8>().ok()? != 0 { // overscan_info_present_flag
        br.skip(1).ok()?;
    }
    if br.read::<1, u8>().ok()? != 0 { // video_signal_type_present_flag
        br.skip(4).ok()?; // video_format(3) + full_range(1)
        if br.read::<1, u8>().ok()? != 0 { // colour_description_present_flag
            br.skip(24).ok()?; // primaries + transfer + matrix
        }
    }
    if br.read::<1, u8>().ok()? != 0 { // chroma_loc_info_present_flag
        ue(br)?; ue(br)?;
    }
    // neutral_chroma + field_seq + frame_field_info_present
    br.skip(3).ok()?;
    if br.read::<1, u8>().ok()? != 0 { // default_display_window_flag
        ue(br)?; ue(br)?; ue(br)?; ue(br)?;
    }

    // vui_timing_info_present_flag
    if br.read::<1, u8>().ok()? != 0 {
        let num_units_in_tick = br.read::<32, u32>().ok()?;
        let time_scale = br.read::<32, u32>().ok()?;
        if num_units_in_tick > 0 && time_scale > 0 {
            let fps = time_scale as f32 / num_units_in_tick as f32;
            if (1.0..=240.0).contains(&fps) {
                return Some(fps);
            }
        }
    }

    None
}

/// Skip HEVC scaling_list_data() in the bitstream
fn hevc_skip_scaling_list_data<R: std::io::Read>(
    br: &mut BitReader<R, BigEndian>,
) -> Option<()> {
    for size_id in 0u32..4 {
        let step: u32 = if size_id == 3 { 3 } else { 1 };
        let mut matrix_id = 0u32;
        while matrix_id < 6 {
            if br.read::<1, u8>().ok()? == 0 { // scaling_list_pred_mode_flag
                ue(br)?; // pred_matrix_id_delta
            } else {
                let coef_num = 64u32.min(1u32 << (4 + (size_id << 1)));
                if size_id > 1 {
                    se(br)?; // scaling_list_dc_coef_minus8
                }
                for _ in 0..coef_num {
                    se(br)?; // scaling_list_delta_coef
                }
            }
            matrix_id += step;
        }
    }
    Some(())
}

/// Skip one HEVC short_term_ref_pic_set and track num_delta_pocs
fn hevc_skip_st_ref_pic_set<R: std::io::Read>(
    br: &mut BitReader<R, BigEndian>,
    idx: usize,
    num_delta_pocs: &mut [u32],
) -> Option<()> {
    let inter = if idx != 0 {
        br.read::<1, u8>().ok()? != 0
    } else {
        false
    };

    if inter {
        // delta_idx_minus1 is always 0 in SPS context
        br.skip(1).ok()?; // delta_rps_sign
        ue(br)?; // abs_delta_rps_minus1
        let ref_num = num_delta_pocs[idx - 1];
        let mut count = 0u32;
        for _ in 0..=ref_num {
            let used = br.read::<1, u8>().ok()? != 0;
            if !used {
                if br.read::<1, u8>().ok()? != 0 { count += 1; }
            } else {
                count += 1;
            }
        }
        num_delta_pocs[idx] = count;
    } else {
        let num_neg = ue(br)?;
        let num_pos = ue(br)?;
        for _ in 0..num_neg {
            ue(br)?; // delta_poc_s0_minus1
            br.skip(1).ok()?; // used_by_curr_pic_s0_flag
        }
        for _ in 0..num_pos {
            ue(br)?; // delta_poc_s1_minus1
            br.skip(1).ok()?; // used_by_curr_pic_s1_flag
        }
        num_delta_pocs[idx] = num_neg + num_pos;
    }

    Some(())
}