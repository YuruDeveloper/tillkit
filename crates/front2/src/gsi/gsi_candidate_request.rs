use crate::gsi::rollback_evidence::RollbackEvidence;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct GsiCandidateRequest<'a> {
    pub backup_directory: &'a Path,
    pub gsi_image: &'a Path,
    pub output_directory: &'a Path,
    pub rollback_evidence: &'a [RollbackEvidence],
}
