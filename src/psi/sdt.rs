// psi/sdt.rs
use super::section::SectionReader;
pub struct SdtSection { 
    pub version:  u8,
    pub services: Vec<Service> 
}
pub struct Service { 
    pub service_id: u16 
}

/// SDT (table_id 0x42 actual / 0x46 other-TS) – minimal fields + CRC check.
pub fn parse_sdt(payload: &[u8]) -> anyhow::Result<(u8, SdtSection)> {
    let sec = SectionReader::new(payload)?;
    if sec.table_id != 0x42 && sec.table_id != 0x46 {
        anyhow::bail!("not SDT");
    }

    let b = sec.body;
    if b.len() < 8 {
        anyhow::bail!("SDT body too short");
    }

    // Fixed header inside SDT body
    let transport_stream_id = u16::from_be_bytes([b[0], b[1]]);
    let original_net_id     = u16::from_be_bytes([b[6], b[7]]);
    // We don’t need them now, but kept for completeness

    let mut idx = 8;                              // start of service loop
    let mut services = Vec::new();

    while idx + 5 <= b.len() {
        let service_id = u16::from_be_bytes([b[idx], b[idx + 1]]);
        let desc_len   = (((b[idx + 3] & 0x0F) as usize) << 8) | b[idx + 4] as usize;
        idx += 5 + desc_len;
        if idx > b.len() { break; }               // graceful exit on malformed len
        services.push(Service { service_id });
    }

    Ok((
        sec.table_id,
        SdtSection {
            version: sec.version,
            services,
        },
    ))
}