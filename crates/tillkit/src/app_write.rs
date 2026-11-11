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
use front2::{ApplyOutcome, SupportOverride};
use iced::Task;
use std::path::PathBuf;

type Outcome<T> = Result<T, String>;

impl App {
    pub fn update_write(&mut self, message: WriteMessage) -> Task<Message> {
        match message {
            WriteMessage::FolderPicked(folder) => self.write_folder_picked(folder),
            WriteMessage::Prepared(result) => self.write_prepared(result),
            WriteMessage::InstallPlanned(result) => self.install_planned(result),
            WriteMessage::Rebooted(result) => self.rebooted(result),
            WriteMessage::AskReboot => self.ask_reboot(),
            WriteMessage::StartOver => self.start_over(),
            WriteMessage::PickFolder => return self.pick_write_folder(),
            WriteMessage::Start => return self.start_write(),
            WriteMessage::Staged(result) => return self.plan_staged(result),
            WriteMessage::Written(result) => self.pair_written(result),
        }
        Task::none()
    }

    pub fn full_backup_for_gsi(&self) -> Option<PathBuf> {
        self.full_backup.clone().filter(|_| self.full_backup_found)
    }

    pub fn can_start_write(&self) -> bool {
        let Some((kind, _)) = self.write.image() else {
            return false;
        };
        let has_source = kind != WriteKind::Gsi || self.full_backup_for_gsi().is_some();
        let device_allows = kind == WriteKind::Gsi || !self.support_stops_writes();
        !self.is_busy() && self.write.can_start() && has_source && device_allows
    }

    pub fn drop_unapproved_plan(&mut self) {
        if self.write.is_unapproved() && self.write.running.is_none() {
            self.write.start_over();
        }
    }

    fn can_choose_write_folder(&self) -> bool {
        !self.is_busy() && !self.write.has_result()
    }

    fn pick_write_folder(&mut self) -> Task<Message> {
        if !self.can_choose_write_folder() {
            return Task::none();
        }
        Task::perform(pick_image_folder(), |folder| {
            Message::Write(WriteMessage::FolderPicked(folder))
        })
    }

    fn write_folder_picked(&mut self, folder: Option<PathBuf>) {
        let (Some(folder), true) = (folder, self.can_choose_write_folder()) else {
            return;
        };
        self.write = WriteFlow::for_folder(folder);
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
        let image = self.write.image().filter(|_| self.can_start_write());
        let Some((kind, image)) = image else {
            return Task::none();
        };
        let full_backup = self.full_backup_for_gsi();
        let accepted = self.support_override();
        let task = prepare_task(kind, image, self.paths.clone(), full_backup, accepted);
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

    fn review_ready(&mut self, review: PlanReview) {
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
    let work = move || gsi_worker::prepare(&paths, &backup, &image);
    Some(run_blocking(work, install_planned))
}

fn prepared(result: Outcome<PlanReview>) -> Message {
    Message::Write(WriteMessage::Prepared(result.map(Box::new)))
}

fn install_planned(result: Outcome<InstallReview>) -> Message {
    Message::Write(WriteMessage::InstallPlanned(result.map(Box::new)))
}

async fn pick_image_folder() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("기록할 이미지가 든 폴더")
        .pick_folder()
        .await
        .map(|folder| folder.path().to_path_buf())
}
