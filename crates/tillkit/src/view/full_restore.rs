use crate::app::App;
use crate::full_restore_message::FullRestoreMessage;
use crate::full_restore_review::FullRestoreReview;
use crate::full_restore_stage::FullRestoreStage;
use crate::message::Message;
use crate::palette::Palette;
use crate::step_status::StepStatus;
use crate::view::install::{gibibytes, progress};
use crate::view::{flow, parts};
use iced::widget::{Column, Space, column, container, row};
use iced::{Alignment, Element, Length};

const INTRODUCTION: &str = "고른 전체 백업과 다른 영역만 기기에 다시 기록해서, 기기를 그 백업과 같게 만듭니다. 위의 01 CONNECT에서 연결한 기기와 02 FULL BACKUP에서 고른 백업을 그대로 씁니다. 설치 단계는 CLOSE FULL RESTORE를 누르면 다시 보입니다.";
const NOT_CONNECTED: &str = "01 CONNECT에서 기기에 먼저 연결해 주세요.";
const USER_DATA_ON: &str = "사용자 데이터(userdata)도 백업의 내용으로 되돌립니다. userdata는 다른 영역을 모두 더한 것보다 커서 시간이 오래 걸립니다.";
const USER_DATA_OFF: &str = "사용자 데이터(userdata)는 지금 그대로 둡니다. 순정으로 부팅하면 Android가 초기화를 요구할 수 있습니다.";
const SAME: &str = "기기가 이 전체 백업과 같습니다. 기록할 것이 없습니다.";
const FIRST_BOOT: &str = "부팅 초기 이미지가 백업과 다릅니다. 이 영역이 일부만 기록되면 기기가 EDL 모드로도 들어오지 못할 수 있어서, TillKit은 이 영역을 기록하지 않습니다. 다른 기기의 백업이거나 펌웨어가 바뀐 기기일 수 있습니다.";
const RESTORED: &str = "전체 백업을 기기에 기록하고 다시 읽어서 확인했습니다. Android로 다시 시작하지 않아도 됩니다. CLOSE FULL RESTORE를 누르면 설치 단계로 돌아가고, 같은 연결에서 설치의 CHECK DEVICE를 새로 할 수 있습니다.";
const RESTART_SENT: &str = "다시 시작을 요청했습니다. 기기가 부팅했는지는 TillKit이 확인하지 못하므로, 기기에서 직접 확인해 주세요.";
const UNFINISHED: &str = "이 기기는 복원이 끝나지 않은 상태입니다. 기기를 다시 시작하거나 끄지 말고, EDL 모드로 연결한 채로 CHECK DEVICE부터 다시 진행해 주세요. 다시 비교하면 아직 백업과 다른 영역만 기록합니다.";

pub fn section<'a>(app: &'a App, page: Column<'a, Message>) -> Column<'a, Message> {
    let palette = app.palette;
    let restore = &app.full_restore;
    let mut page = page
        .push(parts::label("FULL RESTORE", palette))
        .push(parts::body(INTRODUCTION, palette));
    if restore.restart_sent {
        page = page.push(flow::settled("RESTART REQUESTED", RESTART_SENT, palette));
    } else if restore.written {
        page = page.push(flow::settled("RESTORED", RESTORED, palette));
    }
    if restore.holds_the_device() && restore.running.is_none() {
        let caption = "RESTORE NOT FINISHED · KEEP THE DEVICE IN EDL";
        page = page.push(parts::caution(caption, UNFINISHED, palette));
    }
    let rows = FullRestoreStage::ALL
        .into_iter()
        .map(|stage| stage_row(app, stage))
        .collect();
    page = page.push(parts::ruled(rows, palette));
    if restore.can_close() && !app.is_busy() {
        let close = Message::FullRestore(FullRestoreMessage::Close);
        page = page.push(parts::bracket("CLOSE FULL RESTORE", Some(close), palette));
    }
    page
}

fn stage_row(app: &App, stage: FullRestoreStage) -> Element<'_, Message> {
    let palette = app.palette;
    let restore = &app.full_restore;
    let status = restore.status(stage);
    let mut content = column![parts::strong(stage.caption(), palette)].spacing(4);
    if status != StepStatus::Done {
        content = content.push(parts::body(stage.sentence(), palette));
    }
    for detail in details(app, stage, status) {
        content = content.push(detail);
    }
    let focused = restore.focus_stage() == stage && !restore.restart_sent;
    flow::stage_row(stage.number(), content.into(), focused, status, palette)
}

fn details(app: &App, stage: FullRestoreStage, status: StepStatus) -> Vec<Element<'_, Message>> {
    let restore = &app.full_restore;
    let mut details = match (stage, &restore.review) {
        (FullRestoreStage::Check, Some(review)) => check_details(app, review),
        (FullRestoreStage::Check, None) => choice_details(app),
        _ => Vec::new(),
    };
    if status == StepStatus::Running && app.install.transferred_bytes() > 0 {
        let done = app.install.transferred_bytes();
        details.push(progress(done, restore.expected_bytes(), app.palette));
    }
    let failed = restore
        .failure
        .as_ref()
        .filter(|(failed, _)| *failed == stage);
    if let Some((_, error)) = failed {
        let palette = app.palette;
        let sentence = stage.failed_sentence();
        details.push(parts::fault(
            stage.failed_caption(),
            sentence,
            error,
            palette,
        ));
    }
    if restore.next_stage() == Some(stage) {
        details.push(action(app, stage));
    }
    details
}

fn choice_details(app: &App) -> Vec<Element<'_, Message>> {
    let palette = app.palette;
    let backup = app.install.backup.as_ref();
    let shown = backup.map_or_else(String::new, |path| path.display().to_string());
    let mut details = vec![named("FOLDER", shown, None, palette), user_data(app)];
    if !app.install.connected {
        details.push(parts::note(NOT_CONNECTED, palette));
    }
    details
}

fn user_data(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let restore = &app.full_restore;
    let (word, sentence) = if restore.with_user_data() {
        ("RESTORE", USER_DATA_ON)
    } else {
        ("KEEP AS IS", USER_DATA_OFF)
    };
    let open = restore.running.is_none() && !app.is_busy();
    let change = open.then_some(Message::FullRestore(FullRestoreMessage::ToggleUserData));
    let switch = parts::bracket("CHANGE", change, palette);
    column![
        named("USER DATA", word.to_string(), Some(switch), palette),
        parts::note(sentence, palette),
    ]
    .spacing(4)
    .into()
}

fn check_details<'a>(app: &App, review: &'a FullRestoreReview) -> Vec<Element<'a, Message>> {
    let palette = app.palette;
    let user_data = if review.with_user_data {
        "with user data"
    } else {
        "without user data"
    };
    let counted = format!(
        "{} / {} areas differ · {} to write · {user_data}",
        review.different.len(),
        review.areas,
        gibibytes(review.different_bytes)
    );
    let selected = if review.boot_lun_selected {
        "selected now"
    } else {
        "to be selected"
    };
    let mut details = vec![
        parts::value(counted, palette),
        parts::value(
            format!("boot LUN {} · {selected}", review.boot_lun),
            palette,
        ),
    ];
    if !review.different.is_empty() {
        details.push(parts::note(review.different.join(" · "), palette));
    }
    if review.same {
        details.push(parts::note(SAME, palette));
    }
    if !review.first_boot_images.is_empty() {
        details.push(parts::caution(
            "FIRST BOOT IMAGES DIFFER",
            FIRST_BOOT,
            palette,
        ));
        details.push(parts::value(review.first_boot_images.join(" · "), palette));
    }
    details
}

fn named<'a>(
    name: &str,
    shown: String,
    choose: Option<Element<'a, Message>>,
    palette: Palette,
) -> Element<'a, Message> {
    let mut line = row![
        parts::label(name.to_string(), palette),
        container(parts::value(shown, palette)).width(Length::Fill),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    if let Some(choose) = choose {
        line = line.push(choose);
    }
    line.into()
}

fn action<'a>(app: &App, stage: FullRestoreStage) -> Element<'a, Message> {
    let message = match stage {
        FullRestoreStage::Check => FullRestoreMessage::Check,
        FullRestoreStage::Restore => FullRestoreMessage::AskRestore,
        FullRestoreStage::Restart => FullRestoreMessage::AskRestart,
    };
    let message = app
        .can_run_full_restore(stage)
        .then_some(Message::FullRestore(message));
    column![
        Space::new().height(4),
        parts::go(stage.action(), message, app.palette)
    ]
    .into()
}
