use crate::app::App;
use crate::image_refusal::ImageRefusal;
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

const INTRODUCTION: &str = "기록할 이미지 파일 하나를 고르고 WRITE를 누르면, 준비부터 확인까지 차례로 진행합니다. 되돌릴 때 쓰는 백업은 BACKUP 화면에서 만든 것 가운데에서 TillKit이 찾아서 씁니다. 기기에 기록하기 전에 한 번 더 확인을 받습니다.";
const WHICH_IMAGE: &str = "Magisk 앱으로 패치한 init_boot 이미지나 커널이 든 boot 이미지를 골라 주세요. 파일 이름은 무엇이든 괜찮습니다. 어느 영역에 기록할지는 파일의 내용을 읽어서 정합니다.";
const NOT_CHOSEN: &str = "고르지 않음";
const NO_BACKUP_YET: &str = "저장된 백업이 없습니다. 기록하다가 실패하면 백업으로 되돌리므로, BACKUP 화면에서 백업을 먼저 만들어 주세요.";
const INSTALL_PLAN: &str = "선택 사항입니다. 기기 전체 백업 폴더(sha256-manifest.json이 들어 있는 폴더)를 이미 가지고 있으면, 기기에 기록하지 않고 GSI 설치 계획만 만들 수 있습니다. 전체 백업은 TillKit이 만들지 않습니다.";
const FULL_BACKUP_FOUND: &str = "전체 백업을 찾았습니다.";
const FULL_BACKUP_MISSING: &str =
    "이 폴더에는 전체 백업이 없습니다. sha256-manifest.json이 들어 있는 폴더를 골라 주세요.";
const INSTALL_PLANNED: &str = "GSI 설치 계획을 만들어 아래 폴더에 저장했습니다. 이 계획을 기기에 기록하는 일은 아직 지원하지 않습니다.";

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let idle = WriteFlow::default();
    let own = app.flow_of(WriteOrigin::Image).unwrap_or(&idle);
    let is_gsi = own.kind == Some(Ok(WriteKind::Gsi));
    let mut page = column![
        parts::display("WRITE", palette),
        parts::label("USB · ROOT REQUIRED · NO DATA WIPE", palette),
        parts::body(INTRODUCTION, palette),
        parts::label("IMAGE", palette),
        image(app, own),
    ]
    .spacing(14);
    if !is_gsi {
        page = page.push(action(app, own));
    }
    if app.other_work_is_open(WriteOrigin::Image) {
        page = page.push(flow::other_work(app.write.origin, palette));
    }
    if is_gsi {
        page = page
            .push(parts::label("GSI INSTALL PLAN · OPTIONAL", palette))
            .push(install_plan(app));
    }
    page = page
        .push(parts::label("STEPS", palette))
        .push(flow::steps(own, palette));
    if let Some(install) = &own.install {
        page = page.push(install_block(install, palette));
    }
    flow::results(app, own, page).into()
}

fn image<'a>(app: &App, own: &WriteFlow) -> Element<'a, Message> {
    let palette = app.palette;
    let choose = app
        .can_choose_write_image()
        .then_some(Message::Write(WriteMessage::PickImage));
    let path = own
        .image
        .as_ref()
        .map_or_else(|| NOT_CHOSEN.to_string(), |path| path.display().to_string());
    let chosen = row![
        container(parts::value(path, palette)).width(Length::Fill),
        parts::bracket("CHOOSE IMAGE", choose, palette),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    let mut content = column![chosen, verdict(own.kind, palette)].spacing(8);
    if app.write_needs_a_backup() {
        content = content.push(parts::caution("NO BACKUP YET", NO_BACKUP_YET, palette));
    }
    if stopped_by_device(app) {
        content = content.push(flow::device_stops(palette));
    }
    content.into()
}

fn verdict<'a>(
    kind: Option<Result<WriteKind, ImageRefusal>>,
    palette: Palette,
) -> Element<'a, Message> {
    match kind {
        None => parts::body(WHICH_IMAGE, palette),
        Some(Ok(WriteKind::Gsi)) => {
            parts::caution("GSI NOT SUPPORTED", WriteKind::Gsi.found(), palette)
        }
        Some(Ok(kind)) => parts::body(kind.found(), palette),
        Some(Err(refusal)) => parts::caution(refusal.caption(), refusal.sentence(), palette),
    }
}

fn install_plan(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let choose = (!app.is_busy()).then_some(Message::PickFullBackup);
    let folder = app.full_backup.as_ref();
    let path = folder.map_or_else(|| NOT_CHOSEN.to_string(), |path| path.display().to_string());
    let chosen = row![
        container(parts::value(path, palette)).width(Length::Fill),
        parts::bracket("CHOOSE FOLDER", choose, palette),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    let content = column![parts::note(INSTALL_PLAN, palette), chosen].spacing(8);
    match (folder, app.full_backup_found) {
        (None, _) => content,
        (Some(_), false) => content.push(parts::caution(
            "NO FULL BACKUP",
            FULL_BACKUP_MISSING,
            palette,
        )),
        (Some(_), true) => {
            let start = app
                .can_start_write()
                .then_some(Message::Write(WriteMessage::Start));
            content
                .push(parts::body(FULL_BACKUP_FOUND, palette))
                .push(parts::bracket("MAKE PLAN", start, palette))
        }
    }
    .into()
}

fn stopped_by_device(app: &App) -> bool {
    let writes_to_device = matches!(app.write.kind, Some(Ok(kind)) if kind.writes_to_device());
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
