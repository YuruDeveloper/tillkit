use crate::install_review::InstallReview;
use front2::install::InstallProgress;
use std::path::PathBuf;

type Outcome<T> = Result<T, String>;

#[derive(Debug, Clone)]
pub enum InstallMessage {
    Connect,
    Connected(Outcome<()>),
    BackUp,
    BackedUp(Outcome<(PathBuf, Vec<String>)>),
    Plan,
    Planned(Outcome<Box<InstallReview>>),
    Check,
    Checked(Outcome<Box<InstallProgress>>),
    AskWrite,
    Written(Outcome<Box<InstallProgress>>),
    Wiped(Outcome<Box<InstallProgress>>),
    AskRestart,
    Restarted(Outcome<Box<InstallProgress>>),
    CheckAgain,
    Reconnect,
    StartOver,
    Tick,
}
