use crate::clock::now_unix;
use crate::data_paths::DataPaths;
use crate::full_backup_store;
use crate::gsi_review::GsiReview;
use crate::install_review::InstallReview;
use front2::digest::sha256_file;
use front2::gsi::{GsiCandidate, GsiCandidateRequest, install_inputs};
use front2::install::{InstallPlan, PLAN_FILE};
use front2::{DeviceProfile, Front2Error, Result};
use serde::Serialize;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const CANDIDATE_PREFIX: &str = "gsi-";
const PLAN_PREFIX: &str = "install-";
const BACKUP_NOT_READ: &str = "이 이미지로 만든 계획이 쓰던 전체 백업을 읽지 못했습니다. 그 폴더를 읽을 수 있게 한 뒤에 MAKE PLAN을 다시 누르거나, USE EXISTING BACKUP으로 쓸 전체 백업을 직접 골라 주세요.";

pub fn prepare(
    paths: &DataPaths,
    backup: &Path,
    backup_is_found: bool,
    gsi_image: &Path,
    profile: &DeviceProfile,
) -> Result<InstallReview> {
    if let Some(stored) = stored(paths, backup, backup_is_found, gsi_image)? {
        return Ok(stored);
    }
    let review = build_candidate(paths, backup, gsi_image, profile)?;
    let planned = plan_install(paths, &review, backup, profile);
    removed_on_failure(&review.directory, planned)
}

fn removed_on_failure<T>(candidate_directory: &Path, result: Result<T>) -> Result<T> {
    if result.is_err() {
        let _ = std::fs::remove_dir_all(candidate_directory);
    }
    result
}

fn stored(
    paths: &DataPaths,
    backup: &Path,
    backup_is_found: bool,
    gsi_image: &Path,
) -> Result<Option<InstallReview>> {
    let Ok((gsi_bytes, gsi_sha256)) = sha256_file(gsi_image) else {
        return Ok(None);
    };
    let has_image = |plan: &InstallPlan| {
        GsiCandidate::load(&plan.candidate_directory).is_ok_and(|(candidate, _)| {
            candidate.gsi_bytes == gsi_bytes && candidate.gsi_sha256 == gsi_sha256
        })
    };
    if backup_is_found {
        return stored_for_image(paths, has_image, full_backup_store::refusal);
    }
    Ok(stored_where(paths, |plan| {
        plan.backup_directory == backup && has_image(plan)
    }))
}

fn stored_for_image(
    paths: &DataPaths,
    has_image: impl Fn(&InstallPlan) -> bool,
    refusal: impl Fn(&Path) -> Option<String>,
) -> Result<Option<InstallReview>> {
    let Some(stored) = stored_where(paths, has_image) else {
        return Ok(None);
    };
    let Some(reason) = refusal(&stored.plan.backup_directory) else {
        return Ok(Some(stored));
    };
    let backup = stored.plan.backup_directory.display();
    Err(Front2Error::Plan(format!(
        "{BACKUP_NOT_READ} ({backup}: {reason})"
    )))
}

fn stored_where(paths: &DataPaths, fits: impl Fn(&InstallPlan) -> bool) -> Option<InstallReview> {
    newest_plan_folders(paths)
        .into_iter()
        .find_map(|directory| reviewed(directory, &fits))
}

fn newest_plan_folders(paths: &DataPaths) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(paths.install_plans()) else {
        return Vec::new();
    };
    let mut folders: Vec<(u64, PathBuf)> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| Some((created_unix(&entry.file_name())?, entry.path())))
        .collect();
    folders.sort_by_key(|(created, _)| std::cmp::Reverse(*created));
    folders.into_iter().map(|(_, folder)| folder).collect()
}

fn plan_record(directory: &Path) -> Option<InstallPlan> {
    let bytes = std::fs::read(directory.join(PLAN_FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn reviewed(directory: PathBuf, fits: &impl Fn(&InstallPlan) -> bool) -> Option<InstallReview> {
    let plan = plan_record(&directory)?;
    if !fits(&plan) {
        return None;
    }
    let stored = InstallPlan::load(&directory).ok()?;
    Some(InstallReview {
        plan: stored.plan().clone(),
        sha256: stored.plan_sha256().to_string(),
        directory,
    })
}

fn created_unix(name: &OsStr) -> Option<u64> {
    name.to_str()?.strip_prefix(PLAN_PREFIX)?.parse().ok()
}

fn build_candidate(
    paths: &DataPaths,
    backup: &Path,
    gsi_image: &Path,
    profile: &DeviceProfile,
) -> Result<GsiReview> {
    let root = paths.gsi_candidates();
    std::fs::create_dir_all(&root)?;
    let directory = root.join(format!("{CANDIDATE_PREFIX}{}", now_unix()));
    let request = GsiCandidateRequest {
        profile,
        backup_directory: backup,
        gsi_image,
        output_directory: &directory,
        rollback_evidence: &[],
    };
    let candidate = GsiCandidate::build(&request)?;
    let review = reviewed_candidate(directory.clone(), &candidate);
    removed_on_failure(&directory, review)
}

fn reviewed_candidate(directory: PathBuf, built: &GsiCandidate) -> Result<GsiReview> {
    let review = load(directory)?;
    if !is_same_record(&review.candidate, built) {
        return Err(Front2Error::Verification(
            "the stored GSI candidate record differs from the built one".into(),
        ));
    }
    Ok(review)
}

fn is_same_record<T: Serialize>(stored: &T, built: &T) -> bool {
    match (serde_json::to_value(stored), serde_json::to_value(built)) {
        (Ok(stored), Ok(built)) => stored == built,
        _ => false,
    }
}

fn plan_install(
    paths: &DataPaths,
    review: &GsiReview,
    backup: &Path,
    profile: &DeviceProfile,
) -> Result<InstallReview> {
    let (candidate, sha256) = GsiCandidate::load(&review.directory)?;
    if sha256 != review.sha256 || candidate != review.candidate {
        return Err(Front2Error::Plan(
            "the GSI candidate record changed after the review".into(),
        ));
    }
    candidate.verify_files(&review.directory)?;
    let inputs = install_inputs(&candidate, &review.directory, backup, profile)?;
    let directory = paths
        .install_plans()
        .join(format!("{PLAN_PREFIX}{}", now_unix()));
    InstallPlan::build(&inputs)?.save(&directory)?;
    let stored = InstallPlan::load(&directory)?;
    Ok(InstallReview {
        plan: stored.plan().clone(),
        sha256: stored.plan_sha256().to_string(),
        directory,
    })
}

fn load(directory: PathBuf) -> Result<GsiReview> {
    let (candidate, sha256) = GsiCandidate::load(&directory)?;
    Ok(GsiReview {
        candidate,
        sha256,
        directory,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_without_a_manifest_leaves_no_candidate_folder() {
        let directory = tempfile::tempdir().expect("tempdir");
        let paths = DataPaths::new(directory.path().join("data"));
        let backup = directory.path().join("backup");
        std::fs::create_dir(&backup).expect("backup folder");
        let image = directory.path().join("gsi.img");
        std::fs::write(&image, b"not an image").expect("image");

        assert!(
            prepare(
                &paths,
                &backup,
                false,
                &image,
                &front2::profiles::toss_front2::PROFILE
            )
            .is_err()
        );
        let left: Vec<_> = std::fs::read_dir(paths.gsi_candidates())
            .expect("the store exists")
            .collect();
        assert!(left.is_empty());
    }

    #[test]
    fn a_failure_after_the_build_removes_the_candidate_folder() {
        let directory = tempfile::tempdir().expect("tempdir");
        let candidate = directory.path().join("gsi-7");
        std::fs::create_dir(&candidate).expect("candidate folder");
        std::fs::write(candidate.join("super.img"), b"image").expect("image");

        let kept = removed_on_failure(&candidate, Ok(7));
        assert_eq!(kept.expect("the result is not changed"), 7);
        assert!(candidate.join("super.img").is_file());

        let failure: Result<u8> = Err(Front2Error::Plan("no plan".into()));
        let stopped = removed_on_failure(&candidate, failure);
        assert!(
            stopped
                .expect_err("the failure is not changed")
                .to_string()
                .contains("no plan")
        );
        assert!(!candidate.exists());
    }

    #[test]
    fn a_stored_record_is_the_same_without_the_path_that_is_not_stored() {
        use front2::gsi::BackupPartition;

        let built = BackupPartition {
            name: "super".into(),
            lun: 0,
            path: PathBuf::from("full-backups").join("lun0").join("super.img"),
            bytes: 4096,
            sha256: "00".repeat(32),
        };
        let text = serde_json::to_string(&built).expect("store the record");
        let stored: BackupPartition = serde_json::from_str(&text).expect("load the record");
        assert_ne!(stored, built, "the path is not in the stored record");
        assert!(is_same_record(&stored, &built));
        let changed = BackupPartition {
            bytes: 8192,
            ..built.clone()
        };
        assert!(!is_same_record(&stored, &changed));
    }

    #[test]
    fn the_newest_stored_plan_that_fits_is_used_again() {
        use crate::install_support::InstallFixture;

        let fixture = InstallFixture::standard();
        let paths = DataPaths::new(fixture.directory.path().join("data"));
        let plans = paths.install_plans();
        let plan = InstallPlan::build(&fixture.inputs).expect("plan");
        assert!(stored_where(&paths, |_| true).is_none(), "no store yet");
        for name in ["install-5", "install-9", "other-20"] {
            plan.save(&plans.join(name)).expect("save the plan");
        }
        std::fs::create_dir_all(plans.join("install-30")).expect("a folder without a plan");

        let found = stored_where(&paths, |_| true).expect("a stored plan");
        assert_eq!(found.directory, plans.join("install-9"));
        assert_eq!(found.plan.steps, plan.steps);
        let backup = fixture.inputs.backup_directory.clone();
        assert!(stored_where(&paths, |plan| plan.backup_directory == backup).is_some());
        assert!(stored_where(&paths, |_| false).is_none());

        std::fs::write(plans.join("install-9").join(PLAN_FILE), b"{}").expect("change the plan");
        let found = stored_where(&paths, |_| true).expect("the plan before it");
        assert_eq!(found.directory, plans.join("install-5"));
    }

    #[test]
    fn letter_1348_a_found_backup_gives_the_newest_plan_of_the_image_and_its_backup() {
        use crate::install_support::InstallFixture;

        let fixture = InstallFixture::standard();
        let paths = DataPaths::new(fixture.directory.path().join("data"));
        let plans = paths.install_plans();
        let first = InstallPlan::build(&fixture.inputs).expect("plan");
        let mut later = first.clone();
        later.backup_directory = fixture.directory.path().join("later-backup");
        later.candidate_directory = fixture.directory.path().join("later-candidate");
        first.save(&plans.join("install-5")).expect("save the plan");
        later
            .save(&plans.join("install-9"))
            .expect("save the later plan");
        let has_image = |plan: &InstallPlan| plan.candidate_directory == first.candidate_directory;
        let unreadable = |_: &Path| Some("no manifest".to_string());

        let found = stored_for_image(&paths, has_image, |_| None)
            .expect("the backup is complete")
            .expect("a stored plan");
        assert_eq!(found.directory, plans.join("install-5"));
        assert_eq!(found.plan.backup_directory, first.backup_directory);

        let stopped = stored_for_image(&paths, has_image, unreadable)
            .expect_err("the backup of the plan is not read");
        assert!(stopped.to_string().contains("no manifest"));
        let none = stored_for_image(&paths, |_| false, unreadable);
        assert!(none.expect("no plan of the image").is_none());
    }
}
