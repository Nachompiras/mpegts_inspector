use crate::psi::{nit::NitSection, pat::PatSection, pmt::PmtSection, sdt::SdtSection};

#[derive(Default)]
pub struct SiCache {
    pub pat: Option<PatSection>,
    pub pmts: std::collections::HashMap<u16, PmtSection>, // pmt_pid → PMT
    pub sdt: Option<SdtSection>,
    pub nit:  Option<NitSection>,
}

impl SiCache {
    /* called when we receive (and CRC-validate) a table */
    pub fn update_pat(&mut self, pat: PatSection) { self.pat = Some(pat); }
    pub fn update_pmt(&mut self, pid: u16, pmt: PmtSection) { self.pmts.insert(pid, pmt); }
    pub fn update_sdt(&mut self, sdt: SdtSection) { self.sdt = Some(sdt); }
    pub fn update_nit(&mut self, nit: NitSection) { self.nit = Some(nit); }

    /// 3.2-d Service_ID mismatch between SDT and PMT list
    pub fn check_service_id_mismatch(&self) -> bool {
        let sdt = match &self.sdt { Some(s) => s, None => return false };
        let pat = match &self.pat { Some(p) => p, None => return false };

        // collect service_ids from SDT
        let mut sdt_services = std::collections::HashSet::new();
        for svc in &sdt.services {
            sdt_services.insert(svc.service_id);
        }

        // iterate PAT programs that >0 (skip NIT)
        for prg in &pat.programs {
            if prg.program_number == 0 { continue; }
            if !sdt_services.contains(&prg.program_number) {
                return true; // mismatch detected
            }
        }
        false
    }
}