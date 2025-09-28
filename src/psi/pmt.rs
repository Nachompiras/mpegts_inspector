use crc::{Crc, CRC_32_MPEG_2};
use crate::psi::section::SectionReader;
const CRC: Crc<u32> = Crc::<u32>::new(&CRC_32_MPEG_2);
/// ─────────── PMT ───────────
#[derive(Clone)]
pub struct PmtSection {
    pub version:        u8,
    pub program_number: u16,
    pub pcr_pid:        u16,
    pub streams:        Vec<StreamInfo>,
}
#[derive(Clone)]
pub struct StreamInfo {
    pub stream_type:   u8,
    pub elementary_pid:u16,
}

pub fn parse_pmt(payload:&[u8]) -> anyhow::Result<PmtSection> {
    let sec = SectionReader::new(payload)?;
    if sec.table_id != 0x02 { anyhow::bail!("not PMT"); }
    let b = sec.body;

    /* ── cabecera fija dentro del cuerpo ── */
    let pcr_pid       = (((b[0] & 0x1F) as u16) << 8) | (b[1] as u16);
    let prog_info_len = (((b[2] & 0x0F) as usize) << 8) | (b[3] as usize);
    let mut idx       = 4 + prog_info_len;          // saltamos descriptors

    /* ── bucle de ES ── */
    let mut streams = Vec::new();
    while idx + 5 <= b.len() {
        let stype = b[idx];
        let pid   = (((b[idx+1] & 0x1F) as u16) << 8) | (b[idx+2] as u16);
        let eslen = (((b[idx+3] & 0x0F) as usize) << 8) | (b[idx+4] as usize);
        streams.push(StreamInfo{ stream_type:stype, elementary_pid:pid });
        idx += 5 + eslen;                          // saltamos descriptors ES
    }

    Ok(PmtSection{ version:sec.version,
                   program_number:sec.program_number,
                   pcr_pid,
                   streams })
}