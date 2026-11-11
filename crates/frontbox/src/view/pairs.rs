use crate::app::App;
use crate::clock::format_utc;
use crate::message::Message;
use crate::palette::Palette;
use crate::view::{parts, rabbit};
use front2::{PairKind, PairManifest};
use iced::widget::{button, column, container, row};
use iced::{Alignment, Color, Element, Length};

const INTRODUCTION: &str = "init_boot_b와 vbmeta_b를 한 쌍으로 보관합니다. 쌍을 기기에 기록하려면, 기기의 현재 쌍이 먼저 이 보관함에 있어야 합니다.";
const EMPTY: &str = "보관된 쌍이 없습니다. 기기에서 현재 쌍을 읽어 오면 여기에 나타납니다.";
const STORE_FAILED: &str = "작업을 끝내지 못했습니다. 만들던 쌍은 보관함에 남기지 않았습니다.";
const CANDIDATE_CAUTION: &str = "이 프로그램이 만든 후보 쌍은 실제 기기에서 부팅해 본 적이 없습니다. 실기에서 검증된 후보와 서명 내용은 같고 파일 전체 해시는 다릅니다.";
const SHORT_HASH: usize = 12;

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let idle = !app.is_busy();
    let build = (idle && app.selected_readback().is_some()).then_some(Message::PickPatchedImage);
    let plan = (idle && app.selected().is_some() && app.write.review.is_none())
        .then_some(Message::PlanWrite);
    let actions = row![
        parts::go(
            "READ CURRENT PAIR",
            idle.then_some(Message::ReadCurrentPair),
            palette
        ),
        parts::bracket("BUILD CANDIDATE FROM PATCHED INIT_BOOT", build, palette),
        parts::bracket("PLAN WRITE OF SELECTED", plan, palette),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .wrap();
    let mut page = column![
        parts::display("PAIRS", palette),
        parts::label(format!("STORE · {}", app.paths.pairs().display()), palette),
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
    if let Some(error) = &app.pairs_error {
        page = page.push(parts::fault("NOT STORED", STORE_FAILED, error, palette));
    }
    page = page.push(list(app, palette));
    if let Some(pair) = app.selected() {
        page = page.push(detail(pair, palette));
    }
    page.into()
}

fn list<'a>(app: &'a App, palette: Palette) -> Element<'a, Message> {
    if app.pairs.is_empty() {
        return column![rabbit::view(palette), parts::body(EMPTY, palette)]
            .spacing(12)
            .padding([24, 0])
            .into();
    }
    let selected = app.selected_pair.as_deref();
    let rows = app
        .pairs
        .iter()
        .map(|pair| pair_row(pair, selected == Some(pair.id.as_str()), palette))
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

fn pair_row<'a>(pair: &PairManifest, selected: bool, palette: Palette) -> Element<'a, Message> {
    let line = if selected {
        palette.idol
    } else {
        Color::TRANSPARENT
    };
    let cells = row![
        parts::bar(line, Length::Fixed(3.0), Length::Fixed(16.0)),
        container(parts::value(pair.id.clone(), palette)).width(Length::FillPortion(5)),
        container(parts::value(kind_word(pair.kind), palette)).width(Length::FillPortion(2)),
        container(parts::value(format_utc(pair.created_unix), palette))
            .width(Length::FillPortion(4)),
        container(parts::value(short(&pair.init_boot.sha256), palette))
            .width(Length::FillPortion(3)),
        container(parts::value(short(&pair.vbmeta.sha256), palette)).width(Length::FillPortion(3)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    button(cells)
        .padding(0)
        .width(Length::Fill)
        .on_press(Message::SelectPair(pair.id.clone()))
        .style(move |_, status| parts::flat(status, palette))
        .into()
}

fn detail<'a>(pair: &PairManifest, palette: Palette) -> Element<'a, Message> {
    let avb = &pair.avb;
    let facts = [
        ("PAIR", pair.id.clone()),
        ("KIND", kind_word(pair.kind).to_string()),
        ("CREATED (UTC)", format_utc(pair.created_unix)),
        ("SLOT", pair.slot_suffix.clone()),
        (
            "PARENT",
            pair.parent.clone().unwrap_or_else(|| "none".into()),
        ),
        ("NOTE", pair.note.clone()),
        ("INIT_BOOT SHA-256", pair.init_boot.sha256.clone()),
        ("INIT_BOOT BYTES", pair.init_boot.bytes.to_string()),
        ("VBMETA SHA-256", pair.vbmeta.sha256.clone()),
        ("VBMETA BYTES", pair.vbmeta.bytes.to_string()),
        ("TOP ALGORITHM", avb.top_algorithm.clone()),
        ("TOP KEY SHA-1", avb.top_key_sha1.clone()),
        ("TOP ROLLBACK INDEX", avb.top_rollback_index.to_string()),
        (
            "INIT_BOOT DIGEST IN TOP",
            avb.init_boot_digest_in_top.clone(),
        ),
        (
            "INIT_BOOT PAYLOAD BYTES",
            avb.init_boot_payload_bytes.to_string(),
        ),
        ("TOP HASH PARTITIONS", avb.top_hash_partitions.join(" ")),
        ("TOP CHAIN PARTITIONS", avb.top_chain_partitions.join(" ")),
    ];
    let rows = facts
        .into_iter()
        .map(|(caption, text)| parts::key_value(caption, text, palette))
        .collect();
    let content = column![
        parts::label("SELECTED PAIR", palette),
        parts::ruled(rows, palette)
    ]
    .spacing(10);
    let mut blocks = column![parts::panel(content, palette)].spacing(14);
    if pair.kind == PairKind::SignedCandidate {
        blocks = blocks.push(parts::caution(
            "NOT BOOTED ON A DEVICE",
            CANDIDATE_CAUTION,
            palette,
        ));
    }
    blocks.into()
}
