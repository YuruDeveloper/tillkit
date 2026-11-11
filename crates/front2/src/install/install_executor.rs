use crate::error::{Front2Error, Result};
use crate::install::boot_lun::select_boot_lun;
use crate::install::data_wipe_approval::DataWipeApproval;
use crate::install::install_progress::InstallProgress;
use crate::install::install_step::InstallStep;
use crate::install::layout::BOOT_LUN_STEP_NAME;
use crate::install::preflight::run_preflight;
use crate::install::restart_approval::RestartApproval;
use crate::install::sector_storage::SectorStorage;
use crate::install::sector_transfer::write_and_read_back;
use crate::install::stored_install_plan::StoredInstallPlan;
use crate::install::write_approval::WriteApproval;
use crate::install::write_step::WriteStep;

pub struct InstallExecutor<'run> {
    plan: &'run StoredInstallPlan,
    storage: &'run mut dyn SectorStorage,
    progress: InstallProgress,
}

impl<'run> InstallExecutor<'run> {
    pub fn new(plan: &'run StoredInstallPlan, storage: &'run mut dyn SectorStorage) -> Self {
        Self {
            plan,
            storage,
            progress: InstallProgress::new(plan.plan_sha256()),
        }
    }

    pub fn progress(&self) -> &InstallProgress {
        &self.progress
    }

    pub fn run_preflight(&mut self) -> Result<()> {
        self.progress.require_open()?;
        if self.progress.preflight.is_some() {
            return Err(Front2Error::Plan(
                "the preflight already ran for this install".into(),
            ));
        }
        let (record, outcome) = run_preflight(self.plan, &mut *self.storage);
        self.progress.preflight = Some(record);
        outcome.map_err(|err| self.progress.halt(err))
    }

    pub fn run_write_steps(&mut self, write: &WriteApproval) -> Result<()> {
        self.require_writes_open(write)?;
        if !self.next_step_needs_only_write_approval() {
            return Err(Front2Error::Plan(
                "no step that needs only the write approval is left".into(),
            ));
        }
        while self.next_step_needs_only_write_approval() {
            self.run_next_step()?;
        }
        Ok(())
    }

    pub fn run_data_wipe_step(
        &mut self,
        write: &WriteApproval,
        data_wipe: &DataWipeApproval,
    ) -> Result<()> {
        self.require_writes_open(write)?;
        data_wipe.require(self.plan.plan_sha256())?;
        if !self.next_step_is_data_wipe() {
            return Err(Front2Error::Plan(
                "the user data wipe request is not the next step".into(),
            ));
        }
        self.run_next_step()
    }

    pub fn request_restart(&mut self, restart: &RestartApproval) -> Result<()> {
        self.progress.require_open()?;
        restart.require(self.plan.plan_sha256())?;
        self.progress.require_preflight_passed()?;
        self.require_ready_for_restart()?;
        match self.storage.reset() {
            Ok(()) => self.progress.restart_sent = true,
            Err(err) => return Err(self.progress.halt(err)),
        }
        Ok(())
    }

    fn require_writes_open(&self, write: &WriteApproval) -> Result<()> {
        self.progress.require_open()?;
        write.require(self.plan.plan_sha256())?;
        self.progress.require_preflight_passed()
    }

    fn require_ready_for_restart(&self) -> Result<()> {
        let plan = self.plan.plan();
        let only_restart_left = self.progress.verified_steps() + 1 == plan.steps.len()
            && matches!(self.next_step(), Some(InstallStep::Restart));
        let every_write_verified = plan
            .write_steps()
            .into_iter()
            .all(|(i, step)| self.progress.write_verified(i, step));
        if !(only_restart_left && every_write_verified && self.progress.boot_lun_passed()) {
            return Err(Front2Error::Plan(
                "the restart needs a matching readback of every write step and a passed boot LUN check in the progress record".into(),
            ));
        }
        Ok(())
    }

    fn next_step(&self) -> Option<&'run InstallStep> {
        let plan: &'run StoredInstallPlan = self.plan;
        plan.plan().steps.get(self.progress.verified_steps())
    }

    fn next_step_needs_only_write_approval(&self) -> bool {
        let Some(step) = self.next_step() else {
            return false;
        };
        match step.as_write() {
            Some(write) => !write.requires_data_wipe_approval,
            None => step.boot_lun().is_some(),
        }
    }

    fn next_step_is_data_wipe(&self) -> bool {
        self.next_step()
            .and_then(InstallStep::as_write)
            .is_some_and(|write| write.requires_data_wipe_approval)
    }

    fn run_next_step(&mut self) -> Result<()> {
        let index = self.progress.verified_steps();
        let Some(step) = self.next_step() else {
            return Err(Front2Error::Plan("the plan holds no further step".into()));
        };
        match (step.as_write(), step.boot_lun()) {
            (Some(write), _) => self.run_write(index, write),
            (None, Some(lun)) => self.run_boot_lun(index, lun),
            (None, None) => Err(Front2Error::Plan(
                "the next step is the restart, it needs the restart approval".into(),
            )),
        }
    }

    fn run_write(&mut self, index: usize, step: &WriteStep) -> Result<()> {
        self.progress.start_step(index, &step.name);
        match write_and_read_back(self.plan, &mut *self.storage, step) {
            Ok(readback) => self.progress.verify_step(Some(readback)),
            Err(err) => return Err(self.progress.fail_step(err)),
        }
        Ok(())
    }

    fn run_boot_lun(&mut self, index: usize, lun: u8) -> Result<()> {
        self.progress.start_step(index, BOOT_LUN_STEP_NAME);
        let (record, outcome) = select_boot_lun(self.plan.plan(), &mut *self.storage, lun);
        self.progress.boot_lun = Some(record);
        match outcome {
            Ok(()) => self.progress.verify_step(None),
            Err(err) => return Err(self.progress.fail_step(err)),
        }
        Ok(())
    }
}
