// psi/eit.rs
//! Minimal EIT p/f (table_ids 0x4E / 0x4F) CRC validation.

use super::section::SectionReader;

#[derive(Clone)]
pub struct EitPfSection { pub version: u8, }

pub fn parse_eit_pf(payload: &[u8]) -> anyhow::Result<(u8, EitPfSection)> {
    let sec = SectionReader::new(payload)?;
    if sec.table_id != 0x4E && sec.table_id != 0x4F {
        anyhow::bail!("not EIT p/f");
    }
    Ok((sec.table_id, EitPfSection { version: sec.version }))
}