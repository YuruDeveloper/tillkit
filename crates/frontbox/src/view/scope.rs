use crate::app::App;
use crate::fonts;
use crate::message::Message;
use crate::palette::Palette;
use crate::view::parts;
use iced::widget::{column, row, text};
use iced::{Alignment, Element, Length};

const INTRODUCTION: &str =
    "이 도구가 하는 일과 하지 않는 일, 그리고 실제 기기에서 확인하지 못한 일을 구분해서 적습니다.";

const SUPPORTED: [&str; 6] = [
    "기기 식별과 지원 범위 대조 (TOSSPLACE toss_front2, 지문 475, QCM4325, bengal, 슬롯 B, locked, green)",
    "init_boot_b와 vbmeta_b 현재 쌍 읽기 (기록 전, 전송 뒤, 읽은 뒤의 해시 대조)",
    "Magisk 앱이 패치한 init_boot로 서명된 후보 쌍 만들기 (top vbmeta의 키, rollback, 다른 descriptor 보존)",
    "기록 계획 저장, 계획 SHA-256에 묶인 승인, 사전 점검",
    "두 파티션 기록, 다시 읽은 해시 대조, 실패 시 이전 쌍 되돌리기",
    "기록 상태와 해시를 다시 확인한 뒤의 재부팅",
];

const UNVERIFIED: [&str; 5] = [
    "이 프로그램이 만든 후보 쌍의 부팅 (실기 검증 후보와 서명 내용은 같고 파일 전체 해시는 다름)",
    "이 프로그램이 만든 기록 스크립트와 쌍 읽기의 실기 실행 (모의 장치와 로컬 sh에서만 시험)",
    "su를 거친 기록 (실기 검증 경로는 adb root)",
    "읽어 둔 쌍 되돌리기",
    "기록 중 전원 차단 뒤의 EDL 복구",
];

const NOT_IMPLEMENTED: [&str; 5] = [
    "GSI 구성과 설치 (super 재구성, EDL 기록과 재읽기, GPT 슬롯 플래그, 부트 LUN 전환)",
    "커널 이미지 교체 (boot descriptor 교체와 호환성 확인)",
    "슬롯 A 기기, 다른 펌웨어 판, 다른 토스 단말",
    "EDL을 통한 전체 백업과 복구",
    "기기의 임시 폴더 /data/local/tmp/front2-write-* 자동 정리",
];

const NOTICES: [(&str, &str); 7] = [
    (
        "FRONTBOX (WORKING NAME)",
        "LTBox 3.3.5를 고친 판 · GPL-3.0-or-later · 2026-11: Lenovo 기능 제거, 프론트 2 쌍 관리 추가",
    ),
    ("LTBOX", "원저작물 · miner7222 · GPL-3.0-or-later"),
    ("QDL", "Qualcomm · BSD-3-Clause · vendor/qdl"),
    ("MAGISKBOOT-RS", "GPL-3.0-or-later"),
    ("KPTOOLS-RS", "GPL-2.0-or-later"),
    ("AVBTOOL-RS", "Apache-2.0"),
    ("IBM PLEX MONO", "SIL Open Font License 1.1 · assets/fonts"),
];

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    column![
        parts::display("SCOPE", palette),
        parts::label("SUPPORT RANGE · DEVICE VERIFICATION · NOTICES", palette),
        parts::body(INTRODUCTION, palette),
        emphasized(UNVERIFIED.len(), palette),
        section("NOT VERIFIED ON A DEVICE", &UNVERIFIED, palette),
        section("IMPLEMENTED", &SUPPORTED, palette),
        section("NOT IMPLEMENTED", &NOT_IMPLEMENTED, palette),
        parts::label("NOTICES", palette),
        notices(palette),
    ]
    .spacing(14)
    .into()
}

fn emphasized<'a>(count: usize, palette: Palette) -> Element<'a, Message> {
    let number = column![
        parts::bar(palette.idol, Length::Fixed(28.0), Length::Fixed(4.0)),
        text(count.to_string())
            .font(fonts::MONO_BOLD)
            .size(56)
            .color(palette.idol_word()),
    ]
    .spacing(4);
    row![
        number,
        parts::label("ITEMS NOT VERIFIED ON A DEVICE", palette)
    ]
    .spacing(14)
    .align_y(Alignment::End)
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
