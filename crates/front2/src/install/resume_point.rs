use crate::error::{Front2Error, Result};
use crate::install::step_check::StepCheck;
use crate::install::step_judgment::StepJudgment;
use serde::{Deserialize, Serialize};

const RESTART_STEP_NAME: &str = "restart";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumePoint {
    pub first_install: bool,
    pub next_index: usize,
    pub next_name: String,
    pub unknown_content: Option<String>,
}

impl ResumePoint {
    pub fn judge(checks: &[StepCheck], restart_index: usize) -> Result<Self> {
        let first_install = checks
            .iter()
            .all(|check| check.judgment.may_follow_the_resume_step());
        let position = checks.iter().position(|check| check.judgment.needs_work());
        let Some(position) = position else {
            return Ok(Self::at_restart(first_install, restart_index));
        };
        let start = &checks[position];
        if let Some(later) = checks[position + 1..]
            .iter()
            .find(|check| !check.judgment.may_follow_the_resume_step())
        {
            return Err(Front2Error::Verification(format!(
                "preflight: the step {} is {}, but the step {} before it is {}; an interrupted install of this plan does not leave the device in this state",
                later.name,
                later.judgment.describe(),
                start.name,
                start.judgment.describe()
            )));
        }
        Ok(Self {
            first_install,
            next_index: start.index,
            next_name: start.name.clone(),
            unknown_content: (start.judgment == StepJudgment::UnknownContent)
                .then(|| start.name.clone()),
        })
    }

    fn at_restart(first_install: bool, restart_index: usize) -> Self {
        Self {
            first_install,
            next_index: restart_index,
            next_name: RESTART_STEP_NAME.to_string(),
            unknown_content: None,
        }
    }
}
