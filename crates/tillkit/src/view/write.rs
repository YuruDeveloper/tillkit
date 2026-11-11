use crate::app::App;
use crate::folder_refusal::FolderRefusal;
use crate::install_review::InstallReview;
use crate::message::Message;
use crate::palette::Palette;
use crate::step_status::StepStatus;
use crate::view::parts;
use crate::write_flow::WriteFlow;
use crate::write_kind::WriteKind;
use crate::write_message::WriteMessage;
use crate::write_step::WriteStep;
use front2::{ApplyOutcome, ApplyState};
use iced::widget::{Column, column, container, row};
use iced::{Alignment, Color, Element, Length};

const INTRODUCTION: &str = "기록할 이미지가 든 폴더를 고르고 WRITE를 누르면, 준비부터 확인까지 차례로 진행합니다. 기기에 기록하기 전에 한 번 더 확인을 받습니다.";
const FILE_NAMES: &str = "폴더에는 init_boot.img(루트 패치본), boot.img(커널), system.img(GSI) 가운데 하나만 넣어 주세요.";
const NOT_CHOSEN: &str = "고르지 않음";
const FULL_BACKUP_NEEDED: &str = "GSI 설치 계획에는 기기 전체 백업이 필요합니다. BACKUP 화면에서 전체 백업 폴더를 먼저 골라 주세요.";
const DEVICE_STOPS: &str =
    "DEVICE 화면에서 확인한 기기가 지원하는 조건과 맞지 않아서, 이 기기에는 기록하지 않습니다.";
const VERIFIED: &str =
    "기록한 내용을 다시 읽어서 확인했습니다. 기기를 다시 시작해야 새 이미지로 부팅합니다.";
const REBOOT_SENT: &str =
    "재부팅을 요청했습니다. 부팅이 끝나면 DEVICE 화면에서 기기를 다시 확인해 주세요.";
const ROLLED_BACK: &str = "기록이 중간에 실패해서 백업해 둔 이미지로 되돌렸고, 되돌린 내용을 다시 읽어서 확인했습니다. 기기는 기록하기 전과 같습니다.";
const DO_NOT_REBOOT: &str =
    "기록도 되돌리기도 끝내지 못했습니다. 기기를 다시 시작하지 말고 전원을 유지해 주세요.";
const NO_FINAL_STATE: &str =
    "기기가 마지막 상태를 알려 주지 않았습니다. 기기를 다시 시작하지 말고 전원을 유지해 주세요.";
const INSTALL_PLANNED: &str = "GSI 설치 계획을 만들어 아래 폴더에 저장했습니다. 이 계획을 기기에 기록하는 일은 아직 지원하지 않습니다.";
const STATUS_WIDTH: f32 = 120.0;

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let flow = &app.write;
    let mut page = column![
        parts::display("WRITE", palette),
        parts::label("USB · ROOT REQUIRED · NO DATA WIPE", palette),
        parts::body(INTRODUCTION, palette),
        parts::label("IMAGE FOLDER", palette),
        folder(app),
        action(app),
        parts::label("STEPS", palette),
        steps(flow, palette),
    ]
    .spacing(14);
    if let Some((step, error)) = &flow.failure {
        page = page.push(parts::fault(
            step.failed_caption(),
            step.failed_sentence(),
            error,
            palette,
        ));
    }
    if let Some(outcome) = &flow.outcome {
        page = page.push(outcome_block(flow, outcome, palette));
    }
    if let Some(install) = &flow.install {
        page = page.push(install_block(install, palette));
    }
    if flow.has_result() && !app.is_busy() {
        let start_over = Message::Write(WriteMessage::StartOver);
        page = page.push(parts::bracket("START OVER", Some(start_over), palette));
    }
    page.into()
}

fn folder(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let flow = &app.write;
    let can_choose = !app.is_busy() && !flow.has_result();
    let choose = can_choose.then_some(Message::Write(WriteMessage::PickFolder));
    let path = flow
        .folder
        .as_ref()
        .map_or_else(|| NOT_CHOSEN.to_string(), |path| path.display().to_string());
    let chosen = row![
        container(parts::value(path, palette)).width(Length::Fill),
        parts::bracket("CHOOSE FOLDER", choose, palette),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    let mut content = column![chosen, verdict(flow.kind, palette)].spacing(8);
    if needs_full_backup(app) {
        content = content.push(parts::caution(
            "FULL BACKUP NEEDED",
            FULL_BACKUP_NEEDED,
            palette,
        ));
    }
    if stopped_by_device(app) {
        content = content.push(parts::caution(
            "DEVICE NOT SUPPORTED",
            DEVICE_STOPS,
            palette,
        ));
    }
    content.into()
}

fn verdict<'a>(
    kind: Option<Result<WriteKind, FolderRefusal>>,
    palette: Palette,
) -> Element<'a, Message> {
    match kind {
        None => parts::body(FILE_NAMES, palette),
        Some(Ok(kind)) => parts::body(kind.found(), palette),
        Some(Err(refusal)) => parts::caution(refusal.caption(), refusal.sentence(), palette),
    }
}

fn needs_full_backup(app: &App) -> bool {
    app.write.kind == Some(Ok(WriteKind::Gsi)) && app.full_backup_for_gsi().is_none()
}

fn stopped_by_device(app: &App) -> bool {
    let writes_to_device = matches!(
        app.write.kind,
        Some(Ok(WriteKind::RootImage | WriteKind::Kernel))
    );
    writes_to_device && app.support_stops_writes()
}

fn action(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    if app.write.next_step() == Some(WriteStep::Reboot) {
        let ask = Message::Write(WriteMessage::AskReboot);
        return parts::go("REBOOT", Some(ask), palette);
    }
    let start = app
        .can_start_write()
        .then_some(Message::Write(WriteMessage::Start));
    parts::go("WRITE", start, palette)
}

fn steps<'a>(flow: &WriteFlow, palette: Palette) -> Element<'a, Message> {
    let rows = WriteStep::ALL
        .into_iter()
        .map(|step| step_row(flow, step, palette))
        .collect();
    parts::ruled(rows, palette)
}

fn step_row<'a>(flow: &WriteFlow, step: WriteStep, palette: Palette) -> Element<'a, Message> {
    let text = column![
        parts::strong(step.caption(), palette),
        parts::body(step.sentence(), palette),
    ]
    .spacing(4);
    let line = if flow.focus_step() == step {
        palette.idol
    } else {
        Color::TRANSPARENT
    };
    row![
        parts::bar(line, Length::Fixed(3.0), Length::Fill),
        container(parts::value(step.number(), palette)).width(Length::Fixed(28.0)),
        container(text).width(Length::Fill),
        container(status_word(flow.status(step), palette)).width(Length::Fixed(STATUS_WIDTH)),
    ]
    .spacing(12)
    .height(Length::Shrink)
    .into()
}

fn status_word<'a>(status: StepStatus, palette: Palette) -> Element<'a, Message> {
    match status {
        StepStatus::Running => parts::running(status.word(), palette),
        StepStatus::Failed => row![
            parts::square(palette.idol, None),
            parts::strong(status.word(), palette)
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into(),
        StepStatus::Waiting | StepStatus::Closed => parts::label(status.word(), palette),
        StepStatus::Done => parts::value(status.word(), palette),
    }
}

fn outcome_block<'a>(
    flow: &WriteFlow,
    outcome: &ApplyOutcome,
    palette: Palette,
) -> Element<'a, Message> {
    let detail = format!("state {:?}", outcome.state);
    let summary = match &outcome.state {
        ApplyState::VerifiedPendingReboot if flow.rebooted => {
            return settled("REBOOT REQUESTED", REBOOT_SENT, palette);
        }
        ApplyState::VerifiedPendingReboot if outcome.readback_matches_new_pair => {
            return settled("WRITTEN AND VERIFIED", VERIFIED, palette);
        }
        ApplyState::RollbackVerified => parts::caution("ROLLED BACK", ROLLED_BACK, palette),
        ApplyState::RollbackFailedDoNotReboot => parts::fault(
            "ROLLBACK FAILED · DO NOT REBOOT",
            DO_NOT_REBOOT,
            &detail,
            palette,
        ),
        _ => parts::fault(
            "RESULT UNKNOWN · DO NOT REBOOT",
            NO_FINAL_STATE,
            &detail,
            palette,
        ),
    };
    let mut content = Column::new().spacing(10).push(summary);
    if !outcome.device_log.is_empty() {
        content = content
            .push(parts::label("DEVICE LOG", palette))
            .push(parts::value(outcome.device_log.clone(), palette));
    }
    content.into()
}

fn settled<'a>(caption: &str, sentence: &str, palette: Palette) -> Element<'a, Message> {
    parts::panel(
        column![
            parts::strong(caption.to_string(), palette),
            parts::body(sentence.to_string(), palette),
        ]
        .spacing(8),
        palette,
    )
}

fn install_block<'a>(install: &InstallReview, palette: Palette) -> Element<'a, Message> {
    column![
        parts::caution("GSI WRITE NOT SUPPORTED", INSTALL_PLANNED, palette),
        parts::stacked(
            "INSTALL PLAN FOLDER",
            install.directory.display().to_string(),
            palette
        ),
        parts::stacked("INSTALL PLAN SHA-256", install.sha256.clone(), palette),
    ]
    .spacing(10)
    .into()
}
