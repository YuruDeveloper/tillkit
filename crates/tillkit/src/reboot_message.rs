use crate::reboot::{DeviceConnection, RebootTarget};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum RebootMessage {
    Detect,
    Detected(Result<DeviceConnection, String>),
    PickLoader,
    LoaderPicked(Option<PathBuf>),
    Ask(RebootTarget),
    Sent(Result<RebootTarget, String>),
}
