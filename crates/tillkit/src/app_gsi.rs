use crate::app::App;
use crate::gsi_action::GsiAction;
use crate::gsi_message::GsiMessage;
use crate::gsi_review::GsiReview;
use crate::gsi_worker;
use crate::install_review::InstallReview;
use crate::message::Message;
use crate::worker::run_blocking;
use iced::Task;
use std::path::PathBuf;

const BUILD_ACTIVITY: &str = "BUILDING GSI CANDIDATE";
const PLAN_ACTIVITY: &str = "PLANNING INSTALL";

impl App {
    pub fn update_gsi(&mut self, message: GsiMessage) -> Task<Message> {
        match message {
            GsiMessage::PickBackup => return self.pick_gsi_backup(),
            GsiMessage::PickImage => return self.pick_gsi_image(),
            GsiMessage::Build => return self.build_gsi_candidate(),
            GsiMessage::PlanInstall => return self.plan_gsi_install(),
            GsiMessage::BackupPicked(Some(path)) => self.gsi.backup_directory = Some(path),
            GsiMessage::ImagePicked(Some(path)) => self.gsi.gsi_image = Some(path),
            GsiMessage::BackupPicked(None) | GsiMessage::ImagePicked(None) => {}
            GsiMessage::Built(result) => self.gsi_candidate_built(result),
            GsiMessage::InstallPlanned(result) => self.gsi_install_planned(result),
            GsiMessage::Close => self.close_gsi_candidate(),
        }
        Task::none()
    }

    fn gsi_allows(&self, action: GsiAction) -> bool {
        !self.is_busy() && self.gsi.allows(action)
    }

    fn pick_gsi_backup(&mut self) -> Task<Message> {
        if !self.gsi_allows(GsiAction::PickBackup) {
            return Task::none();
        }
        Task::perform(pick_backup_folder(), backup_picked)
    }

    fn pick_gsi_image(&mut self) -> Task<Message> {
        if !self.gsi_allows(GsiAction::PickImage) {
            return Task::none();
        }
        Task::perform(pick_gsi_image(), image_picked)
    }

    fn build_gsi_candidate(&mut self) -> Task<Message> {
        let (Some((backup, image)), false) = (self.gsi.inputs(), self.is_busy()) else {
            return Task::none();
        };
        self.activity = Some(BUILD_ACTIVITY);
        self.gsi.failure = None;
        let paths = self.paths.clone();
        run_blocking(
            move || gsi_worker::build_candidate(&paths, &backup, &image),
            |result| Message::Gsi(GsiMessage::Built(result.map(Box::new))),
        )
    }

    fn gsi_candidate_built(&mut self, result: Result<Box<GsiReview>, String>) {
        self.activity = None;
        match result {
            Ok(review) => self.gsi.review = Some(*review),
            Err(error) => self.gsi.failure = Some(error),
        }
    }

    fn plan_gsi_install(&mut self) -> Task<Message> {
        let (Some((review, backup)), false) = (self.gsi.install_inputs(), self.is_busy()) else {
            return Task::none();
        };
        self.activity = Some(PLAN_ACTIVITY);
        self.gsi.install_failure = None;
        let paths = self.paths.clone();
        run_blocking(
            move || gsi_worker::plan_install(&paths, &review, &backup),
            |result| Message::Gsi(GsiMessage::InstallPlanned(result.map(Box::new))),
        )
    }

    fn gsi_install_planned(&mut self, result: Result<Box<InstallReview>, String>) {
        self.activity = None;
        match result {
            Ok(install) => self.gsi.install = Some(*install),
            Err(error) => self.gsi.install_failure = Some(error),
        }
    }

    fn close_gsi_candidate(&mut self) {
        if !self.is_busy() {
            self.gsi.review = None;
            self.gsi.install = None;
            self.gsi.install_failure = None;
        }
    }
}

fn backup_picked(path: Option<PathBuf>) -> Message {
    Message::Gsi(GsiMessage::BackupPicked(path))
}

fn image_picked(path: Option<PathBuf>) -> Message {
    Message::Gsi(GsiMessage::ImagePicked(path))
}

async fn pick_backup_folder() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("sha256-manifest.json이 있는 기기 저장소 백업 폴더")
        .pick_folder()
        .await
        .map(|folder| folder.path().to_path_buf())
}

async fn pick_gsi_image() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("슬롯 B에 넣을 GSI 이미지")
        .add_filter("system image", &["img"])
        .pick_file()
        .await
        .map(|file| file.path().to_path_buf())
}
