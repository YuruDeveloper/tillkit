use front2::kernel::KernelSet;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum KernelMessage {
    Select(String),
    ReadCurrent,
    PickBootImage,
    BootImagePicked(Option<PathBuf>),
    Stored(Result<Box<KernelSet>, String>),
    PlanWrite,
}
