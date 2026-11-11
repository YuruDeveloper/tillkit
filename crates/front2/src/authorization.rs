use crate::error::{Front2Error, Result};
use crate::scope::Scope;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Authorization {
    pub plan_sha256: String,
    pub scopes: Vec<Scope>,
    pub granted_unix: u64,
}

impl Authorization {
    pub fn require(&self, plan_sha256: &str, scope: Scope) -> Result<()> {
        if self.plan_sha256 != plan_sha256 {
            return Err(Front2Error::Plan(format!(
                "the approval is for plan {}, this plan is {plan_sha256}",
                self.plan_sha256
            )));
        }
        if !self.scopes.contains(&scope) {
            return Err(Front2Error::Plan(format!(
                "the approval does not include {scope:?}"
            )));
        }
        Ok(())
    }
}
