use crate::clock::now_unix;
use crate::data_paths::DataPaths;
use crate::gsi_review::GsiReview;
use crate::install_review::InstallReview;
use front2::gsi::{GsiCandidate, GsiCandidateRequest, install_inputs};
use front2::install::InstallPlan;
use front2::{DeviceProfile, Front2Error, Result};
use std::path::{Path, PathBuf};

const CANDIDATE_PREFIX: &str = "gsi-";

pub fn prepare(
    paths: &DataPaths,
    backup: &Path,
    gsi_image: &Path,
    profile: &DeviceProfile,
) -> Result<InstallReview> {
    let review = build_candidate(paths, backup, gsi_image, profile)?;
    plan_install(paths, &review, backup, profile)
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
        .join(format!("install-{}", now_unix()));
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
}
