use crate::app::App;
use crate::clock::format_utc;
use crate::kernel_message::KernelMessage;
use crate::message::Message;
use crate::palette::Palette;
use crate::view::{parts, rabbit};
use front2::PairKind;
use front2::kernel::KernelSet;
use iced::widget::{button, column, container, row};
use iced::{Alignment, Color, Element, Length};

const INTRODUCTION: &str = "boot_b와 vbmeta_b를 한 세트로 보관합니다. 새 boot 이미지는 현재 boot의 header를 유지하고 kernel만 바꾼 것이어야 하며, KMI가 현재 커널과 같을 때만 후보를 만듭니다.";
const EMPTY: &str =
    "보관된 커널 세트가 없습니다. 기기에서 현재 세트를 읽어 오면 여기에 나타납니다.";
const STORE_FAILED: &str = "작업을 끝내지 못했습니다. 만들던 세트는 보관함에 남기지 않았습니다.";
const CANDIDATE_CAUTION: &str = "이 후보는 실제 기기에서 부팅해 본 적이 없습니다. KMI가 같아도 새 커널이 이 기기에서 부팅된다는 보장은 없습니다. 부팅에 실패하면 ADB를 쓸 수 없고 슬롯 A로 넘어가는 경로도 없으며, 남는 복구 수단은 두 파티션을 EDL로 다시 기록하는 것인데 그 복구는 실기에서 시험하지 않았습니다.";
const READ_CAUTION: &str = "기기에서 세트를 읽는 명령과 기록 절차는 모의 장치에서만 시험했습니다. 실제 기기에서 실행한 적은 없습니다.";
const SHORT_HASH: usize = 12;

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let idle = !app.is_busy();
    let build = (idle && app.selected_kernel_readback().is_some())
        .then_some(Message::Kernel(KernelMessage::PickBootImage));
    let plan = (idle && app.selected_kernel().is_some() && app.write.review.is_none())
        .then_some(Message::Kernel(KernelMessage::PlanWrite));
    let actions = row![
        parts::go(
            "READ CURRENT KERNEL SET",
            idle.then_some(Message::Kernel(KernelMessage::ReadCurrent)),
            palette
        ),
        parts::bracket("BUILD CANDIDATE FROM BOOT IMAGE", build, palette),
        parts::bracket("PLAN WRITE OF SELECTED", plan, palette),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .wrap();
    let mut page = column![
        parts::display("KERNEL", palette),
        parts::label(
            format!("STORE · {}", app.paths.kernel_sets().display()),
            palette
        ),
        parts::body(INTRODUCTION, palette),
        parts::input(
            "메모 (선택)",
            &app.note_input,
            Message::NoteChanged,
            palette
        ),
        actions,
    ]
    .spacing(14);
    if let Some(activity) = app.activity {
        page = page.push(parts::running(activity, palette));
    }
    if let Some(error) = &app.kernel_error {
        page = page.push(parts::fault("NOT STORED", STORE_FAILED, error, palette));
    }
    page = page.push(list(app, palette));
    if let Some(set) = app.selected_kernel() {
        page = page.push(detail(set, palette));
    }
    page.push(parts::caution(
        "NOT VERIFIED ON A DEVICE",
        READ_CAUTION,
        palette,
    ))
    .into()
}

fn list<'a>(app: &'a App, palette: Palette) -> Element<'a, Message> {
    if app.kernel_sets.is_empty() {
        return column![rabbit::view(palette), parts::body(EMPTY, palette)]
            .spacing(12)
            .padding([24, 0])
            .into();
    }
    let selected = app.selected_kernel_set.as_deref();
    let rows = app
        .kernel_sets
        .iter()
        .map(|set| set_row(set, selected == Some(set.id.as_str()), palette))
        .collect();
    parts::ruled(rows, palette)
}

fn kind_word(kind: PairKind) -> &'static str {
    match kind {
        PairKind::DeviceReadback => "readback",
        PairKind::SignedCandidate => "candidate",
    }
}

fn short(hash: &str) -> String {
    hash.chars().take(SHORT_HASH).collect()
}

fn set_row<'a>(set: &KernelSet, selected: bool, palette: Palette) -> Element<'a, Message> {
    let line = if selected {
        palette.idol
    } else {
        Color::TRANSPARENT
    };
    let cells = row![
        parts::bar(line, Length::Fixed(3.0), Length::Fixed(16.0)),
        container(parts::value(set.id.clone(), palette)).width(Length::FillPortion(5)),
        container(parts::value(kind_word(set.kind), palette)).width(Length::FillPortion(2)),
        container(parts::value(format_utc(set.created_unix), palette))
            .width(Length::FillPortion(4)),
        container(parts::value(short(&set.boot.sha256), palette)).width(Length::FillPortion(3)),
        container(parts::value(short(&set.vbmeta.sha256), palette)).width(Length::FillPortion(3)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    button(cells)
        .padding(0)
        .width(Length::Fill)
        .on_press(Message::Kernel(KernelMessage::Select(set.id.clone())))
        .style(move |_, status| parts::flat(status, palette))
        .into()
}

fn detail<'a>(set: &KernelSet, palette: Palette) -> Element<'a, Message> {
    let avb = &set.avb;
    let header = &set.boot_header;
    let mut rows = vec![
        parts::key_value("KERNEL SET", set.id.clone(), palette),
        parts::key_value("KIND", kind_word(set.kind), palette),
        parts::key_value("CREATED (UTC)", format_utc(set.created_unix), palette),
        parts::key_value("SLOT", set.slot_suffix.clone(), palette),
        parts::key_value(
            "PARENT",
            set.parent.clone().unwrap_or_else(|| "none".into()),
            palette,
        ),
        parts::key_value("NOTE", set.note.clone(), palette),
        parts::stacked("KERNEL RELEASE", set.kernel.release.clone(), palette),
        parts::key_value("KMI", set.kernel.kmi.clone(), palette),
        parts::key_value("KERNEL BYTES", header.kernel_size.to_string(), palette),
        parts::stacked("KERNEL SHA-256", set.kernel_sha256.clone(), palette),
        parts::stacked("BOOT SHA-256", set.boot.sha256.clone(), palette),
        parts::key_value("BOOT BYTES", set.boot.bytes.to_string(), palette),
        parts::stacked("VBMETA SHA-256", set.vbmeta.sha256.clone(), palette),
        parts::key_value("VBMETA BYTES", set.vbmeta.bytes.to_string(), palette),
        parts::key_value("TOP ALGORITHM", avb.top_algorithm.clone(), palette),
        parts::key_value("TOP KEY SHA-1", avb.top_key_sha1.clone(), palette),
        parts::key_value(
            "TOP ROLLBACK INDEX",
            avb.top_rollback_index.to_string(),
            palette,
        ),
        parts::stacked(
            "BOOT DIGEST IN TOP",
            avb.boot_digest_in_top.clone(),
            palette,
        ),
    ];
    for reference in &set.references {
        rows.push(parts::stacked(
            &format!("UNCHANGED · {}", reference.label.to_uppercase()),
            reference.sha256.clone(),
            palette,
        ));
    }
    let content = column![
        parts::label("SELECTED KERNEL SET", palette),
        parts::ruled(rows, palette)
    ]
    .spacing(10);
    let mut blocks = column![parts::panel(content, palette)].spacing(14);
    if set.kind == PairKind::SignedCandidate {
        blocks = blocks.push(parts::caution(
            "NOT BOOTED ON A DEVICE",
            CANDIDATE_CAUTION,
            palette,
        ));
    }
    blocks.into()
}
