use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct SuperCandidateRequest<'a> {
    pub original_super: &'a Path,
    pub original_super_sha256: &'a str,
    pub gsi_image: &'a Path,
    pub gsi_sha256: &'a str,
    pub output: &'a Path,
}
