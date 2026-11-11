use crate::plan_review::PlanReview;
use front2::magisk::MagiskPackage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootReview {
    pub review: PlanReview,
    pub package: MagiskPackage,
}
