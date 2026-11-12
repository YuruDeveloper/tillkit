use crate::app::App;
use crate::confirmation::Confirmation;
use crate::install_flow::InstallFlow;
use crate::install_review::InstallReview;
use crate::install_stage::InstallStage;
use front2::install::{
    InstallPlan, InstallProgress, InstallStep, PreflightRecord, StepRecord, StepState,
};
use std::path::PathBuf;
use std::sync::atomic::Ordering;

const POSE_VARIABLE: &str = "TILLKIT_PREVIEW_INSTALL";
const PLAN_VARIABLE: &str = "TILLKIT_PREVIEW_INSTALL_PLAN";
const SHA256_PART: &str = "5d1f0c9a";
const LOADER: &str = "D:\\front2\\prog_firehose_ddr.elf";
const BACKUP: &str = "D:\\tillkit-data\\full-backups\\backup-1762914000";
const PLAN_FOLDER: &str = "D:\\tillkit-data\\install-plans\\5d1f0c9a";
const NO_DEVICE: &str = "no device in EDL mode answered on USB";
const NO_MANIFEST: &str = "the folder holds no sha256-manifest.json";
const WRITE_STOPPED: &str = "write:super: the device gave no answer at sector 1024 of LUN 0";
const MEBIBYTE: u64 = 1 << 20;

pub fn apply(app: &mut App) {
    let Ok(pose) = std::env::var(POSE_VARIABLE) else {
        return;
    };
    let depth = depth(&pose);
    let sha256 = SHA256_PART.repeat(8);
    if depth >= 1 {
        app.reboot.loader = Some(PathBuf::from(LOADER));
    }
    let install = &mut app.install;
    install.connected = depth >= 2;
    if depth >= 3 {
        install.backup = Some(PathBuf::from(BACKUP));
        install.single_read = vec!["super".into(), "userdata".into()];
    }
    if let Some(plan) = read_plan().filter(|_| depth >= 4) {
        install.progress = progress(&plan, &sha256, depth);
        install.review = Some(InstallReview {
            plan,
            sha256,
            directory: PathBuf::from(PLAN_FOLDER),
        });
    }
    finish(app, &pose);
}

fn depth(pose: &str) -> u8 {
    match pose {
        "loader" | "connect-failed" => 1,
        "backup" | "backing-up" | "backup-refused" => 2,
        "plan" => 3,
        "check" => 4,
        "checked" | "confirm" | "writing" | "stopped" => 5,
        "restart" | "restart-confirm" => 6,
        "done" => 7,
        _ => 0,
    }
}

fn read_plan() -> Option<InstallPlan> {
    let path = std::env::var_os(PLAN_VARIABLE)?;
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn progress(plan: &InstallPlan, sha256: &str, depth: u8) -> Option<InstallProgress> {
    if depth < 5 {
        return None;
    }
    let mut preflight = PreflightRecord::started(sha256);
    preflight.passed = true;
    let mut progress = InstallProgress::new(sha256);
    progress.preflight = Some(preflight);
    if depth >= 6 {
        progress.steps = plan
            .steps
            .iter()
            .enumerate()
            .filter(|(_, step)| !matches!(step, InstallStep::Restart))
            .map(verified)
            .collect();
    }
    progress.restart_sent = depth >= 7;
    Some(progress)
}

fn verified((index, step): (usize, &InstallStep)) -> StepRecord {
    StepRecord {
        index,
        name: step.label(),
        state: StepState::Verified,
        readback_sha256: None,
        failure: None,
    }
}

fn finish(app: &mut App, pose: &str) {
    match pose {
        "connect-failed" => app
            .install
            .stage_failed(InstallStage::Connect, NO_DEVICE.into()),
        "backing-up" => run(&mut app.install, InstallStage::Backup, 11_605, 29_812),
        "backup-refused" => app.install.refused_backup = Some(NO_MANIFEST.into()),
        "confirm" => app.confirmation = Some(Confirmation::InstallWrite),
        "writing" => run(&mut app.install, InstallStage::Write, 3_277, 9_626),
        "stopped" => app
            .install
            .stage_failed(InstallStage::Write, WRITE_STOPPED.into()),
        "restart-confirm" => app.confirmation = Some(Confirmation::InstallRestart),
        _ => {}
    }
}

fn run(install: &mut InstallFlow, stage: InstallStage, done: u64, expected: u64) {
    install.running = Some(stage);
    install
        .transferred
        .store(done * MEBIBYTE, Ordering::Relaxed);
    install
        .expected
        .store(expected * MEBIBYTE, Ordering::Relaxed);
}
