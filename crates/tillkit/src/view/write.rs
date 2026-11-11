use crate::app::App;
use crate::folder_refusal::FolderRefusal;
use crate::install_review::InstallReview;
use crate::message::Message;
use crate::palette::Palette;
use crate::view::{flow, parts};
use crate::write_flow::WriteFlow;
use crate::write_kind::WriteKind;
use crate::write_message::WriteMessage;
use crate::write_origin::WriteOrigin;
use crate::write_step::WriteStep;
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

const INTRODUCTION: &str = "기록할 이미지가 든 폴더를 고르고 WRITE를 누르면, 준비부터 확인까지 차례로 진행합니다. 기기에 기록하기 전에 한 번 더 확인을 받습니다.";
const FILE_NAMES: &str = "폴더에는 init_boot.img(루트 패치본), boot.img(커널), system.img(GSI) 가운데 하나만 넣어 주세요.";
const NOT_CHOSEN: &str = "고르지 않음";
const FULL_BACKUP_NEEDED: &str = "GSI 설치 계획에는 기기 전체 백업이 필요합니다. BACKUP 화면에서 전체 백업 폴더를 먼저 골라 주세요.";
const INSTALL_PLANNED: &str = "GSI 설치 계획을 만들어 아래 폴더에 저장했습니다. 이 계획을 기기에 기록하는 일은 아직 지원하지 않습니다.";

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let idle = WriteFlow::default();
    let own = app.flow_of(WriteOrigin::Folder).unwrap_or(&idle);
    let mut page = column![
        parts::display("WRITE", palette),
        parts::label("USB · ROOT REQUIRED · NO DATA WIPE", palette),
        parts::body(INTRODUCTION, palette),
        parts::label("IMAGE FOLDER", palette),
        folder(app, own),
        action(app, own),
    ]
    .spacing(14);
    if app.other_work_is_open(WriteOrigin::Folder) {
        page = page.push(flow::other_work(app.write.origin, palette));
    }
    page = page
        .push(parts::label("STEPS", palette))
        .push(flow::steps(own, palette));
    if let Some(install) = &own.install {
        page = page.push(install_block(install, palette));
    }
    flow::results(app, own, page).into()
}

fn folder<'a>(app: &App, own: &WriteFlow) -> Element<'a, Message> {
    let palette = app.palette;
    let choose = app
        .can_choose_write_folder()
        .then_some(Message::Write(WriteMessage::PickFolder));
    let path = own
        .folder
        .as_ref()
        .map_or_else(|| NOT_CHOSEN.to_string(), |path| path.display().to_string());
    let chosen = row![
        container(parts::value(path, palette)).width(Length::Fill),
        parts::bracket("CHOOSE FOLDER", choose, palette),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    let mut content = column![chosen, verdict(own.kind, palette)].spacing(8);
    if needs_full_backup(app) {
        content = content.push(parts::caution(
            "FULL BACKUP NEEDED",
            FULL_BACKUP_NEEDED,
            palette,
        ));
    }
    if stopped_by_device(app) {
        content = content.push(flow::device_stops(palette));
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

fn action<'a>(app: &App, own: &WriteFlow) -> Element<'a, Message> {
    let palette = app.palette;
    if own.next_step() == Some(WriteStep::Reboot) {
        let ask = Message::Write(WriteMessage::AskReboot);
        return parts::go("REBOOT", Some(ask), palette);
    }
    let start = app
        .can_start_write()
        .then_some(Message::Write(WriteMessage::Start));
    parts::go("WRITE", start, palette)
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
