use crate::app::App;
use crate::kernel_message::KernelMessage;
use crate::kernel_worker;
use crate::message::Message;
use crate::screen::Screen;
use crate::worker::run_blocking;
use crate::write_flow::WriteFlow;
use crate::write_step::WriteStep;
use front2::PairKind;
use front2::kernel::{KernelSet, KernelStore};
use iced::Task;
use std::path::PathBuf;

impl App {
    pub fn update_kernel(&mut self, message: KernelMessage) -> Task<Message> {
        match message {
            KernelMessage::Select(id) => self.selected_kernel_set = Some(id),
            KernelMessage::Stored(result) => self.kernel_set_stored(result),
            KernelMessage::ReadCurrent => return self.read_current_kernel_set(),
            KernelMessage::PickBootImage => return self.pick_boot_image(),
            KernelMessage::BootImagePicked(path) => return self.build_kernel_candidate(path),
            KernelMessage::PlanWrite => return self.plan_kernel_write(),
        }
        Task::none()
    }

    pub fn selected_kernel(&self) -> Option<&KernelSet> {
        let id = self.selected_kernel_set.as_deref()?;
        self.kernel_sets.iter().find(|set| set.id == id)
    }

    pub fn selected_kernel_readback(&self) -> Option<&KernelSet> {
        self.selected_kernel()
            .filter(|set| set.kind == PairKind::DeviceReadback)
    }

    pub fn reload_kernel_sets(&mut self) {
        match KernelStore::new(self.paths.kernel_sets()).list() {
            Ok(sets) => self.kernel_sets = sets,
            Err(error) => self.kernel_error = Some(error.to_string()),
        }
    }

    fn read_current_kernel_set(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        self.activity = Some("READING CURRENT KERNEL SET");
        self.kernel_error = None;
        let paths = self.paths.clone();
        let note = self.note_input.clone();
        run_blocking(
            move || kernel_worker::read_current_set(&paths, &note),
            stored,
        )
    }

    fn pick_boot_image(&mut self) -> Task<Message> {
        if self.is_busy() || self.selected_kernel_readback().is_none() {
            return Task::none();
        }
        Task::perform(pick_image(), |path| {
            Message::Kernel(KernelMessage::BootImagePicked(path))
        })
    }

    fn build_kernel_candidate(&mut self, new_boot: Option<PathBuf>) -> Task<Message> {
        let original = self.selected_kernel_readback().map(|set| set.id.clone());
        let (Some(new_boot), Some(original), false) = (new_boot, original, self.is_busy()) else {
            return Task::none();
        };
        self.activity = Some("BUILDING KERNEL CANDIDATE");
        self.kernel_error = None;
        let paths = self.paths.clone();
        let note = self.note_input.clone();
        run_blocking(
            move || kernel_worker::build_candidate(&paths, &original, &new_boot, &note),
            stored,
        )
    }

    fn kernel_set_stored(&mut self, result: Result<Box<KernelSet>, String>) {
        self.activity = None;
        match result {
            Ok(set) => self.selected_kernel_set = Some(set.id),
            Err(error) => self.kernel_error = Some(error),
        }
        if self.kernel_error.is_none() {
            self.note_input.clear();
        }
        self.reload_kernel_sets();
    }

    fn plan_kernel_write(&mut self) -> Task<Message> {
        let Some(new_set) = self.selected_kernel_set.clone() else {
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
            move || kernel_worker::plan_write(&paths, &new_set),
            Message::PlanReady,
        )
    }
}

fn stored(result: Result<KernelSet, String>) -> Message {
    Message::Kernel(KernelMessage::Stored(result.map(Box::new)))
}

async fn pick_image() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("현재 boot의 header를 유지하고 kernel만 바꾼 boot 이미지")
        .add_filter("boot image", &["img"])
        .pick_file()
        .await
        .map(|file| file.path().to_path_buf())
}
