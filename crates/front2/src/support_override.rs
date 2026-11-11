use crate::error::{Front2Error, Result};
use crate::support_check::SupportCheck;
use crate::support_report::SupportReport;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportOverride {
    pub accepted: Vec<SupportCheck>,
}

impl SupportOverride {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn accept_unmet(report: &SupportReport) -> Result<Self> {
        let accepted: Vec<SupportCheck> = report.unmet().into_iter().cloned().collect();
        let instance = Self { accepted };
        instance.require_acceptable()?;
        Ok(instance)
    }

    pub fn is_empty(&self) -> bool {
        self.accepted.is_empty()
    }

    pub fn accepts(&self, check: &SupportCheck) -> bool {
        self.accepted.contains(check)
    }

    pub fn require_acceptable(&self) -> Result<()> {
        match self.accepted.iter().find(|check| !check.can_be_accepted()) {
            None => Ok(()),
            Some(check) => Err(Front2Error::Unsupported(format!(
                "{} is {:?}, expected {}; no approval can accept this difference",
                check.id, check.observed, check.expected
            ))),
        }
    }
}
