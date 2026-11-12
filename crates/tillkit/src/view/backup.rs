use crate::app::App;
use crate::clock::format_utc;
use crate::message::Message;
use crate::palette::Palette;
use crate::saved_backup::SavedBackup;
use crate::view::{flow, parts, rabbit};
use crate::write_flow::WriteFlow;
use crate::write_message::WriteMessage;
use crate::write_origin::WriteOrigin;
use crate::write_step::WriteStep;
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

const INTRODUCTION: &str = "기기의 부팅 영역을 PC에 읽어 둡니다. 기록하기 전에 먼저 백업해야, 기록한 것을 이전 상태와 대조하고 되돌릴 수 있습니다.";
const EMPTY: &str = "저장된 백업이 없습니다. 백업을 누르면 여기에 나타납니다.";
const RESTORE_HINT: &str = "RESTORE를 누르면 그 백업의 이미지를 기기에 다시 기록합니다. 기기에 기록하기 전에 한 번 더 확인을 받습니다.";
const BACKUP_STOPPED: &str = "백업하는 도중에 오류가 나서 끝까지 하지 못했습니다. 아래 목록에 보이는 것만 저장되었으니, 목록을 확인한 뒤에 다시 백업해 주세요.";
const STORE_NOT_READ: &str =
    "PC에 저장된 백업을 읽지 못했습니다. 읽지 못한 백업은 아래 목록에 나오지 않습니다.";
const DATA_FOLDER: &str = "백업을 이 폴더에 저장합니다. 내려받은 Magisk와 기록 계획처럼 TillKit이 만드는 다른 파일도 이 폴더에 둡니다.";
const CHOICE_KEPT: &str = "고른 폴더는 다음에 실행할 때도 그대로 씁니다.";
const CHOICE_FOR_THIS_RUN: &str =
    "TILLKIT_DATA_DIR로 정한 폴더로 시작했기 때문에, 여기에서 고른 폴더는 이번 실행에만 씁니다.";
const CHOICE_NOT_READ: &str = "전에 고른 폴더를 적어 둔 파일을 읽지 못해서 실행 파일 옆의 기본 폴더를 쓰고 있습니다. 백업이 보이지 않으면 폴더를 다시 골라 주세요.";
const CHOICE_NOT_KEPT: &str = "지금은 고른 폴더를 쓰지만, 고른 폴더를 적어 두지 못해서 다음에 실행할 때는 다시 골라야 합니다.";
const CHOSEN_FOLDER_ABSENT: &str = "전에 고른 폴더가 지금은 이 경로에 없어서, 그 폴더에 저장한 백업이 아래 목록에 나오지 않습니다. 드라이브가 연결되어 있는지 확인하거나 폴더를 다시 골라 주세요. 이대로 백업하면 이 경로에 폴더를 새로 만들어서 저장합니다.";
const TIME_WIDTH: f32 = 210.0;

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let idle = !app.is_busy();
    let mut page = column![
        parts::display("BACKUP", palette),
        parts::label("USB · ROOT REQUIRED", palette),
        parts::body(INTRODUCTION, palette),
        parts::label("BACKUP FOLDER", palette),
        data_folder(app),
        parts::go("BACK UP NOW", idle.then_some(Message::BackUp), palette),
    ]
    .spacing(14);
    if let Some(activity) = app.activity {
        page = page.push(parts::running(activity, palette));
    }
    if let Some(error) = &app.backup_error {
        page = page.push(parts::fault(
            "BACKUP STOPPED",
            BACKUP_STOPPED,
            error,
            palette,
        ));
    }
    if let Some(error) = &app.store_error {
        page = page.push(parts::fault(
            "BACKUPS NOT READ",
            STORE_NOT_READ,
            error,
            palette,
        ));
    }
    page = page
        .push(parts::label("SAVED BACKUPS", palette))
        .push(saved(app, palette));
    if app.other_work_is_open(WriteOrigin::Backup) {
        page = page.push(flow::other_work(app.write.origin, palette));
    }
    if app.support_stops_writes() && !app.saved_backups().is_empty() {
        page = page.push(flow::device_stops(palette));
    }
    if let Some(own) = app.flow_of(WriteOrigin::Backup) {
        page = restore(app, own, page);
    }
    page.into()
}

fn data_folder(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let choose = app
        .can_choose_data_folder()
        .then_some(Message::PickDataFolder);
    let path = app.paths.root().display().to_string();
    let chosen = row![
        container(parts::value(path, palette)).width(Length::Fill),
        parts::bracket("CHOOSE FOLDER", choose, palette),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    let kept = if app.choice_home.is_some() {
        CHOICE_KEPT
    } else {
        CHOICE_FOR_THIS_RUN
    };
    let mut content = column![
        chosen,
        parts::note(format!("{DATA_FOLDER} {kept}"), palette)
    ]
    .spacing(8);
    if app.chosen_folder_absent {
        content = content.push(parts::caution(
            "FOLDER NOT FOUND",
            CHOSEN_FOLDER_ABSENT,
            palette,
        ));
    }
    if let Some(error) = &app.choice_unread {
        content = content.push(parts::fault(
            "FOLDER CHOICE NOT READ",
            CHOICE_NOT_READ,
            error,
            palette,
        ));
    }
    if let Some(error) = &app.choice_error {
        content = content.push(parts::fault(
            "FOLDER NOT REMEMBERED",
            CHOICE_NOT_KEPT,
            error,
            palette,
        ));
    }
    content.into()
}

fn saved<'a>(app: &App, palette: Palette) -> Element<'a, Message> {
    let backups = app.saved_backups();
    if backups.is_empty() {
        return column![rabbit::view(palette), parts::body(EMPTY, palette)]
            .spacing(12)
            .padding([12, 0])
            .into();
    }
    let can_restore = app.can_start_restore();
    let rows = backups
        .iter()
        .map(|backup| saved_row(backup, can_restore, palette))
        .collect();
    column![
        parts::ruled(rows, palette),
        parts::note(RESTORE_HINT, palette)
    ]
    .spacing(8)
    .into()
}

fn saved_row<'a>(
    backup: &SavedBackup,
    can_restore: bool,
    palette: Palette,
) -> Element<'a, Message> {
    let restore = can_restore.then_some(Message::RestoreBackup(backup.created_unix));
    row![
        container(parts::value(format_utc(backup.created_unix), palette))
            .width(Length::Fixed(TIME_WIDTH)),
        container(parts::value(backup.contents(), palette)).width(Length::Fill),
        parts::bracket("RESTORE", restore, palette),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

fn restore<'a>(
    app: &App,
    own: &WriteFlow,
    mut page: iced::widget::Column<'a, Message>,
) -> iced::widget::Column<'a, Message> {
    let palette = app.palette;
    let caption = match own.restored_backup {
        Some(created_unix) => format!("RESTORE · {}", format_utc(created_unix)),
        None => "RESTORE".to_string(),
    };
    page = page
        .push(parts::label(caption, palette))
        .push(flow::steps(own, palette));
    if own.next_step() == Some(WriteStep::Reboot) {
        let ask = Message::Write(WriteMessage::AskReboot);
        page = page.push(parts::bracket("REBOOT", Some(ask), palette));
    }
    flow::results(app, own, page)
}
