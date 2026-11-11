use crate::error::Result;
use crate::install::approval::require_plan_binding;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestartApproval {
    pub plan_sha256: String,
    pub granted_unix: u64,
}

impl RestartApproval {
    pub fn require(&self, plan_sha256: &str) -> Result<()> {
        require_plan_binding("restart", &self.plan_sha256, plan_sha256)
    }
}
