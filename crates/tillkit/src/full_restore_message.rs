use crate::full_restore_review::FullRestoreReview;

type Outcome<T> = Result<T, String>;

#[derive(Debug, Clone)]
pub enum FullRestoreMessage {
    Open,
    Close,
    ToggleUserData,
    Check,
    Checked(Outcome<Box<FullRestoreReview>>),
    AskRestore,
    Restored(Outcome<()>),
    AskRestart,
    Restarted(Outcome<()>),
}
