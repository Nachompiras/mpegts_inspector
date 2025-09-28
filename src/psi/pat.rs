use crc::{Crc, CRC_32_MPEG_2};
use crate::psi::section::SectionReader;
const CRC: Crc<u32> = Crc::<u32>::new(&CRC_32_MPEG_2);

/// ─────────── PAT ───────────
#[derive(Clone)]
pub struct PatSection {
    pub version:      u8,
    pub current_next: bool,
    pub programs:     Vec<PatEntry>,
}
#[derive(Clone)]
pub struct PatEntry {
    pub program_number: u16,
    pub pmt_pid:        u16,
}

pub fn parse_pat(payload:&[u8]) -> anyhow::Result<PatSection> {
    let sec = SectionReader::new(payload)?;
    if sec.table_id != 0x00 { anyhow::bail!("not PAT"); }

    let mut idx = 0;
    let mut programs = Vec::new();
    while idx + 4 <= sec.body.len() {
        let pn  = u16::from_be_bytes(sec.body[idx..idx+2].try_into()?);
        let pid = (((sec.body[idx+2] & 0x1F) as u16) << 8) | (sec.body[idx+3] as u16);
        idx += 4;
        if pn != 0 { programs.push(PatEntry{ program_number:pn, pmt_pid:pid }); }
    }
    Ok(PatSection{ version:sec.version, current_next:sec.current_next, programs })
}