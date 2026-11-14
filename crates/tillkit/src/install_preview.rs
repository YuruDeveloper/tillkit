use crate::app::App;
use crate::confirmation::Confirmation;
use crate::full_restore_preview;
use crate::install_flow::InstallFlow;
use crate::install_review::InstallReview;
use crate::install_stage::InstallStage;
use front2::install::{
    InstallPlan, InstallProgress, InstallStep, PreflightRecord, ResumePoint, StepRecord, StepState,
};
use std::path::{Path, PathBuf};
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
const OTHER_STATE_STOP: &str = "Verification failed: preflight: the current gpt_main1 has SHA-256 0d6c9e1a, the plan records 7f2b4c88";
const MEBIBYTE: u64 = 1 << 20;
const RESUMED: &str = "resumed";
const RESUMED_UNKNOWN: &str = "resumed-unknown";
const RESUMED_STEPS: usize = 3;
const FINISHED_DEPTH: u8 = 7;

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
    install.connected = keeps_the_connection(depth);
    if depth >= 3 {
        install.backup = Some(PathBuf::from(BACKUP));
        install.single_read = vec!["super".into(), "userdata".into()];
    }
    if let Some(plan) = read_plan().filter(|_| depth >= 4) {
        install.progress = progress(&plan, &sha256, &pose);
        install.review = Some(InstallReview {
            plan,
            sha256,
            directory: PathBuf::from(PLAN_FOLDER),
            build: crate::plan_build::PlanBuild::Tested,
        });
    }
    finish(app, &pose);
}

fn depth(pose: &str) -> u8 {
    if full_restore_preview::POSES.contains(&pose) {
        return 3;
    }
    match pose {
        "loader" | "connect-failed" => 1,
        "backup" | "backing-up" | "backup-refused" => 2,
        "plan" => 3,
        "check" | "check-other-state" => 4,
        "checked" | "confirm" | "writing" | "stopped" => 5,
        "resumed" | "resumed-unknown" | "resumed-unknown-confirm" => 5,
        "restart" | "restart-confirm" => 6,
        "done" => FINISHED_DEPTH,
        _ => 0,
    }
}

fn keeps_the_connection(depth: u8) -> bool {
    (2..FINISHED_DEPTH).contains(&depth)
}

fn read_plan() -> Option<InstallPlan> {
    let path = std::env::var_os(PLAN_VARIABLE)?;
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn progress(plan: &InstallPlan, sha256: &str, pose: &str) -> Option<InstallProgress> {
    let depth = depth(pose);
    if depth < 5 {
        return None;
    }
    let checked = if pose.starts_with(RESUMED) {
        RESUMED_STEPS
    } else {
        0
    };
    let done = match depth {
        5 => checked,
        _ => plan.steps.len().saturating_sub(1),
    };
    let mut preflight = PreflightRecord::started(sha256);
    preflight.passed = true;
    preflight.resume = Some(resume(plan, pose, checked));
    let mut progress = InstallProgress::new(sha256);
    progress.preflight = Some(preflight);
    progress.steps = plan
        .steps
        .iter()
        .enumerate()
        .take(done)
        .map(verified)
        .collect();
    progress.restart_sent = depth >= FINISHED_DEPTH;
    Some(progress)
}

fn resume(plan: &InstallPlan, pose: &str, done: usize) -> ResumePoint {
    let write = plan.steps.get(done).and_then(InstallStep::as_write);
    let name = write.map(|step| step.name.clone());
    ResumePoint {
        first_install: !pose.starts_with(RESUMED),
        next_index: done,
        next_name: name.clone().unwrap_or_default(),
        unknown_content: name.filter(|_| pose.starts_with(RESUMED_UNKNOWN)),
    }
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
    app.install.device_changed = pose.starts_with(RESUMED);
    match pose {
        "connect-failed" => app
            .install
            .stage_failed(InstallStage::Connect, NO_DEVICE.into()),
        "backing-up" => run(&mut app.install, InstallStage::Backup, 11_605, 29_812),
        "backup-refused" => app.install.refused_backup = Some(NO_MANIFEST.into()),
        "confirm" | "resumed-unknown-confirm" => {
            app.confirmation = Some(Confirmation::InstallWrite);
        }
        "writing" => run(&mut app.install, InstallStage::Write, 3_277, 9_626),
        "stopped" => {
            app.install.device_changed = true;
            app.install
                .stage_failed(InstallStage::Write, WRITE_STOPPED.into());
        }
        "restart-confirm" => app.confirmation = Some(Confirmation::InstallRestart),
        "check-other-state" => app
            .install
            .stage_failed(InstallStage::Check, OTHER_STATE_STOP.into()),
        _ => {}
    }
    if full_restore_preview::POSES.contains(&pose) {
        full_restore_preview::apply(app, pose, Path::new(BACKUP));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_done_pose_closes_the_connection_as_a_real_restart_does() {
        assert!(!keeps_the_connection(depth("done")));
    }

    #[test]
    fn the_poses_from_the_backup_to_the_restart_keep_the_connection() {
        for pose in ["backup", "plan", "check", "writing", "restart"] {
            assert!(keeps_the_connection(depth(pose)), "{pose}");
        }
    }

    #[test]
    fn the_poses_before_the_connection_do_not_have_it() {
        assert!(!keeps_the_connection(depth("loader")));
        assert!(!keeps_the_connection(depth("connect-failed")));
    }
}
