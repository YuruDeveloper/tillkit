use crate::app::App;
use crate::message::Message;
use crate::palette::Palette;
use crate::view::parts;
use iced::Element;
use iced::widget::{column, row};

const INTRODUCTION: &str = "TillKit은 토스 프론트 2를 백업하고 이미지를 기록하는 도구입니다. LTBox 3.3.5를 고쳐서 만들었습니다.";

const NOT_SUPPORTED: [&str; 7] = [
    "토스 프론트 2가 아닌 기기, 슬롯 A로 부팅한 프론트 2, 펌웨어 판이 다른 프론트 2 (DEVICE 화면에서 강제로 진행할 수는 있지만, 기록한 뒤에 부팅하는 것을 보장하지 않습니다)",
    "루트 권한이 없는 기기에 기록하기, 그리고 루트 권한이 없는 기기의 루트 확보",
    "Magisk가 아닌 방식의 루트 패치 만들기 (다른 도구로 만든 init_boot.img는 WRITE 화면에서 기록할 수 있습니다)",
    "GSI 설치의 실행 (설치에 쓸 파일을 준비하는 데까지만 합니다)",
    "기기 전체의 백업과 복구",
    "기록한 뒤에 부팅하지 못하는 기기의 복구",
    "압축된 커널 이미지",
];

const NOTICES: [(&str, &str); 10] = [
    (
        "TILLKIT",
        "LTBox 3.3.5를 고친 판 · GPL-3.0-or-later · 2026-11: Lenovo 기능 제거, 프론트 2 기능 추가",
    ),
    ("LTBOX", "원저작물 · miner7222 · GPL-3.0-or-later"),
    ("QDL", "Qualcomm · BSD-3-Clause · vendor/qdl"),
    ("MAGISKBOOT-RS", "GPL-3.0-or-later"),
    (
        "MAGISK",
        "topjohnwu · GPL-3.0-or-later · ROOT 화면에서 내려받고, 함께 배포하지 않음",
    ),
    ("KPTOOLS-RS", "GPL-2.0-or-later"),
    ("AVBTOOL-RS", "Apache-2.0"),
    ("IBM PLEX MONO", "SIL Open Font License 1.1 · assets/fonts"),
    (
        "IBM PLEX SANS KR",
        "SIL Open Font License 1.1 · assets/fonts",
    ),
    ("ARCHIVO NARROW", "SIL Open Font License 1.1 · assets/fonts"),
];

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let version = format!(
        "TILLKIT · V{} · GPL-3.0-OR-LATER",
        env!("CARGO_PKG_VERSION")
    );
    column![
        parts::display("INFO", palette),
        parts::label(version, palette),
        parts::body(INTRODUCTION, palette),
        section("NOT SUPPORTED", &NOT_SUPPORTED, palette),
        parts::label("NOTICES", palette),
        notices(palette),
    ]
    .spacing(14)
    .into()
}

fn section<'a>(caption: &str, items: &[&str], palette: Palette) -> Element<'a, Message> {
    let rows = items
        .iter()
        .enumerate()
        .map(|(index, item)| numbered(index + 1, item, palette))
        .collect();
    column![
        parts::label(caption.to_string(), palette),
        parts::ruled(rows, palette)
    ]
    .spacing(6)
    .into()
}

fn numbered<'a>(number: usize, item: &str, palette: Palette) -> Element<'a, Message> {
    row![
        parts::value(format!("{number:02}"), palette),
        parts::body(item.to_string(), palette),
    ]
    .spacing(14)
    .into()
}

fn notices<'a>(palette: Palette) -> Element<'a, Message> {
    let rows = NOTICES
        .iter()
        .map(|(name, notice)| parts::key_value(name, *notice, palette))
        .collect();
    parts::ruled(rows, palette)
}
