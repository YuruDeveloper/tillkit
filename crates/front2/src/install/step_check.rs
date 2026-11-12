use crate::install::step_judgment::StepJudgment;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepCheck {
    pub index: usize,
    pub name: String,
    pub sha256: String,
    pub judgment: StepJudgment,
}
