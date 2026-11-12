use crate::install_review::InstallReview;
use crate::install_stage::InstallStage;
use crate::step_status::StepStatus;
use front2::install::{
    DataWipeApproval, InstallProgress, InstallStep, RestartApproval, WriteApproval,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct InstallFlow {
    pub connected: bool,
    pub backup: Option<PathBuf>,
    pub refused_backup: Option<String>,
    pub review: Option<InstallReview>,
    pub progress: Option<InstallProgress>,
    pub approved_sha256: Option<String>,
    pub single_read: Vec<String>,
    pub transferred: Arc<AtomicU64>,
    pub expected: Arc<AtomicU64>,
    pub running: Option<InstallStage>,
    pub failure: Option<(InstallStage, String)>,
}

impl InstallFlow {
    pub fn has_plan(&self) -> bool {
        self.review.is_some() || self.progress.is_some()
    }

    pub fn is_finished(&self) -> bool {
        self.progress
            .as_ref()
            .is_some_and(|progress| progress.restart_sent)
    }

    pub fn holds_the_device(&self) -> bool {
        self.connected && self.progress.is_some() && !self.is_finished()
    }

    pub fn begin(&mut self, stage: InstallStage) {
        self.failure = None;
        self.transferred.store(0, Ordering::Relaxed);
        let expected = self.write_bytes().filter(|_| stage == InstallStage::Write);
        self.expected
            .store(expected.unwrap_or(0), Ordering::Relaxed);
        self.running = Some(stage);
    }

    pub fn stage_failed(&mut self, stage: InstallStage, error: String) {
        self.running = None;
        self.failure = Some((stage, error));
    }

    pub fn disconnect(&mut self) {
        self.connected = false;
        self.forget_the_check();
    }

    pub fn forget_the_check(&mut self) {
        self.progress = None;
        self.failure = None;
        self.approved_sha256 = None;
    }

    pub fn forget_the_plan(&mut self) {
        self.review = None;
        self.forget_the_check();
    }

    pub fn use_backup(&mut self, folder: PathBuf) {
        self.use_new_backup(folder, Vec::new());
    }

    pub fn use_new_backup(&mut self, folder: PathBuf, single_read: Vec<String>) {
        self.backup = Some(folder);
        self.single_read = single_read;
        self.refused_backup = None;
        self.forget_the_plan();
    }

    pub fn approve(&mut self) -> bool {
        let open = matches!(
            self.next_stage(),
            Some(InstallStage::Write | InstallStage::Wipe)
        );
        let sha256 = self.review.as_ref().map(|review| review.sha256.clone());
        if open {
            self.approved_sha256 = sha256;
        }
        open && self.approved_sha256.is_some()
    }

    pub fn transferred_bytes(&self) -> u64 {
        self.transferred.load(Ordering::Relaxed)
    }

    pub fn expected_bytes(&self) -> Option<u64> {
        Some(self.expected.load(Ordering::Relaxed)).filter(|bytes| *bytes > 0)
    }

    pub fn write_bytes(&self) -> Option<u64> {
        let plan = &self.review.as_ref()?.plan;
        let sectors: u64 = plan
            .write_steps()
            .into_iter()
            .filter(|(_, step)| !step.requires_data_wipe_approval)
            .map(|(_, step)| step.sectors)
            .sum();
        sectors.checked_mul(plan.sector_bytes)?.checked_mul(2)
    }

    pub fn needs_a_new_check(&self) -> bool {
        let stopped_late = matches!(
            &self.failure,
            Some((stage, _)) if *stage == InstallStage::Check || stage.changes_the_device()
        );
        stopped_late && self.running.is_none()
    }

    fn verified_steps(&self) -> usize {
        self.progress
            .as_ref()
            .map_or(0, InstallProgress::verified_steps)
    }

    fn wipe_position(&self) -> Option<usize> {
        let steps = &self.review.as_ref()?.plan.steps;
        steps.iter().position(|step| {
            step.as_write()
                .is_some_and(|write| write.requires_data_wipe_approval)
        })
    }

    fn is_checked(&self) -> bool {
        let sha256 = self.review.as_ref().map(|review| review.sha256.as_str());
        self.progress.as_ref().is_some_and(|progress| {
            Some(progress.plan_sha256.as_str()) == sha256
                && progress
                    .preflight
                    .as_ref()
                    .is_some_and(|record| record.passed)
        })
    }

    fn restart_is_next(&self) -> bool {
        let steps = self.review.as_ref().map(|review| &review.plan.steps);
        let next = steps.and_then(|steps| steps.get(self.verified_steps()));
        matches!(next, Some(InstallStep::Restart))
    }

    fn is_done(&self, stage: InstallStage) -> bool {
        let wipe = self.wipe_position().filter(|_| self.is_checked());
        match stage {
            InstallStage::Connect => self.connected || self.is_finished(),
            InstallStage::Backup => self.backup.is_some(),
            InstallStage::Plan => self.review.is_some(),
            InstallStage::Check => self.is_checked(),
            InstallStage::Write => wipe.is_some_and(|wipe| self.verified_steps() >= wipe),
            InstallStage::Wipe => wipe.is_some_and(|wipe| self.verified_steps() > wipe),
            InstallStage::Restart => self
                .progress
                .as_ref()
                .is_some_and(|progress| progress.restart_sent),
        }
    }

    pub fn next_stage(&self) -> Option<InstallStage> {
        if self.running.is_some() || self.needs_a_new_check() || self.is_finished() {
            return None;
        }
        InstallStage::ALL
            .into_iter()
            .find(|stage| !self.is_done(*stage))
    }

    pub fn focus_stage(&self) -> InstallStage {
        let failed = self.failure.as_ref().map(|(stage, _)| *stage);
        self.running
            .or(failed)
            .or(self.next_stage())
            .unwrap_or(InstallStage::Restart)
    }

    pub fn status(&self, stage: InstallStage) -> StepStatus {
        let failed_here = matches!(&self.failure, Some((failed, _)) if *failed == stage);
        if self.running == Some(stage) {
            StepStatus::Running
        } else if failed_here {
            StepStatus::Failed
        } else if self.is_done(stage) {
            StepStatus::Done
        } else {
            StepStatus::Waiting
        }
    }

    pub fn write_approvals(&self, granted_unix: u64) -> Option<(WriteApproval, DataWipeApproval)> {
        let review = self.review.as_ref()?;
        let open = matches!(
            self.next_stage(),
            Some(InstallStage::Write | InstallStage::Wipe)
        );
        let approved = self.approved_sha256.as_deref() == Some(review.sha256.as_str());
        if !(open && approved) {
            return None;
        }
        let plan_sha256 = review.sha256.clone();
        let write = WriteApproval {
            plan_sha256: plan_sha256.clone(),
            granted_unix,
        };
        let data_wipe = DataWipeApproval {
            plan_sha256,
            granted_unix,
        };
        Some((write, data_wipe))
    }

    pub fn restart_approval(&self, granted_unix: u64) -> Option<RestartApproval> {
        let review = self.review.as_ref()?;
        let open = self.next_stage() == Some(InstallStage::Restart) && self.restart_is_next();
        open.then(|| RestartApproval {
            plan_sha256: review.sha256.clone(),
            granted_unix,
        })
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::edl_link::EdlLink;
    use crate::install_support::{InstallFixture, MockStorage};
    use crate::install_worker;

    pub fn planned() -> (InstallFixture, InstallFlow) {
        let fixture = InstallFixture::standard();
        let stored = fixture.stored_plan();
        let review = InstallReview {
            plan: stored.plan().clone(),
            sha256: stored.plan_sha256().to_string(),
            directory: fixture.plan_directory(),
        };
        let flow = InstallFlow {
            connected: true,
            backup: Some(fixture.inputs.backup_directory.clone()),
            review: Some(review),
            ..InstallFlow::default()
        };
        (fixture, flow)
    }

    fn link_of(fixture: InstallFixture) -> (tempfile::TempDir, EdlLink<MockStorage>) {
        let InstallFixture {
            directory, storage, ..
        } = fixture;
        (directory, EdlLink::new(storage))
    }

    pub fn checked() -> (tempfile::TempDir, InstallFlow) {
        let (fixture, mut flow) = planned();
        let directory = fixture.plan_directory();
        let (kept, mut link) = link_of(fixture);
        flow.progress = Some(install_worker::check(&mut link, &directory).expect("check"));
        (kept, flow)
    }

    #[test]
    fn doc_1053_a_new_full_backup_drops_the_plan_and_the_check() {
        let (_kept, mut flow) = checked();
        assert!(flow.approve());
        flow.refused_backup = Some("no manifest".into());
        flow.use_backup(PathBuf::from("other"));
        assert_eq!(flow.backup, Some(PathBuf::from("other")));
        assert!(flow.review.is_none() && flow.progress.is_none());
        assert!(flow.approved_sha256.is_none() && flow.refused_backup.is_none());
        assert_eq!(flow.next_stage(), Some(InstallStage::Plan));
    }

    #[test]
    fn the_write_stage_tells_the_bytes_that_it_sends_and_reads_back() {
        let (_kept, mut flow) = checked();
        let plan = &flow.review.as_ref().expect("plan").plan;
        let sectors: u64 = plan
            .write_steps()
            .into_iter()
            .filter(|(_, step)| step.name != "misc")
            .map(|(_, step)| step.sectors)
            .sum();
        let expected = sectors * plan.sector_bytes * 2;
        assert_eq!(flow.write_bytes(), Some(expected));
        flow.begin(InstallStage::Check);
        assert_eq!(flow.expected_bytes(), None);
        flow.running = None;
        flow.begin(InstallStage::Write);
        assert_eq!(flow.expected_bytes(), Some(expected));
    }

    #[test]
    fn the_first_stages_open_one_at_a_time() {
        let mut flow = InstallFlow::default();
        assert_eq!(flow.next_stage(), Some(InstallStage::Connect));
        assert_eq!(flow.status(InstallStage::Backup), StepStatus::Waiting);
        flow.connected = true;
        assert_eq!(flow.next_stage(), Some(InstallStage::Backup));
        flow.backup = Some(PathBuf::from("backup"));
        assert_eq!(flow.next_stage(), Some(InstallStage::Plan));
        assert!(flow.write_approvals(1).is_none());
        assert!(flow.restart_approval(1).is_none());
        assert!(!flow.has_plan());
    }

    #[test]
    fn the_approvals_open_in_the_order_of_the_engine() {
        let (fixture, mut flow) = planned();
        let directory = fixture.plan_directory();
        let (_directory, mut link) = link_of(fixture);
        assert_eq!(flow.next_stage(), Some(InstallStage::Check));
        assert!(flow.write_approvals(1).is_none());

        flow.progress = Some(install_worker::check(&mut link, &directory).expect("check"));
        assert!(flow.has_plan() && flow.holds_the_device());
        assert_eq!(flow.next_stage(), Some(InstallStage::Write));
        assert!(flow.restart_approval(2).is_none());
        assert!(flow.write_approvals(2).is_none(), "no approval of the user");
        assert!(flow.approve());
        let (write, wipe) = flow.write_approvals(2).expect("the write is next");
        assert_eq!(
            write.plan_sha256,
            flow.review.as_ref().expect("plan").sha256
        );

        flow.progress = Some(install_worker::write(&mut link, &write, &wipe).expect("write"));
        assert_eq!(flow.status(InstallStage::Write), StepStatus::Done);
        assert_eq!(flow.next_stage(), Some(InstallStage::Wipe));
        assert!(flow.restart_approval(3).is_none());

        flow.progress = Some(install_worker::wipe(&mut link, &write, &wipe).expect("wipe"));
        assert_eq!(flow.next_stage(), Some(InstallStage::Restart));
        assert!(flow.write_approvals(4).is_none());
        let restart = flow.restart_approval(4).expect("the restart is next");

        flow.progress = Some(install_worker::restart(&mut link, &restart).expect("restart"));
        assert_eq!(flow.next_stage(), None);
        assert_eq!(flow.status(InstallStage::Restart), StepStatus::Done);
        assert!(flow.restart_approval(5).is_none());
        assert!(flow.is_finished() && !flow.holds_the_device());
        assert_eq!(flow.status(InstallStage::Connect), StepStatus::Done);
    }

    #[test]
    fn a_stage_that_stopped_early_can_run_again() {
        let mut flow = InstallFlow::default();
        flow.begin(InstallStage::Connect);
        assert_eq!(flow.next_stage(), None);
        flow.stage_failed(InstallStage::Connect, "no device".into());
        assert_eq!(flow.status(InstallStage::Connect), StepStatus::Failed);
        assert!(!flow.needs_a_new_check());
        assert_eq!(flow.next_stage(), Some(InstallStage::Connect));
        flow.begin(InstallStage::Connect);
        assert!(flow.failure.is_none());
    }

    #[test]
    fn a_stage_that_stopped_late_needs_a_new_check() {
        let (fixture, mut flow) = planned();
        let directory = fixture.plan_directory();
        let (_directory, mut link) = link_of(fixture);
        flow.progress = Some(install_worker::check(&mut link, &directory).expect("check"));
        flow.begin(InstallStage::Write);
        flow.stage_failed(InstallStage::Write, "the cable came out".into());

        assert!(flow.needs_a_new_check());
        assert_eq!(flow.next_stage(), None);
        assert!(flow.write_approvals(1).is_none());
        assert_eq!(flow.focus_stage(), InstallStage::Write);

        flow.forget_the_check();
        assert_eq!(flow.next_stage(), Some(InstallStage::Check));
        assert!(flow.backup.is_some() && flow.review.is_some());
    }

    #[test]
    fn a_check_of_another_plan_opens_no_approval() {
        let (fixture, mut flow) = planned();
        let directory = fixture.plan_directory();
        let (_directory, mut link) = link_of(fixture);
        flow.progress = Some(install_worker::check(&mut link, &directory).expect("check"));
        flow.review.as_mut().expect("plan").sha256 = "f".repeat(64);

        assert_eq!(flow.next_stage(), Some(InstallStage::Check));
        assert!(flow.write_approvals(1).is_none());
    }

    #[test]
    fn a_lost_connection_drops_the_check_and_keeps_the_backup_and_the_plan() {
        let (fixture, mut flow) = planned();
        let directory = fixture.plan_directory();
        let (_directory, mut link) = link_of(fixture);
        flow.progress = Some(install_worker::check(&mut link, &directory).expect("check"));
        flow.disconnect();
        assert_eq!(flow.next_stage(), Some(InstallStage::Connect));
        assert!(flow.progress.is_none() && !flow.holds_the_device());
        assert!(flow.backup.is_some() && flow.review.is_some());
    }
}
