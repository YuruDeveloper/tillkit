use crate::root_review::RootReview;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum RootMessage {
    PickPackage,
    PackagePicked(Option<PathBuf>),
    UseLatest,
    Start,
    Prepared(Result<Box<RootReview>, String>),
}
