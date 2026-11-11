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
        Ok("pairs") => Screen::Pairs,
        Ok("kernel") => Screen::Kernel,
        Ok("write") => Screen::Write,
        Ok("gsi") => Screen::Gsi,
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
    use crate::gsi_message::GsiMessage;
    use crate::install_review::InstallReview;
    use crate::message::Message;
    use crate::plan_review::PlanReview;
    use crate::plan_source::PlanSource;
    use front2::install::InstallPlan;
    use front2::kernel::KernelPlanFacts;
    use front2::{DeviceState, SupportReport, WritePlan};
    use std::path::PathBuf;

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
        let _ = app.update(Message::Gsi(GsiMessage::BackupPicked(Some(folder))));
    }
    match std::env::var("TILLKIT_PREVIEW_FORCE").as_deref() {
        Ok("ask") => app.confirmation = Some(Confirmation::ForceSupport),
        Ok("forced") => app.forced = true,
        _ => {}
    }
    if let Some(plan) = read::<WritePlan>("TILLKIT_PREVIEW_PLAN") {
        let source = preview_source(&plan);
        app.write.review = Some(PlanReview {
            plan,
            sha256: "5d1f0c9a".repeat(8),
            directory: PathBuf::from("preview"),
            source,
        });
        app.write.staged = std::env::var_os("TILLKIT_PREVIEW_STAGED").is_some();
    }
    if let Some(directory) = std::env::var_os("TILLKIT_PREVIEW_INSTALL_PLAN").map(PathBuf::from) {
        app.gsi.install = InstallPlan::load(&directory)
            .ok()
            .map(|stored| InstallReview {
                plan: stored.plan().clone(),
                sha256: stored.plan_sha256().to_string(),
                directory,
            });
    }
    if std::env::var_os("TILLKIT_PREVIEW_SELECT").is_some() {
        app.selected_pair = app.pairs.first().map(|pair| pair.id.clone());
        app.selected_kernel_set = app.kernel_sets.first().map(|set| set.id.clone());
    }
}
