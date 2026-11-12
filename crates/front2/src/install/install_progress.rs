use crate::error::{Front2Error, Result};
use crate::install::boot_lun_record::BootLunRecord;
use crate::install::install_plan::InstallPlan;
use crate::install::install_step::InstallStep;
use crate::install::preflight_record::PreflightRecord;
use crate::install::step_check::StepCheck;
use crate::install::step_record::StepRecord;
use crate::install::step_state::StepState;
use crate::install::write_step::WriteStep;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallProgress {
    pub plan_sha256: String,
    pub preflight: Option<PreflightRecord>,
    pub steps: Vec<StepRecord>,
    pub boot_lun: Option<BootLunRecord>,
    pub halted: Option<String>,
    pub restart_sent: bool,
}

impl InstallProgress {
    pub fn new(plan_sha256: &str) -> Self {
        Self {
            plan_sha256: plan_sha256.to_string(),
            preflight: None,
            steps: Vec::new(),
            boot_lun: None,
            halted: None,
            restart_sent: false,
        }
    }

    pub fn verified_steps(&self) -> usize {
        self.steps
            .iter()
            .filter(|record| record.state == StepState::Verified)
            .count()
    }

    pub fn write_verified(&self, index: usize, step: &WriteStep) -> bool {
        self.steps.iter().any(|record| {
            record.index == index
                && record.name == step.name
                && record.state == StepState::Verified
                && record.readback_sha256.as_deref() == Some(step.sha256.as_str())
        })
    }

    pub fn boot_lun_passed(&self) -> bool {
        self.boot_lun.as_ref().is_some_and(|record| record.passed)
    }

    pub fn require_open(&self) -> Result<()> {
        if let Some(reason) = &self.halted {
            return Err(Front2Error::Plan(format!(
                "the install stopped and takes no more commands: {reason}"
            )));
        }
        if self.restart_sent {
            return Err(Front2Error::Plan(
                "the restart was sent, the install takes no more commands".into(),
            ));
        }
        Ok(())
    }

    pub fn require_preflight_passed(&self) -> Result<()> {
        let passed = self
            .preflight
            .as_ref()
            .is_some_and(|record| record.passed && record.plan_sha256 == self.plan_sha256);
        if !passed {
            return Err(Front2Error::Plan(
                "the progress record holds no passed preflight for this plan".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn take_steps_before_the_resume_point(
        &mut self,
        plan: &InstallPlan,
        record: &PreflightRecord,
    ) {
        let Some(resume) = &record.resume else {
            return;
        };
        for check in record
            .steps
            .iter()
            .filter(|check| check.index < resume.next_index)
        {
            self.take_checked_step(plan, check);
        }
    }

    fn take_checked_step(&mut self, plan: &InstallPlan, check: &StepCheck) {
        self.start_step(check.index, &check.name);
        let boot_lun = plan.steps.get(check.index).and_then(InstallStep::boot_lun);
        let Some(lun) = boot_lun else {
            self.verify_step(Some(check.sha256.clone()));
            return;
        };
        self.boot_lun = Some(BootLunRecord::observed(lun, plan.boot_wlun, &check.sha256));
        self.verify_step(None);
    }

    pub(crate) fn start_step(&mut self, index: usize, name: &str) {
        self.steps.push(StepRecord {
            index,
            name: name.to_string(),
            state: StepState::Started,
            readback_sha256: None,
            failure: None,
        });
    }

    pub(crate) fn verify_step(&mut self, readback_sha256: Option<String>) {
        if let Some(record) = self.steps.last_mut() {
            record.state = StepState::Verified;
            record.readback_sha256 = readback_sha256;
        }
    }

    pub(crate) fn fail_step(&mut self, err: Front2Error) -> Front2Error {
        if let Some(record) = self.steps.last_mut() {
            record.state = StepState::Failed;
            record.failure = Some(err.to_string());
        }
        self.halt(err)
    }

    pub(crate) fn halt(&mut self, err: Front2Error) -> Front2Error {
        self.halted = Some(err.to_string());
        err
    }
}
