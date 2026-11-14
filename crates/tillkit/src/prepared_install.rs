use crate::install_review::InstallReview;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreparedInstall {
    Planned(Box<InstallReview>),
    UntestedBuild(String),
}
