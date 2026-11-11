use crate::app::App;
use crate::confirmation::Confirmation;
use crate::message::Message;
use crate::reboot::{DeviceConnection, RebootTarget, reboot_device};
use crate::reboot_message::RebootMessage;
use crate::worker::run_blocking;
use iced::Task;
use std::path::PathBuf;

type Outcome<T> = Result<T, String>;

impl App {
    pub fn update_reboot(&mut self, message: RebootMessage) -> Task<Message> {
        match message {
            RebootMessage::Detected(result) => self.connection_detected(result),
            RebootMessage::LoaderPicked(file) => self.loader_picked(file),
            RebootMessage::Ask(target) => self.ask_reboot_to(target),
            RebootMessage::Sent(result) => self.reboot_sent_to(result),
            RebootMessage::Detect => return self.detect_connection(),
            RebootMessage::PickLoader => return self.pick_loader(),
        }
        Task::none()
    }

    pub fn reboot_is_held(&self) -> bool {
        self.write.write_is_uncertain()
    }

    pub fn can_reboot_to(&self, target: RebootTarget) -> bool {
        !self.is_busy() && !self.reboot_is_held() && self.reboot.can_ask(target)
    }

    fn detect_connection(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        self.activity = Some("CHECKING CONNECTION");
        self.reboot.failure = None;
        self.reboot.sent = None;
        run_blocking(
            || Ok(DeviceConnection::detect()),
            |result| Message::Reboot(RebootMessage::Detected(result)),
        )
    }

    fn connection_detected(&mut self, result: Outcome<DeviceConnection>) {
        self.activity = None;
        match result {
            Ok(connection) => self.reboot.connection = Some(connection),
            Err(error) => self.connection_lost(error),
        }
    }

    fn connection_lost(&mut self, error: String) {
        self.reboot.connection = None;
        self.reboot.failure = Some(error);
    }

    fn pick_loader(&mut self) -> Task<Message> {
        if self.is_busy() {
            return Task::none();
        }
        Task::perform(pick_loader_file(), |file| {
            Message::Reboot(RebootMessage::LoaderPicked(file))
        })
    }

    fn loader_picked(&mut self, file: Option<PathBuf>) {
        if let (Some(file), false) = (file, self.is_busy()) {
            self.reboot.loader = Some(file);
        }
    }

    fn ask_reboot_to(&mut self, target: RebootTarget) {
        if self.can_reboot_to(target) {
            self.reboot.asked = Some(target);
            self.confirmation = Some(Confirmation::RebootTo);
        }
    }

    pub fn reboot_to(&mut self) -> Task<Message> {
        let asked = self.reboot.asked.take();
        let (Some(target), Some(connection)) = (asked, self.reboot.connection) else {
            return Task::none();
        };
        if !self.can_reboot_to(target) {
            return Task::none();
        }
        self.activity = Some("REBOOTING");
        self.reboot.failure = None;
        self.reboot.sent = None;
        let loader = self.reboot.loader.clone();
        run_blocking(
            move || reboot_device(connection, target, loader.as_deref()).map(|()| target),
            |result| Message::Reboot(RebootMessage::Sent(result)),
        )
    }

    fn reboot_sent_to(&mut self, result: Outcome<RebootTarget>) {
        self.activity = None;
        self.reboot.connection = None;
        match result {
            Ok(target) => self.reboot.sent = Some(target),
            Err(error) => self.reboot.failure = Some(error),
        }
    }
}

async fn pick_loader_file() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("이 기기의 Firehose 로더 파일")
        .pick_file()
        .await
        .map(|file| file.path().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_paths::DataPaths;
    use crate::write_flow::tests::review;
    use crate::write_step::WriteStep;

    fn app() -> (tempfile::TempDir, App) {
        let directory = tempfile::tempdir().expect("tempdir");
        let app = App::with_paths(DataPaths::new(directory.path()));
        (directory, app)
    }

    fn ask(target: RebootTarget) -> Message {
        Message::Reboot(RebootMessage::Ask(target))
    }

    #[test]
    fn a_reboot_needs_a_known_connection_and_an_answer() {
        let (_directory, mut app) = app();
        let _ = app.update(ask(RebootTarget::Recovery));
        assert!(app.confirmation.is_none(), "the connection is not known");
        app.reboot.connection = Some(DeviceConnection::Android);
        let _ = app.update(ask(RebootTarget::Recovery));
        assert_eq!(app.confirmation, Some(Confirmation::RebootTo));
        assert!(!app.is_busy(), "nothing is sent before the answer");
        let _ = app.update(Message::Cancel);
        assert!(app.reboot.asked.is_none() && app.confirmation.is_none());
        let _ = app.update(Message::Confirm);
        assert!(!app.is_busy(), "a cancelled question sends nothing");
        let _ = app.update(ask(RebootTarget::Recovery));
        let _ = app.update(Message::Confirm);
        assert_eq!(app.activity, Some("REBOOTING"));
        assert!(app.reboot.asked.is_none());
    }

    #[test]
    fn a_reboot_is_held_while_a_write_has_no_known_result() {
        let (_directory, mut app) = app();
        app.reboot.connection = Some(DeviceConnection::Android);
        app.write.review = Some(review());
        app.write.failure = Some((WriteStep::Write, "the connection closed".into()));
        assert!(app.reboot_is_held());
        let _ = app.update(ask(RebootTarget::System));
        assert!(app.confirmation.is_none());
        app.reboot.asked = Some(RebootTarget::System);
        let _ = app.update(Message::Confirm);
        assert!(app.activity.is_none());
        app.write.running = Some(WriteStep::Write);
        app.write.failure = None;
        assert!(!app.can_reboot_to(RebootTarget::System), "a write runs");
    }

    #[test]
    fn a_sent_reboot_drops_the_connection_that_it_used() {
        let (_directory, mut app) = app();
        app.reboot.connection = Some(DeviceConnection::Android);
        app.activity = Some("REBOOTING");
        let sent = RebootMessage::Sent(Ok(RebootTarget::Edl));
        let _ = app.update(Message::Reboot(sent));
        assert_eq!(app.reboot.sent, Some(RebootTarget::Edl));
        assert!(app.reboot.connection.is_none() && !app.is_busy());
        assert!(!app.can_reboot_to(RebootTarget::System));
        let failed = RebootMessage::Sent(Err("no device".into()));
        let _ = app.update(Message::Reboot(failed));
        assert!(app.reboot.failure.is_some());
    }
}
