use crate::install_review::InstallReview;
use crate::plan_review::PlanReview;
use front2::ApplyOutcome;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum WriteMessage {
    PickFolder,
    FolderPicked(Option<PathBuf>),
    Start,
    Prepared(Result<Box<PlanReview>, String>),
    InstallPlanned(Result<Box<InstallReview>, String>),
    Staged(Result<(), String>),
    Written(Result<ApplyOutcome, String>),
    AskReboot,
    Rebooted(Result<ApplyOutcome, String>),
    StartOver,
}
