use crate::app::App;
use crate::data_paths::DataPaths;
use crate::palette::{DUSK, PAPER, Palette};
use crate::screen::Screen;

pub const DATA_DIRECTORY_VARIABLE: &str = "FRONTBOX_DATA_DIR";
pub const SCREEN_VARIABLE: &str = "FRONTBOX_SCREEN";
pub const THEME_VARIABLE: &str = "FRONTBOX_THEME";

pub fn paths() -> DataPaths {
    std::env::var_os(DATA_DIRECTORY_VARIABLE).map_or_else(DataPaths::for_user, DataPaths::new)
}

pub fn screen() -> Screen {
    match std::env::var(SCREEN_VARIABLE).as_deref() {
        Ok("pairs") => Screen::Pairs,
        Ok("write") => Screen::Write,
        Ok("scope") => Screen::Scope,
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
    use crate::diagnosis::Diagnosis;
    use crate::plan_review::PlanReview;
    use front2::{DeviceState, SupportReport, WritePlan};
    use std::path::PathBuf;

    fn read<T: serde::de::DeserializeOwned>(variable: &str) -> Option<T> {
        let path = std::env::var_os(variable)?;
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    if let Some(state) = read::<DeviceState>("FRONTBOX_PREVIEW_DEVICE") {
        let report = SupportReport::evaluate(&state);
        app.diagnosis = Some(Diagnosis { state, report });
    }
    if let Some(plan) = read::<WritePlan>("FRONTBOX_PREVIEW_PLAN") {
        app.write.review = Some(PlanReview {
            plan,
            sha256: "5d1f0c9a".repeat(8),
            directory: PathBuf::from("preview"),
        });
        app.write.staged = std::env::var_os("FRONTBOX_PREVIEW_STAGED").is_some();
    }
    if std::env::var_os("FRONTBOX_PREVIEW_SELECT").is_some() {
        app.selected_pair = app.pairs.first().map(|pair| pair.id.clone());
    }
}
