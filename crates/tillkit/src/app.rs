use crate::confirmation::Confirmation;
use crate::data_paths::DataPaths;
use crate::diagnosis::Diagnosis;
use crate::folder_choice::FolderChoice;
use crate::grain;
use crate::launch;
use crate::message::Message;
use crate::palette::Palette;
use crate::reboot_flow::RebootFlow;
use crate::saved_backup::SavedBackup;
use crate::screen::Screen;
use crate::support_words;
use crate::view;
use crate::worker::{self, run_blocking};
use crate::write_flow::WriteFlow;
use front2::gsi::backup_manifest::BACKUP_MANIFEST_FILE;
use front2::kernel::{KernelSet, KernelStore};
use front2::profiles::toss_front2;
use front2::{DeviceProfile, PairKind, PairManifest, PairStore, SupportOverride};
use iced::widget::image::Handle;
use iced::{Element, Task, Theme};
use std::path::PathBuf;

type Outcome<T> = Result<T, String>;

pub struct App {
    pub palette: Palette,
    pub grain: Handle,
    pub screen: Screen,
    pub paths: DataPaths,
    pub choice_home: Option<PathBuf>,
    pub choice_error: Option<String>,
    pub choice_unread: Option<String>,
    pub chosen_folder_absent: bool,
    pub activity: Option<&'static str>,
    pub diagnosis: Option<Diagnosis>,
    pub device_error: Option<String>,
    pub details_open: bool,
    pub forced: bool,
    pub confirmation: Option<Confirmation>,
    pub backup_error: Option<String>,
    pub store_error: Option<String>,
    pub full_backup: Option<PathBuf>,
    pub full_backup_found: bool,
    pub pairs: Vec<PairManifest>,
    pub kernel_sets: Vec<KernelSet>,
    pub write: WriteFlow,
    pub magisk_apk: Option<PathBuf>,
    pub reboot: RebootFlow,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let mut app = match launch::given_paths() {
            Some(paths) => Self::with_paths(paths),
            None => Self::beside_program(launch::program_folder()),
        };
        launch::preview(&mut app);
        (app, Task::none())
    }

    pub fn beside_program(program_folder: PathBuf) -> Self {
        let (choice, unread) = match FolderChoice::read(&program_folder) {
            Ok(choice) => (choice, None),
            Err(error) => (None, Some(error)),
        };
        let chosen = choice.is_some();
        let paths = choice.map_or_else(
            || DataPaths::beside_program(&program_folder),
            |choice| DataPaths::new(choice.data_folder),
        );
        let mut app = Self::with_paths(paths);
        app.chosen_folder_absent = chosen && !app.paths.root().is_dir();
        app.choice_home = Some(program_folder);
        app.choice_unread = unread;
        app
    }

    pub fn with_paths(paths: DataPaths) -> Self {
        let palette = launch::palette();
        let mut app = Self {
            palette,
            grain: grain::handle(palette),
            screen: launch::screen(),
            paths,
            choice_home: None,
            choice_error: None,
            choice_unread: None,
            chosen_folder_absent: false,
            activity: None,
            diagnosis: None,
            device_error: None,
            details_open: false,
            forced: false,
            confirmation: None,
            backup_error: None,
            store_error: None,
            full_backup: None,
            full_backup_found: false,
            pairs: Vec::new(),
            kernel_sets: Vec::new(),
            write: WriteFlow::default(),
            magisk_apk: None,
            reboot: RebootFlow::default(),
        };
        app.reload_backups();
        app
    }

    pub fn theme(&self) -> Theme {
        self.palette.theme()
    }

    pub fn view(&self) -> Element<'_, Message> {
        view::root(self)
    }

    pub fn is_busy(&self) -> bool {
        self.activity.is_some() || self.write.running.is_some()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(screen) => self.screen = screen,
            Message::ToggleTheme => self.set_palette(self.palette.toggled()),
            Message::BackedUp(result) => self.backed_up(result),
            Message::DeviceRead(result) => self.device_read(result),
            Message::ToggleDetails => self.details_open = !self.details_open,
            Message::AskForce => self.ask_force(),
            Message::Cancel => self.cancel(),
            Message::FullBackupPicked(folder) => self.full_backup_picked(folder),
            Message::DataFolderPicked(folder) => self.data_folder_picked(folder),
            Message::Confirm => return self.confirm(),
            Message::ReadDevice => return self.read_device(),
            Message::BackUp => return self.back_up(),
            Message::PickDataFolder => return self.pick_data_folder(),
            Message::PickFullBackup => return self.pick_full_backup(),
            Message::RestoreBackup(created_unix) => return self.start_restore(created_unix),
            Message::Write(message) => return self.update_write(message),
            Message::Root(message) => return self.update_root(message),
            Message::Reboot(message) => return self.update_reboot(message),
        }
        Task::none()
    }

    fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
        self.grain = grain::handle(palette);
    }

    fn read_device(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        self.activity = Some("READING DEVICE");
        self.device_error = None;
        self.forced = false;
        self.confirmation = None;
        run_blocking(worker::read_device, Message::DeviceRead)
    }

    fn device_read(&mut self, result: Outcome<Diagnosis>) {
        self.activity = None;
        let (diagnosis, error) = match result {
            Ok(diagnosis) => (Some(diagnosis), None),
            Err(error) => (None, Some(error)),
        };
        self.diagnosis = diagnosis;
        self.device_error = error;
        self.forced = false;
        self.confirmation = None;
        self.drop_unapproved_plan();
    }

    pub fn support_stops_writes(&self) -> bool {
        let unmet = self
            .diagnosis
            .as_ref()
            .is_some_and(|diagnosis| !diagnosis.report.writes_allowed());
        unmet && !self.forced
    }

    pub fn support_override(&self) -> SupportOverride {
        self.diagnosis
            .as_ref()
            .filter(|_| self.forced)
            .and_then(|diagnosis| SupportOverride::accept_unmet(&diagnosis.report).ok())
            .unwrap_or_default()
    }

    pub fn device_profile(&self) -> &'static DeviceProfile {
        self.diagnosis
            .as_ref()
            .map_or(&toss_front2::PROFILE, |diagnosis| diagnosis.profile)
    }

    fn can_be_forced(&self) -> bool {
        self.diagnosis
            .as_ref()
            .is_some_and(|diagnosis| support_words::can_be_forced(&diagnosis.report))
    }

    fn ask_force(&mut self) {
        if self.can_be_forced() && !self.is_busy() {
            self.confirmation = Some(Confirmation::ForceSupport);
        }
    }

    fn force_support(&mut self) -> Task<Message> {
        self.forced = self.can_be_forced();
        self.drop_unapproved_plan();
        Task::none()
    }

    fn confirm(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        match self.confirmation.take() {
            Some(Confirmation::ForceSupport) => self.force_support(),
            Some(Confirmation::Write) => self.stage_plan(),
            Some(Confirmation::Reboot) => self.reboot(),
            Some(Confirmation::RebootTo) => self.reboot_to(),
            None => Task::none(),
        }
    }

    fn cancel(&mut self) {
        self.reboot.asked = None;
        if self.confirmation.take() == Some(Confirmation::Write) {
            self.drop_unapproved_plan();
        }
    }

    pub fn saved_backups(&self) -> Vec<SavedBackup> {
        let root_images = self
            .pairs
            .iter()
            .filter(|pair| pair.kind == PairKind::DeviceReadback)
            .map(|pair| pair.created_unix);
        let kernel_images = self
            .kernel_sets
            .iter()
            .filter(|set| set.kind == PairKind::DeviceReadback)
            .map(|set| set.created_unix);
        SavedBackup::newest_first(root_images, kernel_images)
    }

    fn reload_backups(&mut self) {
        let profile = self.device_profile();
        let pairs = PairStore::new(self.paths.pairs(), profile).list();
        let kernel_sets = KernelStore::new(self.paths.kernel_sets(), profile).list();
        let pairs_error = pairs.as_ref().err().map(ToString::to_string);
        let kernel_error = kernel_sets.as_ref().err().map(ToString::to_string);
        self.store_error = pairs_error.or(kernel_error);
        self.pairs = pairs.unwrap_or_default();
        self.kernel_sets = kernel_sets.unwrap_or_default();
        self.chosen_folder_absent = self.chosen_folder_absent && !self.paths.root().is_dir();
    }

    fn back_up(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        self.activity = Some("BACKING UP");
        self.backup_error = None;
        let paths = self.paths.clone();
        run_blocking(move || worker::back_up(&paths), Message::BackedUp)
    }

    fn backed_up(&mut self, result: Outcome<()>) {
        self.activity = None;
        self.backup_error = result.err();
        self.reload_backups();
    }

    pub fn can_choose_data_folder(&self) -> bool {
        !self.is_busy() && !self.write.has_result()
    }

    fn pick_data_folder(&mut self) -> Task<Message> {
        if !self.can_choose_data_folder() {
            return Task::none();
        }
        Task::perform(pick_data_folder(), Message::DataFolderPicked)
    }

    fn data_folder_picked(&mut self, folder: Option<PathBuf>) {
        let (Some(folder), true) = (folder, self.can_choose_data_folder()) else {
            return;
        };
        let choice = FolderChoice::new(&folder);
        let written = self.choice_home.as_ref().map(|home| choice.write(home));
        self.choice_error = written.and_then(Result::err);
        self.choice_unread = None;
        self.chosen_folder_absent = false;
        self.paths = DataPaths::new(folder);
        self.backup_error = None;
        self.reload_backups();
    }

    fn pick_full_backup(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        Task::perform(pick_full_backup_folder(), Message::FullBackupPicked)
    }

    fn full_backup_picked(&mut self, folder: Option<PathBuf>) {
        let (Some(folder), false) = (folder, self.is_busy()) else {
            return;
        };
        self.full_backup_found = folder.join(BACKUP_MANIFEST_FILE).is_file();
        self.full_backup = Some(folder);
        self.write.full_backup_changed();
    }
}

async fn pick_data_folder() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("백업을 저장할 폴더")
        .pick_folder()
        .await
        .map(|folder| folder.path().to_path_buf())
}

async fn pick_full_backup_folder() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("sha256-manifest.json이 있는 기기 전체 백업 폴더")
        .pick_folder()
        .await
        .map(|folder| folder.path().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use front2::{DeviceState, SupportCheck, SupportReport};

    fn app() -> (tempfile::TempDir, App) {
        let directory = tempfile::tempdir().expect("tempdir");
        let app = App::with_paths(DataPaths::new(directory.path()));
        (directory, app)
    }

    #[test]
    fn doc_1051_a_new_device_reading_drops_the_force_state_and_its_question() {
        let (_directory, mut app) = app();
        app.forced = true;
        app.confirmation = Some(Confirmation::ForceSupport);
        let _ = app.update(Message::ReadDevice);
        assert!(app.is_busy());
        assert!(!app.forced && app.confirmation.is_none());
        let _ = app.update(Message::Confirm);
        assert!(!app.forced);
    }

    #[test]
    fn the_approval_holds_the_differences_only_while_the_force_state_is_on() {
        let (_directory, mut app) = app();
        let other_slot = SupportCheck::exact("ro.boot.slot_suffix", "_b", "_a");
        let report = SupportReport {
            checks: vec![other_slot.clone()],
        };
        let state = DeviceState {
            properties: Default::default(),
            shell_uid: None,
            root_access: None,
            magisk_version: None,
            kernel_release: None,
        };
        app.diagnosis = Some(Diagnosis {
            state,
            report,
            profile: &toss_front2::PROFILE,
        });
        assert!(app.support_override().is_empty());
        app.forced = true;
        assert_eq!(app.support_override().accepted, [other_slot]);
        let _ = app.update(Message::ReadDevice);
        assert!(app.support_override().is_empty());
    }

    #[test]
    fn doc_1051_a_confirmation_waits_while_an_operation_runs() {
        let (_directory, mut app) = app();
        app.activity = Some("BACKING UP");
        app.confirmation = Some(Confirmation::ForceSupport);
        let _ = app.update(Message::Confirm);
        assert_eq!(app.confirmation, Some(Confirmation::ForceSupport));
        assert!(!app.forced);
    }

    #[test]
    fn a_store_that_cannot_be_read_is_told_apart_from_a_stopped_backup() {
        let directory = tempfile::tempdir().expect("tempdir");
        let paths = DataPaths::new(directory.path());
        std::fs::create_dir_all(paths.pairs().parent().expect("data folder")).expect("folder");
        std::fs::write(paths.pairs(), b"a file where the store folder must be").expect("file");
        let app = App::with_paths(paths);
        assert!(app.store_error.is_some());
        assert!(app.backup_error.is_none());
    }

    fn folder_with_a_broken_store(parent: &std::path::Path, name: &str) -> PathBuf {
        let folder = parent.join(name);
        std::fs::create_dir_all(&folder).expect("folder");
        std::fs::write(
            folder.join("pairs"),
            b"a file where the store folder must be",
        )
        .expect("file");
        folder
    }

    #[test]
    fn a_chosen_data_folder_is_read_at_once_and_remembered() {
        let (directory, mut app) = app();
        let home = directory.path().join("program");
        std::fs::create_dir_all(&home).expect("folder");
        app.choice_home = Some(home.clone());
        let chosen = folder_with_a_broken_store(directory.path(), "chosen");
        assert!(app.store_error.is_none());
        let _ = app.update(Message::DataFolderPicked(Some(chosen.clone())));
        assert_eq!(app.paths.root(), chosen);
        assert!(app.store_error.is_some());
        assert!(app.choice_error.is_none());
        assert_eq!(App::beside_program(home).paths.root(), chosen);
    }

    #[test]
    fn the_folder_beside_the_program_is_used_until_a_folder_is_chosen() {
        let directory = tempfile::tempdir().expect("tempdir");
        let app = App::beside_program(directory.path().to_path_buf());
        let beside = DataPaths::beside_program(directory.path());
        assert_eq!(app.paths, beside);
        assert_eq!(app.choice_home.as_deref(), Some(directory.path()));
        assert!(app.choice_unread.is_none());
    }

    #[test]
    fn doc_1262_a_choice_that_cannot_be_read_is_told_until_a_folder_is_chosen() {
        let directory = tempfile::tempdir().expect("tempdir");
        let file = directory.path().join(crate::folder_choice::CHOICE_FILE);
        std::fs::write(file, b"{ not json").expect("file");
        let mut app = App::beside_program(directory.path().to_path_buf());
        assert_eq!(app.paths, DataPaths::beside_program(directory.path()));
        assert!(app.choice_unread.is_some());
        let chosen = directory.path().join("chosen");
        let _ = app.update(Message::DataFolderPicked(Some(chosen.clone())));
        assert!(app.choice_unread.is_none() && app.choice_error.is_none());
        assert_eq!(
            App::beside_program(directory.path().to_path_buf())
                .paths
                .root(),
            chosen
        );
    }

    #[test]
    fn doc_1062_a_remembered_folder_that_is_absent_is_told_until_it_is_there() {
        let directory = tempfile::tempdir().expect("tempdir");
        let home = directory.path().to_path_buf();
        assert!(!App::beside_program(home.clone()).chosen_folder_absent);
        let chosen = directory.path().join("chosen");
        FolderChoice::new(&chosen).write(&home).expect("choice");
        let mut app = App::beside_program(home.clone());
        assert_eq!(app.paths.root(), chosen);
        assert!(app.chosen_folder_absent);
        assert!(app.store_error.is_none() && app.saved_backups().is_empty());
        std::fs::create_dir_all(&chosen).expect("folder");
        let _ = app.update(Message::BackedUp(Ok(())));
        assert!(!app.chosen_folder_absent);
        assert!(!App::beside_program(home).chosen_folder_absent);
    }

    #[test]
    fn doc_1062_a_newly_chosen_folder_is_not_told_as_absent() {
        let directory = tempfile::tempdir().expect("tempdir");
        let home = directory.path().to_path_buf();
        FolderChoice::new(directory.path().join("absent"))
            .write(&home)
            .expect("choice");
        let mut app = App::beside_program(home);
        assert!(app.chosen_folder_absent);
        let other = directory.path().join("other");
        let _ = app.update(Message::DataFolderPicked(Some(other)));
        assert!(!app.chosen_folder_absent);
    }

    #[test]
    fn a_choice_that_cannot_be_remembered_is_used_and_told() {
        let (directory, mut app) = app();
        app.choice_home = Some(directory.path().join("absent"));
        let chosen = directory.path().join("chosen");
        let _ = app.update(Message::DataFolderPicked(Some(chosen.clone())));
        assert_eq!(app.paths.root(), chosen);
        assert!(app.choice_error.is_some());
        app.choice_home = Some(directory.path().to_path_buf());
        let _ = app.update(Message::DataFolderPicked(Some(chosen)));
        assert!(app.choice_error.is_none());
    }

    #[test]
    fn the_data_folder_stays_while_an_operation_runs_or_a_plan_is_open() {
        let (directory, mut app) = app();
        let chosen = directory.path().join("chosen");
        app.activity = Some("BACKING UP");
        let _ = app.update(Message::DataFolderPicked(Some(chosen.clone())));
        assert_eq!(app.paths.root(), directory.path());
        app.activity = None;
        app.write.review = Some(crate::write_flow::tests::review());
        assert!(!app.can_choose_data_folder());
        let _ = app.update(Message::DataFolderPicked(Some(chosen.clone())));
        assert_eq!(app.paths.root(), directory.path());
        let _ = app.update(Message::DataFolderPicked(None));
        assert_eq!(app.paths.root(), directory.path());
        app.write.review = None;
        let _ = app.update(Message::DataFolderPicked(Some(chosen.clone())));
        assert_eq!(app.paths.root(), chosen);
    }
}
