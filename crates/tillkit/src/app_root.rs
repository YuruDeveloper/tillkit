use crate::app::App;
use crate::message::Message;
use crate::root_message::RootMessage;
use crate::root_review::RootReview;
use crate::root_worker;
use crate::worker::run_blocking;
use crate::write_flow::WriteFlow;
use crate::write_origin::WriteOrigin;
use crate::write_step::WriteStep;
use iced::Task;
use std::path::PathBuf;

type Outcome<T> = Result<T, String>;

impl App {
    pub fn update_root(&mut self, message: RootMessage) -> Task<Message> {
        match message {
            RootMessage::PackagePicked(file) => self.root_package_picked(file),
            RootMessage::UseLatest => self.use_latest_package(),
            RootMessage::Prepared(result) => self.root_prepared(result),
            RootMessage::PickPackage => return self.pick_root_package(),
            RootMessage::Start => return self.start_root(),
        }
        Task::none()
    }

    pub fn flow_of(&self, origin: WriteOrigin) -> Option<&WriteFlow> {
        (self.write.origin == origin).then_some(&self.write)
    }

    pub fn other_work_is_open(&self, origin: WriteOrigin) -> bool {
        self.write.origin != origin && self.write.is_open()
    }

    pub fn can_change_root_package(&self) -> bool {
        let used = self.write.origin == WriteOrigin::Magisk && self.write.has_result();
        !self.is_busy() && !used
    }

    pub fn can_start_root(&self) -> bool {
        !self.is_busy() && !self.write.has_result() && !self.support_stops_writes()
    }

    fn pick_root_package(&mut self) -> Task<Message> {
        if !self.can_change_root_package() {
            return Task::none();
        }
        Task::perform(pick_package_file(), |file| {
            Message::Root(RootMessage::PackagePicked(file))
        })
    }

    fn root_package_picked(&mut self, file: Option<PathBuf>) {
        if let (Some(file), true) = (file, self.can_change_root_package()) {
            self.magisk_apk = Some(file);
        }
    }

    fn use_latest_package(&mut self) {
        if self.can_change_root_package() {
            self.magisk_apk = None;
        }
    }

    fn start_root(&mut self) -> Task<Message> {
        if !self.can_start_root() {
            return Task::none();
        }
        self.write = WriteFlow::for_magisk();
        self.write.running = Some(WriteStep::Prepare);
        let paths = self.paths.clone();
        let package = self.magisk_apk.clone();
        let accepted = self.support_override();
        run_blocking(
            move || root_worker::prepare(&paths, package.as_deref(), accepted),
            |result| Message::Root(RootMessage::Prepared(result.map(Box::new))),
        )
    }

    fn root_prepared(&mut self, result: Outcome<Box<RootReview>>) {
        self.write.running = None;
        match result {
            Ok(root) => self.root_review_ready(*root),
            Err(error) => self.write.failure = Some((WriteStep::Prepare, error)),
        }
    }

    fn root_review_ready(&mut self, root: RootReview) {
        self.write.package = Some(root.package);
        self.review_ready(root.review);
    }
}

async fn pick_package_file() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Magisk APK 파일")
        .add_filter("Magisk APK", &["apk"])
        .pick_file()
        .await
        .map(|file| file.path().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confirmation::Confirmation;
    use crate::data_paths::DataPaths;
    use crate::write_flow::tests::review;
    use front2::magisk::MagiskPackage;

    fn app() -> (tempfile::TempDir, App) {
        let directory = tempfile::tempdir().expect("tempdir");
        let app = App::with_paths(DataPaths::new(directory.path()));
        (directory, app)
    }

    fn package() -> MagiskPackage {
        MagiskPackage {
            path: PathBuf::from("Magisk-v1.apk"),
            version: "v1".into(),
            sha256: "ab".repeat(32),
        }
    }

    fn prepared(result: Outcome<RootReview>) -> Message {
        Message::Root(RootMessage::Prepared(result.map(Box::new)))
    }

    #[test]
    fn a_root_start_waits_for_work_that_another_screen_opened() {
        let (_directory, mut app) = app();
        assert!(app.can_start_root());
        app.write.review = Some(review());
        assert!(app.other_work_is_open(WriteOrigin::Magisk));
        assert!(!app.other_work_is_open(WriteOrigin::Folder));
        assert!(!app.can_start_root());
        let _ = app.update(Message::Root(RootMessage::Start));
        assert_eq!(app.write.origin, WriteOrigin::Folder);
        assert!(app.write.review.is_some() && app.write.running.is_none());
    }

    #[test]
    fn a_root_start_opens_its_own_flow_and_drops_a_folder_without_a_result() {
        let (_directory, mut app) = app();
        app.write = WriteFlow::for_folder(PathBuf::from("images"));
        let _ = app.update(Message::Root(RootMessage::Start));
        assert_eq!(app.write.origin, WriteOrigin::Magisk);
        assert_eq!(app.write.running, Some(WriteStep::Prepare));
        assert!(app.write.folder.is_none());
        assert!(app.is_busy() && !app.can_start_root());
        assert!(app.flow_of(WriteOrigin::Folder).is_none());
        assert!(app.other_work_is_open(WriteOrigin::Folder));
    }

    #[test]
    fn a_prepared_root_plan_asks_before_the_write_and_keeps_its_package() {
        let (_directory, mut app) = app();
        let _ = app.update(Message::Root(RootMessage::Start));
        let root = RootReview {
            review: review(),
            package: package(),
        };
        let _ = app.update(prepared(Ok(root)));
        assert_eq!(app.confirmation, Some(Confirmation::Write));
        assert_eq!(app.write.package, Some(package()));
        assert!(app.write.is_unapproved() && !app.is_busy());
        assert!(!app.can_change_root_package());
        let _ = app.update(Message::Cancel);
        assert_eq!(app.write.origin, WriteOrigin::Folder);
        assert!(app.write.package.is_none() && app.can_start_root());
    }

    #[test]
    fn a_failed_root_preparation_can_start_again_and_frees_the_write_screen() {
        let (_directory, mut app) = app();
        let _ = app.update(Message::Root(RootMessage::Start));
        let _ = app.update(prepared(Err("no device".into())));
        assert!(app.write.failure.is_some() && !app.is_busy());
        assert!(app.can_start_root());
        assert!(!app.other_work_is_open(WriteOrigin::Folder));
        assert!(app.flow_of(WriteOrigin::Folder).is_none());
    }

    #[test]
    fn the_package_choice_waits_while_an_operation_runs() {
        let (_directory, mut app) = app();
        let file = PathBuf::from("Magisk.apk");
        let picked = |file: &PathBuf| Message::Root(RootMessage::PackagePicked(Some(file.clone())));
        let _ = app.update(picked(&file));
        assert_eq!(app.magisk_apk, Some(file.clone()));
        app.activity = Some("BACKING UP");
        let _ = app.update(Message::Root(RootMessage::UseLatest));
        assert_eq!(app.magisk_apk, Some(file));
        app.activity = None;
        let _ = app.update(Message::Root(RootMessage::UseLatest));
        assert!(app.magisk_apk.is_none());
    }
}
