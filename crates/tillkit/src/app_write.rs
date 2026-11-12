use crate::app::App;
use crate::clock::now_unix;
use crate::confirmation::Confirmation;
use crate::data_paths::DataPaths;
use crate::gsi_worker;
use crate::install_review::InstallReview;
use crate::kernel_worker;
use crate::message::Message;
use crate::plan_review::PlanReview;
use crate::worker::{self, run_blocking};
use crate::write_flow::WriteFlow;
use crate::write_kind::WriteKind;
use crate::write_message::WriteMessage;
use crate::write_step::WriteStep;
use front2::{ApplyOutcome, DeviceProfile, SupportOverride};
use iced::Task;
use std::path::PathBuf;

type Outcome<T> = Result<T, String>;

impl App {
    pub fn update_write(&mut self, message: WriteMessage) -> Task<Message> {
        match message {
            WriteMessage::ImagePicked(image) => self.write_image_picked(image),
            WriteMessage::Prepared(result) => self.write_prepared(result),
            WriteMessage::InstallPlanned(result) => self.install_planned(result),
            WriteMessage::Rebooted(result) => self.rebooted(result),
            WriteMessage::AskReboot => self.ask_reboot(),
            WriteMessage::StartOver => self.start_over(),
            WriteMessage::PickImage => return self.pick_write_image(),
            WriteMessage::Start => return self.start_write(),
            WriteMessage::Staged(result) => return self.plan_staged(result),
            WriteMessage::Written(result) => self.pair_written(result),
        }
        Task::none()
    }

    pub fn full_backup_for_gsi(&self) -> Option<PathBuf> {
        self.full_backup.clone().filter(|_| self.full_backup_found)
    }

    pub fn write_needs_a_backup(&self) -> bool {
        let writes = matches!(self.write.kind, Some(Ok(kind)) if kind.writes_to_device());
        writes && self.saved_backups().is_empty()
    }

    pub fn can_start_write(&self) -> bool {
        let Some((kind, _)) = self.write.chosen_image() else {
            return false;
        };
        let has_source = if kind.writes_to_device() {
            !self.write_needs_a_backup()
        } else {
            self.full_backup_for_gsi().is_some()
        };
        let device_allows = !kind.writes_to_device() || !self.support_stops_writes();
        !self.is_busy() && self.write.can_start() && has_source && device_allows
    }

    pub fn drop_unapproved_plan(&mut self) {
        if self.write.is_unapproved() && self.write.running.is_none() {
            self.write.start_over();
        }
    }

    pub fn can_choose_write_image(&self) -> bool {
        !self.is_busy() && !self.write.has_result()
    }

    fn pick_write_image(&mut self) -> Task<Message> {
        if !self.can_choose_write_image() {
            return Task::none();
        }
        Task::perform(pick_image_file(), |image| {
            Message::Write(WriteMessage::ImagePicked(image))
        })
    }

    fn write_image_picked(&mut self, image: Option<PathBuf>) {
        let (Some(image), true) = (image, self.can_choose_write_image()) else {
            return;
        };
        self.write = WriteFlow::for_image(image);
    }

    fn start_over(&mut self) {
        if !self.is_busy() && self.write.can_start_over() {
            self.write.start_over();
        }
    }

    fn start_write(&mut self) -> Task<Message> {
        if !self.is_busy() && self.write.can_start() {
            self.write.start_over();
        }
        let image = self.write.chosen_image().filter(|_| self.can_start_write());
        let Some((kind, image)) = image else {
            return Task::none();
        };
        let full_backup = self.full_backup_for_gsi();
        let accepted = self.support_override();
        let profile = self.device_profile();
        let task = prepare_task(
            kind,
            image,
            self.paths.clone(),
            full_backup,
            accepted,
            profile,
        );
        self.write.running = task.as_ref().map(|_| WriteStep::Prepare);
        task.unwrap_or_else(Task::none)
    }

    fn write_prepared(&mut self, result: Outcome<Box<PlanReview>>) {
        self.write.running = None;
        match result {
            Ok(review) => self.review_ready(*review),
            Err(error) => self.write.failure = Some((WriteStep::Prepare, error)),
        }
    }

    pub fn review_ready(&mut self, review: PlanReview) {
        self.write.review = Some(review);
        self.confirmation = Some(Confirmation::Write);
    }

    fn install_planned(&mut self, result: Outcome<Box<InstallReview>>) {
        self.write.running = None;
        match result {
            Ok(install) => self.write.install = Some(*install),
            Err(error) => self.write.failure = Some((WriteStep::Prepare, error)),
        }
    }

    pub fn stage_plan(&mut self) -> Task<Message> {
        let review = self.write.review.clone();
        let (Some(review), false) = (review, self.is_busy()) else {
            return Task::none();
        };
        if !self.write.approve() {
            return Task::none();
        }
        self.write.running = Some(WriteStep::Stage);
        let paths = self.paths.clone();
        run_blocking(
            move || worker::stage_plan(&paths, &review),
            |result| Message::Write(WriteMessage::Staged(result)),
        )
    }

    fn plan_staged(&mut self, result: Outcome<()>) -> Task<Message> {
        self.write.running = None;
        if let Err(error) = result {
            self.write.failure = Some((WriteStep::Stage, error));
            return Task::none();
        }
        self.write.staged = true;
        self.write_pair()
    }

    fn write_pair(&mut self) -> Task<Message> {
        let authorization = self.write.write_authorization(now_unix());
        let review = self.write.review.clone();
        let (Some(authorization), Some(review), false) = (authorization, review, self.is_busy())
        else {
            return Task::none();
        };
        self.write.running = Some(WriteStep::Write);
        run_blocking(
            move || worker::write_pair(&review, &authorization),
            |result| Message::Write(WriteMessage::Written(result)),
        )
    }

    fn pair_written(&mut self, result: Outcome<ApplyOutcome>) {
        self.write.running = None;
        match result {
            Ok(outcome) => self.write.outcome = Some(outcome),
            Err(error) => self.write.failure = Some((WriteStep::Write, error)),
        }
        self.ask_reboot();
    }

    fn ask_reboot(&mut self) {
        if !self.is_busy() && self.write.next_step() == Some(WriteStep::Reboot) {
            self.confirmation = Some(Confirmation::Reboot);
        }
    }

    pub fn reboot(&mut self) -> Task<Message> {
        let authorization = self.write.reboot_authorization(now_unix());
        let review = self.write.review.clone();
        let outcome = self.write.outcome.clone();
        let (Some(authorization), Some(review), Some(outcome), false) =
            (authorization, review, outcome, self.is_busy())
        else {
            return Task::none();
        };
        self.write.running = Some(WriteStep::Reboot);
        run_blocking(
            move || worker::reboot(&review, &authorization, outcome),
            |result| Message::Write(WriteMessage::Rebooted(result)),
        )
    }

    fn rebooted(&mut self, result: Outcome<ApplyOutcome>) {
        self.write.running = None;
        match result {
            Ok(outcome) => self.reboot_sent(outcome),
            Err(error) => self.write.failure = Some((WriteStep::Reboot, error)),
        }
    }

    fn reboot_sent(&mut self, outcome: ApplyOutcome) {
        self.write.rebooted = outcome.reboot_requested;
        self.write.outcome = Some(outcome);
    }
}

fn prepare_task(
    kind: WriteKind,
    image: PathBuf,
    paths: DataPaths,
    full_backup: Option<PathBuf>,
    accepted: SupportOverride,
    profile: &'static DeviceProfile,
) -> Option<Task<Message>> {
    if kind == WriteKind::RootImage {
        let work = move || worker::prepare_root_write(&paths, &image, accepted);
        return Some(run_blocking(work, prepared));
    }
    if kind == WriteKind::Kernel {
        let work = move || kernel_worker::prepare_write(&paths, &image, accepted);
        return Some(run_blocking(work, prepared));
    }
    let backup = full_backup?;
    let work = move || gsi_worker::prepare(&paths, &backup, &image, profile);
    Some(run_blocking(work, install_planned))
}

fn prepared(result: Outcome<PlanReview>) -> Message {
    Message::Write(WriteMessage::Prepared(result.map(Box::new)))
}

fn install_planned(result: Outcome<InstallReview>) -> Message {
    Message::Write(WriteMessage::InstallPlanned(result.map(Box::new)))
}

async fn pick_image_file() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("기록할 이미지 파일")
        .add_filter("이미지 파일", &["img"])
        .add_filter("모든 파일", &["*"])
        .pick_file()
        .await
        .map(|image| image.path().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write_kind::tests::boot_head;
    use front2::PairKind;

    fn app_with_image(head: &[u8]) -> (tempfile::TempDir, App) {
        let directory = tempfile::tempdir().expect("tempdir");
        let image = directory.path().join("magisk_patched-31100_AbCdE.img");
        std::fs::write(&image, head).expect("image");
        let mut app = App::with_paths(DataPaths::new(directory.path().join("data")));
        let _ = app.update(Message::Write(WriteMessage::ImagePicked(Some(image))));
        (directory, app)
    }

    fn add_backup(app: &mut App) {
        let backup = crate::worker::tests::pair("readback-1", PairKind::DeviceReadback);
        app.pairs.push(backup);
    }

    #[test]
    fn a_boot_image_starts_with_a_saved_backup_and_without_another_folder() {
        let (_directory, mut app) = app_with_image(&boot_head(0));
        assert_eq!(app.write.kind, Some(Ok(WriteKind::RootImage)));
        assert!(app.write_needs_a_backup() && !app.can_start_write());
        add_backup(&mut app);
        assert!(!app.write_needs_a_backup() && app.can_start_write());
        assert!(app.full_backup.is_none(), "no full backup folder is asked");
    }

    #[test]
    fn a_candidate_alone_is_no_backup_for_a_write() {
        let (_directory, mut app) = app_with_image(&boot_head(9));
        assert_eq!(app.write.kind, Some(Ok(WriteKind::Kernel)));
        let candidate = crate::worker::tests::pair("candidate-1", PairKind::SignedCandidate);
        app.pairs.push(candidate);
        assert!(app.write_needs_a_backup() && !app.can_start_write());
    }

    #[test]
    fn a_refused_file_and_no_file_ask_for_no_backup_and_do_not_start() {
        let (_directory, mut app) = app_with_image(b"not an image");
        assert!(!app.write_needs_a_backup() && !app.can_start_write());
        add_backup(&mut app);
        assert!(!app.can_start_write());
        let directory = tempfile::tempdir().expect("tempdir");
        let empty = App::with_paths(DataPaths::new(directory.path()));
        assert!(!empty.write_needs_a_backup() && !empty.can_start_write());
    }

    #[test]
    fn a_gsi_image_asks_for_no_backup_and_starts_only_a_plan_with_a_full_backup() {
        let mut head = vec![0; 2048];
        head[..4].copy_from_slice(&[0x3a, 0xff, 0x26, 0xed]);
        let (directory, mut app) = app_with_image(&head);
        assert_eq!(app.write.kind, Some(Ok(WriteKind::Gsi)));
        assert!(!app.write_needs_a_backup() && !app.can_start_write());
        add_backup(&mut app);
        assert!(!app.can_start_write(), "a saved backup is no full backup");
        let full_backup = directory.path().join("full");
        std::fs::create_dir(&full_backup).expect("folder");
        let _ = app.update(Message::FullBackupPicked(Some(full_backup.clone())));
        assert!(!app.can_start_write(), "no manifest");
        std::fs::write(full_backup.join("sha256-manifest.json"), b"{}").expect("manifest");
        let _ = app.update(Message::FullBackupPicked(Some(full_backup)));
        assert!(app.can_start_write());
    }

    #[test]
    fn a_new_image_cannot_replace_a_result() {
        let (directory, mut app) = app_with_image(&boot_head(0));
        app.write.review = Some(crate::write_flow::tests::review());
        let other = directory.path().join("other.img");
        std::fs::write(&other, boot_head(9)).expect("image");
        assert!(!app.can_choose_write_image());
        let _ = app.update(Message::Write(WriteMessage::ImagePicked(Some(other))));
        assert_eq!(app.write.kind, Some(Ok(WriteKind::RootImage)));
        let _ = app.update(Message::Write(WriteMessage::ImagePicked(None)));
        assert!(app.write.image.is_some());
    }
}
