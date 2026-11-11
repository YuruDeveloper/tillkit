use crate::install::step_state::StepState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepRecord {
    pub index: usize,
    pub name: String,
    pub state: StepState,
    pub readback_sha256: Option<String>,
    pub failure: Option<String>,
}
