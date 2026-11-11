use crate::diagnosis::Diagnosis;
use crate::screen::Screen;
use crate::write_message::WriteMessage;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Screen),
    ToggleTheme,
    ReadDevice,
    DeviceRead(Result<Diagnosis, String>),
    ToggleDetails,
    AskForce,
    Confirm,
    Cancel,
    BackUp,
    BackedUp(Result<(), String>),
    PickFullBackup,
    FullBackupPicked(Option<PathBuf>),
    Write(WriteMessage),
}
