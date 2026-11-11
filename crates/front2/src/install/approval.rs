use crate::error::{Front2Error, Result};

pub fn require_plan_binding(kind: &str, approved_sha256: &str, plan_sha256: &str) -> Result<()> {
    if approved_sha256 != plan_sha256 {
        return Err(Front2Error::Plan(format!(
            "the {kind} approval is for plan {approved_sha256}, this plan is {plan_sha256}"
        )));
    }
    Ok(())
}
