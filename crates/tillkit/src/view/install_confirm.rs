use crate::app::App;
use crate::install_review::InstallReview;
use crate::message::Message;
use crate::palette::Palette;
use crate::plan_build::PlanBuild;
use crate::view::confirm::{answers, stated};
use crate::view::{install_resume, parts};
use front2::gpt::BootSlot;
use front2::install::InstallPlan;
use front2::profiles::toss_front2;
use iced::Element;
use iced::widget::{Column, column};
use std::path::Path;

const WRITE_WARNING: &str = "아래 내용을 기기에 기록합니다. 이 기록은 기기의 사용자 데이터를 지웁니다. 기록을 시작한 뒤에는 데이터를 지우는 일만 따로 취소할 수 없습니다. 기록하는 동안에는 USB 케이블을 뽑거나 기기를 끄지 말아 주세요.";
const WRITE_KEPT: &str = "기록한 영역은 모두 다시 읽어서 확인합니다. 기록하는 도중에 멈추면 기기가 부팅하지 못할 수 있습니다. 그때는 기기를 EDL 모드로 연결해 둔 채로 다시 점검해서, 멈춘 곳부터 이어 할 수 있는지 확인합니다.";
const NO_RESTORE: &str = "설치한 뒤에 되돌리려면, 이 화면의 RESTORE TO DEVICE로 전체 백업을 기기에 다시 기록할 수 있습니다. 되돌린 기기가 부팅하는지는 아직 실제 기기에서 확인하지 못했습니다.";
const NOT_TESTED: &str =
    "TillKit으로 GSI를 기록한 기기가 부팅하는지는 아직 실제 기기에서 확인하지 못했습니다.";
const ROLLBACK_UNKNOWN: &str =
    "기기에 저장된 rollback 값과 기록할 이미지의 값이 맞는지는 TillKit이 확인하지 못했습니다.";
const BOOT_LUN_UNKNOWN: &str =
    "바꾼 부팅 위치가 다시 시작한 뒤에도 유지되는지는 TillKit이 확인하지 못했습니다.";
const SLOT_B_NOT_TESTED: &str =
    "부팅 슬롯 B에서 시작하는 설치는 실제 기기에서 실행해 본 적이 없습니다.";
const BOOT_LUN_KEPT: &str = "바꾸지 않음";
const OTHER_BUILD_WARNING: &str = "이 전체 백업의 순정 빌드는 TillKit이 GSI 설치 절차의 기준으로 삼은 빌드와 빌드 번호가 다릅니다. 그래도 진행하면 이 빌드의 부팅 이미지와 vbmeta로 설치 계획을 만듭니다. 이 빌드에 설치한 GSI가 부팅하는 것은 보장하지 않습니다.";
const OTHER_BUILD_KEPT: &str = "서명, 파티션 구성, rollback 값의 검사는 그대로 합니다. 이 단계에서는 기기에 기록하지 않으며, 기기에 기록하기 전에 한 번 더 확인을 받습니다.";
pub const OTHER_BUILD_PLAN: &str = "이 계획은 기준 빌드와 빌드 번호가 다른 순정 빌드의 전체 백업으로 만들었습니다. 이 빌드에 설치한 GSI가 부팅하는 것은 보장하지 않습니다.";
pub const BUILD_NOT_READ_TITLE: &str = "STOCK BUILD NOT CHECKED";
pub const BUILD_NOT_READ: &str = "이 계획이 쓰는 전체 백업에서 순정 빌드를 다시 읽지 못했습니다. 그래서 이 계획이 기준 빌드의 전체 백업으로 만든 것인지, 빌드 번호가 다른 빌드의 전체 백업으로 만든 것인지 TillKit이 알 수 없습니다. 전체 백업 폴더의 파일이 바뀌지 않았는지 확인해 주세요.";
const RESTART_QUESTION: &str = "기록한 내용을 다시 읽어서 확인했습니다. 기기를 지금 다시 시작하면, 기기는 사용자 데이터를 지운 뒤에 GSI로 부팅을 시도합니다. 부팅에 성공하는지는 TillKit이 확인하지 못합니다. 나중에 하려면 LATER를 눌러 주세요.";

pub fn untested_build(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let mut content = Column::new()
        .spacing(12)
        .push(parts::warned("OTHER STOCK BUILD", palette))
        .push(parts::body(OTHER_BUILD_WARNING, palette));
    if let Some(fingerprint) = &app.install.asked_build {
        content = content.push(builds(fingerprint, palette));
    }
    content = content
        .push(parts::body(OTHER_BUILD_KEPT, palette))
        .push(answers("CONTINUE ANYWAY", "CANCEL", palette));
    parts::block(content, palette.tint_warn, palette)
}

fn builds<'a>(fingerprint: &str, palette: Palette) -> Element<'a, Message> {
    let rows = vec![
        stated(
            "이 백업의 빌드",
            parts::value(fingerprint.to_string(), palette),
            palette,
        ),
        stated(
            "기준 빌드",
            parts::value(toss_front2::VENDOR_BUILD_FINGERPRINT, palette),
            palette,
        ),
    ];
    parts::ruled(rows, palette)
}

pub fn write(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let mut content = Column::new()
        .spacing(12)
        .push(parts::warned("WRITE GSI AND ERASE USER DATA", palette))
        .push(parts::body(WRITE_WARNING, palette));
    if let Some(review) = &app.install.review {
        let image = app.gsi_image();
        let backup = app.install.backup.as_deref();
        let first = app.install.verified_steps();
        content = content.push(facts(review, image.as_deref(), backup, first, palette));
        content = continued(content, app, review, palette);
        content = unknowns(content, review, palette);
    }
    let build = app.install.review.as_ref().map(|review| &review.build);
    content = match build {
        Some(PlanBuild::Untested(fingerprint)) => content
            .push(parts::body(OTHER_BUILD_PLAN, palette))
            .push(builds(fingerprint, palette)),
        Some(PlanBuild::NotRead(reason)) => content
            .push(parts::body(BUILD_NOT_READ, palette))
            .push(parts::value(reason.clone(), palette)),
        Some(PlanBuild::Tested) | None => content,
    };
    content = content
        .push(parts::body(WRITE_KEPT, palette))
        .push(parts::body(NO_RESTORE, palette))
        .push(parts::body(NOT_TESTED, palette));
    let slot_b_plan = app
        .install
        .review
        .as_ref()
        .is_some_and(|review| shows_slot_b_not_tested(&review.plan));
    if slot_b_plan {
        content = content.push(parts::body(SLOT_B_NOT_TESTED, palette));
    }
    content = content.push(answers("WRITE AND ERASE", "CANCEL", palette));
    parts::block(content, palette.tint_warn, palette)
}

fn facts<'a>(
    review: &InstallReview,
    image: Option<&Path>,
    backup: Option<&Path>,
    first: usize,
    palette: Palette,
) -> Element<'a, Message> {
    let plan = &review.plan;
    let shown = |path: Option<&Path>| path.map(|path| path.display().to_string());
    let targets: Vec<&str> = plan
        .write_steps()
        .into_iter()
        .filter(|(index, _)| *index >= first)
        .map(|(_, step)| step.name.as_str())
        .collect();
    let boot_lun = boot_lun_text(plan);
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
            parts::value(halved(&review.sha256), palette),
            palette,
        ),
    ];
    parts::ruled(rows, palette)
}

fn boot_lun_text(plan: &InstallPlan) -> String {
    if plan.stock_slot != plan.target_slot {
        format!("LUN {} → LUN {}", plan.original_boot_lun, plan.new_boot_lun)
    } else {
        format!("LUN {}, {BOOT_LUN_KEPT}", plan.new_boot_lun)
    }
}

fn shows_boot_lun_unknown(plan: &InstallPlan) -> bool {
    plan.stock_slot != plan.target_slot && !plan.boot_lun_persistence_confirmed
}

fn shows_slot_b_not_tested(plan: &InstallPlan) -> bool {
    plan.stock_slot == BootSlot::B && plan.target_slot == BootSlot::B
}

fn halved(sha256: &str) -> String {
    let (first, second) = sha256.split_at(sha256.len() / 2);
    format!(
        "{first}
{second}"
    )
}

fn continued<'a>(
    mut content: Column<'a, Message>,
    app: &App,
    review: &InstallReview,
    palette: Palette,
) -> Column<'a, Message> {
    let Some(resume) = app.install.resume().filter(|resume| !resume.first_install) else {
        return content;
    };
    for line in install_resume::lines(resume, &review.plan) {
        content = content.push(parts::body(line, palette));
    }
    content
}

fn unknowns<'a>(
    mut content: Column<'a, Message>,
    review: &InstallReview,
    palette: Palette,
) -> Column<'a, Message> {
    if !review.plan.stored_rollback_source_confirmed {
        content = content.push(parts::body(ROLLBACK_UNKNOWN, palette));
    }
    if shows_boot_lun_unknown(&review.plan) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install_support::InstallFixture;

    #[test]
    fn the_boot_lun_text_and_warnings_follow_the_boot_switch() {
        let first_conversion =
            InstallPlan::build(&InstallFixture::new(BootSlot::A, BootSlot::B).inputs)
                .expect("slot A plan");
        assert_eq!(boot_lun_text(&first_conversion), "LUN 1 → LUN 2");
        assert!(shows_boot_lun_unknown(&first_conversion));
        assert!(!shows_slot_b_not_tested(&first_conversion));

        let in_place_b = InstallPlan::build(&InstallFixture::new(BootSlot::B, BootSlot::B).inputs)
            .expect("slot B plan");
        assert_eq!(
            boot_lun_text(&in_place_b),
            format!("LUN 2, {BOOT_LUN_KEPT}")
        );
        assert!(!shows_boot_lun_unknown(&in_place_b));
        assert!(shows_slot_b_not_tested(&in_place_b));

        let switch_to_a = InstallPlan::build(&InstallFixture::new(BootSlot::B, BootSlot::A).inputs)
            .expect("slot A target plan");
        assert_eq!(switch_to_a.stock_slot, BootSlot::B);
        assert_eq!(switch_to_a.target_slot, BootSlot::A);
        assert_eq!(boot_lun_text(&switch_to_a), "LUN 2 → LUN 1");
        assert!(shows_boot_lun_unknown(&switch_to_a));
        assert!(!shows_slot_b_not_tested(&switch_to_a));
    }
}
