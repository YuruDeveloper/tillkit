use crate::clock::now_unix;
use crate::data_paths::DataPaths;
use crate::gsi_review::GsiReview;
use crate::install_review::InstallReview;
use front2::digest::sha256_file;
use front2::gsi::{GsiCandidate, GsiCandidateRequest, install_inputs};
use front2::install::{InstallPlan, PLAN_FILE};
use front2::{DeviceProfile, Front2Error, Result};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const CANDIDATE_PREFIX: &str = "gsi-";
const PLAN_PREFIX: &str = "install-";

pub fn prepare(
    paths: &DataPaths,
    backup: &Path,
    gsi_image: &Path,
    profile: &DeviceProfile,
) -> Result<InstallReview> {
    if let Some(stored) = stored(paths, backup, gsi_image) {
        return Ok(stored);
    }
    let review = build_candidate(paths, backup, gsi_image, profile)?;
    plan_install(paths, &review, backup, profile)
}

fn stored(paths: &DataPaths, backup: &Path, gsi_image: &Path) -> Option<InstallReview> {
    let (gsi_bytes, gsi_sha256) = sha256_file(gsi_image).ok()?;
    stored_where(paths, |plan| {
        let candidate = GsiCandidate::load(&plan.candidate_directory).ok();
        plan.backup_directory == backup
            && candidate.is_some_and(|(candidate, _)| {
                candidate.gsi_bytes == gsi_bytes && candidate.gsi_sha256 == gsi_sha256
            })
    })
}

pub fn planned_backup(paths: &DataPaths) -> Option<PathBuf> {
    newest_plan_folders(paths)
        .iter()
        .find_map(|directory| plan_record(directory))
        .map(|plan| plan.backup_directory)
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
    let review = load(directory)?;
    if review.candidate != candidate {
        return Err(Front2Error::Verification(
            "the stored GSI candidate record differs from the built one".into(),
        ));
    }
    Ok(review)
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
        assert_eq!(planned_backup(&paths), Some(backup.clone()));
        assert!(stored_where(&paths, |plan| plan.backup_directory == backup).is_some());
        assert!(stored_where(&paths, |_| false).is_none());

        std::fs::write(plans.join("install-9").join(PLAN_FILE), b"{}").expect("change the plan");
        let found = stored_where(&paths, |_| true).expect("the plan before it");
        assert_eq!(found.directory, plans.join("install-5"));
    }
}
