use bitstream_io::{BitRead, BitReader, BigEndian};

use crate::core::{AacInfo, VideoInfo};

/// Tries to find the first SPS in a H.264 or HEVC ES payload and returns parsed info
pub fn parse_h26x_sps(data: &[u8]) -> Option<VideoInfo> {
    // naive: find NAL start 0x000001 / 0x00000001 and check nal_unit_type
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

// fn ue<R: std::io::Read>(r: &mut BitReader<R, BE>) -> Option<u32> {
//     let mut zeros = 0;
//     while let Ok(b) = r.read_bit() {
//         if b {
//             break;
//         }
//         zeros += 1;
//     }
//     if zeros > 31 {
//         return None;
//     }
//     let mut v = 1u32;
//     for _ in 0..zeros {
//         v = (v << 1) | r.read_bit().ok()? as u32;
//     }
//     Some(v - 1)
// }

fn parse_avc_sps(raw: &[u8]) -> Option<VideoInfo> {
    let rbsp = remove_ep(raw);
    let mut br = BitReader::endian(&rbsp[..], BigEndian);

    /* ——— cabecera ——— */
    let profile_idc = br.read::<8, u8>().ok()?;
    br.skip(16).ok()?;                          // constraint flags + level_idc
    ue(&mut br)?;                                   // seq_parameter_set_id

    /* ——— High profiles ——— */
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
                    // scaling_list_present_flag[i] ⇒ consumir lista
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

    /* ——— campos necesarios antes del tamaño ——— */
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

    /* ——— tamaño ——— */
    let pic_width_in_mbs_minus1 = ue(&mut br)? as u32;
    let pic_height_in_map_units_minus1 = ue(&mut br)? as u32;
    let frame_mbs_only_flag = br.read::<1, u8>().ok()? != 0;
    if !frame_mbs_only_flag {
        br.skip(1).ok()?; // mb_adaptive_frame_field_flag
    }
    br.skip(1).ok()?; // direct_8x8_inference_flag

    /* ——— cropping ——— */
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

    /* ——— VUI → fps ——— */
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
            let num_units_in_tick = br.read::<32, u32>().ok()? as f32;
            let time_scale = br.read::<32, u32>().ok()? as f32;
            let fixed = br.read::<1, u8>().ok()? != 0;
            if fixed && num_units_in_tick != 0.0 {
                fps = time_scale / (2.0 * num_units_in_tick);
            }
        }
        // el resto (HRD, pic_struct, etc.) lo ignoramos
    }

    /* ——— cálculo final de ancho/alto ——— */
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
        codec: "H.264",
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
        .into(),
    })
}

fn parse_hevc_sps(raw: &[u8]) -> Option<VideoInfo> {
    let rbsp = remove_emulation_prevention(raw);
    let mut rdr = BitReader::endian(&rbsp[..], bitstream_io::BigEndian);
    rdr.skip(4 * 8).ok()?; // skip sps_video_parameter_set_id .. etc
    let width = ue(&mut rdr)? as u16; // misleading – real parsing needs more, simplified
    let height = ue(&mut rdr)? as u16;
    Some(VideoInfo {
        codec: "HEVC",
        width,
        height,
        fps: 0.0,
        chroma: String::new(),
    })
}

fn remove_emulation_prevention(data: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if i + 2 < data.len() && data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 3 {
            v.push(0);
            v.push(0);
            i += 3;
        } else {
            v.push(data[i]);
            i += 1;
        }
    }
    v
}

/// Parse first ADTS header in the payload
pub fn parse_aac_adts(data: &[u8]) -> Option<AacInfo> {
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
            return Some(AacInfo {
                profile: "LC",
                sr: sample_rate,
                channels: channel_cfg,
            });
        }
    }
    None
}

/* ───────────────── helpers ───────────────── */

/// unsigned Exp-Golomb
fn ue<R: std::io::Read>(br: &mut BitReader<R, BigEndian>) -> Option<u32> {
    let mut zeros = 0;
    while br.read::<1, u8>().ok()? == 0 {
        zeros += 1;
    }
    let mut val = 1u32;
    for _ in 0..zeros {
        val = (val << 1) | br.read::<1, u8>().ok()? as u32;
    }
    Some(val - 1)
}

/// signed Exp-Golomb
fn se<R: std::io::Read>(br: &mut BitReader<R, BigEndian>) -> Option<i32> {
    let k = ue(br)? as i32;
    Some(if k & 1 == 0 { -(k as i32 + 1) / 2 } else { (k + 1) / 2 })
}

/// elimina bytes 0x000003
fn remove_ep(data: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if i + 2 < data.len() && data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 3 {
            v.extend_from_slice(&data[i..i + 2]);
            i += 3;
        } else {
            v.push(data[i]);
            i += 1;
        }
    }
    v
}