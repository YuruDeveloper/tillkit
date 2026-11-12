use crate::app::App;
use crate::confirmation::Confirmation;
use crate::full_restore_message::FullRestoreMessage;
use crate::full_restore_review::FullRestoreReview;
use crate::full_restore_stage::FullRestoreStage;
use crate::full_restore_worker;
use crate::install_device::{DeviceSlot, with_link};
use crate::message::Message;
use crate::worker::run_blocking;
use iced::Task;
use std::path::Path;
use std::sync::atomic::AtomicU64;

type Outcome<T> = Result<T, String>;

impl App {
    pub fn update_full_restore(&mut self, message: FullRestoreMessage) -> Task<Message> {
        match message {
            FullRestoreMessage::Open => self.open_full_restore(),
            FullRestoreMessage::Close => self.close_full_restore(),
            FullRestoreMessage::ToggleUserData => self.toggle_restored_user_data(),
            FullRestoreMessage::Checked(result) => self.full_restore_checked(result),
            FullRestoreMessage::AskRestore => self.ask_full_restore(),
            FullRestoreMessage::Restored(result) => self.full_restore_written(result),
            FullRestoreMessage::AskRestart => self.ask_full_restore_restart(),
            FullRestoreMessage::Restarted(result) => self.full_restore_restarted(result),
            FullRestoreMessage::Check => return self.full_restore_check(),
        }
        Task::none()
    }

    pub fn can_open_full_restore(&self) -> bool {
        !self.is_busy() && !self.full_restore.shown && self.install.backup.is_some()
    }

    pub fn can_run_full_restore(&self, stage: FullRestoreStage) -> bool {
        let ready = self.install.connected && self.install.backup.is_some();
        let shown = self.full_restore.shown;
        !self.is_busy() && ready && shown && self.full_restore.next_stage() == Some(stage)
    }

    fn open_full_restore(&mut self) {
        if self.can_open_full_restore() {
            self.full_restore.shown = true;
        }
    }

    fn close_full_restore(&mut self) {
        if !self.is_busy() {
            self.full_restore.close();
        }
    }

    fn toggle_restored_user_data(&mut self) {
        if !self.is_busy() {
            self.full_restore.toggle_user_data();
        }
    }

    fn full_restore_check(&mut self) -> Task<Message> {
        let backup = self.install.backup.clone();
        let ready = self.can_run_full_restore(FullRestoreStage::Check);
        let (Some(backup), true) = (backup, ready) else {
            return Task::none();
        };
        self.full_restore.begin(FullRestoreStage::Check);
        let slot = self.device.clone();
        let transferred = self.install.transferred.clone();
        let expected = self.full_restore.expected.clone();
        let with_user_data = self.full_restore.with_user_data();
        run_blocking(
            move || check_on(&slot, &backup, with_user_data, &transferred, &expected),
            checked,
        )
    }

    fn full_restore_checked(&mut self, result: Outcome<Box<FullRestoreReview>>) {
        match result {
            Ok(review) => self.full_restore.checked(*review),
            Err(error) => self
                .full_restore
                .stage_failed(FullRestoreStage::Check, error),
        }
    }

    fn ask_full_restore(&mut self) {
        if self.can_run_full_restore(FullRestoreStage::Restore) {
            self.confirmation = Some(Confirmation::FullRestoreWrite);
        }
    }

    pub fn full_restore_write(&mut self) -> Task<Message> {
        let review = self.full_restore.review.clone();
        let ready = self.can_run_full_restore(FullRestoreStage::Restore);
        let (Some(review), true) = (review, ready) else {
            return Task::none();
        };
        self.full_restore.begin(FullRestoreStage::Restore);
        self.install.forget_the_check();
        let slot = self.device.clone();
        let transferred = self.install.transferred.clone();
        let expected = self.full_restore.expected.clone();
        run_blocking(
            move || restore_on(&slot, &review, &transferred, &expected),
            restored,
        )
    }

    fn full_restore_written(&mut self, result: Outcome<()>) {
        match result {
            Ok(()) => self.full_restore.restored(),
            Err(error) => self
                .full_restore
                .stage_failed(FullRestoreStage::Restore, error),
        }
    }

    fn ask_full_restore_restart(&mut self) {
        if self.can_run_full_restore(FullRestoreStage::Restart) {
            self.confirmation = Some(Confirmation::FullRestoreRestart);
        }
    }

    pub fn full_restore_restart(&mut self) -> Task<Message> {
        if !self.can_run_full_restore(FullRestoreStage::Restart) {
            return Task::none();
        }
        self.full_restore.begin(FullRestoreStage::Restart);
        let slot = self.device.clone();
        run_blocking(
            move || with_link(&slot, full_restore_worker::restart),
            restarted,
        )
    }

    fn full_restore_restarted(&mut self, result: Outcome<()>) {
        match result {
            Ok(()) => self.full_restore_restart_sent(),
            Err(error) => self
                .full_restore
                .stage_failed(FullRestoreStage::Restart, error),
        }
    }

    fn full_restore_restart_sent(&mut self) {
        self.full_restore.restarted();
        self.install.disconnect();
    }
}

fn check_on(
    slot: &DeviceSlot,
    backup: &Path,
    with_user_data: bool,
    transferred: &AtomicU64,
    expected: &AtomicU64,
) -> front2::Result<FullRestoreReview> {
    with_link(slot, |link| {
        full_restore_worker::check(link, backup, with_user_data, transferred, expected)
    })
}

fn restore_on(
    slot: &DeviceSlot,
    review: &FullRestoreReview,
    transferred: &AtomicU64,
    expected: &AtomicU64,
) -> front2::Result<()> {
    with_link(slot, |link| {
        full_restore_worker::restore(link, review, transferred, expected)
    })
}

fn checked(result: Outcome<FullRestoreReview>) -> Message {
    Message::FullRestore(FullRestoreMessage::Checked(result.map(Box::new)))
}

fn restored(result: Outcome<()>) -> Message {
    Message::FullRestore(FullRestoreMessage::Restored(result))
}

fn restarted(result: Outcome<()>) -> Message {
    Message::FullRestore(FullRestoreMessage::Restarted(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_paths::DataPaths;
    use crate::full_restore_flow::tests::review;
    use crate::install_flow::tests::checked as checked_install;
    use crate::install_message::InstallMessage;
    use crate::install_stage::InstallStage;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn restore(message: FullRestoreMessage) -> Message {
        Message::FullRestore(message)
    }

    fn app() -> (TempDir, App) {
        let directory = tempfile::tempdir().expect("tempdir");
        let app = App::with_paths(DataPaths::new(directory.path()));
        (directory, app)
    }

    fn app_with_both_checks() -> (TempDir, TempDir, App) {
        let (directory, mut app) = app();
        let (kept, flow) = checked_install();
        app.install = flow;
        let mut checked = review(&["super"]);
        checked.backup = app.install.backup.clone().expect("backup");
        app.full_restore.shown = true;
        app.full_restore.checked(checked);
        (directory, kept, app)
    }

    #[test]
    fn the_restore_uses_the_connection_and_the_backup_of_the_install() {
        let (_directory, mut app) = app();
        let _ = app.update(restore(FullRestoreMessage::Open));
        assert!(!app.full_restore.shown, "no full backup is chosen");
        app.install.backup = Some(PathBuf::from("backup"));
        let _ = app.update(restore(FullRestoreMessage::Open));
        assert!(app.full_restore.shown);
        let _ = app.update(restore(FullRestoreMessage::Check));
        assert!(!app.is_busy(), "no connection");
        app.install.connected = true;
        let _ = app.update(restore(FullRestoreMessage::Check));
        assert_eq!(app.full_restore.running, Some(FullRestoreStage::Check));
        assert!(app.full_restore.with_user_data());
        let failed = FullRestoreMessage::Checked(Err("no answer".into()));
        let _ = app.update(restore(failed));
        assert_eq!(app.full_restore.next_stage(), Some(FullRestoreStage::Check));
    }

    #[test]
    fn letter_1371_a_restore_that_starts_drops_the_check_and_the_approval_of_the_install() {
        let (_directory, _kept, mut app) = app_with_both_checks();
        assert!(app.install.approve());
        let _ = app.update(restore(FullRestoreMessage::AskRestore));
        assert_eq!(app.confirmation, Some(Confirmation::FullRestoreWrite));
        assert!(!app.is_busy(), "nothing starts before the answer");
        let _ = app.update(Message::Confirm);
        assert_eq!(app.full_restore.running, Some(FullRestoreStage::Restore));
        assert!(app.install.progress.is_none() && app.install.approved_sha256.is_none());
        assert!(app.reboot_is_held());

        let _ = app.update(restore(FullRestoreMessage::Restored(Ok(()))));
        assert!(app.full_restore.written && !app.reboot_is_held());
        assert_eq!(app.install.next_stage(), Some(InstallStage::Check));
        assert_eq!(
            app.full_restore.next_stage(),
            Some(FullRestoreStage::Restart)
        );
    }

    #[test]
    fn a_stopped_restore_holds_the_reboot_until_a_restore_passes() {
        let (_directory, _kept, mut app) = app_with_both_checks();
        let _ = app.update(restore(FullRestoreMessage::AskRestore));
        let _ = app.update(Message::Confirm);
        let stopped = FullRestoreMessage::Restored(Err("the cable came out".into()));
        let _ = app.update(restore(stopped));
        assert!(app.reboot_is_held() && app.full_restore.review.is_none());
        let _ = app.update(restore(FullRestoreMessage::Close));
        assert!(app.full_restore.shown);
        let _ = app.update(restore(FullRestoreMessage::AskRestore));
        assert!(app.confirmation.is_none(), "a new check is needed");
    }

    #[test]
    fn letter_1397_a_check_without_a_difference_ends_the_hold_of_a_stopped_restore() {
        let (_directory, _kept, mut app) = app_with_both_checks();
        let _ = app.update(restore(FullRestoreMessage::AskRestore));
        let _ = app.update(Message::Confirm);
        let stopped = FullRestoreMessage::Restored(Err("no answer to the last read".into()));
        let _ = app.update(restore(stopped));
        assert!(app.reboot_is_held() && !app.full_restore.can_close());

        let mut different = review(&["super"]);
        different.backup = app.install.backup.clone().expect("backup");
        let _ = app.update(restore(FullRestoreMessage::Check));
        let again = FullRestoreMessage::Checked(Ok(Box::new(different)));
        let _ = app.update(restore(again));
        assert!(app.reboot_is_held(), "the device is not the same yet");

        let mut same = review(&[]);
        same.backup = app.install.backup.clone().expect("backup");
        let _ = app.update(restore(FullRestoreMessage::Checked(Ok(Box::new(same)))));
        assert!(!app.reboot_is_held() && app.full_restore.can_close());
        assert_eq!(app.full_restore.next_stage(), None);
    }

    #[test]
    fn letter_1371_a_reconnect_drops_the_restore_check() {
        let (_directory, _kept, mut app) = app_with_both_checks();
        let _ = app.update(Message::Install(InstallMessage::Reconnect));
        assert!(app.full_restore.review.is_none());
        assert!(!app.can_run_full_restore(FullRestoreStage::Check));
    }

    #[test]
    fn letter_1371_a_new_full_backup_drops_the_restore_check() {
        let (directory, _kept, mut app) = app_with_both_checks();
        let other = directory.path().join("other");
        let made = InstallMessage::BackedUp(Ok((other, Vec::new())));
        let _ = app.update(Message::Install(made));
        assert!(app.full_restore.review.is_none());
    }

    #[test]
    fn letter_1371_an_install_write_drops_the_restore_check() {
        let (_directory, _kept, mut app) = app_with_both_checks();
        app.install.running = Some(InstallStage::Check);
        let _ = app.update(Message::Install(InstallMessage::Tick));
        assert!(
            app.full_restore.review.is_some(),
            "the install check only reads"
        );
        app.install.running = Some(InstallStage::Write);
        let _ = app.update(Message::Install(InstallMessage::Tick));
        assert!(app.full_restore.review.is_none());
    }

    #[test]
    fn letter_1371_the_user_data_choice_drops_the_restore_check() {
        let (_directory, _kept, mut app) = app_with_both_checks();
        let _ = app.update(restore(FullRestoreMessage::ToggleUserData));
        assert!(app.full_restore.review.is_none());
        assert!(!app.full_restore.with_user_data());
    }

    #[test]
    fn the_restart_after_a_restore_ends_the_connection() {
        let (_directory, _kept, mut app) = app_with_both_checks();
        app.full_restore.written = true;
        let _ = app.update(restore(FullRestoreMessage::AskRestart));
        assert_eq!(app.confirmation, Some(Confirmation::FullRestoreRestart));
        let _ = app.update(Message::Confirm);
        assert_eq!(app.full_restore.running, Some(FullRestoreStage::Restart));
        let _ = app.update(restore(FullRestoreMessage::Restarted(Ok(()))));
        assert!(app.full_restore.restart_sent && !app.install.connected);
        assert_eq!(app.install.next_stage(), Some(InstallStage::Connect));
    }
}
