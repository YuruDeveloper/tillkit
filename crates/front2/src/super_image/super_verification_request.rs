use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct SuperVerificationRequest<'a> {
    pub candidate: &'a Path,
    pub original_super: &'a Path,
    pub gsi_bytes: u64,
    pub gsi_sha256: &'a str,
}
