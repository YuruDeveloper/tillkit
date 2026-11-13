use crate::app::App;
use crate::diagnosis::Diagnosis;
use crate::message::Message;
use crate::palette::Palette;
use crate::support_words;
use crate::view::{parts, support_rows};
use front2::{DeviceState, SupportCheck, Verdict};
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

const INTRODUCTION: &str = "기기를 USB로 연결한 뒤에 확인을 누르면, 이 도구로 다룰 수 있는 기기인지 알려 드립니다. 이 화면은 기기에 아무것도 기록하지 않습니다.";
const READ_FAILED: &str = "기기를 읽지 못했습니다. USB 연결과 USB 디버깅 허용을 확인한 뒤에 다시 눌러 주세요. GSI를 설치하려고 EDL 모드로 연결한 기기는 이 화면에서 읽지 않으니 WRITE 화면에서 시작해 주세요.";
const EDL_FOUND: &str = "EDL 모드로 연결된 기기를 찾았습니다. 이 화면은 EDL 모드의 기기에서 정보를 읽지 않으므로, 지원하는 기기인지는 여기에서 알려 드리지 못합니다. GSI 설치와 전체 백업 복원은 WRITE 화면에서 시작해 주세요.";
const SUPPORTED: &str = "지원하는 기기입니다. 백업과 기록을 진행할 수 있습니다.";
const UNSUPPORTED: &str = "지원하는 조건과 맞지 않는 항목이 있어서, 이 기기에는 기록하지 않습니다.";
const ROOT_MISSING: &str = "루트 권한이 없어서 이 기기에는 기록할 수 없습니다.";
const BOOT_UNFINISHED: &str =
    "기기가 아직 부팅을 끝내지 않았습니다. 부팅이 끝난 뒤에 다시 확인해 주세요.";
const FORCED: &str = "맞지 않는 항목을 확인하고 강제로 진행하는 중입니다. 파일 대조와 기록 뒤의 확인은 그대로 합니다.";
const DETAIL_KEYS: [(&str, &str); 6] = [
    ("ANDROID RELEASE", "ro.build.version.release"),
    ("SDK", "ro.build.version.sdk"),
    ("BUILD ID", "ro.build.display.id"),
    ("BUILD TYPE", "ro.build.type"),
    ("VENDOR MODEL", "ro.product.vendor.model"),
    ("SOC MANUFACTURER", "ro.soc.manufacturer"),
];

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let check = (!app.is_busy()).then_some(Message::ReadDevice);
    let mut page = column![
        parts::display("DEVICE", palette),
        parts::label("USB · READ ONLY", palette),
        parts::body(INTRODUCTION, palette),
        parts::go("CHECK DEVICE", check, palette),
    ]
    .spacing(14);
    if let Some(activity) = app.activity {
        page = page.push(parts::running(activity, palette));
    }
    if let Some(error) = &app.device_error {
        page = page.push(parts::fault("CHECK FAILED", READ_FAILED, error, palette));
    }
    if app.device_in_edl {
        page = page.push(edl_notice(palette));
    }
    if let Some(diagnosis) = &app.diagnosis {
        page = page
            .push(verdict(app, diagnosis))
            .push(parts::label("THIS DEVICE", palette))
            .push(summary(&diagnosis.state, palette))
            .push(details(app, diagnosis));
    }
    page.into()
}

fn edl_notice<'a>(palette: Palette) -> Element<'a, Message> {
    let content = column![
        parts::strong("EDL MODE", palette),
        parts::body(EDL_FOUND, palette)
    ];
    parts::panel(content.spacing(8), palette)
}

fn verdict<'a>(app: &App, diagnosis: &Diagnosis) -> Element<'a, Message> {
    let palette = app.palette;
    let report = &diagnosis.report;
    if report.writes_allowed() {
        let content = column![
            parts::strong("SUPPORTED", palette),
            parts::body(SUPPORTED, palette)
        ];
        return parts::panel(content.spacing(8), palette);
    }
    let unmet = support_rows::unmet(report, palette);
    if app.forced {
        return column![parts::caution("FORCED", FORCED, palette), unmet]
            .spacing(10)
            .into();
    }
    let sentence = if support_words::root_is_missing(report) {
        ROOT_MISSING
    } else if support_words::boot_is_unfinished(report) {
        BOOT_UNFINISHED
    } else {
        UNSUPPORTED
    };
    let mut block =
        column![parts::fault("NOT SUPPORTED", sentence, "", palette), unmet].spacing(10);
    if support_words::can_be_forced(report) {
        let ask = (!app.is_busy()).then_some(Message::AskForce);
        block = block.push(parts::bracket("CONTINUE ANYWAY", ask, palette));
    }
    block.into()
}

fn summary<'a>(state: &DeviceState, palette: Palette) -> Element<'a, Message> {
    let device = format!(
        "{} {}",
        state.property("ro.product.vendor.manufacturer"),
        state.property("ro.product.vendor.model")
    );
    let slot = state.slot_suffix().trim_start_matches('_').to_uppercase();
    let (root, magisk) = match (state.root_access, &state.magisk_version) {
        (Some(_), Some(version)) => ("있음", format!("Magisk {version}")),
        (Some(_), None) => ("있음", String::new()),
        (None, _) => ("없음", String::new()),
    };
    let lock = match state.property("ro.boot.vbmeta.device_state") {
        "locked" => "잠김",
        "unlocked" => "풀림",
        _ => "읽지 못함",
    };
    let rows = vec![
        parts::fact("기기", "", device.trim(), palette),
        parts::fact(
            "Android 버전",
            "",
            state.property("ro.build.version.release"),
            palette,
        ),
        parts::fact("현재 슬롯", "", &slot, palette),
        parts::fact("루트 권한", root, &magisk, palette),
        parts::fact("부트로더 잠금", lock, "", palette),
    ];
    parts::ruled(rows, palette)
}

fn details<'a>(app: &App, diagnosis: &Diagnosis) -> Element<'a, Message> {
    let palette = app.palette;
    let caption = if app.details_open {
        "HIDE DETAILS"
    } else {
        "DETAILS"
    };
    let toggle = parts::bracket(caption, Some(Message::ToggleDetails), palette);
    if !app.details_open {
        return toggle;
    }
    column![
        toggle,
        parts::label("SUPPORT CHECKS", palette),
        checks(diagnosis, palette),
        parts::label("DEVICE FACTS", palette),
        facts(&diagnosis.state, palette),
    ]
    .spacing(14)
    .into()
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

fn facts<'a>(state: &DeviceState, palette: Palette) -> Element<'a, Message> {
    let optional = |text: &Option<String>| text.clone().unwrap_or_else(|| "not read".into());
    let uid = state
        .shell_uid
        .map_or_else(|| "not read".to_string(), |uid| uid.to_string());
    let root = state
        .root_access
        .map_or_else(|| "none".to_string(), |access| format!("{access:?}"));
    let mut rows: Vec<Element<'a, Message>> = DETAIL_KEYS
        .iter()
        .map(|(caption, key)| parts::key_value(caption, state.property(key), palette))
        .collect();
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
