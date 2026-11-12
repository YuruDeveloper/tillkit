use crate::app::App;
use crate::clock::now_unix;
use crate::confirmation::Confirmation;
use crate::full_backup_store;
use crate::gsi_worker;
use crate::install_device::{self, DeviceSlot, with_link};
use crate::install_message::InstallMessage;
use crate::install_review::InstallReview;
use crate::install_stage::InstallStage;
use crate::install_worker;
use crate::message::Message;
use crate::worker::run_blocking;
use crate::write_kind::WriteKind;
use crate::write_origin::WriteOrigin;
use front2::install::{DataWipeApproval, InstallProgress, WriteApproval};
use iced::Task;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;

type Outcome<T> = Result<T, String>;

impl App {
    pub fn update_install(&mut self, message: InstallMessage) -> Task<Message> {
        match message {
            InstallMessage::Connected(result) => self.install_connected(result),
            InstallMessage::BackedUp(result) => self.install_backed_up(result),
            InstallMessage::Planned(result) => self.install_planned(result),
            InstallMessage::Checked(result) => self.stage_ended(InstallStage::Check, result),
            InstallMessage::Wiped(result) => self.stage_ended(InstallStage::Wipe, result),
            InstallMessage::Restarted(result) => self.install_restarted(result),
            InstallMessage::AskWrite => self.ask_install_write(),
            InstallMessage::AskRestart => self.ask_install_restart(),
            InstallMessage::StartOver => self.install_start_over(),
            InstallMessage::Tick => {}
            InstallMessage::Connect => return self.install_connect(),
            InstallMessage::BackUp => return self.install_back_up(),
            InstallMessage::Plan => return self.install_plan(),
            InstallMessage::Check => return self.install_check(),
            InstallMessage::CheckAgain => return self.install_check_again(),
            InstallMessage::Reconnect => return self.install_reconnect(),
            InstallMessage::Written(result) => return self.install_written(result),
        }
        Task::none()
    }

    pub fn gsi_image(&self) -> Option<PathBuf> {
        let own = self.flow_of(WriteOrigin::Image)?;
        let (kind, image) = own.chosen_image()?;
        (kind == WriteKind::Gsi).then_some(image)
    }

    pub fn can_run_install(&self, stage: InstallStage) -> bool {
        !self.is_busy() && self.gsi_image().is_some() && self.install.next_stage() == Some(stage)
    }

    pub fn find_full_backup(&mut self) {
        if self.install.backup.is_none() && !self.is_busy() {
            self.install.backup = full_backup_store::newest(&self.paths);
        }
    }

    pub fn full_backup_picked(&mut self, folder: Option<PathBuf>) {
        let closed = self.is_busy() || self.install.keeps_the_plan();
        let (Some(folder), false) = (folder, closed) else {
            return;
        };
        match full_backup_store::refusal(&folder) {
            None => self.install.use_backup(folder),
            Some(reason) => self.install.refused_backup = Some(reason),
        }
    }

    fn install_connect(&mut self) -> Task<Message> {
        let loader = self.reboot.loader.clone();
        let (Some(loader), true) = (loader, self.can_run_install(InstallStage::Connect)) else {
            return Task::none();
        };
        self.install.begin(InstallStage::Connect);
        let slot = self.device.clone();
        let transferred = self.install.transferred.clone();
        run_blocking(
            move || install_device::connect(&slot, &loader, transferred),
            connected,
        )
    }

    fn install_connected(&mut self, result: Outcome<()>) {
        self.install.running = None;
        match result {
            Ok(()) => self.install.connected = true,
            Err(error) => self.install.stage_failed(InstallStage::Connect, error),
        }
    }

    fn install_reconnect(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        self.install.disconnect();
        self.install_connect()
    }

    fn install_back_up(&mut self) -> Task<Message> {
        if !self.can_run_install(InstallStage::Backup) {
            return Task::none();
        }
        self.install.begin(InstallStage::Backup);
        let slot = self.device.clone();
        let folder = full_backup_store::new_folder(&self.paths, now_unix());
        let transferred = self.install.transferred.clone();
        let expected = self.install.expected.clone();
        run_blocking(
            move || back_up(&slot, &folder, &transferred, &expected),
            backed_up,
        )
    }

    fn install_backed_up(&mut self, result: Outcome<(PathBuf, Vec<String>)>) {
        self.install.running = None;
        match result {
            Ok((folder, single_read)) => self.install.use_new_backup(folder, single_read),
            Err(error) => self.install.stage_failed(InstallStage::Backup, error),
        }
    }

    fn install_plan(&mut self) -> Task<Message> {
        let backup = self.install.backup.clone();
        let image = self.gsi_image();
        let (Some(backup), Some(image), true) =
            (backup, image, self.can_run_install(InstallStage::Plan))
        else {
            return Task::none();
        };
        self.install.begin(InstallStage::Plan);
        let paths = self.paths.clone();
        let profile = self.device_profile();
        run_blocking(
            move || gsi_worker::prepare(&paths, &backup, &image, profile),
            planned,
        )
    }

    fn install_planned(&mut self, result: Outcome<Box<InstallReview>>) {
        self.install.running = None;
        match result {
            Ok(review) => self.install.review = Some(*review),
            Err(error) => self.install.stage_failed(InstallStage::Plan, error),
        }
    }

    fn install_check(&mut self) -> Task<Message> {
        let directory = self
            .install
            .review
            .as_ref()
            .map(|review| review.directory.clone());
        let (Some(directory), true) = (directory, self.can_run_install(InstallStage::Check)) else {
            return Task::none();
        };
        self.install.begin(InstallStage::Check);
        let slot = self.device.clone();
        run_blocking(
            move || with_link(&slot, |link| install_worker::check(link, &directory)),
            checked,
        )
    }

    fn install_check_again(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        self.install.forget_the_check();
        self.install_check()
    }

    fn stage_ended(&mut self, stage: InstallStage, result: Outcome<Box<InstallProgress>>) {
        match result {
            Ok(progress) => self.install.stage_done(stage, *progress),
            Err(error) => self.install.stage_failed(stage, error),
        }
    }

    fn ask_install_write(&mut self) {
        let open =
            self.can_run_install(InstallStage::Write) || self.can_run_install(InstallStage::Wipe);
        if open {
            self.confirmation = Some(Confirmation::InstallWrite);
        }
    }

    fn ask_install_restart(&mut self) {
        if self.can_run_install(InstallStage::Restart) {
            self.confirmation = Some(Confirmation::InstallRestart);
        }
    }

    pub fn install_write(&mut self) -> Task<Message> {
        if self.is_busy() || self.gsi_image().is_none() || !self.install.approve() {
            return Task::none();
        }
        self.run_install_write()
    }

    fn run_install_write(&mut self) -> Task<Message> {
        let approvals = self.install.write_approvals(now_unix());
        let (Some((write, data_wipe)), Some(stage)) = (approvals, self.install.next_stage()) else {
            return Task::none();
        };
        self.install.begin(stage);
        let slot = self.device.clone();
        let done: fn(Outcome<InstallProgress>) -> Message = if stage == InstallStage::Wipe {
            wiped
        } else {
            written
        };
        run_blocking(move || write_stage(&slot, stage, &write, &data_wipe), done)
    }

    fn install_written(&mut self, result: Outcome<Box<InstallProgress>>) -> Task<Message> {
        self.stage_ended(InstallStage::Write, result);
        self.run_install_write()
    }

    pub fn install_restart(&mut self) -> Task<Message> {
        let approval = self.install.restart_approval(now_unix());
        let (Some(approval), false) = (approval, self.is_busy()) else {
            return Task::none();
        };
        self.install.begin(InstallStage::Restart);
        let slot = self.device.clone();
        run_blocking(
            move || with_link(&slot, |link| install_worker::restart(link, &approval)),
            restarted,
        )
    }

    fn install_restarted(&mut self, result: Outcome<Box<InstallProgress>>) {
        self.stage_ended(InstallStage::Restart, result);
        if self.install.is_finished() {
            self.install.connected = false;
        }
    }

    fn install_start_over(&mut self) {
        if !self.is_busy() && !self.install.keeps_the_plan() {
            self.install.forget_the_plan();
        }
    }
}

fn back_up(
    slot: &DeviceSlot,
    folder: &Path,
    transferred: &AtomicU64,
    expected: &AtomicU64,
) -> front2::Result<(PathBuf, Vec<String>)> {
    with_link(slot, |link| {
        install_worker::back_up(link, folder, transferred, expected)
    })
}

fn write_stage(
    slot: &DeviceSlot,
    stage: InstallStage,
    write: &WriteApproval,
    data_wipe: &DataWipeApproval,
) -> front2::Result<InstallProgress> {
    if stage == InstallStage::Wipe {
        return with_link(slot, |link| install_worker::wipe(link, write, data_wipe));
    }
    with_link(slot, |link| install_worker::write(link, write, data_wipe))
}

fn connected(result: Outcome<()>) -> Message {
    Message::Install(InstallMessage::Connected(result))
}

fn backed_up(result: Outcome<(PathBuf, Vec<String>)>) -> Message {
    Message::Install(InstallMessage::BackedUp(result))
}

fn planned(result: Outcome<InstallReview>) -> Message {
    Message::Install(InstallMessage::Planned(result.map(Box::new)))
}

fn checked(result: Outcome<InstallProgress>) -> Message {
    Message::Install(InstallMessage::Checked(result.map(Box::new)))
}

fn written(result: Outcome<InstallProgress>) -> Message {
    Message::Install(InstallMessage::Written(result.map(Box::new)))
}

fn wiped(result: Outcome<InstallProgress>) -> Message {
    Message::Install(InstallMessage::Wiped(result.map(Box::new)))
}

fn restarted(result: Outcome<InstallProgress>) -> Message {
    Message::Install(InstallMessage::Restarted(result.map(Box::new)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_paths::DataPaths;
    use crate::install_flow::tests::checked;
    use crate::reboot::{DeviceConnection, RebootTarget};
    use crate::reboot_message::RebootMessage;
    use crate::write_message::WriteMessage;

    fn app_with_gsi() -> (tempfile::TempDir, App) {
        let directory = tempfile::tempdir().expect("tempdir");
        let image = directory.path().join("system.img");
        let mut head = vec![0; 2048];
        head[..4].copy_from_slice(&[0x3a, 0xff, 0x26, 0xed]);
        std::fs::write(&image, head).expect("image");
        let mut app = App::with_paths(DataPaths::new(directory.path().join("data")));
        let _ = app.update(Message::Write(WriteMessage::ImagePicked(Some(image))));
        (directory, app)
    }

    fn install(message: InstallMessage) -> Message {
        Message::Install(message)
    }

    #[test]
    fn a_gsi_image_opens_the_stages_and_the_connection_needs_a_loader() {
        let (directory, mut app) = app_with_gsi();
        assert!(app.gsi_image().is_some());
        assert!(app.can_run_install(InstallStage::Connect));
        let _ = app.update(install(InstallMessage::Connect));
        assert!(!app.is_busy(), "no loader file");
        app.reboot.loader = Some(directory.path().join("loader.elf"));
        let _ = app.update(install(InstallMessage::Connect));
        assert_eq!(app.install.running, Some(InstallStage::Connect));
        assert!(app.is_busy() && !app.can_choose_write_image());
        let _ = app.update(install(InstallMessage::Connected(Err("no port".into()))));
        assert!(!app.is_busy() && !app.install.connected);
        assert!(
            app.can_run_install(InstallStage::Connect),
            "it can run again"
        );
    }

    #[test]
    fn a_stage_starts_only_when_it_is_the_next_one() {
        let (_directory, mut app) = app_with_gsi();
        for message in [
            InstallMessage::BackUp,
            InstallMessage::Plan,
            InstallMessage::Check,
            InstallMessage::AskWrite,
            InstallMessage::AskRestart,
        ] {
            let _ = app.update(install(message));
            assert!(!app.is_busy() && app.confirmation.is_none());
        }
        let _ = app.update(Message::Confirm);
        assert!(!app.is_busy());
        let _ = app.update(install(InstallMessage::Connected(Ok(()))));
        let _ = app.update(install(InstallMessage::Plan));
        assert!(!app.is_busy(), "no full backup");
        let _ = app.update(install(InstallMessage::BackUp));
        assert_eq!(app.install.running, Some(InstallStage::Backup));
    }

    #[test]
    fn no_stage_starts_without_a_gsi_image() {
        let directory = tempfile::tempdir().expect("tempdir");
        let mut app = App::with_paths(DataPaths::new(directory.path()));
        app.reboot.loader = Some(directory.path().join("loader.elf"));
        let _ = app.update(install(InstallMessage::Connect));
        assert!(!app.is_busy());
        let (_kept, flow) = checked();
        app.install = flow;
        let _ = app.update(install(InstallMessage::AskWrite));
        assert!(app.confirmation.is_none());
        app.confirmation = Some(Confirmation::InstallWrite);
        let _ = app.update(Message::Confirm);
        assert!(!app.is_busy() && app.install.approved_sha256.is_none());
    }

    #[test]
    fn a_folder_that_is_no_full_backup_is_refused_and_told() {
        let (directory, mut app) = app_with_gsi();
        let _ = app.update(Message::FullBackupPicked(Some(directory.path().into())));
        assert!(app.install.backup.is_none());
        assert!(app.install.refused_backup.is_some());
        let _ = app.update(Message::Navigate(crate::screen::Screen::Write));
        assert!(app.install.backup.is_none(), "the data folder has none");
    }

    #[test]
    fn the_write_starts_only_after_the_answer_of_the_user() {
        let (_directory, mut app) = app_with_gsi();
        let (_kept, flow) = checked();
        app.install = flow;
        let _ = app.update(install(InstallMessage::AskRestart));
        assert!(app.confirmation.is_none(), "nothing is written yet");
        let _ = app.update(install(InstallMessage::AskWrite));
        assert_eq!(app.confirmation, Some(Confirmation::InstallWrite));
        assert!(!app.is_busy(), "nothing starts before the answer");
        let _ = app.update(Message::Cancel);
        let _ = app.update(Message::Confirm);
        assert!(!app.is_busy() && app.install.approved_sha256.is_none());
        let _ = app.update(install(InstallMessage::AskWrite));
        let _ = app.update(Message::Confirm);
        assert_eq!(app.install.running, Some(InstallStage::Write));
        assert!(app.install.approved_sha256.is_some());
        assert!(app.install.expected_bytes().is_some());
    }

    #[test]
    fn a_stopped_write_asks_for_a_new_check_and_holds_the_other_stages() {
        let (_directory, mut app) = app_with_gsi();
        let (_kept, flow) = checked();
        app.install = flow;
        let _ = app.update(install(InstallMessage::AskWrite));
        let _ = app.update(Message::Confirm);
        let stopped = InstallMessage::Written(Err("the cable came out".into()));
        let _ = app.update(install(stopped));
        assert!(!app.is_busy() && app.install.needs_a_new_check());
        let _ = app.update(install(InstallMessage::AskWrite));
        let _ = app.update(install(InstallMessage::AskRestart));
        assert!(app.confirmation.is_none());
        let _ = app.update(install(InstallMessage::CheckAgain));
        assert_eq!(app.install.running, Some(InstallStage::Check));
        assert!(
            app.install.approved_sha256.is_none(),
            "a new answer is needed"
        );
    }

    #[test]
    fn a_new_image_drops_the_plan_and_keeps_the_connection_and_the_backup() {
        let (directory, mut app) = app_with_gsi();
        let (_kept, flow) = checked();
        app.install = flow;
        assert!(!app.can_choose_data_folder(), "a plan is open");
        let other = directory.path().join("other.img");
        std::fs::copy(app.gsi_image().expect("image"), &other).expect("image");
        let _ = app.update(Message::Write(WriteMessage::ImagePicked(Some(other))));
        assert!(app.install.review.is_none() && app.install.progress.is_none());
        assert!(app.install.connected && app.install.backup.is_some());
        assert!(app.can_run_install(InstallStage::Plan));
    }

    #[test]
    fn a_reboot_from_another_screen_waits_while_a_checked_device_is_held() {
        let (_directory, mut app) = app_with_gsi();
        let (_kept, flow) = checked();
        app.install = flow;
        app.reboot.connection = Some(DeviceConnection::Edl);
        assert!(app.reboot_is_held());
        let ask = RebootMessage::Ask(RebootTarget::System);
        let _ = app.update(Message::Reboot(ask));
        assert!(app.confirmation.is_none());
        let _ = app.update(install(InstallMessage::StartOver));
        assert!(!app.reboot_is_held() && app.install.connected);
        assert_eq!(app.install.next_stage(), Some(InstallStage::Plan));
    }

    fn stopped_write(app: &mut App) {
        let (_kept, flow) = checked();
        app.install = flow;
        app.reboot.connection = Some(DeviceConnection::Edl);
        let _ = app.update(install(InstallMessage::AskWrite));
        let _ = app.update(Message::Confirm);
        let stopped = InstallMessage::Written(Err("the cable came out".into()));
        let _ = app.update(install(stopped));
        assert!(app.install.needs_a_new_check() && app.reboot_is_held());
        assert!(app.install.write_is_unfinished());
    }

    fn no_restart_opens(app: &mut App) {
        assert!(app.reboot_is_held() && app.install.write_is_unfinished());
        let ask = RebootMessage::Ask(RebootTarget::System);
        let _ = app.update(Message::Reboot(ask));
        assert!(app.confirmation.is_none());
    }

    #[test]
    fn doc_1072_a_new_image_after_a_stopped_write_keeps_the_plan_and_the_reboot_hold() {
        let (directory, mut app) = app_with_gsi();
        stopped_write(&mut app);
        app.reboot.loader = Some(directory.path().join("loader.elf"));
        let image = app.gsi_image().expect("image");
        let other = directory.path().join("other.img");
        std::fs::copy(&image, &other).expect("image");
        assert!(!app.can_choose_write_image());
        let _ = app.update(Message::Write(WriteMessage::ImagePicked(Some(other))));
        assert_eq!(app.gsi_image(), Some(image));
        assert!(app.install.review.is_some() && app.install.failure.is_some());
        no_restart_opens(&mut app);
    }

    #[test]
    fn doc_1072_start_over_and_a_new_backup_after_a_stopped_write_keep_the_plan() {
        let (_directory, mut app) = app_with_gsi();
        stopped_write(&mut app);
        let backup = app.install.backup.clone().expect("backup");
        let _ = app.update(install(InstallMessage::StartOver));
        let _ = app.update(Message::FullBackupPicked(Some(backup.clone())));
        assert_eq!(app.install.backup, Some(backup));
        assert!(app.install.review.is_some() && app.install.failure.is_some());
        no_restart_opens(&mut app);
    }

    #[test]
    fn doc_1072_a_failed_reconnect_after_a_stopped_write_keeps_the_reboot_hold() {
        let (directory, mut app) = app_with_gsi();
        app.reboot.loader = Some(directory.path().join("loader.elf"));
        stopped_write(&mut app);
        let _ = app.update(install(InstallMessage::Reconnect));
        assert_eq!(app.install.running, Some(InstallStage::Connect));
        let failed = InstallMessage::Connected(Err("no port".into()));
        let _ = app.update(install(failed));
        assert!(!app.install.connected && app.install.progress.is_none());
        no_restart_opens(&mut app);
    }

    #[test]
    fn doc_1072_the_reboot_hold_ends_when_the_restart_is_sent() {
        let (_directory, mut app) = app_with_gsi();
        stopped_write(&mut app);
        let progress = app.install.progress.as_mut().expect("progress");
        progress.restart_sent = true;
        assert!(app.install.is_finished());
        assert!(!app.reboot_is_held() && !app.install.write_is_unfinished());
        assert!(!app.install.keeps_the_plan());
    }
}
