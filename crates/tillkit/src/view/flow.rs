use crate::app::App;
use crate::message::Message;
use crate::palette::Palette;
use crate::restore_session;
use crate::root_worker;
use crate::step_status::StepStatus;
use crate::view::parts;
use crate::write_flow::WriteFlow;
use crate::write_message::WriteMessage;
use crate::write_origin::WriteOrigin;
use crate::write_step::WriteStep;
use front2::{ApplyOutcome, ApplyState};
use iced::widget::{Column, column, container, row};
use iced::{Alignment, Color, Element, Length};

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
const NO_NEW_START: &str = "결과를 확인하지 못한 기록이 있어서, 이 화면에서는 새로 시작하지 않습니다. 기기의 전원을 유지한 채로 기기의 상태를 먼저 확인해 주세요.";
const ALREADY_PATCHED: &str = "이 기기의 init_boot에는 이미 Magisk가 들어 있어서 다시 패치하지 않습니다. 기기에는 기록하지 않았습니다.";
const SAME_IMAGES: &str = "기기에는 이미 이 백업과 같은 이미지가 들어 있어서 다시 기록하지 않습니다. 기기에는 기록하지 않았습니다.";
const OTHER_WORK: &str = "화면에서 시작한 작업이 끝나지 않았습니다. 그 화면에서 작업을 끝내거나 START OVER를 누른 뒤에 여기에서 시작할 수 있습니다.";
const STATUS_WIDTH: f32 = 120.0;

pub fn device_stops<'a>(palette: Palette) -> Element<'a, Message> {
    parts::caution("DEVICE NOT SUPPORTED", DEVICE_STOPS, palette)
}

pub fn other_work<'a>(open: WriteOrigin, palette: Palette) -> Element<'a, Message> {
    let sentence = format!("{} {OTHER_WORK}", open.screen_name());
    parts::caution("OTHER WORK OPEN", &sentence, palette)
}

pub fn steps<'a>(flow: &WriteFlow, palette: Palette) -> Element<'a, Message> {
    let rows = WriteStep::ALL
        .into_iter()
        .map(|step| step_row(flow, step, palette))
        .collect();
    parts::ruled(rows, palette)
}

fn step_row<'a>(flow: &WriteFlow, step: WriteStep, palette: Palette) -> Element<'a, Message> {
    let text = column![
        parts::strong(step.caption(), palette),
        parts::body(flow.origin.sentence(step), palette),
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

pub fn results<'a>(
    app: &App,
    flow: &WriteFlow,
    mut page: Column<'a, Message>,
) -> Column<'a, Message> {
    let palette = app.palette;
    if let Some((step, error)) = &flow.failure {
        page = page.push(failure(*step, error, palette));
    }
    let outcome = flow.outcome.as_ref().filter(|_| !flow.reboot_failed());
    if let Some(outcome) = outcome {
        page = page.push(outcome_block(flow, outcome, palette));
    }
    if flow.can_start_over() && !app.is_busy() {
        let start_over = Message::Write(WriteMessage::StartOver);
        page = page.push(parts::bracket("START OVER", Some(start_over), palette));
    }
    if flow.write_is_uncertain() && !app.is_busy() {
        page = page.push(parts::body(NO_NEW_START, palette));
    }
    page
}

fn failure<'a>(step: WriteStep, error: &str, palette: Palette) -> Element<'a, Message> {
    if step == WriteStep::Prepare && root_worker::is_already_patched(error) {
        return parts::caution("ALREADY PATCHED", ALREADY_PATCHED, palette);
    }
    if step == WriteStep::Prepare && restore_session::is_same_images(error) {
        return parts::caution("NOTHING TO RESTORE", SAME_IMAGES, palette);
    }
    parts::fault(
        step.failed_caption(),
        step.failed_sentence(),
        error,
        palette,
    )
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

pub fn settled<'a>(caption: &str, sentence: &str, palette: Palette) -> Element<'a, Message> {
    parts::panel(
        column![
            parts::strong(caption.to_string(), palette),
            parts::body(sentence.to_string(), palette),
        ]
        .spacing(8),
        palette,
    )
}
