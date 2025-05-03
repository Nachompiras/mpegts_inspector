use crate::psi::section::SectionReader;
#[derive(Clone)]
pub struct CatSection {
    pub version: u8,
}
pub fn parse_cat(payload: &[u8]) -> anyhow::Result<(u8, CatSection)> {
    let sec = SectionReader::new(payload)?;          // CRC verified
    if sec.table_id != 0x01 {
        anyhow::bail!("not CAT");
    }
    Ok((sec.table_id, CatSection { version: sec.version }))
}