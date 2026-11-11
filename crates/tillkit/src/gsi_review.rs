use front2::gsi::GsiCandidate;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GsiReview {
    pub candidate: GsiCandidate,
    pub sha256: String,
    pub directory: PathBuf,
}
