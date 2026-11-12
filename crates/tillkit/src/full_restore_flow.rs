use crate::full_restore_review::FullRestoreReview;
use crate::full_restore_stage::FullRestoreStage;
use crate::install_flow::InstallFlow;
use crate::install_stage::InstallStage;
use crate::step_status::StepStatus;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct FullRestoreFlow {
    pub shown: bool,
    pub leaves_user_data: bool,
    pub review: Option<FullRestoreReview>,
    pub running: Option<FullRestoreStage>,
    pub failure: Option<(FullRestoreStage, String)>,
    pub written: bool,
    pub restart_sent: bool,
    pub device_changed: bool,
    pub expected: Arc<AtomicU64>,
}

impl FullRestoreFlow {
    pub fn with_user_data(&self) -> bool {
        !self.leaves_user_data
    }

    pub fn can_close(&self) -> bool {
        self.running.is_none() && !self.device_changed
    }

    pub fn close(&mut self) {
        if self.can_close() {
            self.shown = false;
            self.forget_the_check();
            self.failure = None;
        }
    }

    pub fn toggle_user_data(&mut self) {
        if self.running.is_none() {
            self.leaves_user_data = !self.leaves_user_data;
            self.forget_the_check();
        }
    }

    pub fn begin(&mut self, stage: FullRestoreStage) {
        self.failure = None;
        self.expected.store(0, Ordering::Relaxed);
        self.running = Some(stage);
        if stage == FullRestoreStage::Check {
            self.forget_the_check();
            self.restart_sent = false;
        }
        self.device_changed |= stage == FullRestoreStage::Restore;
    }

    pub fn checked(&mut self, review: FullRestoreReview) {
        self.running = None;
        self.review = Some(review);
    }

    pub fn restored(&mut self) {
        self.running = None;
        self.written = true;
        self.device_changed = false;
    }

    pub fn restarted(&mut self) {
        self.running = None;
        self.restart_sent = true;
    }

    pub fn stage_failed(&mut self, stage: FullRestoreStage, error: String) {
        self.running = None;
        if stage == FullRestoreStage::Restore {
            self.forget_the_check();
        }
        self.failure = Some((stage, error));
    }

    pub fn forget_the_check(&mut self) {
        self.review = None;
        self.written = false;
    }

    pub fn keep_only_for(&mut self, install: &InstallFlow) {
        let Some(review) = &self.review else {
            return;
        };
        let device_written = install
            .running
            .is_some_and(InstallStage::changes_the_device);
        let same_backup = install.backup.as_ref() == Some(&review.backup);
        let same_choice = review.with_user_data == self.with_user_data();
        if !(install.connected && same_backup && same_choice) || device_written {
            self.forget_the_check();
        }
    }

    pub fn holds_the_device(&self) -> bool {
        self.device_changed
    }

    pub fn expected_bytes(&self) -> Option<u64> {
        Some(self.expected.load(Ordering::Relaxed)).filter(|bytes| *bytes > 0)
    }

    pub fn next_stage(&self) -> Option<FullRestoreStage> {
        if self.running.is_some() {
            return None;
        }
        let review = match &self.review {
            None => return Some(FullRestoreStage::Check),
            Some(review) => review,
        };
        if self.restart_sent {
            None
        } else if self.written {
            Some(FullRestoreStage::Restart)
        } else if review.can_be_written() {
            Some(FullRestoreStage::Restore)
        } else {
            None
        }
    }

    pub fn focus_stage(&self) -> FullRestoreStage {
        let failed = self.failure.as_ref().map(|(stage, _)| *stage);
        self.running
            .or(failed)
            .or(self.next_stage())
            .unwrap_or(FullRestoreStage::Check)
    }

    pub fn status(&self, stage: FullRestoreStage) -> StepStatus {
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

    fn is_done(&self, stage: FullRestoreStage) -> bool {
        match stage {
            FullRestoreStage::Check => self.review.is_some(),
            FullRestoreStage::Restore => self.written,
            FullRestoreStage::Restart => self.restart_sent,
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::path::PathBuf;

    pub const BACKUP: &str = "backup-1";

    pub fn review(different: &[&str]) -> FullRestoreReview {
        FullRestoreReview {
            backup: PathBuf::from(BACKUP),
            with_user_data: true,
            boot_lun: 1,
            boot_lun_selected: true,
            areas: 111,
            different: different.iter().map(|name| name.to_string()).collect(),
            different_bytes: 4096 * different.len() as u64,
            first_boot_images: Vec::new(),
            same: different.is_empty(),
        }
    }

    fn connected() -> InstallFlow {
        InstallFlow {
            connected: true,
            backup: Some(PathBuf::from(BACKUP)),
            ..InstallFlow::default()
        }
    }

    fn checked(different: &[&str]) -> FullRestoreFlow {
        checked_with(review(different))
    }

    fn checked_with(review: FullRestoreReview) -> FullRestoreFlow {
        let mut flow = FullRestoreFlow {
            shown: true,
            ..FullRestoreFlow::default()
        };
        flow.begin(FullRestoreStage::Check);
        flow.checked(review);
        flow
    }

    #[test]
    fn the_stages_open_one_at_a_time() {
        let mut flow = FullRestoreFlow::default();
        assert!(
            flow.with_user_data(),
            "the user data is restored unless the user turns it off"
        );
        assert_eq!(flow.next_stage(), Some(FullRestoreStage::Check));
        flow = checked(&["super"]);
        assert_eq!(flow.next_stage(), Some(FullRestoreStage::Restore));
        flow.begin(FullRestoreStage::Restore);
        assert!(flow.holds_the_device() && !flow.can_close());
        assert_eq!(flow.next_stage(), None);
        flow.restored();
        assert!(!flow.holds_the_device());
        assert_eq!(flow.next_stage(), Some(FullRestoreStage::Restart));
        assert_eq!(flow.status(FullRestoreStage::Restore), StepStatus::Done);
        flow.begin(FullRestoreStage::Restart);
        flow.restarted();
        assert_eq!(flow.next_stage(), None);
        assert_eq!(flow.status(FullRestoreStage::Restart), StepStatus::Done);
    }

    #[test]
    fn a_device_that_is_the_same_as_the_backup_opens_no_restore() {
        let flow = checked(&[]);
        assert!(flow.review.as_ref().expect("review").same);
        assert_eq!(flow.next_stage(), None);
        let mut moved = review(&[]);
        moved.boot_lun_selected = false;
        moved.same = false;
        let flow = checked_with(moved);
        assert_eq!(
            flow.next_stage(),
            Some(FullRestoreStage::Restore),
            "only the boot LUN is to be selected"
        );
    }

    #[test]
    fn letter_1371_different_first_boot_images_open_no_restore() {
        let mut first_boot = review(&["xbl_a", "super"]);
        first_boot.first_boot_images = vec!["xbl_a".into()];
        let flow = checked_with(first_boot);
        assert_eq!(flow.next_stage(), None);
    }

    #[test]
    fn letter_1371_the_check_is_dropped_when_the_user_data_choice_changes() {
        let mut flow = checked(&["userdata"]);
        flow.toggle_user_data();
        assert!(!flow.with_user_data() && flow.review.is_none());
        assert_eq!(flow.next_stage(), Some(FullRestoreStage::Check));
        let mut kept = checked(&["super"]);
        kept.leaves_user_data = true;
        kept.keep_only_for(&connected());
        assert!(
            kept.review.is_none(),
            "the check was made with the user data"
        );
    }

    #[test]
    fn letter_1371_the_check_is_dropped_when_the_connection_ends() {
        let mut flow = checked(&["super"]);
        let mut install = connected();
        flow.keep_only_for(&install);
        assert!(flow.review.is_some());
        install.disconnect();
        flow.keep_only_for(&install);
        assert!(flow.review.is_none());
    }

    #[test]
    fn letter_1371_the_check_is_dropped_when_the_backup_changes() {
        let mut flow = checked(&["super"]);
        let mut install = connected();
        install.use_backup(PathBuf::from("backup-2"));
        flow.keep_only_for(&install);
        assert!(flow.review.is_none());
    }

    #[test]
    fn letter_1371_the_check_is_dropped_when_the_install_writes() {
        let mut install = connected();
        for stage in [InstallStage::Check, InstallStage::Plan] {
            let mut flow = checked(&["super"]);
            install.running = Some(stage);
            flow.keep_only_for(&install);
            assert!(flow.review.is_some(), "{stage:?} does not write");
        }
        for stage in [
            InstallStage::Write,
            InstallStage::Wipe,
            InstallStage::Restart,
        ] {
            let mut flow = checked(&["super"]);
            install.running = Some(stage);
            flow.keep_only_for(&install);
            assert!(flow.review.is_none(), "{stage:?} writes");
        }
    }

    #[test]
    fn a_stopped_restore_needs_a_new_check_and_holds_the_device() {
        let mut flow = checked(&["super"]);
        flow.begin(FullRestoreStage::Restore);
        flow.stage_failed(FullRestoreStage::Restore, "the cable came out".into());
        assert!(flow.review.is_none() && flow.holds_the_device());
        assert_eq!(flow.status(FullRestoreStage::Restore), StepStatus::Failed);
        assert_eq!(flow.next_stage(), Some(FullRestoreStage::Check));
        flow.close();
        assert!(flow.shown, "a stopped restore stays on the screen");
        flow.begin(FullRestoreStage::Check);
        assert!(flow.holds_the_device() && flow.failure.is_none());
    }
}
