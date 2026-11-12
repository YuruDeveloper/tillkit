use crate::install::write_step::WriteStep;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepJudgment {
    Done,
    NotYet,
    Neutral,
    UnknownContent,
}

impl StepJudgment {
    pub fn of_write(current_sha256: &str, step: &WriteStep) -> Self {
        if step.sha256 == step.expected_before_sha256 {
            return Self::of_unchanged_content(current_sha256, step);
        }
        if current_sha256 == step.sha256 {
            Self::Done
        } else if current_sha256 == step.expected_before_sha256 {
            Self::NotYet
        } else {
            Self::UnknownContent
        }
    }

    pub fn needs_work(self) -> bool {
        matches!(self, Self::NotYet | Self::UnknownContent)
    }

    pub fn may_follow_the_resume_step(self) -> bool {
        matches!(self, Self::NotYet | Self::Neutral)
    }

    pub fn describe(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::NotYet => "not done yet",
            Self::Neutral => "the same in the backup and in the plan",
            Self::UnknownContent => "of unknown content",
        }
    }

    fn of_unchanged_content(current_sha256: &str, step: &WriteStep) -> Self {
        if current_sha256 == step.sha256 {
            Self::Neutral
        } else {
            Self::UnknownContent
        }
    }
}
