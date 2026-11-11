use crate::plan_review::PlanReview;
use crate::step_status::StepStatus;
use crate::write_step::WriteStep;
use front2::{ApplyOutcome, ApplyState, Authorization, Scope};

pub const REBOOT_WORD: &str = "reboot";

#[derive(Debug, Default)]
pub struct WriteFlow {
    pub review: Option<PlanReview>,
    pub staged: bool,
    pub approval_input: String,
    pub outcome: Option<ApplyOutcome>,
    pub reboot_input: String,
    pub rebooted: bool,
    pub running: Option<WriteStep>,
    pub failure: Option<(WriteStep, String)>,
}

impl WriteFlow {
    pub fn write_verified(&self) -> bool {
        self.outcome.as_ref().is_some_and(|outcome| {
            outcome.state == ApplyState::VerifiedPendingReboot && outcome.readback_matches_new_pair
        })
    }

    fn is_done(&self, step: WriteStep) -> bool {
        match step {
            WriteStep::Plan => self.review.is_some(),
            WriteStep::Stage => self.staged,
            WriteStep::Write => self.write_verified(),
            WriteStep::Reboot => self.rebooted,
        }
    }

    fn is_stopped(&self) -> bool {
        let plan_failed = matches!(self.failure, Some((WriteStep::Plan, _)));
        let failed_after_plan = self.failure.is_some() && !plan_failed;
        failed_after_plan || (self.outcome.is_some() && !self.write_verified())
    }

    pub fn next_step(&self) -> Option<WriteStep> {
        if self.running.is_some() || self.is_stopped() {
            return None;
        }
        WriteStep::ALL.into_iter().find(|step| !self.is_done(*step))
    }

    pub fn focus_step(&self) -> WriteStep {
        let failed = self.failure.as_ref().map(|(step, _)| *step);
        let bad_write =
            (self.outcome.is_some() && !self.write_verified()).then_some(WriteStep::Write);
        self.running
            .or(self.next_step())
            .or(failed)
            .or(bad_write)
            .unwrap_or(WriteStep::Reboot)
    }

    pub fn status(&self, step: WriteStep) -> StepStatus {
        let failed_here = matches!(&self.failure, Some((failed, _)) if *failed == step);
        let bad_write =
            step == WriteStep::Write && self.outcome.is_some() && !self.write_verified();
        if self.running == Some(step) {
            StepStatus::Running
        } else if self.is_done(step) {
            StepStatus::Done
        } else if failed_here || bad_write {
            StepStatus::Failed
        } else if self.next_step() == Some(step) {
            StepStatus::Ready
        } else {
            StepStatus::Waiting
        }
    }

    pub fn write_authorization(&self, granted_unix: u64) -> Option<Authorization> {
        let review = self.review.as_ref()?;
        let typed = self.approval_input.trim();
        let approved = self.next_step() == Some(WriteStep::Write)
            && !typed.is_empty()
            && typed == review.approval_code();
        approved.then(|| Authorization {
            plan_sha256: review.sha256.clone(),
            scopes: vec![Scope::WritePair],
            granted_unix,
        })
    }

    pub fn reboot_authorization(&self, granted_unix: u64) -> Option<Authorization> {
        let review = self.review.as_ref()?;
        let approved =
            self.next_step() == Some(WriteStep::Reboot) && self.reboot_input.trim() == REBOOT_WORD;
        approved.then(|| Authorization {
            plan_sha256: review.sha256.clone(),
            scopes: vec![Scope::Reboot],
            granted_unix,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan_source::PlanSource;
    use front2::{RootAccess, WritePlan, WritePurpose};
    use std::path::PathBuf;

    fn plan() -> WritePlan {
        WritePlan {
            schema: 1,
            id: "plan-1".into(),
            created_unix: 1,
            purpose: WritePurpose::ApplyCandidate,
            new_pair: "new".into(),
            old_pair: "old".into(),
            slot_suffix: "_b".into(),
            device_state: "locked".into(),
            verified_boot_state: "green".into(),
            root_access: RootAccess::AdbRoot,
            remote_stage: "/data/local/tmp/front2-write-plan-1".into(),
            writes: Vec::new(),
            top_rollback_index: 0,
            script_sha256: String::new(),
            wipes_user_data: false,
            changes_active_slot: false,
            changes_bootloader_lock: false,
            restore_tested_on_device: false,
            power_loss_recovery: String::new(),
        }
    }

    fn review() -> PlanReview {
        PlanReview {
            plan: plan(),
            sha256: "0123456789abcdef".repeat(4),
            directory: PathBuf::from("plans/plan-1"),
            source: PlanSource::Pair,
        }
    }

    fn outcome(state: ApplyState, matches: bool) -> ApplyOutcome {
        ApplyOutcome {
            plan_id: "plan-1".into(),
            plan_sha256: "0123456789abcdef".repeat(4),
            state,
            device_log: String::new(),
            readback_sha256: Vec::new(),
            readback_matches_new_pair: matches,
            reboot_requested: false,
        }
    }

    fn staged() -> WriteFlow {
        WriteFlow {
            review: Some(review()),
            staged: true,
            ..WriteFlow::default()
        }
    }

    #[test]
    fn steps_open_one_at_a_time() {
        let mut flow = WriteFlow::default();
        assert_eq!(flow.next_step(), Some(WriteStep::Plan));
        assert_eq!(flow.status(WriteStep::Stage), StepStatus::Waiting);
        flow.review = Some(review());
        assert_eq!(flow.next_step(), Some(WriteStep::Stage));
        flow.staged = true;
        assert_eq!(flow.next_step(), Some(WriteStep::Write));
        flow.outcome = Some(outcome(ApplyState::VerifiedPendingReboot, true));
        assert_eq!(flow.next_step(), Some(WriteStep::Reboot));
        assert_eq!(flow.focus_step(), WriteStep::Reboot);
        flow.rebooted = true;
        assert_eq!(flow.next_step(), None);
        assert_eq!(flow.status(WriteStep::Reboot), StepStatus::Done);
        assert_eq!(flow.focus_step(), WriteStep::Reboot);
    }

    #[test]
    fn focus_follows_the_running_or_stopped_step() {
        let mut flow = staged();
        assert_eq!(flow.focus_step(), WriteStep::Write);
        flow.running = Some(WriteStep::Write);
        assert_eq!(flow.focus_step(), WriteStep::Write);
        flow.running = None;
        flow.outcome = Some(outcome(ApplyState::RollbackVerified, false));
        assert_eq!(flow.focus_step(), WriteStep::Write);

        let mut stopped = staged();
        stopped.staged = false;
        stopped.failure = Some((WriteStep::Stage, "push failed".into()));
        assert_eq!(stopped.focus_step(), WriteStep::Stage);
    }

    #[test]
    fn write_approval_needs_the_code_of_this_plan_at_the_write_step() {
        let mut flow = staged();
        assert!(flow.write_authorization(5).is_none());
        flow.approval_input = "01234568".into();
        assert!(flow.write_authorization(5).is_none());
        flow.approval_input = " 01234567 ".into();
        let approval = flow.write_authorization(5).expect("approval");
        assert_eq!(approval.scopes, vec![Scope::WritePair]);
        assert_eq!(approval.plan_sha256, review().sha256);

        flow.staged = false;
        assert!(flow.write_authorization(5).is_none(), "not staged yet");
        flow.staged = true;
        flow.running = Some(WriteStep::Write);
        assert!(flow.write_authorization(5).is_none(), "already running");
    }

    #[test]
    fn reboot_approval_needs_a_verified_write() {
        let mut flow = staged();
        flow.reboot_input = REBOOT_WORD.into();
        assert!(flow.reboot_authorization(5).is_none(), "nothing written");
        for (state, matches) in [
            (ApplyState::RollbackVerified, false),
            (ApplyState::RollbackFailedDoNotReboot, false),
            (ApplyState::VerifiedPendingReboot, false),
            (ApplyState::Writing("WRITING_VBMETA".into()), true),
        ] {
            flow.outcome = Some(outcome(state, matches));
            assert!(flow.reboot_authorization(5).is_none());
            assert_eq!(flow.status(WriteStep::Write), StepStatus::Failed);
            assert_eq!(flow.next_step(), None);
        }
        flow.outcome = Some(outcome(ApplyState::VerifiedPendingReboot, true));
        let approval = flow.reboot_authorization(5).expect("approval");
        assert_eq!(approval.scopes, vec![Scope::Reboot]);
        flow.reboot_input = "REBOOT now".into();
        assert!(flow.reboot_authorization(5).is_none());
    }

    #[test]
    fn a_failure_after_the_plan_stops_the_flow() {
        let mut flow = staged();
        flow.failure = Some((WriteStep::Stage, "push failed".into()));
        flow.staged = false;
        assert_eq!(flow.next_step(), None);
        assert_eq!(flow.status(WriteStep::Stage), StepStatus::Failed);

        let mut retry = WriteFlow {
            failure: Some((WriteStep::Plan, "no device".into())),
            ..WriteFlow::default()
        };
        assert_eq!(retry.next_step(), Some(WriteStep::Plan));
        retry.failure = None;
        assert_eq!(retry.status(WriteStep::Plan), StepStatus::Ready);
    }
}
