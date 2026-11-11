use crate::clock::now_unix;
use crate::data_paths::DataPaths;
use crate::gsi_review::GsiReview;
use crate::install_review::InstallReview;
use front2::gsi::{GsiCandidate, GsiCandidateRequest, install_inputs};
use front2::install::InstallPlan;
use front2::{Front2Error, Result};
use std::path::{Path, PathBuf};

const CANDIDATE_PREFIX: &str = "gsi-";

pub fn build_candidate(paths: &DataPaths, backup: &Path, gsi_image: &Path) -> Result<GsiReview> {
    let root = paths.gsi_candidates();
    std::fs::create_dir_all(&root)?;
    let directory = root.join(format!("{CANDIDATE_PREFIX}{}", now_unix()));
    let request = GsiCandidateRequest {
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

pub fn plan_install(paths: &DataPaths, review: &GsiReview, backup: &Path) -> Result<InstallReview> {
    let (candidate, sha256) = GsiCandidate::load(&review.directory)?;
    if sha256 != review.sha256 || candidate != review.candidate {
        return Err(Front2Error::Plan(
            "the GSI candidate record changed after the review".into(),
        ));
    }
    candidate.verify_files(&review.directory)?;
    let inputs = install_inputs(&candidate, &review.directory, backup)?;
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

pub fn newest_candidate(paths: &DataPaths) -> Option<GsiReview> {
    let mut directories: Vec<PathBuf> = std::fs::read_dir(paths.gsi_candidates())
        .ok()?
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| is_candidate_directory(path))
        .collect();
    directories.sort();
    directories
        .into_iter()
        .rev()
        .find_map(|directory| load(directory).ok())
}

fn is_candidate_directory(path: &Path) -> bool {
    let named = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(CANDIDATE_PREFIX));
    named && path.is_dir()
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
    fn a_store_without_a_readable_candidate_gives_none() {
        let directory = tempfile::tempdir().expect("tempdir");
        let paths = DataPaths::new(directory.path());
        assert!(newest_candidate(&paths).is_none());

        let root = paths.gsi_candidates();
        std::fs::create_dir_all(root.join("gsi-1700000000")).expect("empty candidate folder");
        std::fs::create_dir_all(root.join("other")).expect("other folder");
        std::fs::write(root.join("gsi-1800000000"), b"a file").expect("a file");
        assert!(newest_candidate(&paths).is_none());
    }

    #[test]
    fn a_build_without_a_manifest_leaves_no_candidate_folder() {
        let directory = tempfile::tempdir().expect("tempdir");
        let paths = DataPaths::new(directory.path().join("data"));
        let backup = directory.path().join("backup");
        std::fs::create_dir(&backup).expect("backup folder");
        let image = directory.path().join("gsi.img");
        std::fs::write(&image, b"not an image").expect("image");

        assert!(build_candidate(&paths, &backup, &image).is_err());
        let left: Vec<_> = std::fs::read_dir(paths.gsi_candidates())
            .expect("the store exists")
            .collect();
        assert!(left.is_empty());
    }
}
