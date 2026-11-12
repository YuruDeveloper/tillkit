use crate::app::App;
use crate::install_review::InstallReview;
use crate::message::Message;
use crate::palette::Palette;
use crate::view::confirm::{answers, stated};
use crate::view::parts;
use iced::Element;
use iced::widget::{Column, column};
use std::path::Path;

const WRITE_WARNING: &str = "아래 내용을 기기에 기록합니다. 이 기록은 기기의 사용자 데이터를 지웁니다. 기록을 시작한 뒤에는 데이터를 지우는 일만 따로 취소할 수 없습니다. 기록하는 동안에는 USB 케이블을 뽑거나 기기를 끄지 말아 주세요.";
const WRITE_KEPT: &str = "기록한 영역은 모두 다시 읽어서 확인합니다. 기록하는 도중에 멈추면 기기가 부팅하지 못할 수 있습니다.";
const NO_RESTORE: &str = "순정으로 되돌리는 일은 아직 TillKit으로 할 수 없습니다. 되돌리려면 전체 백업을 다른 도구로 기기에 기록해야 합니다.";
const NOT_TESTED: &str =
    "TillKit으로 GSI를 기록한 기기가 부팅하는지는 아직 실제 기기에서 확인하지 못했습니다.";
const ROLLBACK_UNKNOWN: &str =
    "기기에 저장된 rollback 값과 기록할 이미지의 값이 맞는지는 TillKit이 확인하지 못했습니다.";
const BOOT_LUN_UNKNOWN: &str =
    "바꾼 부팅 위치가 다시 시작한 뒤에도 유지되는지는 TillKit이 확인하지 못했습니다.";
const RESTART_QUESTION: &str = "기록한 내용을 다시 읽어서 확인했습니다. 기기를 지금 다시 시작하면 GSI로 부팅하고, 첫 부팅에서 사용자 데이터를 지웁니다. 부팅에 성공하는지는 TillKit이 확인하지 못합니다. 나중에 하려면 LATER를 눌러 주세요.";

pub fn write(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let mut content = Column::new()
        .spacing(12)
        .push(parts::warned("WRITE GSI AND ERASE USER DATA", palette))
        .push(parts::body(WRITE_WARNING, palette));
    if let Some(review) = &app.install.review {
        let image = app.gsi_image();
        let backup = app.install.backup.as_deref();
        content = content.push(facts(review, image.as_deref(), backup, palette));
        content = unknowns(content, review, palette);
    }
    content = content
        .push(parts::body(WRITE_KEPT, palette))
        .push(parts::body(NO_RESTORE, palette))
        .push(parts::body(NOT_TESTED, palette))
        .push(answers("WRITE AND ERASE", "CANCEL", palette));
    parts::block(content, palette.tint_warn, palette)
}

fn facts<'a>(
    review: &InstallReview,
    image: Option<&Path>,
    backup: Option<&Path>,
    palette: Palette,
) -> Element<'a, Message> {
    let plan = &review.plan;
    let shown = |path: Option<&Path>| path.map(|path| path.display().to_string());
    let targets: Vec<&str> = plan
        .write_steps()
        .into_iter()
        .map(|(_, step)| step.name.as_str())
        .collect();
    let boot_lun = format!("LUN {} → LUN {}", plan.original_boot_lun, plan.new_boot_lun);
    let rows = vec![
        stated(
            "고른 이미지",
            parts::value(shown(image).unwrap_or_default(), palette),
            palette,
        ),
        stated(
            "전체 백업",
            parts::value(shown(backup).unwrap_or_default(), palette),
            palette,
        ),
        stated(
            "기록할 영역",
            parts::value(targets.join(" · "), palette),
            palette,
        ),
        stated("부팅 위치", parts::value(boot_lun, palette), palette),
        stated(
            "계획 SHA-256",
            parts::value(review.sha256.clone(), palette),
            palette,
        ),
    ];
    parts::ruled(rows, palette)
}

fn unknowns<'a>(
    mut content: Column<'a, Message>,
    review: &InstallReview,
    palette: Palette,
) -> Column<'a, Message> {
    if !review.plan.stored_rollback_source_confirmed {
        content = content.push(parts::body(ROLLBACK_UNKNOWN, palette));
    }
    if !review.plan.boot_lun_persistence_confirmed {
        content = content.push(parts::body(BOOT_LUN_UNKNOWN, palette));
    }
    content
}

pub fn restart<'a>(palette: Palette) -> Element<'a, Message> {
    let content = column![
        parts::strong("RESTART THE DEVICE", palette),
        parts::body(RESTART_QUESTION, palette),
        answers("RESTART", "LATER", palette),
    ]
    .spacing(12);
    parts::panel(content, palette)
}
