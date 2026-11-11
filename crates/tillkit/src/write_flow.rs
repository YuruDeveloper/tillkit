use crate::folder_refusal::FolderRefusal;
use crate::install_review::InstallReview;
use crate::plan_review::PlanReview;
use crate::step_status::StepStatus;
use crate::write_kind::WriteKind;
use crate::write_origin::WriteOrigin;
use crate::write_step::WriteStep;
use front2::magisk::MagiskPackage;
use front2::{ApplyOutcome, ApplyState, Authorization, Scope};
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct WriteFlow {
    pub origin: WriteOrigin,
    pub package: Option<MagiskPackage>,
    pub folder: Option<PathBuf>,
    pub kind: Option<Result<WriteKind, FolderRefusal>>,
    pub review: Option<PlanReview>,
    pub approved_sha256: Option<String>,
    pub staged: bool,
    pub outcome: Option<ApplyOutcome>,
    pub rebooted: bool,
    pub install: Option<InstallReview>,
    pub running: Option<WriteStep>,
    pub failure: Option<(WriteStep, String)>,
}

impl WriteFlow {
    pub fn for_folder(folder: PathBuf) -> Self {
        Self {
            kind: Some(WriteKind::in_folder(&folder)),
            folder: Some(folder),
            ..Self::default()
        }
    }

    pub fn for_magisk() -> Self {
        Self {
            origin: WriteOrigin::Magisk,
            ..Self::default()
        }
    }

    pub fn is_open(&self) -> bool {
        self.running.is_some() || self.has_result()
    }

    pub fn start_over(&mut self) {
        *self = self.folder.take().map(Self::for_folder).unwrap_or_default();
    }

    pub fn write_is_uncertain(&self) -> bool {
        let write_failed = matches!(&self.failure, Some((WriteStep::Write, _)));
        let unsettled = self.outcome.as_ref().is_some_and(|outcome| {
            !self.write_verified() && outcome.state != ApplyState::RollbackVerified
        });
        write_failed || unsettled
    }

    pub fn can_start_over(&self) -> bool {
        self.running.is_none() && self.has_result() && !self.write_is_uncertain()
    }

    pub fn full_backup_changed(&mut self) {
        if self.kind == Some(Ok(WriteKind::Gsi)) && self.running.is_none() {
            self.start_over();
        }
    }

    pub fn image(&self) -> Option<(WriteKind, PathBuf)> {
        let kind = self.kind?.ok()?;
        let folder = self.folder.as_ref()?;
        Some((kind, folder.join(kind.file_name())))
    }

    pub fn has_result(&self) -> bool {
        self.review.is_some() || self.install.is_some()
    }

    pub fn can_start(&self) -> bool {
        self.running.is_none() && !self.has_result() && self.image().is_some()
    }

    pub fn is_unapproved(&self) -> bool {
        self.review.is_some() && self.approved_sha256.is_none()
    }

    pub fn approve(&mut self) -> bool {
        let Some(review) = self.review.as_ref() else {
            return false;
        };
        if self.next_step() != Some(WriteStep::Stage) {
            return false;
        }
        self.approved_sha256 = Some(review.sha256.clone());
        true
    }

    pub fn write_verified(&self) -> bool {
        self.outcome.as_ref().is_some_and(|outcome| {
            outcome.state == ApplyState::VerifiedPendingReboot && outcome.readback_matches_new_pair
        })
    }

    fn is_done(&self, step: WriteStep) -> bool {
        match step {
            WriteStep::Prepare => self.has_result(),
            WriteStep::Stage => self.staged,
            WriteStep::Write => self.write_verified(),
            WriteStep::Reboot => self.rebooted,
        }
    }

    fn is_stopped(&self) -> bool {
        let bad_write = self.outcome.is_some() && !self.write_verified();
        self.failure.is_some() || self.install.is_some() || bad_write
    }

    fn is_closed(&self, step: WriteStep) -> bool {
        self.kind == Some(Ok(WriteKind::Gsi)) && step != WriteStep::Prepare
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
        let resting = if self.rebooted {
            WriteStep::Reboot
        } else {
            WriteStep::Prepare
        };
        self.running
            .or(self.next_step())
            .or(failed)
            .or(bad_write)
            .unwrap_or(resting)
    }

    pub fn status(&self, step: WriteStep) -> StepStatus {
        let failed_here = matches!(&self.failure, Some((failed, _)) if *failed == step);
        let bad_write =
            step == WriteStep::Write && self.outcome.is_some() && !self.write_verified();
        if self.is_closed(step) {
            StepStatus::Closed
        } else if self.running == Some(step) {
            StepStatus::Running
        } else if self.is_done(step) {
            StepStatus::Done
        } else if failed_here || bad_write {
            StepStatus::Failed
        } else {
            StepStatus::Waiting
        }
    }

    pub fn write_authorization(&self, granted_unix: u64) -> Option<Authorization> {
        let review = self.review.as_ref()?;
        let approved = self.next_step() == Some(WriteStep::Write)
            && self.approved_sha256.as_deref() == Some(review.sha256.as_str());
        approved.then(|| Authorization {
            plan_sha256: review.sha256.clone(),
            scopes: vec![Scope::WritePair],
            granted_unix,
        })
    }

    pub fn reboot_authorization(&self, granted_unix: u64) -> Option<Authorization> {
        let review = self.review.as_ref()?;
        let approved = self.next_step() == Some(WriteStep::Reboot);
        approved.then(|| Authorization {
            plan_sha256: review.sha256.clone(),
            scopes: vec![Scope::Reboot],
            granted_unix,
        })
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::plan_source::PlanSource;
    use front2::{RootAccess, WritePlan, WritePurpose};

    fn plan() -> WritePlan {
        WritePlan {
            schema: front2::write_plan::PLAN_SCHEMA,
            id: "plan-1".into(),
            created_unix: 1,
            purpose: WritePurpose::ApplyCandidate,
            profile: front2::profile::ID.into(),
            support_override: front2::SupportOverride::none(),
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

    pub fn review() -> PlanReview {
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

    fn chosen(kind: WriteKind) -> WriteFlow {
        WriteFlow {
            folder: Some(PathBuf::from("images")),
            kind: Some(Ok(kind)),
            ..WriteFlow::default()
        }
    }

    fn approved() -> WriteFlow {
        let mut flow = chosen(WriteKind::RootImage);
        flow.review = Some(review());
        assert!(flow.approve());
        flow
    }

    fn staged() -> WriteFlow {
        let mut flow = approved();
        flow.staged = true;
        flow
    }

    #[test]
    fn a_start_needs_one_image_and_no_earlier_result() {
        assert!(!WriteFlow::default().can_start());
        let mut refused = chosen(WriteKind::Kernel);
        refused.kind = Some(Err(FolderRefusal::SeveralImages));
        assert!(!refused.can_start());

        let mut flow = chosen(WriteKind::Kernel);
        assert!(flow.can_start());
        assert_eq!(
            flow.image(),
            Some((WriteKind::Kernel, PathBuf::from("images").join("boot.img")))
        );
        flow.failure = Some((WriteStep::Prepare, "no device".into()));
        assert!(flow.can_start(), "a failed preparation can start again");
        flow.running = Some(WriteStep::Prepare);
        assert!(!flow.can_start());
        flow.running = None;
        flow.review = Some(review());
        assert!(!flow.can_start());
    }

    #[test]
    fn steps_open_one_at_a_time() {
        let mut flow = chosen(WriteKind::RootImage);
        assert_eq!(flow.next_step(), Some(WriteStep::Prepare));
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

        let mut stopped = approved();
        stopped.failure = Some((WriteStep::Stage, "push failed".into()));
        assert_eq!(stopped.focus_step(), WriteStep::Stage);
        assert_eq!(stopped.next_step(), None);
        assert_eq!(stopped.status(WriteStep::Stage), StepStatus::Failed);
    }

    #[test]
    fn the_write_needs_the_approval_of_this_plan() {
        let mut flow = chosen(WriteKind::RootImage);
        assert!(!flow.approve(), "no plan");
        flow.review = Some(review());
        assert!(flow.is_unapproved());
        flow.staged = true;
        assert!(flow.write_authorization(5).is_none(), "not approved");

        let mut flow = staged();
        let approval = flow.write_authorization(5).expect("approval");
        assert_eq!(approval.scopes, vec![Scope::WritePair]);
        assert_eq!(approval.plan_sha256, review().sha256);

        flow.approved_sha256 = Some("f".repeat(64));
        assert!(flow.write_authorization(5).is_none(), "another plan");
        flow.approved_sha256 = Some(review().sha256);
        flow.staged = false;
        assert!(flow.write_authorization(5).is_none(), "not staged");
        flow.staged = true;
        flow.running = Some(WriteStep::Write);
        assert!(flow.write_authorization(5).is_none(), "already running");
        assert!(!flow.approve(), "no second approval while it runs");
    }

    #[test]
    fn the_reboot_needs_a_verified_write() {
        let mut flow = staged();
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
        flow.rebooted = true;
        assert!(flow.reboot_authorization(5).is_none(), "one reboot");
    }

    #[test]
    fn a_gsi_folder_stops_after_the_plan() {
        let flow = chosen(WriteKind::Gsi);
        assert_eq!(flow.status(WriteStep::Prepare), StepStatus::Waiting);
        for step in [WriteStep::Stage, WriteStep::Write, WriteStep::Reboot] {
            assert_eq!(flow.status(step), StepStatus::Closed);
        }
        assert_eq!(flow.focus_step(), WriteStep::Prepare);
    }

    #[test]
    fn doc_1053_a_write_without_a_known_result_cannot_start_over() {
        let mut failed = staged();
        failed.failure = Some((WriteStep::Write, "the connection closed".into()));
        assert!(failed.write_is_uncertain() && !failed.can_start_over());
        for (state, matches) in [
            (ApplyState::NotStarted, false),
            (ApplyState::Writing("WRITING_VBMETA".into()), false),
            (ApplyState::ApplyFailedRestoring, false),
            (ApplyState::RollbackFailedDoNotReboot, false),
            (ApplyState::Unrecognized("garbage".into()), false),
            (ApplyState::VerifiedPendingReboot, false),
        ] {
            let mut flow = staged();
            flow.outcome = Some(outcome(state.clone(), matches));
            assert!(flow.write_is_uncertain(), "{state:?}");
            assert!(!flow.can_start_over(), "{state:?}");
        }
    }

    #[test]
    fn doc_1053_a_known_result_can_start_over() {
        let unapproved = {
            let mut flow = chosen(WriteKind::RootImage);
            flow.review = Some(review());
            flow
        };
        let mut upload_failed = approved();
        upload_failed.failure = Some((WriteStep::Stage, "push failed".into()));
        let mut rolled_back = staged();
        rolled_back.outcome = Some(outcome(ApplyState::RollbackVerified, false));
        let mut verified = staged();
        verified.outcome = Some(outcome(ApplyState::VerifiedPendingReboot, true));
        let mut reboot_failed = staged();
        reboot_failed.outcome = Some(outcome(ApplyState::VerifiedPendingReboot, true));
        reboot_failed.failure = Some((WriteStep::Reboot, "no device".into()));
        for flow in [
            unapproved,
            upload_failed,
            rolled_back,
            verified,
            reboot_failed,
        ] {
            assert!(!flow.write_is_uncertain(), "{flow:?}");
            assert!(flow.can_start_over(), "{flow:?}");
        }
        let mut running = staged();
        running.running = Some(WriteStep::Write);
        assert!(!running.can_start_over());
        assert!(!chosen(WriteKind::RootImage).can_start_over(), "no result");
    }

    #[test]
    fn doc_1053_a_new_full_backup_drops_the_gsi_result_only() {
        let mut gsi = chosen(WriteKind::Gsi);
        gsi.failure = Some((WriteStep::Prepare, "the backup has no super".into()));
        gsi.full_backup_changed();
        assert!(gsi.failure.is_none());
        assert_eq!(gsi.folder, Some(PathBuf::from("images")));

        let mut planning = chosen(WriteKind::Gsi);
        planning.running = Some(WriteStep::Prepare);
        planning.full_backup_changed();
        assert_eq!(planning.running, Some(WriteStep::Prepare));

        let mut pair = staged();
        pair.outcome = Some(outcome(ApplyState::Writing("WRITING_VBMETA".into()), false));
        pair.full_backup_changed();
        assert!(pair.outcome.is_some() && pair.review.is_some());
    }

    #[test]
    fn a_root_flow_goes_back_to_an_empty_folder_flow_at_a_new_start() {
        let mut flow = WriteFlow::for_magisk();
        assert!(!flow.is_open() && !flow.can_start(), "no image folder");
        flow.running = Some(WriteStep::Prepare);
        assert!(flow.is_open());
        flow.running = None;
        flow.review = Some(review());
        assert!(flow.is_open() && flow.can_start_over());
        flow.start_over();
        assert_eq!(flow.origin, WriteOrigin::Folder);
        assert!(flow.review.is_none() && flow.package.is_none() && !flow.is_open());
    }

    #[test]
    fn a_new_start_keeps_the_folder_only() {
        let mut flow = staged();
        flow.folder = None;
        flow.start_over();
        assert!(flow.review.is_none() && flow.kind.is_none() && !flow.staged);
    }
}
