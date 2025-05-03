// psi/tdt.rs
//! TDT (0x70, no CRC)  &  TOT (0x73, CRC present) checker.

use anyhow::bail;

pub enum TdtTot<'a> {
    Tdt(&'a [u8]),          // UTC time only (5 bytes BCD)
    Tot(&'a [u8]),          // UTC time + descriptors
}

pub fn parse_tdt_tot(payload: &[u8]) -> anyhow::Result<(u8, TdtTot)> {
    if payload.is_empty() { bail!("payload empty"); }
    let pointer = payload[0] as usize;
    let start   = 1 + pointer;
    if payload.len() < start + 3 { bail!("short TDT/TOT"); }

    let tid      = payload[start];
    let sec_len  = ((payload[start+1] & 0x0F) as usize) << 8 | payload[start+2] as usize;
    let end      = start + 3 + sec_len;
    if end > payload.len() { bail!("truncated"); }

    match tid {
        0x70 => Ok((tid, TdtTot::Tdt(&payload[start+3 .. end]))),      // no CRC
        0x73 => {
            // TOT has CRC-32 at end
            use crc::{Crc, CRC_32_MPEG_2};
            let crc_calc = Crc::<u32>::new(&CRC_32_MPEG_2)
                .checksum(&payload[start .. end-4]);
            let crc_pkt = u32::from_be_bytes(payload[end-4..end].try_into()?);
            if crc_calc != crc_pkt { bail!("TOT CRC mismatch"); }
            Ok((tid, TdtTot::Tot(&payload[start+3 .. end-4])))
        }
        _ => bail!("not TDT/TOT"),
    }
}