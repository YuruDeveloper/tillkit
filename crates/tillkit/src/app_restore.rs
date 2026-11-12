use crate::app::App;
use crate::message::Message;
use crate::plan_review::PlanReview;
use crate::restore_session::RestoreSession;
use crate::worker::run_blocking;
use crate::write_flow::WriteFlow;
use crate::write_message::WriteMessage;
use crate::write_step::WriteStep;
use iced::Task;

impl App {
    pub fn can_start_restore(&self) -> bool {
        !self.is_busy() && !self.write.has_result() && !self.support_stops_writes()
    }

    pub fn start_restore(&mut self, created_unix: u64) -> Task<Message> {
        let is_saved = self
            .saved_backups()
            .iter()
            .any(|backup| backup.created_unix == created_unix);
        if !self.can_start_restore() || !is_saved {
            return Task::none();
        }
        self.write = WriteFlow::for_backup(created_unix);
        self.write.running = Some(WriteStep::Prepare);
        let paths = self.paths.clone();
        let accepted = self.support_override();
        run_blocking(
            move || RestoreSession::open(accepted)?.plan(&paths, created_unix),
            prepared,
        )
    }
}

fn prepared(result: Result<PlanReview, String>) -> Message {
    Message::Write(WriteMessage::Prepared(result.map(Box::new)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confirmation::Confirmation;
    use crate::data_paths::DataPaths;
    use crate::worker::tests::pair;
    use crate::write_flow::tests::review;
    use crate::write_origin::WriteOrigin;
    use front2::PairKind;

    const SAVED: u64 = 1_794_412_800;

    fn app() -> (tempfile::TempDir, App) {
        let directory = tempfile::tempdir().expect("tempdir");
        let mut app = App::with_paths(DataPaths::new(directory.path()));
        let mut saved = pair("readback-1", PairKind::DeviceReadback);
        saved.created_unix = SAVED;
        app.pairs = vec![saved];
        (directory, app)
    }

    #[test]
    fn a_restore_starts_for_a_saved_backup_only() {
        let (_directory, mut app) = app();
        let _ = app.update(Message::RestoreBackup(SAVED + 1));
        assert!(!app.is_busy(), "no backup has this time");
        let _ = app.update(Message::RestoreBackup(SAVED));
        assert_eq!(app.write.origin, WriteOrigin::Backup);
        assert_eq!(app.write.restored_backup, Some(SAVED));
        assert_eq!(app.write.running, Some(WriteStep::Prepare));
        assert!(app.other_work_is_open(WriteOrigin::Image));
        assert!(app.other_work_is_open(WriteOrigin::Magisk));
    }

    #[test]
    fn a_restore_waits_for_work_that_another_screen_opened() {
        let (_directory, mut app) = app();
        app.write.review = Some(review());
        assert!(!app.can_start_restore());
        let _ = app.update(Message::RestoreBackup(SAVED));
        assert_eq!(app.write.origin, WriteOrigin::Image);
        assert!(app.write.running.is_none());
    }

    #[test]
    fn a_prepared_restore_asks_before_the_write() {
        let (_directory, mut app) = app();
        let _ = app.update(Message::RestoreBackup(SAVED));
        let _ = app.update(prepared(Ok(review())));
        assert_eq!(app.confirmation, Some(Confirmation::Write));
        assert!(app.write.is_unapproved() && !app.is_busy());
        assert_eq!(app.write.restored_backup, Some(SAVED));
        let _ = app.update(Message::Cancel);
        assert_eq!(app.write.origin, WriteOrigin::Image);
        assert!(app.write.restored_backup.is_none() && app.can_start_restore());
    }

    #[test]
    fn a_failed_restore_preparation_can_start_again() {
        let (_directory, mut app) = app();
        let _ = app.update(Message::RestoreBackup(SAVED));
        let _ = app.update(prepared(Err("no device".into())));
        assert!(app.write.failure.is_some() && app.can_start_restore());
        assert!(!app.other_work_is_open(WriteOrigin::Image));
    }
}
