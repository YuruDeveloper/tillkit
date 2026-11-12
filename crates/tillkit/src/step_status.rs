#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Waiting,
    Running,
    Done,
    Failed,
}

impl StepStatus {
    pub fn word(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }
}
