use crate::app::App;
use crate::message::Message;
use crate::palette::Palette;
use crate::plan_review::PlanReview;
use crate::step_status::StepStatus;
use crate::view::parts;
use crate::write_flow::{REBOOT_WORD, WriteFlow};
use crate::write_step::WriteStep;
use front2::{ApplyOutcome, ApplyState, PlannedWrite, RootAccess, WritePurpose};
use iced::widget::{Column, column, container, row};
use iced::{Alignment, Color, Element, Length};

const INTRODUCTION: &str = "보관함의 쌍 하나를 기기의 init_boot_b와 vbmeta_b에 기록합니다. 슬롯을 바꾸지 않고, 사용자 데이터를 지우지 않으며, 부트로더 잠금을 건드리지 않습니다.";
const NO_PAIR: &str =
    "기록할 쌍을 고르지 않았습니다. PAIRS 화면에서 쌍을 고른 뒤에 계획을 세워 주세요.";
const STEP_FAILED: &str = "이 단계에서 멈췄습니다. 아래 원문을 확인한 뒤, 계획을 버리고 기기 상태를 다시 읽어서 새 계획을 세워 주세요.";
const SU_CAUTION: &str = "이 계획은 su를 거쳐 기록합니다. 실기에서 검증된 기록 경로는 adb root(uid 0)뿐이고, su를 거친 기록은 검증하지 않았습니다.";
const RESTORE_CAUTION: &str = "이 계획은 읽어 둔 쌍을 되돌려 기록합니다. 되돌리기는 모의 장치에서만 시험했고 실제 기기에서는 시험하지 않았습니다.";
const POWER_LOSS: &str = "두 파티션을 차례로 기록하는 동안 전원이 끊기면 기기가 부팅하지 못할 수 있습니다. 그때의 복구 수단은 전체 백업을 EDL로 되돌리는 것이며, 그 복구는 실기에서 시험하지 않았습니다.";
const UNVERIFIED: &str = "실기에서 검증하지 않은 것: 이 프로그램이 만든 후보의 부팅, 이 프로그램이 만든 기록 스크립트의 실행, su를 거친 기록, 되돌리기, 전원 차단 뒤의 EDL 복구.";
const ROLLED_BACK: &str = "기록이 중간에 실패해서 되돌릴 쌍을 다시 기록했고, 다시 읽은 해시가 되돌릴 쌍과 일치합니다. 재부팅하지 않았습니다.";
const DO_NOT_REBOOT: &str = "기록도 되돌리기도 끝나지 않았습니다. 기기를 재부팅하지 말고 전원을 유지한 채, 기기 로그와 파티션 해시를 확인해 주세요.";
const NO_FINAL_STATE: &str = "기기가 정해진 시간 안에 마지막 상태를 보고하지 않았습니다. 재부팅하지 말고 기기의 상태 파일과 로그를 확인해 주세요.";
const VERIFIED: &str = "두 파티션을 다시 읽은 해시가 새 쌍과 일치합니다. 재부팅 전까지는 이전 쌍으로 부팅된 상태입니다.";
const REBOOT_SENT: &str =
    "재부팅을 요청했습니다. 부팅이 끝나면 DEVICE 화면에서 기기를 다시 읽어 상태를 확인해 주세요.";

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let flow = &app.write;
    let mut page = column![
        parts::display("WRITE", palette),
        parts::label(
            "INIT_BOOT_B + VBMETA_B · ROOT · NO SLOT CHANGE · NO DATA WIPE",
            palette
        ),
        parts::body(INTRODUCTION, palette),
        target_line(app, palette),
        steps(app, palette),
    ]
    .spacing(14);
    if let Some((step, error)) = &flow.failure {
        let caption = format!("{} FAILED", step.caption());
        page = page.push(parts::fault(&caption, STEP_FAILED, error, palette));
    }
    if let Some(outcome) = &flow.outcome {
        page = page.push(outcome_block(flow, outcome, palette));
    }
    if let Some(review) = &flow.review {
        page = page.push(plan_block(review, palette));
    }
    page.push(parts::caution(
        "NOT VERIFIED ON A DEVICE",
        UNVERIFIED,
        palette,
    ))
    .into()
}

fn target_line<'a>(app: &App, palette: Palette) -> Element<'a, Message> {
    let planned = app
        .write
        .review
        .as_ref()
        .map(|review| &review.plan.new_pair);
    match planned.or(app.selected_pair.as_ref()) {
        Some(id) => parts::key_value("PAIR TO WRITE", id.clone(), palette),
        None => parts::body(NO_PAIR, palette),
    }
}

fn steps<'a>(app: &'a App, palette: Palette) -> Element<'a, Message> {
    let rows = WriteStep::ALL
        .into_iter()
        .map(|step| step_row(app, step, palette))
        .collect();
    parts::ruled(rows, palette)
}

fn step_row<'a>(app: &'a App, step: WriteStep, palette: Palette) -> Element<'a, Message> {
    let flow = &app.write;
    let status = flow.status(step);
    let mut text = column![
        parts::strong(step.caption(), palette),
        parts::body(step.sentence(), palette),
    ]
    .spacing(4);
    if status == StepStatus::Ready {
        text = text.push(action(app, step, palette));
    }
    let line = if flow.focus_step() == step {
        palette.idol
    } else {
        Color::TRANSPARENT
    };
    row![
        parts::bar(line, Length::Fixed(3.0), Length::Fill),
        container(parts::value(step.number(), palette)).width(Length::Fixed(28.0)),
        container(text).width(Length::Fill),
        container(status_word(status, palette)).width(Length::Fixed(96.0)),
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
        StepStatus::Waiting => parts::label(status.word(), palette),
        StepStatus::Ready | StepStatus::Done => parts::value(status.word(), palette),
    }
}

fn action<'a>(app: &'a App, step: WriteStep, palette: Palette) -> Element<'a, Message> {
    let flow = &app.write;
    match step {
        WriteStep::Plan => {
            let message = app.selected_pair.as_ref().map(|_| Message::PlanWrite);
            parts::go("PLAN", message, palette)
        }
        WriteStep::Stage => parts::go("STAGE AND CHECK", Some(Message::StagePlan), palette),
        WriteStep::Write => approval(
            write_prompt(flow),
            &flow.approval_input,
            Message::ApprovalChanged,
            parts::go(
                "WRITE PAIR",
                flow.write_authorization(0).map(|_| Message::WritePair),
                palette,
            ),
            palette,
        ),
        WriteStep::Reboot => approval(
            format!("재부팅하려면 {REBOOT_WORD}를 입력해 주세요."),
            &flow.reboot_input,
            Message::RebootChanged,
            parts::go(
                "REBOOT",
                flow.reboot_authorization(0).map(|_| Message::Reboot),
                palette,
            ),
            palette,
        ),
    }
}

fn write_prompt(flow: &WriteFlow) -> String {
    let code = flow.review.as_ref().map_or("", PlanReview::approval_code);
    format!("아래 계획을 확인한 뒤, 계획 SHA-256의 앞 8자리 {code}를 입력해 주세요.")
}

fn approval<'a>(
    prompt: String,
    current: &str,
    on_input: fn(String) -> Message,
    button: Element<'a, Message>,
    palette: Palette,
) -> Element<'a, Message> {
    let field = container(parts::input("", current, on_input, palette)).width(Length::Fixed(180.0));
    column![
        parts::body(prompt, palette),
        row![field, button].spacing(12).align_y(Alignment::Center),
    ]
    .spacing(8)
    .into()
}

fn yes_no(flag: bool) -> &'static str {
    if flag { "yes" } else { "no" }
}

fn plan_block<'a>(review: &PlanReview, palette: Palette) -> Element<'a, Message> {
    let plan = &review.plan;
    let purpose = match plan.purpose {
        WritePurpose::ApplyCandidate => "apply candidate",
        WritePurpose::RestoreReadback => "restore readback",
    };
    let facts = [
        ("PLAN", plan.id.clone()),
        ("PLAN SHA-256", review.sha256.clone()),
        ("PURPOSE", purpose.to_string()),
        ("NEW PAIR", plan.new_pair.clone()),
        ("OLD PAIR (ROLLBACK SOURCE)", plan.old_pair.clone()),
        ("SLOT", plan.slot_suffix.clone()),
        ("DEVICE STATE", plan.device_state.clone()),
        ("VERIFIED BOOT STATE", plan.verified_boot_state.clone()),
        ("ROOT ACCESS", format!("{:?}", plan.root_access)),
        ("REMOTE STAGE", plan.remote_stage.clone()),
        ("TOP ROLLBACK INDEX", plan.top_rollback_index.to_string()),
        ("WIPES USER DATA", yes_no(plan.wipes_user_data).to_string()),
        (
            "CHANGES ACTIVE SLOT",
            yes_no(plan.changes_active_slot).to_string(),
        ),
        (
            "CHANGES BOOTLOADER LOCK",
            yes_no(plan.changes_bootloader_lock).to_string(),
        ),
        (
            "RESTORE TESTED ON DEVICE",
            yes_no(plan.restore_tested_on_device).to_string(),
        ),
        ("POWER LOSS RECOVERY", plan.power_loss_recovery.clone()),
    ];
    let mut rows: Vec<Element<'a, Message>> = facts
        .into_iter()
        .map(|(caption, text)| parts::key_value(caption, text, palette))
        .collect();
    for write in &plan.writes {
        rows.extend(write_rows(write, palette));
    }
    let mut content = Column::new()
        .spacing(10)
        .push(parts::label("PLAN UNDER REVIEW", palette))
        .push(parts::ruled(rows, palette))
        .push(parts::body(POWER_LOSS, palette));
    if plan.root_access == RootAccess::Su {
        content = content.push(parts::caution("SU WRITE PATH", SU_CAUTION, palette));
    }
    if plan.purpose == WritePurpose::RestoreReadback {
        content = content.push(parts::caution("RESTORE", RESTORE_CAUTION, palette));
    }
    content = content.push(parts::bracket(
        "DISCARD PLAN",
        Some(Message::DiscardPlan),
        palette,
    ));
    parts::panel(content, palette)
}

fn write_rows<'a>(write: &PlannedWrite, palette: Palette) -> [Element<'a, Message>; 3] {
    let target = &write.target;
    [
        parts::key_value(
            "TARGET",
            format!(
                "{} · {} · {} bytes",
                target.label, target.node, target.bytes
            ),
            palette,
        ),
        parts::key_value("  OLD SHA-256", write.old_image.sha256.clone(), palette),
        parts::key_value("  NEW SHA-256", write.new_image.sha256.clone(), palette),
    ]
}

fn outcome_block<'a>(
    flow: &WriteFlow,
    outcome: &ApplyOutcome,
    palette: Palette,
) -> Element<'a, Message> {
    let detail = format!(
        "state {:?} · readback matches new pair: {} · readback {}",
        outcome.state,
        yes_no(outcome.readback_matches_new_pair),
        outcome.readback_sha256.join(" ")
    );
    let summary = match &outcome.state {
        ApplyState::VerifiedPendingReboot if flow.rebooted => {
            verified("REBOOT REQUESTED", REBOOT_SENT, &detail, palette)
        }
        ApplyState::VerifiedPendingReboot if outcome.readback_matches_new_pair => verified(
            "WRITE VERIFIED · PENDING REBOOT",
            VERIFIED,
            &detail,
            palette,
        ),
        ApplyState::RollbackVerified => parts::caution("ROLLED BACK", ROLLED_BACK, palette),
        ApplyState::RollbackFailedDoNotReboot => parts::fault(
            "ROLLBACK FAILED · DO NOT REBOOT",
            DO_NOT_REBOOT,
            &detail,
            palette,
        ),
        _ => parts::fault(
            "NO FINAL STATE · DO NOT REBOOT",
            NO_FINAL_STATE,
            &detail,
            palette,
        ),
    };
    column![
        summary,
        parts::label("DEVICE LOG", palette),
        parts::value(outcome.device_log.clone(), palette),
    ]
    .spacing(10)
    .into()
}

fn verified<'a>(
    caption: &str,
    sentence: &str,
    detail: &str,
    palette: Palette,
) -> Element<'a, Message> {
    parts::panel(
        column![
            parts::strong(caption.to_string(), palette),
            parts::body(sentence.to_string(), palette),
            parts::value(detail.to_string(), palette),
        ]
        .spacing(8),
        palette,
    )
}
