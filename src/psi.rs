//! Parsers de PAT / PMT compatibles con bitstream-io 4.x
use crc::{Crc, CRC_32_MPEG_2};

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
    let sec = Section::new(payload)?;
    if sec.table_id != 0x00 { anyhow::bail!("not PAT"); }

    let mut idx = 0;
    let mut programs = Vec::new();
    while idx + 4 <= sec.body.len() {
        let pn  = u16::from_be_bytes(sec.body[idx..idx+2].try_into()?);
        let pid = ((sec.body[idx+2] & 0x1F) as u16) << 8 | sec.body[idx+3] as u16;
        idx += 4;
        if pn != 0 { programs.push(PatEntry{ program_number:pn, pmt_pid:pid }); }
    }
    Ok(PatSection{ version:sec.version, current_next:sec.current_next, programs })
}

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
    let sec = Section::new(payload)?;
    if sec.table_id != 0x02 { anyhow::bail!("not PMT"); }
    let b = sec.body;

    /* ── cabecera fija dentro del cuerpo ── */
    let pcr_pid       = ((b[0] & 0x1F) as u16) << 8 | b[1] as u16;
    let prog_info_len = ((b[2] & 0x0F) as usize) << 8 | b[3] as usize;
    let mut idx       = 4 + prog_info_len;          // saltamos descriptors

    /* ── bucle de ES ── */
    let mut streams = Vec::new();
    while idx + 5 <= b.len() {
        let stype = b[idx];
        let pid   = ((b[idx+1] & 0x1F) as u16) << 8 | b[idx+2] as u16;
        let eslen = ((b[idx+3] & 0x0F) as usize) << 8 | b[idx+4] as usize;
        streams.push(StreamInfo{ stream_type:stype, elementary_pid:pid });
        idx += 5 + eslen;                          // saltamos descriptors ES
    }

    Ok(PmtSection{ version:sec.version,
                   program_number:sec.program_number,
                   pcr_pid,
                   streams })
}

/// ─────────── helper genérico para secciones PSI ───────────
struct Section<'a>{
    table_id:      u8,
    version:       u8,
    current_next:  bool,
    program_number:u16,
    body:          &'a [u8],    // bytes SIN CRC ni cabecera de sección
}
impl<'a> Section<'a>{
    fn new(payload:&'a [u8]) -> anyhow::Result<Self>{
        if payload.is_empty(){ anyhow::bail!("no payload") }
        let ptr  = payload[0] as usize;
        let start= 1+ptr;
        if payload.len() < start+8 { anyhow::bail!("short section") }

        let tid    = payload[start];
        let slen   = ((payload[start+1] & 0x0F) as usize) << 8 | payload[start+2] as usize;
        let end    = start+3+slen;
        if end > payload.len(){ anyhow::bail!("truncated section") }

        // CRC-32 check
        let calc = CRC.checksum(&payload[start..end-4]);
        let crc  = u32::from_be_bytes(payload[end-4..end].try_into()?);
        if calc!=crc { anyhow::bail!("CRC mismatch") }

        Ok(Self{
            table_id: tid,
            version:  (payload[start+5] & 0x3E) >> 1,
            current_next: payload[start+5] & 0x01 != 0,
            program_number: u16::from_be_bytes(payload[start+3..start+5].try_into()?),
            body: &payload[start+8 .. end-4],
        })
    }
}