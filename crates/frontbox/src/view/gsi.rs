use crate::app::App;
use crate::gsi_action::GsiAction;
use crate::gsi_message::GsiMessage;
use crate::gsi_review::GsiReview;
use crate::install_review::InstallReview;
use crate::message::Message;
use crate::palette::Palette;
use crate::view::parts;
use front2::gsi::CandidateImage;
use front2::install::{InstallStep, RestoreEntry};
use iced::widget::{column, row};
use iced::{Alignment, Element};
use std::path::{Path, PathBuf};

const INTRODUCTION: &str = "기기 저장소 전체 백업과 GSI 이미지로, 슬롯 B에 기록할 이미지 여덟 개를 PC에서 만듭니다. 이 화면의 작업은 기기에 접근하지 않습니다.";
const BUILD_NOTE: &str = "후보 폴더에는 super 전체와 슬롯 B 논리 파티션의 사본이 새로 기록됩니다. 실물 백업으로 시험했을 때 약 6분이 걸렸습니다.";
const NOT_BUILT: &str = "후보를 만들지 못했습니다. 만들던 폴더는 지우려고 했지만, 지우지 못했으면 남아 있을 수 있습니다.";
const CANDIDATE_CAUTION: &str = "이 프로그램이 만든 후보로 기기를 부팅해 본 적은 없습니다. 실기 전환과 같은 입력으로 만든 후보는 여덟 이미지의 SHA-256이 그 전환과 같았고, 다른 입력으로 만든 후보에는 대조할 기준이 없습니다.";
const NOT_PICKED: &str = "선택하지 않음";
const NOT_PLANNED: &str = "설치 계획을 만들지 못했습니다. 기기에는 접근하지 않았습니다.";
const PLAN_NOTE: &str = "설치 계획은 후보 파일을 다시 해시한 뒤, 백업의 GPT와 후보 이미지로 기록 위치와 순서를 정해 파일로 저장합니다. 기기에는 접근하지 않습니다.";
const EXECUTION_CLOSED: &str = "이 판에서는 설치 계획을 실행할 수 없습니다. 실행기는 모의 저장 장치에서만 시험했고, 실제 EDL 전송에 연결하는 부분은 만들지 않았습니다. 이 계획대로 실행하면 사용자 데이터가 지워지고 슬롯 A로 되돌아가는 경로가 남지 않으며, 백업으로 되돌리는 복구는 실기에서 시험한 적이 없습니다.";

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = app.palette;
    let flow = &app.gsi;
    let idle = !app.is_busy();
    let next = flow.next_action();
    let button = |action: GsiAction, enabled: bool| {
        let message = enabled.then_some(Message::Gsi(action.message()));
        if next == Some(action) {
            parts::go(action.caption(), message, palette)
        } else {
            parts::bracket(action.caption(), message, palette)
        }
    };
    let actions = row(GsiAction::ALL.map(|action| button(action, idle && flow.allows(action))))
        .spacing(10)
        .align_y(Alignment::Center)
        .wrap();
    let inputs = vec![
        parts::stacked("BACKUP FOLDER", path_text(&flow.backup_directory), palette),
        parts::stacked("GSI IMAGE", path_text(&flow.gsi_image), palette),
    ];
    let mut page = column![
        parts::display("GSI", palette),
        parts::label(
            format!("STORE · {}", app.paths.gsi_candidates().display()),
            palette
        ),
        parts::body(INTRODUCTION, palette),
        parts::ruled(inputs, palette),
        actions,
        parts::body(BUILD_NOTE, palette),
        parts::body(PLAN_NOTE, palette),
    ]
    .spacing(14);
    if let Some(activity) = app.activity {
        page = page.push(parts::running(activity, palette));
    }
    if let Some(error) = &flow.failure {
        page = page.push(parts::fault("NOT BUILT", NOT_BUILT, error, palette));
    }
    if let Some(review) = &flow.review {
        page = page.push(candidate(review, idle, palette));
    }
    if let Some(error) = &flow.install_failure {
        page = page.push(parts::fault("NOT PLANNED", NOT_PLANNED, error, palette));
    }
    if let Some(install) = &flow.install {
        page = page.push(install_plan(install, palette));
    }
    page.into()
}

fn install_plan<'a>(install: &InstallReview, palette: Palette) -> Element<'a, Message> {
    let plan = &install.plan;
    let facts = vec![
        parts::key_value("FOLDER", display_path(&install.directory), palette),
        parts::key_value("INSTALL-PLAN.JSON SHA-256", install.sha256.clone(), palette),
        parts::key_value(
            "FIRST CONVERSION FROM SLOT A ONLY",
            yes_or_no(plan.first_conversion_only),
            palette,
        ),
        parts::key_value("ERASES USER DATA", yes_or_no(plan.wipes_user_data), palette),
        parts::key_value(
            "KEEPS SLOT A AS A FALLBACK",
            yes_or_no(plan.preserves_slot_a_fallback),
            palette,
        ),
        parts::key_value(
            "RESTORE TESTED ON A DEVICE",
            yes_or_no(plan.restore_tested_on_device),
            palette,
        ),
        parts::key_value(
            "STORED ROLLBACK SOURCE CONFIRMED",
            yes_or_no(plan.stored_rollback_source_confirmed),
            palette,
        ),
        parts::key_value(
            "BOOT LUN PERSISTENCE CONFIRMED",
            yes_or_no(plan.boot_lun_persistence_confirmed),
            palette,
        ),
        restore_row(&plan.data_restore, palette),
    ];
    let steps = plan
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| step_row(index + 1, step, palette))
        .collect();
    let content = column![
        parts::label("INSTALL PLAN", palette),
        parts::ruled(facts, palette),
        parts::label("STEPS IN ORDER", palette),
        parts::ruled(steps, palette),
    ]
    .spacing(10);
    column![
        parts::panel(content, palette),
        parts::caution("EXECUTION IS CLOSED", EXECUTION_CLOSED, palette),
    ]
    .spacing(14)
    .into()
}

fn restore_row<'a>(restore: &RestoreEntry, palette: Palette) -> Element<'a, Message> {
    let heading = row![
        parts::label("BACKUP NEEDED AFTER THE DATA ERASE", palette),
        parts::label(
            format!(
                "{} · LUN {} · SECTOR {} · {} BYTES",
                restore.name.to_uppercase(),
                restore.lun,
                restore.start_sector,
                restore.bytes
            ),
            palette
        ),
    ]
    .spacing(12)
    .wrap();
    column![
        heading,
        parts::value(restore.file.clone(), palette),
        parts::value(restore.sha256.clone(), palette),
    ]
    .spacing(3)
    .into()
}

fn step_row<'a>(number: usize, step: &InstallStep, palette: Palette) -> Element<'a, Message> {
    let (title, place, digest) = match step {
        InstallStep::Write(write) => (
            format!("WRITE {}", write.name),
            format!(
                "LUN {} · SECTOR {} · {} SECTORS",
                write.lun, write.start_sector, write.sectors
            ),
            Some(write.sha256.clone()),
        ),
        InstallStep::SetBootLun { lun } => (
            "SET BOOT LUN".to_string(),
            format!("LUN {lun} · READ BACK FROM THE BOOT WLUN"),
            None,
        ),
        InstallStep::Restart => (
            "RESTART".to_string(),
            "RECOVERY ERASES THE USER DATA".to_string(),
            None,
        ),
    };
    let heading = row![
        parts::value(format!("{number:02}"), palette),
        parts::strong(title, palette),
        parts::label(place, palette),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .wrap();
    let mut lines = column![heading].spacing(3);
    if let Some(digest) = digest {
        lines = lines.push(parts::value(digest, palette));
    }
    lines.into()
}

fn path_text(path: &Option<PathBuf>) -> String {
    path.as_deref()
        .map_or_else(|| NOT_PICKED.to_string(), display_path)
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn yes_or_no(fact: bool) -> &'static str {
    if fact { "yes" } else { "no" }
}

fn candidate<'a>(review: &GsiReview, idle: bool, palette: Palette) -> Element<'a, Message> {
    let record = &review.candidate;
    let facts = vec![
        parts::stacked("FOLDER", display_path(&review.directory), palette),
        parts::stacked("CANDIDATE.JSON SHA-256", review.sha256.clone(), palette),
        parts::stacked("GSI SHA-256", record.gsi_sha256.clone(), palette),
        parts::key_value("GSI BYTES", record.gsi_bytes.to_string(), palette),
        parts::key_value(
            "TOP ROLLBACK INDEX",
            record.top_vbmeta.rollback_index.to_string(),
            palette,
        ),
        parts::key_value(
            "FIRST CONVERSION FROM SLOT A ONLY",
            yes_or_no(record.first_conversion_only),
            palette,
        ),
        parts::key_value(
            "FEC DATA CALCULATED AGAIN",
            yes_or_no(record.fec_recomputed),
            palette,
        ),
        parts::key_value(
            "COMPARED WITH STORED ROLLBACK",
            yes_or_no(record.stored_rollback_verified),
            palette,
        ),
        parts::key_value(
            "BOOTED ON A DEVICE",
            yes_or_no(record.device_boot_verified),
            palette,
        ),
    ];
    let images = record
        .images()
        .iter()
        .map(|image| image_row(image, palette))
        .collect();
    let close = idle.then_some(Message::Gsi(GsiMessage::Close));
    let content = column![
        parts::label("CANDIDATE", palette),
        parts::ruled(facts, palette),
        parts::label("IMAGES FOR SLOT B", palette),
        parts::ruled(images, palette),
        parts::bracket("CLOSE (THE FOLDER STAYS)", close, palette),
    ]
    .spacing(10);
    column![
        parts::panel(content, palette),
        parts::caution("NOT BOOTED ON A DEVICE", CANDIDATE_CAUTION, palette),
    ]
    .spacing(14)
    .into()
}

fn image_row<'a>(image: &CandidateImage, palette: Palette) -> Element<'a, Message> {
    let heading = row![
        parts::strong(image.partition.clone(), palette),
        parts::label(format!("{} BYTES", image.image.bytes), palette),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    column![heading, parts::value(image.image.sha256.clone(), palette)]
        .spacing(3)
        .into()
}
