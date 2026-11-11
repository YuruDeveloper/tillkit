use crate::diagnosis::Diagnosis;
use crate::gsi_message::GsiMessage;
use crate::kernel_message::KernelMessage;
use crate::plan_review::PlanReview;
use crate::screen::Screen;
use front2::{ApplyOutcome, PairManifest};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Screen),
    ToggleTheme,
    ReadDevice,
    DeviceRead(Result<Diagnosis, String>),
    SelectPair(String),
    NoteChanged(String),
    ReadCurrentPair,
    PickPatchedImage,
    PatchedImagePicked(Option<PathBuf>),
    PairStored(Result<PairManifest, String>),
    PlanWrite,
    PlanReady(Result<PlanReview, String>),
    StagePlan,
    PlanStaged(Result<(), String>),
    ApprovalChanged(String),
    WritePair,
    PairWritten(Result<ApplyOutcome, String>),
    RebootChanged(String),
    Reboot,
    Rebooted(Result<ApplyOutcome, String>),
    DiscardPlan,
    Gsi(GsiMessage),
    Kernel(KernelMessage),
}
