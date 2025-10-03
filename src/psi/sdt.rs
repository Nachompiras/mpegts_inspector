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

/// Decode DVB text (EN 300 468)
/// Supports ISO 6937 (default), UTF-8, and basic Latin-1
fn decode_dvb_text(data: &[u8]) -> Option<String> {
    if data.is_empty() {
        return None;
    }

    // Check for encoding prefix
    let (encoding, text_data) = if data[0] < 0x20 {
        match data[0] {
            0x15 => (Encoding::Utf8, &data[1..]),           // UTF-8
            0x10 => {
                // ISO 8859 with code page in next 2 bytes
                if data.len() >= 3 {
                    (Encoding::Iso8859(data[2]), &data[3..])
                } else {
                    return None;
                }
            }
            _ => (Encoding::Iso6937, &data[1..]),           // Other encodings default to ISO 6937
        }
    } else {
        (Encoding::Iso6937, data)                           // No prefix = ISO 6937 (DVB default)
    };

    match encoding {
        Encoding::Utf8 => String::from_utf8(text_data.to_vec()).ok(),
        Encoding::Iso8859(1) | Encoding::Iso6937 => {
            // ISO 8859-1 (Latin-1) and basic ISO 6937 can be converted directly
            // For full ISO 6937 support, a proper conversion table would be needed
            Some(text_data.iter().map(|&b| b as char).collect())
        }
        _ => {
            // Fallback: try UTF-8, then Latin-1
            String::from_utf8(text_data.to_vec())
                .ok()
                .or_else(|| Some(text_data.iter().map(|&b| b as char).collect()))
        }
    }
}

#[derive(Debug)]
enum Encoding {
    Iso6937,
    Utf8,
    Iso8859(u8),
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
    // We don’t need them now, but kept for completeness

    let mut idx = 8;                              // start of service loop
    let mut services = Vec::new();

    while idx + 5 <= b.len() {
        let service_id = u16::from_be_bytes([b[idx], b[idx + 1]]);
        let desc_len   = (((b[idx + 3] & 0x0F) as usize) << 8) | b[idx + 4] as usize;

        // Parse descriptors to extract service_name (descriptor tag 0x48)
        let mut service_name = None;
        let desc_start = idx + 5;
        let desc_end = desc_start + desc_len;

        if desc_end <= b.len() {
            let mut desc_idx = desc_start;
            while desc_idx + 2 <= desc_end {
                let tag = b[desc_idx];
                let len = b[desc_idx + 1] as usize;
                if desc_idx + 2 + len > desc_end { break; }

                // Service descriptor (0x48)
                if tag == 0x48 && len >= 3 {
                    let _service_type = b[desc_idx + 2];
                    let provider_name_len = b[desc_idx + 3] as usize;
                    let name_start = desc_idx + 4 + provider_name_len;

                    if name_start < desc_idx + 2 + len {
                        let service_name_len = b[name_start] as usize;
                        let name_data_start = name_start + 1;
                        let name_data_end = name_data_start + service_name_len;

                        if name_data_end <= desc_idx + 2 + len {
                            // Extract service name, handling DVB text encoding
                            let name_bytes = &b[name_data_start..name_data_end];
                            service_name = decode_dvb_text(name_bytes);
                        }
                    }
                }

                desc_idx += 2 + len;
            }
        }

        idx = desc_end;
        if idx > b.len() { break; }               // graceful exit on malformed len
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