use crate::diagnosis::Diagnosis;
use crate::install_message::InstallMessage;
use crate::reboot_message::RebootMessage;
use crate::root_message::RootMessage;
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
    PickDataFolder,
    DataFolderPicked(Option<PathBuf>),
    PickFullBackup,
    FullBackupPicked(Option<PathBuf>),
    RestoreBackup(u64),
    Write(WriteMessage),
    Root(RootMessage),
    Reboot(RebootMessage),
    Install(InstallMessage),
}
