// psi/nit.rs
//! Very-light Network Information Table parser (actual network, tid 0x40)
use crate::psi::section::SectionReader;

#[derive(Clone)]
pub struct NitSection {
    pub version: u8,
    pub network_id: u16,
    pub transports: Vec<Transport>,
}

#[derive(Clone)]
pub struct Transport {
    pub ts_id: u16,
    pub orig_net_id: u16,
}

pub fn parse_nit(payload: &[u8]) -> anyhow::Result<(u8, NitSection)> {

    let sec = SectionReader::new(payload)?;
    if sec.table_id != 0x40 && sec.table_id != 0x41 {
        anyhow::bail!("not NIT");
    }

    let b = sec.body;                 // shorthand – already stripped CRC
    if b.len() < 2 {
        anyhow::bail!("NIT body too short");
    }

    // NIT body after common section header (8 bytes consumed by SectionReader):
    //   reserved_future_use(4 bits) + network_descriptors_length(12 bits) → b[0..2]
    //   network_descriptor() loop → b[2..2+net_desc_len]
    //   reserved_future_use(4 bits) + transport_stream_loop_length(12 bits) → 2 bytes
    //   transport_stream() loop
    // network_id is already in sec.program_number (table_id_extension)
    let network_id = sec.program_number;
    let net_desc_len = (((b[0] & 0x0F) as usize) << 8) | b[1] as usize;

    let mut idx = 2 + net_desc_len;   // skip network-descriptors
    if idx + 2 > b.len() { anyhow::bail!("truncated network descriptors"); }

    // Skip transport_stream_loop_length (2 bytes)
    let _ts_loop_len = (((b[idx] & 0x0F) as usize) << 8) | b[idx + 1] as usize;
    idx += 2;

    let mut transports = Vec::new();
    while idx + 6 <= b.len() {
        let ts_id       = u16::from_be_bytes([b[idx], b[idx + 1]]);
        let orig_net_id = u16::from_be_bytes([b[idx + 2], b[idx + 3]]);
        let desc_len    = (((b[idx + 4] & 0x0F) as usize) << 8) | b[idx + 5] as usize;
        idx += 6 + desc_len;
        if idx > b.len() { break; }   // graceful exit on malformed len
        transports.push(Transport { ts_id, orig_net_id });
    }

    Ok((
        sec.table_id,
        NitSection {
            version: sec.version,
            network_id,
            transports,
        },
    ))
}