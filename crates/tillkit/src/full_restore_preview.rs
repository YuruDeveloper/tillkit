use crate::app::App;
use crate::confirmation::Confirmation;
use crate::full_restore_review::FullRestoreReview;
use crate::full_restore_stage::FullRestoreStage;
use std::path::Path;
use std::sync::atomic::Ordering;

const MEBIBYTE: u64 = 1 << 20;
const TABLE_BYTES: u64 = 6 * 4096;
const AREAS: usize = 111;
const RESTORE_STOPPED: &str = "Verification failed: super: the 1310720 sectors at sector 16384 of LUN 0 have SHA-256 0d6c9e1a, the backup records 7f2b4c88";
const DIFFERENT: [(&str, u64); 12] = [
    ("frp", 1),
    ("persist", 32),
    ("reserve_sm1", 1),
    ("super", 5120),
    ("metadata", 16),
    ("init_boot_b", 8),
    ("vbmeta_b", 1),
    ("modemst1", 2),
    ("modemst2", 2),
    ("uefivarstore", 1),
    ("gpt_backup1", 0),
    ("gpt_main1", 0),
];
const FIRST_BOOT_IMAGES: [(&str, u64); 2] = [("xbl_a", 4), ("xbl_config_a", 1)];

pub const POSES: [&str; 9] = [
    "restore-open",
    "restore-checked",
    "restore-same",
    "restore-refused",
    "restore-confirm",
    "restoring",
    "restore-stopped",
    "restored",
    "restore-restart-confirm",
];

pub fn apply(app: &mut App, pose: &str, backup: &Path) {
    let restore = &mut app.full_restore;
    restore.shown = true;
    restore.leaves_user_data = pose == "restore-confirm";
    if pose == "restore-open" {
        return;
    }
    let different: &[(&str, u64)] = if pose == "restore-same" {
        &[]
    } else {
        &DIFFERENT
    };
    let first_boot: &[(&str, u64)] = if pose == "restore-refused" {
        &FIRST_BOOT_IMAGES
    } else {
        &[]
    };
    let areas = || first_boot.iter().chain(different);
    let same = areas().next().is_none();
    restore.review = Some(FullRestoreReview {
        backup: backup.to_path_buf(),
        with_user_data: restore.with_user_data(),
        boot_lun: 1,
        boot_lun_selected: same,
        areas: AREAS,
        different: areas().map(|(name, _)| name.to_string()).collect(),
        different_bytes: areas().map(|(_, mebibytes)| bytes(*mebibytes)).sum(),
        first_boot_images: first_boot
            .iter()
            .map(|(name, _)| name.to_string())
            .collect(),
        same,
    });
    finish(app, pose);
}

fn finish(app: &mut App, pose: &str) {
    let restore = &mut app.full_restore;
    match pose {
        "restore-confirm" => app.confirmation = Some(Confirmation::FullRestoreWrite),
        "restoring" => restoring(app),
        "restore-stopped" => {
            restore.device_changed = true;
            let error = RESTORE_STOPPED.to_string();
            restore.stage_failed(FullRestoreStage::Restore, error);
        }
        "restored" => restore.written = true,
        "restore-restart-confirm" => {
            restore.written = true;
            app.confirmation = Some(Confirmation::FullRestoreRestart);
        }
        _ => {}
    }
}

fn restoring(app: &mut App) {
    let restore = &mut app.full_restore;
    restore.running = Some(FullRestoreStage::Restore);
    restore.device_changed = true;
    let different = restore.review.as_ref().map(|review| review.different_bytes);
    let expected = different.unwrap_or(0) * 2;
    restore.expected.store(expected, Ordering::Relaxed);
    app.install
        .transferred
        .store(expected / 3, Ordering::Relaxed);
}

fn bytes(mebibytes: u64) -> u64 {
    if mebibytes == 0 {
        TABLE_BYTES
    } else {
        mebibytes * MEBIBYTE
    }
}
