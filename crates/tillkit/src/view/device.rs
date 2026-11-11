use crate::app::App;
use crate::diagnosis::Diagnosis;
use crate::message::Message;
use crate::palette::Palette;
use crate::view::parts;
use front2::{SupportCheck, Verdict};
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

const INTRODUCTION: &str = "기기의 속성과 루트 접근 방법을 읽어서 지원 범위와 대조합니다. 이 화면은 기기에 아무것도 기록하지 않습니다.";
const READ_FAILED: &str =
    "기기를 읽지 못했습니다. ADB 연결과 USB 디버깅 승인을 확인한 뒤에 다시 읽어 주세요.";
const ALLOWED: &str =
    "모든 항목이 측정된 프론트 2와 일치합니다. 쌍 읽기와 기록 계획을 진행할 수 있습니다.";
const BLOCKED: &str =
    "일치하지 않는 항목이 있어서 이 기기에는 기록하지 않습니다. 읽기 전용 기능만 쓸 수 있습니다.";
const FACT_KEYS: [(&str, &str); 6] = [
    ("ANDROID RELEASE", "ro.build.version.release"),
    ("SDK", "ro.build.version.sdk"),
    ("BUILD ID", "ro.build.display.id"),
    ("BUILD TYPE", "ro.build.type"),
    ("VENDOR MODEL", "ro.product.vendor.model"),
    ("SOC MANUFACTURER", "ro.soc.manufacturer"),
];

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let read = (!app.is_busy()).then_some(Message::ReadDevice);
    let mut page = column![
        parts::display("DEVICE", palette),
        parts::label("ADB · READ ONLY · FRONT 2 / 1.609.5 BASE / SLOT B", palette),
        parts::body(INTRODUCTION, palette),
        parts::go("READ DEVICE", read, palette),
    ]
    .spacing(14);
    if let Some(activity) = app.activity {
        page = page.push(parts::running(activity, palette));
    }
    if let Some(error) = &app.device_error {
        page = page.push(parts::fault("READ FAILED", READ_FAILED, error, palette));
    }
    if let Some(diagnosis) = &app.diagnosis {
        page = page
            .push(verdict(diagnosis, palette))
            .push(parts::label("SUPPORT CHECKS", palette))
            .push(checks(diagnosis, palette))
            .push(parts::label("DEVICE FACTS", palette))
            .push(facts(diagnosis, palette));
    }
    page.into()
}

fn verdict<'a>(diagnosis: &Diagnosis, palette: Palette) -> Element<'a, Message> {
    match diagnosis.report.first_blocker() {
        None => parts::panel(
            column![
                parts::strong("WRITES ALLOWED", palette),
                parts::body(ALLOWED, palette)
            ]
            .spacing(8),
            palette,
        ),
        Some(check) => parts::fault(
            &format!("WRITES BLOCKED · {}", check.id),
            BLOCKED,
            &format!("expected {} · observed {}", check.expected, check.observed),
            palette,
        ),
    }
}

fn checks<'a>(diagnosis: &Diagnosis, palette: Palette) -> Element<'a, Message> {
    let rows = diagnosis
        .report
        .checks
        .iter()
        .map(|check| check_row(check, palette))
        .collect();
    parts::ruled(rows, palette)
}

fn check_row<'a>(check: &SupportCheck, palette: Palette) -> Element<'a, Message> {
    row![
        container(parts::value(check.id.clone(), palette)).width(Length::FillPortion(4)),
        container(parts::value(check.observed.clone(), palette)).width(Length::FillPortion(5)),
        container(verdict_word(check.verdict, palette)).width(Length::Fixed(84.0)),
    ]
    .spacing(12)
    .into()
}

fn verdict_word<'a>(verdict: Verdict, palette: Palette) -> Element<'a, Message> {
    let marked = |mark: Element<'a, Message>, word: &str| -> Element<'a, Message> {
        row![mark, parts::strong(word.to_string(), palette)]
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
    };
    match verdict {
        Verdict::Pass => parts::value("pass", palette),
        Verdict::Fail => marked(parts::square(palette.idol, None), "fail"),
        Verdict::Unknown => marked(parts::square(palette.warn, Some(palette.ink)), "unknown"),
    }
}

fn facts<'a>(diagnosis: &Diagnosis, palette: Palette) -> Element<'a, Message> {
    let state = &diagnosis.state;
    let optional = |text: &Option<String>| text.clone().unwrap_or_else(|| "not read".into());
    let mut rows: Vec<Element<'a, Message>> = FACT_KEYS
        .iter()
        .map(|(caption, key)| parts::key_value(caption, state.property(key), palette))
        .collect();
    let uid = state
        .shell_uid
        .map_or_else(|| "not read".to_string(), |uid| uid.to_string());
    let root = state
        .root_access
        .map_or_else(|| "none".to_string(), |access| format!("{access:?}"));
    rows.push(parts::key_value("SHELL UID", uid, palette));
    rows.push(parts::key_value("ROOT ACCESS", root, palette));
    rows.push(parts::key_value(
        "MAGISK",
        optional(&state.magisk_version),
        palette,
    ));
    rows.push(parts::key_value(
        "KERNEL",
        optional(&state.kernel_release),
        palette,
    ));
    parts::ruled(rows, palette)
}
