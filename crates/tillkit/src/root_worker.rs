use crate::data_paths::DataPaths;
use crate::pair_session::PairSession;
use crate::root_review::RootReview;
use front2::magisk::{MagiskPackage, PatchInputs, build_magisk_candidate, read_preinit_device};
use front2::{Front2Error, Result, SupportOverride};
use std::path::Path;

pub fn is_already_patched(error: &str) -> bool {
    let prefix = Front2Error::AlreadyPatched(String::new()).to_string();
    error.starts_with(&prefix)
}

pub fn prepare(
    paths: &DataPaths,
    local_package: Option<&Path>,
    support_override: SupportOverride,
) -> Result<RootReview> {
    let mut session = PairSession::open(paths, support_override)?;
    let preinit_device = read_preinit_device(&mut session.shell)?;
    let mut log = Vec::new();
    let cache = paths.magisk_cache();
    let package = match local_package {
        Some(source) => MagiskPackage::from_local_file(source, &cache)?,
        None => MagiskPackage::fetch_latest(&cache, &mut log)?,
    };
    let work_dir = paths
        .magisk_work()
        .join(format!("patch-{}", session.created_unix));
    let candidate_id = format!("magisk-{}", session.created_unix);
    let inputs = PatchInputs {
        package: &package,
        preinit_device: &preinit_device,
        work_dir: &work_dir,
    };
    let candidate = build_magisk_candidate(
        &session.store,
        &session.backup.id,
        session.state.root_access,
        inputs,
        session.request(&candidate_id),
        &mut log,
    );
    let _ = std::fs::remove_dir_all(&work_dir);
    let review = session.plan(paths, &candidate?)?;
    Ok(RootReview { review, package })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_that_magisk_patched_before_is_told_apart_from_other_errors() {
        let patched = Front2Error::AlreadyPatched("init_boot.img".into()).to_string();
        assert!(is_already_patched(&patched));
        let other = Front2Error::Image("init_boot.img has no ramdisk.cpio".into()).to_string();
        assert!(!is_already_patched(&other));
        assert!(!is_already_patched(""));
    }
}
