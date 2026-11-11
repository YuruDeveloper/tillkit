use crate::app::App;
use crate::clock::format_utc;
use crate::message::Message;
use crate::palette::Palette;
use crate::saved_backup::SavedBackup;
use crate::view::{parts, rabbit};
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

const INTRODUCTION: &str = "기기의 부팅 영역을 PC에 읽어 둡니다. 기록하기 전에 먼저 백업해야, 기록한 것을 이전 상태와 대조하고 되돌릴 수 있습니다.";
const EMPTY: &str = "저장된 백업이 없습니다. 백업을 누르면 여기에 나타납니다.";
const BACKUP_FAILED: &str = "백업을 끝내지 못했습니다. 읽다가 만 파일은 남기지 않았습니다.";
const FULL_BACKUP: &str = "GSI를 기록할 때 원본으로 쓰는 기기 전체 백업 폴더입니다. 전체 백업은 TillKit이 만들지 않으므로, 이미 가지고 있는 폴더를 골라 주세요.";
const FULL_BACKUP_FOUND: &str = "전체 백업을 찾았습니다.";
const FULL_BACKUP_MISSING: &str =
    "이 폴더에는 전체 백업이 없습니다. sha256-manifest.json이 들어 있는 폴더를 골라 주세요.";
const NOT_CHOSEN: &str = "고르지 않음";
const TIME_WIDTH: f32 = 210.0;

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let idle = !app.is_busy();
    let mut page = column![
        parts::display("BACKUP", palette),
        parts::label("USB · ROOT REQUIRED", palette),
        parts::body(INTRODUCTION, palette),
        parts::go("BACK UP NOW", idle.then_some(Message::BackUp), palette),
    ]
    .spacing(14);
    if let Some(activity) = app.activity {
        page = page.push(parts::running(activity, palette));
    }
    if let Some(error) = &app.backup_error {
        page = page.push(parts::fault("NOT SAVED", BACKUP_FAILED, error, palette));
    }
    page.push(parts::label("SAVED BACKUPS", palette))
        .push(saved(app, palette))
        .push(parts::label("FULL BACKUP FOLDER", palette))
        .push(full_backup(app, idle))
        .into()
}

fn saved<'a>(app: &App, palette: Palette) -> Element<'a, Message> {
    let backups = app.saved_backups();
    if backups.is_empty() {
        return column![rabbit::view(palette), parts::body(EMPTY, palette)]
            .spacing(12)
            .padding([12, 0])
            .into();
    }
    let rows = backups
        .iter()
        .map(|backup| saved_row(backup, palette))
        .collect();
    parts::ruled(rows, palette)
}

fn saved_row<'a>(backup: &SavedBackup, palette: Palette) -> Element<'a, Message> {
    row![
        container(parts::value(format_utc(backup.created_unix), palette))
            .width(Length::Fixed(TIME_WIDTH)),
        container(parts::value(backup.contents(), palette)).width(Length::Fill),
    ]
    .spacing(12)
    .into()
}

fn full_backup(app: &App, idle: bool) -> Element<'_, Message> {
    let palette = app.palette;
    let choose = idle.then_some(Message::PickFullBackup);
    let folder = app.full_backup.as_ref();
    let path = folder.map_or_else(|| NOT_CHOSEN.to_string(), |path| path.display().to_string());
    let chosen = row![
        container(parts::value(path, palette)).width(Length::Fill),
        parts::bracket("CHOOSE FOLDER", choose, palette),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    let mut content = column![parts::body(FULL_BACKUP, palette), chosen].spacing(8);
    if folder.is_some() {
        content = content.push(full_backup_state(app.full_backup_found, palette));
    }
    content.into()
}

fn full_backup_state<'a>(found: bool, palette: Palette) -> Element<'a, Message> {
    if found {
        return parts::body(FULL_BACKUP_FOUND, palette);
    }
    parts::caution("NO FULL BACKUP", FULL_BACKUP_MISSING, palette)
}
