// psi/section.rs
//! Generic PSI / SI section reader with CRC-32 (MPEG-2) validation.

use crc::{Crc, CRC_32_MPEG_2};

/// Returned by [`SectionReader::new`].
pub struct SectionReader<'a> {
    pub table_id:      u8,
    pub version:       u8,
    pub current_next:  bool,
    pub section_number:u8,
    pub last_section:  u8,
    pub program_number:u16,
    pub body:          &'a [u8],   // bytes between fixed header & CRC
}

const CRC_MPEG: Crc<u32> = Crc::<u32>::new(&CRC_32_MPEG_2);

impl<'a> SectionReader<'a> {
    /// Validates pointer, length and (if present) CRC-32.
    pub fn new(payload: &'a [u8]) -> anyhow::Result<Self> {
        if payload.is_empty() { anyhow::bail!("payload empty") }
        let pointer = payload[0] as usize;
        let start   = 1 + pointer;
        if payload.len() < start + 8 { anyhow::bail!("short section") }

        let table_id = payload[start];
        let sec_len  = ((payload[start+1] & 0x0F) as usize) << 8 | payload[start+2] as usize;
        if sec_len < 5 { anyhow::bail!("invalid section_length") }
        let end      = start + 3 + sec_len;
        if end > payload.len() { anyhow::bail!("truncated section") }

        // If the spec says CRC present ⇒ last 4 bytes of section
        let crc_calc = CRC_MPEG.checksum(&payload[start..end-4]);
        let crc_pkt  = u32::from_be_bytes(payload[end-4..end].try_into()?);
        if crc_calc != crc_pkt {
            anyhow::bail!("CRC-32 mismatch");
        }

        Ok(Self {
            table_id,
            version:       (payload[start+5] & 0x3E) >> 1,
            current_next:  payload[start+5] & 0x01 != 0,
            section_number:payload[start+6],
            last_section:  payload[start+7],
            program_number: u16::from_be_bytes(payload[start+3..start+5].try_into()?),
            body:          &payload[start+8 .. end-4],
        })
    }
}