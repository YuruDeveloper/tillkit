use crate::app::App;
use crate::message::Message;
use crate::root_message::RootMessage;
use crate::view::{flow, parts};
use crate::write_flow::WriteFlow;
use crate::write_message::WriteMessage;
use crate::write_origin::WriteOrigin;
use crate::write_step::WriteStep;
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

const INTRODUCTION: &str = "Magisk로 기기의 init_boot를 패치해서 기록합니다. ROOT를 누르면 준비부터 확인까지 차례로 진행하고, 기기에 기록하기 전에 한 번 더 확인을 받습니다.";
const BEFORE: &str = "루트 권한이 이미 있는 기기에서만 됩니다. 기기의 지금 상태를 BACKUP 화면에서 먼저 백업해 주세요.";
const LATEST: &str = "최신 판 · ROOT를 누르면 github.com/topjohnwu/Magisk에서 내려받습니다";

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let idle = WriteFlow::for_magisk();
    let own = app.flow_of(WriteOrigin::Magisk).unwrap_or(&idle);
    let mut page = column![
        parts::display("ROOT", palette),
        parts::label("USB · MAGISK · NO DATA WIPE", palette),
        parts::body(INTRODUCTION, palette),
        parts::body(BEFORE, palette),
        parts::label("MAGISK", palette),
        package(app, own),
        action(app, own),
    ]
    .spacing(14);
    if app.other_work_is_open(WriteOrigin::Magisk) {
        page = page.push(flow::other_work(app.write.origin, palette));
    }
    if app.support_stops_writes() {
        page = page.push(flow::device_stops(palette));
    }
    page = page
        .push(parts::label("STEPS", palette))
        .push(flow::steps(own, palette));
    flow::results(app, own, page).into()
}

fn package<'a>(app: &App, own: &WriteFlow) -> Element<'a, Message> {
    let palette = app.palette;
    let can_change = app.can_change_root_package();
    let source = app
        .magisk_apk
        .as_ref()
        .map_or_else(|| LATEST.to_string(), |path| path.display().to_string());
    let pick = can_change.then_some(Message::Root(RootMessage::PickPackage));
    let mut choice = row![
        container(parts::value(source, palette)).width(Length::Fill),
        parts::bracket("CHOOSE APK", pick, palette),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    if app.magisk_apk.is_some() {
        let latest = can_change.then_some(Message::Root(RootMessage::UseLatest));
        choice = choice.push(parts::bracket("USE LATEST", latest, palette));
    }
    let mut content = column![choice].spacing(8);
    if let Some(package) = &own.package {
        let used = format!(
            "Magisk {} · SHA-256 {}",
            package.version,
            package.short_sha256()
        );
        content = content.push(parts::key_value("PATCHED WITH", used, palette));
    }
    content.into()
}

fn action<'a>(app: &App, own: &WriteFlow) -> Element<'a, Message> {
    let palette = app.palette;
    if own.next_step() == Some(WriteStep::Reboot) {
        let ask = Message::Write(WriteMessage::AskReboot);
        return parts::go("REBOOT", Some(ask), palette);
    }
    let start = app
        .can_start_root()
        .then_some(Message::Root(RootMessage::Start));
    parts::go("ROOT", start, palette)
}
