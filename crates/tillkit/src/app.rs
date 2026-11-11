use crate::clock::now_unix;
use crate::confirmation::Confirmation;
use crate::data_paths::DataPaths;
use crate::diagnosis::Diagnosis;
use crate::grain;
use crate::gsi_flow::GsiFlow;
use crate::gsi_worker;
use crate::launch;
use crate::message::Message;
use crate::palette::Palette;
use crate::plan_review::PlanReview;
use crate::saved_backup::SavedBackup;
use crate::screen::Screen;
use crate::support_words;
use crate::view;
use crate::worker::{self, run_blocking};
use crate::write_flow::WriteFlow;
use crate::write_step::WriteStep;
use front2::kernel::KernelSet;
use front2::{ApplyOutcome, PairKind, PairManifest, PairStore};
use iced::widget::image::Handle;
use iced::{Element, Task, Theme};
use std::path::PathBuf;

type Outcome<T> = Result<T, String>;

pub struct App {
    pub palette: Palette,
    pub grain: Handle,
    pub screen: Screen,
    pub paths: DataPaths,
    pub activity: Option<&'static str>,
    pub diagnosis: Option<Diagnosis>,
    pub device_error: Option<String>,
    pub details_open: bool,
    pub forced: bool,
    pub confirmation: Option<Confirmation>,
    pub backup_error: Option<String>,
    pub full_backup_found: bool,
    pub pairs: Vec<PairManifest>,
    pub pairs_error: Option<String>,
    pub selected_pair: Option<String>,
    pub note_input: String,
    pub kernel_sets: Vec<KernelSet>,
    pub kernel_error: Option<String>,
    pub selected_kernel_set: Option<String>,
    pub write: WriteFlow,
    pub gsi: GsiFlow,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let mut app = Self::with_paths(launch::paths());
        launch::preview(&mut app);
        (app, Task::none())
    }

    pub fn with_paths(paths: DataPaths) -> Self {
        let palette = launch::palette();
        let mut app = Self {
            palette,
            grain: grain::handle(palette),
            screen: launch::screen(),
            paths,
            activity: None,
            diagnosis: None,
            device_error: None,
            details_open: false,
            forced: false,
            confirmation: None,
            backup_error: None,
            full_backup_found: false,
            pairs: Vec::new(),
            pairs_error: None,
            selected_pair: None,
            note_input: String::new(),
            kernel_sets: Vec::new(),
            kernel_error: None,
            selected_kernel_set: None,
            write: WriteFlow::default(),
            gsi: GsiFlow::default(),
        };
        app.reload_pairs();
        app.reload_kernel_sets();
        app.gsi.review = gsi_worker::newest_candidate(&app.paths);
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

    pub fn selected(&self) -> Option<&PairManifest> {
        let id = self.selected_pair.as_deref()?;
        self.pairs.iter().find(|pair| pair.id == id)
    }

    pub fn selected_readback(&self) -> Option<&PairManifest> {
        self.selected()
            .filter(|pair| pair.kind == PairKind::DeviceReadback)
    }

    fn reload_pairs(&mut self) {
        match PairStore::new(self.paths.pairs()).list() {
            Ok(pairs) => self.pairs = newest_first(pairs),
            Err(error) => self.pairs_error = Some(error.to_string()),
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(screen) => self.screen = screen,
            Message::ToggleTheme => self.set_palette(self.palette.toggled()),
            Message::BackedUp(result) => self.backed_up(result),
            Message::SelectPair(id) => self.selected_pair = Some(id),
            Message::NoteChanged(note) => self.note_input = note,
            Message::ApprovalChanged(text) => self.write.approval_input = text,
            Message::RebootChanged(text) => self.write.reboot_input = text,
            Message::DiscardPlan => self.discard_plan(),
            Message::DeviceRead(result) => self.device_read(result),
            Message::ToggleDetails => self.details_open = !self.details_open,
            Message::AskForce => self.ask_force(),
            Message::Confirm => self.confirm(),
            Message::Cancel => self.confirmation = None,
            Message::PairStored(result) => self.pair_stored(result),
            Message::PlanReady(result) => self.plan_ready(result),
            Message::PlanStaged(result) => self.plan_staged(result),
            Message::PairWritten(result) => self.pair_written(result),
            Message::Rebooted(result) => self.rebooted(result),
            Message::ReadDevice => return self.read_device(),
            Message::BackUp => return self.back_up(),
            Message::ReadCurrentPair => return self.read_current_pair(),
            Message::PickPatchedImage => return self.pick_patched_image(),
            Message::PatchedImagePicked(path) => return self.build_candidate(path),
            Message::PlanWrite => return self.plan_write(),
            Message::StagePlan => return self.stage_plan(),
            Message::WritePair => return self.write_pair(),
            Message::Reboot => return self.reboot(),
            Message::Gsi(message) => return self.update_gsi(message),
            Message::Kernel(message) => return self.update_kernel(message),
        }
        Task::none()
    }

    fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
        self.grain = grain::handle(palette);
    }

    fn discard_plan(&mut self) {
        if self.write.running.is_none() {
            self.write = WriteFlow::default();
        }
    }

    fn read_device(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        self.activity = Some("READING DEVICE");
        self.device_error = None;
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

    fn confirm(&mut self) {
        match self.confirmation.take() {
            Some(Confirmation::ForceSupport) => self.forced = self.can_be_forced(),
            None => {}
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
        self.reload_pairs();
        self.reload_kernel_sets();
    }

    fn read_current_pair(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        self.activity = Some("READING CURRENT PAIR");
        self.pairs_error = None;
        let paths = self.paths.clone();
        let note = self.note_input.clone();
        run_blocking(
            move || worker::read_current_pair(&paths, &note),
            Message::PairStored,
        )
    }

    fn pick_patched_image(&mut self) -> Task<Message> {
        if self.is_busy() || self.selected_readback().is_none() {
            return Task::none();
        }
        Task::perform(pick_image(), Message::PatchedImagePicked)
    }

    fn build_candidate(&mut self, patched: Option<PathBuf>) -> Task<Message> {
        let original = self.selected_readback().map(|pair| pair.id.clone());
        let (Some(patched), Some(original), false) = (patched, original, self.is_busy()) else {
            return Task::none();
        };
        self.activity = Some("BUILDING CANDIDATE");
        self.pairs_error = None;
        let paths = self.paths.clone();
        let note = self.note_input.clone();
        run_blocking(
            move || worker::build_candidate(&paths, &original, &patched, &note),
            Message::PairStored,
        )
    }

    fn pair_stored(&mut self, result: Outcome<PairManifest>) {
        self.activity = None;
        match result {
            Ok(manifest) => self.selected_pair = Some(manifest.id),
            Err(error) => self.pairs_error = Some(error),
        }
        if self.pairs_error.is_none() {
            self.note_input.clear();
        }
        self.reload_pairs();
    }

    fn plan_write(&mut self) -> Task<Message> {
        let Some(new_pair) = self.selected_pair.clone() else {
            return Task::none();
        };
        if self.is_busy() || self.write.review.is_some() {
            return Task::none();
        }
        self.screen = Screen::Write;
        self.write = WriteFlow {
            running: Some(WriteStep::Plan),
            ..WriteFlow::default()
        };
        let paths = self.paths.clone();
        run_blocking(
            move || worker::plan_write(&paths, &new_pair),
            Message::PlanReady,
        )
    }

    fn plan_ready(&mut self, result: Outcome<PlanReview>) {
        self.write.running = None;
        match result {
            Ok(review) => self.write.review = Some(review),
            Err(error) => self.write.failure = Some((WriteStep::Plan, error)),
        }
    }

    fn stage_plan(&mut self) -> Task<Message> {
        let ready = self.write.next_step() == Some(WriteStep::Stage) && !self.is_busy();
        let Some(review) = self.write.review.clone().filter(|_| ready) else {
            return Task::none();
        };
        self.write.running = Some(WriteStep::Stage);
        let paths = self.paths.clone();
        run_blocking(
            move || worker::stage_plan(&paths, &review),
            Message::PlanStaged,
        )
    }

    fn plan_staged(&mut self, result: Outcome<()>) {
        self.write.running = None;
        match result {
            Ok(()) => self.write.staged = true,
            Err(error) => self.write.failure = Some((WriteStep::Stage, error)),
        }
    }

    fn write_pair(&mut self) -> Task<Message> {
        let authorization = self.write.write_authorization(now_unix());
        let review = self.write.review.clone();
        let (Some(authorization), Some(review), false) = (authorization, review, self.is_busy())
        else {
            return Task::none();
        };
        self.write.running = Some(WriteStep::Write);
        self.write.approval_input.clear();
        run_blocking(
            move || worker::write_pair(&review, &authorization),
            Message::PairWritten,
        )
    }

    fn pair_written(&mut self, result: Outcome<ApplyOutcome>) {
        self.write.running = None;
        match result {
            Ok(outcome) => self.write.outcome = Some(outcome),
            Err(error) => self.write.failure = Some((WriteStep::Write, error)),
        }
    }

    fn reboot(&mut self) -> Task<Message> {
        let authorization = self.write.reboot_authorization(now_unix());
        let review = self.write.review.clone();
        let outcome = self.write.outcome.clone();
        let (Some(authorization), Some(review), Some(outcome), false) =
            (authorization, review, outcome, self.is_busy())
        else {
            return Task::none();
        };
        self.write.running = Some(WriteStep::Reboot);
        self.write.reboot_input.clear();
        run_blocking(
            move || worker::reboot(&review, &authorization, outcome),
            Message::Rebooted,
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

fn newest_first(mut pairs: Vec<PairManifest>) -> Vec<PairManifest> {
    pairs.sort_by_key(|pair| std::cmp::Reverse(pair.created_unix));
    pairs
}

async fn pick_image() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Magisk 앱이 패치한 init_boot 이미지")
        .add_filter("boot image", &["img"])
        .pick_file()
        .await
        .map(|file| file.path().to_path_buf())
}
