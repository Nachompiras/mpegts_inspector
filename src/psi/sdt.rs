// psi/sdt.rs
use super::section::SectionReader;
pub struct SdtSection {
    pub version:  u8,
    pub services: Vec<Service>
}
pub struct Service {
    pub service_id: u16,
    pub service_name: Option<String>,
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
    let _transport_stream_id = u16::from_be_bytes([b[0], b[1]]);
    let _original_net_id     = u16::from_be_bytes([b[6], b[7]]);

    let mut idx = 8;                              // start of service loop
    let mut services = Vec::new();

    while idx + 5 <= b.len() {
        let service_id = u16::from_be_bytes([b[idx], b[idx + 1]]);
        let desc_len   = (((b[idx + 3] & 0x0F) as usize) << 8) | b[idx + 4] as usize;
        let desc_start = idx + 5;
        let desc_end   = desc_start + desc_len;
        if desc_end > b.len() { break; }

        // Parse descriptors to find service_descriptor (tag 0x48)
        let service_name = parse_service_name(&b[desc_start..desc_end]);

        idx = desc_end;
        services.push(Service { service_id, service_name });
    }

    Ok((
        sec.table_id,
        SdtSection {
            version: sec.version,
            services,
        },
    ))
}

/// Walk the descriptor loop looking for tag 0x48 (service_descriptor).
/// Returns the service_name if found.
fn parse_service_name(descriptors: &[u8]) -> Option<String> {
    let mut pos = 0;
    while pos + 2 <= descriptors.len() {
        let tag = descriptors[pos];
        let len = descriptors[pos + 1] as usize;
        let desc_end = pos + 2 + len;
        if desc_end > descriptors.len() {
            break;
        }

        if tag == 0x48 && len >= 2 {
            // service_descriptor: service_type (1) + provider_name_length (1) + provider_name + service_name_length (1) + service_name
            let body = &descriptors[pos + 2..desc_end];
            // body[0] = service_type
            let provider_len = body[1] as usize;
            let sn_offset = 2 + provider_len;
            if sn_offset < body.len() {
                let sn_len = body[sn_offset] as usize;
                let sn_start = sn_offset + 1;
                if sn_start + sn_len <= body.len() {
                    let raw = &body[sn_start..sn_start + sn_len];
                    return Some(decode_dvb_text(raw));
                }
            }
        }
        pos = desc_end;
    }
    None
}

/// Decode DVB text: handle the character table prefix byte.
/// If the first byte is < 0x20, it's a character table selector; skip it
/// and decode as Latin-1. Otherwise treat the whole thing as Latin-1/UTF-8.
fn decode_dvb_text(raw: &[u8]) -> String {
    if raw.is_empty() {
        return String::new();
    }

    let (bytes, _is_utf8) = if raw[0] < 0x20 {
        match raw[0] {
            0x15 => (&raw[1..], true),  // UTF-8 encoding
            _ => (&raw[1..], false),     // Other character tables, treat as Latin-1
        }
    } else {
        (raw, false)
    };

    if _is_utf8 {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        // Latin-1 (ISO 8859-1) decoding
        bytes.iter().map(|&b| b as char).collect()
    }
}
