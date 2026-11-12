use crate::app::App;
use crate::install_message::InstallMessage;
use crate::install_stage::InstallStage;
use crate::message::Message;
use crate::palette::Palette;
use crate::reboot_message::RebootMessage;
use crate::step_status::StepStatus;
use crate::view::{flow, install_resume, parts};
use iced::widget::{Column, Row, Space, column, container, row};
use iced::{Alignment, Element, Length};
use std::path::PathBuf;

pub const SPEC: &str = "EDL · FULL BACKUP FIRST · ERASES USER DATA";
pub const INTRODUCTION: &str = "GSI 이미지를 기기에 설치합니다. 기기를 EDL 모드로 연결하고, 기기 전체를 PC에 백업한 뒤에 기록합니다. 설치하면 기기의 사용자 데이터가 지워집니다. 기기에 기록하기 전에 한 번 더 확인을 받습니다.";
const HOW_TO_CONNECT: &str = "기기가 Android로 켜져 있으면 ADVANCED 화면에서 EDL 모드로 다시 시작할 수 있습니다. 로더 파일은 이 기기의 Firehose 로더 파일입니다.";
const NOT_CHOSEN: &str = "고르지 않음";
const NOT_FOUND: &str = "아직 없음";
const NO_FULL_BACKUP: &str = "고른 폴더는 설치에 쓸 수 있는 전체 백업이 아닙니다. 다른 폴더를 고르거나, 전체 백업을 새로 만들어 주세요.";
const SINGLE_READ: &str = "크기 때문에 한 번만 읽어서, 다시 읽어 대조하지 못한 영역";
const CHECKED: &str = "기기의 내용이 이 계획과 맞습니다.";
const READY: &str = "기록을 시작할 수 있습니다.";
const RESTART_SENT: &str = "다시 시작을 요청했습니다. 첫 부팅은 사용자 데이터를 지우느라 오래 걸릴 수 있습니다. 기기가 GSI로 부팅했는지는 TillKit이 확인하지 못하므로, 기기에서 직접 확인해 주세요.";
const METER_PARTS: u16 = 1000;
const METER_LEAST: u16 = 4;
const GIBIBYTE: f64 = 1_073_741_824.0;

pub fn steps<'a>(app: &'a App, mut page: Column<'a, Message>) -> Column<'a, Message> {
    let palette = app.palette;
    let install = &app.install;
    if install.is_finished() {
        page = page.push(flow::settled("RESTART REQUESTED", RESTART_SENT, palette));
    }
    let rows = InstallStage::ALL
        .into_iter()
        .map(|stage| stage_row(app, stage))
        .collect();
    let mut page = page
        .push(parts::label("STEPS", palette))
        .push(parts::ruled(rows, palette));
    if install.has_plan() && !app.is_busy() {
        let start_over = Message::Install(InstallMessage::StartOver);
        page = page.push(parts::bracket("START OVER", Some(start_over), palette));
    }
    page
}

fn stage_row(app: &App, stage: InstallStage) -> Element<'_, Message> {
    let palette = app.palette;
    let install = &app.install;
    let status = install.status(stage);
    let mut content = column![parts::strong(stage.caption(), palette)].spacing(4);
    if status != StepStatus::Done {
        content = content.push(parts::body(stage.sentence(), palette));
    }
    for detail in details(app, stage, status) {
        content = content.push(detail);
    }
    let focused = install.focus_stage() == stage && !install.is_finished();
    flow::stage_row(stage.number(), content.into(), focused, status, palette)
}

fn details(app: &App, stage: InstallStage, status: StepStatus) -> Vec<Element<'_, Message>> {
    let install = &app.install;
    let checked = status == StepStatus::Done && !install.needs_a_new_check();
    let mut details = match stage {
        InstallStage::Connect => connect_details(app, status),
        InstallStage::Backup => backup_details(app),
        InstallStage::Plan => plan_details(app),
        InstallStage::Check if checked => check_details(app),
        _ => Vec::new(),
    };
    if status == StepStatus::Running && install.transferred_bytes() > 0 {
        details.push(transfer(app));
    }
    let failed = install
        .failure
        .as_ref()
        .filter(|(failed, _)| *failed == stage);
    if let Some((_, error)) = failed {
        details.push(failure(app, stage, error));
    }
    if install.next_stage() == Some(stage) {
        details.push(action(app, stage));
    }
    details
}

fn connect_details(app: &App, status: StepStatus) -> Vec<Element<'_, Message>> {
    let palette = app.palette;
    let open = !app.is_busy() && !app.install.connected;
    let choose = open.then_some(Message::Reboot(RebootMessage::PickLoader));
    let mut details = vec![picked(
        "LOADER",
        app.reboot.loader.as_ref(),
        NOT_CHOSEN,
        parts::bracket("CHOOSE LOADER", choose, palette),
        palette,
    )];
    if status != StepStatus::Done {
        details.push(parts::note(HOW_TO_CONNECT, palette));
    }
    details
}

fn backup_details(app: &App) -> Vec<Element<'_, Message>> {
    let palette = app.palette;
    let install = &app.install;
    let open = !app.is_busy() && install.progress.is_none();
    let choose = open.then_some(Message::PickFullBackup);
    let mut details = vec![picked(
        "FOLDER",
        install.backup.as_ref(),
        NOT_FOUND,
        parts::bracket("CHOOSE FOLDER", choose, palette),
        palette,
    )];
    if let Some(reason) = &install.refused_backup {
        details.push(parts::caution("NOT A FULL BACKUP", NO_FULL_BACKUP, palette));
        details.push(parts::value(reason.clone(), palette));
    }
    if !install.single_read.is_empty() {
        let names = install.single_read.join(" · ");
        details.push(parts::note(format!("{SINGLE_READ}: {names}"), palette));
    }
    details
}

fn check_details(app: &App) -> Vec<Element<'_, Message>> {
    let palette = app.palette;
    let install = &app.install;
    let continued = install.resume().filter(|resume| !resume.first_install);
    let mut details: Vec<Element<'_, Message>> = match (install.resume(), &install.review) {
        (Some(resume), Some(review)) => install_resume::lines(resume, &review.plan)
            .into_iter()
            .map(|line| resume_line(line, continued.is_some(), palette))
            .collect(),
        _ => vec![parts::note(CHECKED, palette)],
    };
    if install.next_stage() == Some(InstallStage::Write) {
        details.push(parts::note(READY, palette));
    }
    details
}

fn resume_line<'a>(line: String, continued: bool, palette: Palette) -> Element<'a, Message> {
    if continued {
        parts::body(line, palette)
    } else {
        parts::note(line, palette)
    }
}

fn plan_details(app: &App) -> Vec<Element<'_, Message>> {
    let palette = app.palette;
    let Some(review) = &app.install.review else {
        return Vec::new();
    };
    let writes = review.plan.write_steps().len();
    vec![
        parts::value(
            format!("{writes} writes · SHA-256 {}", review.sha256),
            palette,
        ),
        parts::label(review.directory.display().to_string(), palette),
    ]
}

fn picked<'a>(
    name: &str,
    path: Option<&PathBuf>,
    absent: &str,
    choose: Element<'a, Message>,
    palette: Palette,
) -> Element<'a, Message> {
    let shown = path.map_or_else(|| absent.to_string(), |path| path.display().to_string());
    row![
        parts::label(name.to_string(), palette),
        container(parts::value(shown, palette)).width(Length::Fill),
        choose,
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

fn transfer(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let done = app.install.transferred_bytes();
    let Some(expected) = app.install.expected_bytes() else {
        return parts::value(format!("{} transferred", gibibytes(done)), palette);
    };
    let words = format!("{} / {}", gibibytes(done), gibibytes(expected));
    column![meter(done, expected, palette), parts::value(words, palette)]
        .spacing(4)
        .into()
}

fn meter<'a>(done: u64, expected: u64, palette: Palette) -> Element<'a, Message> {
    let filled = filled_parts(done, expected);
    let height = Length::Fixed(6.0);
    let mut line = Row::new().width(Length::Fill);
    if filled > 0 {
        line = line.push(parts::bar(palette.ink, Length::FillPortion(filled), height));
    }
    if filled < METER_PARTS {
        let rest = Length::FillPortion(METER_PARTS - filled);
        line = line.push(parts::bar(palette.rule, rest, height));
    }
    line.into()
}

pub fn filled_parts(done: u64, expected: u64) -> u16 {
    if done == 0 || expected == 0 {
        return 0;
    }
    let parts = u128::from(done.min(expected)) * u128::from(METER_PARTS) / u128::from(expected);
    u16::try_from(parts)
        .unwrap_or(METER_PARTS)
        .clamp(METER_LEAST, METER_PARTS)
}

pub fn gibibytes(bytes: u64) -> String {
    format!("{:.2} GiB", bytes as f64 / GIBIBYTE)
}

fn action<'a>(app: &App, stage: InstallStage) -> Element<'a, Message> {
    let loader_is_ready = stage != InstallStage::Connect || app.reboot.loader.is_some();
    let ready = app.can_run_install(stage) && loader_is_ready;
    let message = match stage {
        InstallStage::Connect => InstallMessage::Connect,
        InstallStage::Backup => InstallMessage::BackUp,
        InstallStage::Plan => InstallMessage::Plan,
        InstallStage::Check => InstallMessage::Check,
        InstallStage::Write | InstallStage::Wipe => InstallMessage::AskWrite,
        InstallStage::Restart => InstallMessage::AskRestart,
    };
    let message = ready.then_some(Message::Install(message));
    column![
        Space::new().height(4),
        parts::go(stage.action(), message, app.palette)
    ]
    .into()
}

fn failure<'a>(app: &App, stage: InstallStage, error: &str) -> Element<'a, Message> {
    let palette = app.palette;
    let fault = parts::fault(
        stage.failed_caption(),
        stage.failed_sentence(),
        error,
        palette,
    );
    if !app.install.needs_a_new_check() || app.is_busy() {
        return fault;
    }
    let check_again = Message::Install(InstallMessage::CheckAgain);
    let reconnect = Message::Install(InstallMessage::Reconnect);
    let answers = row![
        parts::bracket("CHECK AGAIN", Some(check_again), palette),
        parts::bracket("RECONNECT", Some(reconnect), palette),
    ]
    .spacing(18);
    column![fault, answers].spacing(10).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_meter_is_as_long_as_the_part_that_is_done() {
        assert_eq!(filled_parts(0, 100), 0);
        assert_eq!(
            filled_parts(1, 1_000_000),
            METER_LEAST,
            "a start is visible"
        );
        assert_eq!(filled_parts(50, 100), 500);
        assert_eq!(filled_parts(100, 100), METER_PARTS);
        assert_eq!(filled_parts(101, 100), METER_PARTS);
        assert_eq!(filled_parts(5, 0), 0, "no known total");
        assert_eq!(gibibytes(1_610_612_736), "1.50 GiB");
    }
}
