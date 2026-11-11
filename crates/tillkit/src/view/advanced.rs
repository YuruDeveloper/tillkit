use crate::app::App;
use crate::message::Message;
use crate::reboot::{DeviceConnection, RebootTarget};
use crate::reboot_message::RebootMessage;
use crate::reboot_words;
use crate::view::{flow, parts};
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

const INTRODUCTION: &str = "기기를 다른 모드로 다시 시작합니다. 이 화면은 기기의 저장 영역에 아무것도 기록하지 않습니다. 백업과 기록에는 이 화면이 필요하지 않습니다.";
const NOT_CHECKED: &str =
    "연결을 먼저 확인해 주세요. 기기가 지금 어떤 모드인지 알아야 갈 수 있는 모드를 보여 드립니다.";
const CHECK_FAILED: &str =
    "요청을 끝내지 못했습니다. 기기가 다시 시작했는지 직접 확인한 뒤에 연결을 다시 확인해 주세요.";
const SENT: &str = "재부팅을 요청했습니다. 기기가 그 모드로 들어갔는지는 확인하지 않았으니, 연결을 다시 확인해 주세요.";
const HELD: &str = "결과를 확인하지 못한 기록이 있어서 재부팅을 요청하지 않습니다. 기기의 전원을 유지한 채로 기기의 상태를 먼저 확인해 주세요.";
const LOADER: &str = "EDL 모드에서 나가려면 이 기기의 Firehose 로더 파일이 필요합니다. TillKit은 이 파일을 함께 주지 않습니다.";
const NOT_CHOSEN: &str = "고르지 않음";
const NOT_REACHED: &str = "이 연결에서는 갈 수 없습니다";
const LOADER_NEEDED: &str = "Firehose 로더가 필요합니다";
const CURRENT: &str = "지금 모드입니다";
const CODE_WIDTH: f32 = 130.0;
const NAME_WIDTH: f32 = 170.0;

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let check = (!app.is_busy()).then_some(Message::Reboot(RebootMessage::Detect));
    let mut page = column![
        parts::display("ADVANCED", palette),
        parts::label("USB · REBOOT ONLY · NO WRITE", palette),
        parts::body(INTRODUCTION, palette),
        parts::go("CHECK CONNECTION", check, palette),
    ]
    .spacing(14);
    if let Some(activity) = app.activity {
        page = page.push(parts::running(activity, palette));
    }
    if app.reboot_is_held() {
        page = page.push(parts::caution("REBOOT HELD", HELD, palette));
    }
    if let Some(error) = &app.reboot.failure {
        page = page.push(parts::fault("NOT CONFIRMED", CHECK_FAILED, error, palette));
    }
    if let Some(target) = app.reboot.sent {
        let caption = format!("REBOOT REQUESTED · {}", reboot_words::code(target));
        page = page.push(flow::settled(&caption, SENT, palette));
    }
    let Some(connection) = app.reboot.connection else {
        return page.push(parts::body(NOT_CHECKED, palette)).into();
    };
    page = page
        .push(parts::label("CONNECTION", palette))
        .push(parts::body(reboot_words::connection(connection), palette));
    if app.reboot.has_no_target() {
        let advice = reboot_words::no_target(connection);
        return page
            .push(parts::caution("NO REBOOT FROM HERE", advice, palette))
            .into();
    }
    page = page
        .push(parts::label("REBOOT TO", palette))
        .push(targets(app, connection));
    if connection == DeviceConnection::Edl {
        page = page
            .push(parts::label("FIREHOSE LOADER", palette))
            .push(loader(app));
    }
    page.into()
}

fn targets<'a>(app: &App, connection: DeviceConnection) -> Element<'a, Message> {
    let rows = RebootTarget::ALL
        .into_iter()
        .map(|target| target_row(app, target, connection))
        .collect();
    parts::ruled(rows, app.palette)
}

fn note(app: &App, target: RebootTarget, connection: DeviceConnection) -> &'static str {
    if !app.reboot.reaches(target) {
        NOT_REACHED
    } else if app.reboot.lacks_loader(target) {
        LOADER_NEEDED
    } else if target.is_current(connection) {
        CURRENT
    } else {
        ""
    }
}

fn target_row<'a>(
    app: &App,
    target: RebootTarget,
    connection: DeviceConnection,
) -> Element<'a, Message> {
    let palette = app.palette;
    let ask = app
        .can_reboot_to(target)
        .then_some(Message::Reboot(RebootMessage::Ask(target)));
    row![
        container(parts::strong(reboot_words::code(target), palette))
            .width(Length::Fixed(CODE_WIDTH)),
        container(parts::body(reboot_words::name(target), palette))
            .width(Length::Fixed(NAME_WIDTH)),
        container(parts::note(note(app, target, connection), palette)).width(Length::Fill),
        parts::bracket("REBOOT", ask, palette),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

fn loader<'a>(app: &App) -> Element<'a, Message> {
    let palette = app.palette;
    let pick = (!app.is_busy()).then_some(Message::Reboot(RebootMessage::PickLoader));
    let path = app
        .reboot
        .loader
        .as_ref()
        .map_or_else(|| NOT_CHOSEN.to_string(), |path| path.display().to_string());
    let chosen = row![
        container(parts::value(path, palette)).width(Length::Fill),
        parts::bracket("CHOOSE FILE", pick, palette),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    column![parts::body(LOADER, palette), chosen]
        .spacing(8)
        .into()
}
