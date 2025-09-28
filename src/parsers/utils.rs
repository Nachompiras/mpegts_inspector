//! Common parsing utilities

use bitstream_io::{BitRead, BitReader, BigEndian};

/// Unsigned Exp-Golomb decoder
pub fn ue<R: std::io::Read>(br: &mut BitReader<R, BigEndian>) -> Option<u32> {
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

/// Signed Exp-Golomb decoder
pub fn se<R: std::io::Read>(br: &mut BitReader<R, BigEndian>) -> Option<i32> {
    let k = ue(br)? as i32;
    Some(if k & 1 == 0 { -(k as i32 + 1) / 2 } else { (k + 1) / 2 })
}

/// Remove emulation prevention bytes (0x000003)
pub fn remove_emulation_prevention(data: &[u8]) -> Vec<u8> {
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

/// Remove emulation prevention bytes (alternative implementation)
pub fn remove_ep(data: &[u8]) -> Vec<u8> {
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