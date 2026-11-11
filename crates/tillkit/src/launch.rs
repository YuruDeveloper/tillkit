use crate::app::App;
use crate::data_paths::DataPaths;
use crate::palette::{DUSK, PAPER, Palette};
use crate::screen::Screen;

pub const DATA_DIRECTORY_VARIABLE: &str = "TILLKIT_DATA_DIR";
pub const SCREEN_VARIABLE: &str = "TILLKIT_SCREEN";
pub const THEME_VARIABLE: &str = "TILLKIT_THEME";

pub fn paths() -> DataPaths {
    std::env::var_os(DATA_DIRECTORY_VARIABLE).map_or_else(DataPaths::for_user, DataPaths::new)
}

pub fn screen() -> Screen {
    match std::env::var(SCREEN_VARIABLE).as_deref() {
        Ok("backup") => Screen::Backup,
        Ok("write") => Screen::Write,
        Ok("info") => Screen::Info,
        _ => Screen::Device,
    }
}

pub fn palette() -> Palette {
    match std::env::var(THEME_VARIABLE).as_deref() {
        Ok("dusk") => DUSK,
        Ok("paper") => PAPER,
        _ => Palette::for_system(),
    }
}

#[cfg(not(debug_assertions))]
pub fn preview(_app: &mut App) {}

#[cfg(debug_assertions)]
pub fn preview(app: &mut App) {
    use crate::confirmation::Confirmation;
    use crate::diagnosis::Diagnosis;
    use crate::message::Message;
    use crate::plan_review::PlanReview;
    use crate::plan_source::PlanSource;
    use crate::write_flow::WriteFlow;
    use crate::write_step::WriteStep;
    use front2::kernel::KernelPlanFacts;
    use front2::{ApplyOutcome, ApplyState, DeviceState, SupportReport, WritePlan};
    use std::path::PathBuf;

    fn outcome(review: &PlanReview, state: ApplyState, matches: bool) -> ApplyOutcome {
        ApplyOutcome {
            plan_id: review.plan.id.clone(),
            plan_sha256: review.sha256.clone(),
            state,
            device_log: "WRITING_INIT_BOOT\nWRITING_VBMETA".into(),
            readback_sha256: Vec::new(),
            readback_matches_new_pair: matches,
            reboot_requested: false,
        }
    }

    fn pose(app: &mut App, review: PlanReview, pose: &str) {
        let verified = outcome(&review, ApplyState::VerifiedPendingReboot, true);
        let rolled_back = outcome(&review, ApplyState::RollbackVerified, false);
        let unknown = outcome(&review, ApplyState::Writing("WRITING_VBMETA".into()), false);
        let flow = &mut app.write;
        flow.review = Some(review);
        if pose == "confirm" {
            app.confirmation = Some(Confirmation::Write);
            return;
        }
        flow.approve();
        let upload_failed = (WriteStep::Stage, "adb push: no space left".to_string());
        match pose {
            "uploading" => flow.running = Some(WriteStep::Stage),
            "upload-failed" => flow.failure = Some(upload_failed),
            "writing" => flow.running = Some(WriteStep::Write),
            "verified" | "reboot" | "rebooted" => flow.outcome = Some(verified),
            "rolled-back" => flow.outcome = Some(rolled_back),
            "unknown" => flow.outcome = Some(unknown),
            _ => {}
        }
        flow.staged = !matches!(pose, "uploading" | "upload-failed");
        flow.rebooted = pose == "rebooted";
        if pose == "reboot" {
            app.confirmation = Some(Confirmation::Reboot);
        }
    }

    fn preview_source(plan: &WritePlan) -> PlanSource {
        read::<KernelPlanFacts>("TILLKIT_PREVIEW_KERNEL_FACTS")
            .filter(|facts| facts.require_matches(plan).is_ok())
            .map(Box::new)
            .map_or(PlanSource::Pair, PlanSource::Kernel)
    }

    fn read<T: serde::de::DeserializeOwned>(variable: &str) -> Option<T> {
        let path = std::env::var_os(variable)?;
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    if let Some(state) = read::<DeviceState>("TILLKIT_PREVIEW_DEVICE") {
        let report = SupportReport::evaluate(&state);
        app.diagnosis = Some(Diagnosis { state, report });
    }
    app.details_open = std::env::var_os("TILLKIT_PREVIEW_DETAILS").is_some();
    if let Some(folder) = std::env::var_os("TILLKIT_PREVIEW_FULL_BACKUP").map(PathBuf::from) {
        let _ = app.update(Message::FullBackupPicked(Some(folder)));
    }
    if let Some(folder) = std::env::var_os("TILLKIT_PREVIEW_IMAGES").map(PathBuf::from) {
        app.write = WriteFlow::for_folder(folder);
    }
    match std::env::var("TILLKIT_PREVIEW_FORCE").as_deref() {
        Ok("ask") => app.confirmation = Some(Confirmation::ForceSupport),
        Ok("forced") => app.forced = true,
        _ => {}
    }
    if let Some(plan) = read::<WritePlan>("TILLKIT_PREVIEW_PLAN") {
        let source = preview_source(&plan);
        let review = PlanReview {
            plan,
            sha256: "5d1f0c9a".repeat(8),
            directory: PathBuf::from("preview"),
            source,
        };
        let chosen = std::env::var("TILLKIT_PREVIEW_WRITE").unwrap_or_default();
        pose(app, review, &chosen);
    }
    if let Ok(error) = std::env::var("TILLKIT_PREVIEW_PREPARE_ERROR") {
        app.write.failure = Some((WriteStep::Prepare, error));
    }
}
