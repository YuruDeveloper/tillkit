use crate::clock::now_unix;
use crate::data_paths::DataPaths;
use crate::full_backup_store;
use crate::gsi_review::GsiReview;
use crate::install_review::InstallReview;
use crate::plan_build::PlanBuild;
use crate::prepared_install::PreparedInstall;
use front2::digest::sha256_file;
use front2::gpt::BootSlot;
use front2::gsi::{
    GsiCandidate, GsiCandidateRequest, StockBuild, SystemOrigin, backup_boot_slot,
    backup_stock_build, backup_system_origin, find_testkey_slot, install_inputs,
    install_source_slot, require_slot_a_active,
};
use front2::install::{InstallPlan, PLAN_FILE};
use front2::{DeviceProfile, Front2Error, Result};
use serde::Serialize;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const CANDIDATE_PREFIX: &str = "gsi-";
const PLAN_PREFIX: &str = "install-";
const BACKUP_NOT_READ: &str = "이 이미지로 만든 계획이 쓰던 전체 백업을 읽지 못했습니다. 그 폴더를 읽을 수 있게 한 뒤에 MAKE PLAN을 다시 누르거나, USE EXISTING BACKUP으로 쓸 전체 백업을 직접 골라 주세요.";
const BACKUP_SLOTS_UNSUPPORTED: &str = "이 전체 백업으로는 설치 계획을 만들 수 없습니다. 백업에 기록된 파티션 표가 설치에 필요한 슬롯 상태와 맞지 않습니다. 자세한 이유는 뒤에 함께 표시됩니다. 순정 상태에서 미리 만들어 둔 전체 백업이 있다면, USE EXISTING BACKUP으로 그 폴더를 선택해 주세요.";
const IN_PLACE_REFUSED: &str = "이 전체 백업으로는 부팅 슬롯을 바꾸지 않고 제자리에 GSI를 설치하는 계획을 만들 수 없습니다. 뒤에 적힌 이유를 확인해 주세요. 순정 상태에서 만들어 둔 전체 백업이 있다면 USE EXISTING BACKUP으로 그 폴더를 골라 주세요. 기기가 그 백업과 다른 상태이면 RESTORE TO DEVICE로 먼저 되돌려야 합니다.";
const NO_SYSTEM_FINGERPRINT: &str = "(system fingerprint 없음)";

pub fn prepare(
    paths: &DataPaths,
    backup: &Path,
    backup_is_found: bool,
    gsi_image: &Path,
    profile: &DeviceProfile,
    accepted_build: Option<&str>,
) -> Result<PreparedInstall> {
    if let Some(stored) = stored(paths, backup, backup_is_found, gsi_image)? {
        let target_slot = stored.plan.target_slot;
        let source_slot = stored_source_slot(&stored.plan)?;
        let build = backup_stock_build(&stored.plan.backup_directory, source_slot);
        let replaced = replaced_system(&stored.plan.backup_directory, source_slot, target_slot);
        return Ok(planned(stored, build, replaced));
    }
    let (stock_slot, target_slot) = installable_slots(backup)?;
    let source_slot = install_source_slot(stock_slot, target_slot)?;
    let build = backup_stock_build(backup, source_slot)?;
    if let Some(fingerprint) = build.unaccepted_fingerprint(accepted_build) {
        return Ok(PreparedInstall::UntestedBuild(fingerprint.to_string()));
    }
    let review = build_candidate(
        paths,
        backup,
        gsi_image,
        profile,
        accepted_build,
        stock_slot,
        target_slot,
    );
    let review = told_if_in_place(stock_slot, target_slot, review)?;
    let install = plan_install(paths, &review, backup, profile);
    let install = removed_on_failure(&review.directory, install);
    let install = told_if_in_place(stock_slot, target_slot, install)?;
    let replaced = replaced_system(backup, source_slot, target_slot);
    Ok(planned(install, Ok(build), replaced))
}

fn stored_source_slot(plan: &InstallPlan) -> Result<BootSlot> {
    let (candidate, _) = GsiCandidate::load(&plan.candidate_directory)?;
    Ok(candidate.source_slot)
}

fn planned(
    review: InstallReview,
    build: Result<StockBuild>,
    replaced_system: Option<String>,
) -> PreparedInstall {
    let build = PlanBuild::from_read(build);
    PreparedInstall::Planned(Box::new(InstallReview {
        build,
        replaced_system,
        ..review
    }))
}

fn replaced_system(backup: &Path, source_slot: BootSlot, target_slot: BootSlot) -> Option<String> {
    if source_slot != target_slot {
        return None;
    }
    match backup_system_origin(backup, source_slot).ok()? {
        SystemOrigin::Stock => None,
        SystemOrigin::Other(fingerprint) => Some(fingerprint),
        SystemOrigin::NoFingerprint => Some(NO_SYSTEM_FINGERPRINT.to_string()),
    }
}

fn installable_slots(backup: &Path) -> Result<(BootSlot, BootSlot)> {
    let stock_slot = told_as_unsupported_slots(backup_boot_slot(backup))?;
    let target_slot = install_target_slot(backup)?;
    if stock_slot == BootSlot::A && target_slot == BootSlot::B {
        told_as_unsupported_slots(require_slot_a_active(backup))?;
    }
    Ok((stock_slot, target_slot))
}

// The install target is the test key slot, because the tool can only reproduce
// the test key. The kk device holds the test key on slot A. find_testkey_slot
// returns a slot only when one slot holds the test key. For no test key slot or
// two test key slots, the target is slot B.
fn install_target_slot(backup: &Path) -> Result<BootSlot> {
    match find_testkey_slot(backup) {
        Ok(slot) => Ok(slot),
        Err(Front2Error::Unsupported(_)) => Ok(BootSlot::B),
        Err(other) => Err(other),
    }
}

fn told_as_unsupported_slots<Value>(result: Result<Value>) -> Result<Value> {
    match result {
        Err(Front2Error::Unsupported(reason)) => Err(Front2Error::Plan(format!(
            "{BACKUP_SLOTS_UNSUPPORTED} ({reason})"
        ))),
        other => other,
    }
}

fn told_if_in_place<Value>(
    stock_slot: BootSlot,
    target_slot: BootSlot,
    result: Result<Value>,
) -> Result<Value> {
    match result {
        Err(Front2Error::Unsupported(reason)) if stock_slot == target_slot => {
            Err(Front2Error::Plan(format!("{IN_PLACE_REFUSED} ({reason})")))
        }
        other => other,
    }
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
        build: PlanBuild::Tested,
        replaced_system: None,
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
    accepted_build: Option<&str>,
    stock_slot: BootSlot,
    target_slot: BootSlot,
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
        accepted_untested_build: accepted_build,
        stock_slot,
        target_slot,
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
        build: PlanBuild::Tested,
        replaced_system: None,
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
                &front2::profiles::toss_front2::PROFILE,
                None
            )
            .is_err()
        );
        let left = std::fs::read_dir(paths.gsi_candidates()).map_or(0, Iterator::count);
        assert_eq!(left, 0);
    }

    #[test]
    fn letter_1462_a_plan_keeps_the_build_or_the_failure_of_its_read() {
        use crate::install_support::InstallFixture;

        let fixture = InstallFixture::new(BootSlot::A, BootSlot::B);
        let stored = fixture.stored_plan();
        let review = InstallReview {
            plan: stored.plan().clone(),
            sha256: stored.plan_sha256().to_string(),
            directory: fixture.plan_directory(),
            build: PlanBuild::Tested,
            replaced_system: None,
        };
        let other = "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/468:user/dev-keys";
        let untested = StockBuild::Untested(other.to_string());
        let shown = |build: Result<StockBuild>| match planned(review.clone(), build, None) {
            PreparedInstall::Planned(planned) => planned.build,
            PreparedInstall::UntestedBuild(asked) => panic!("{asked} is not a plan"),
        };

        assert_eq!(shown(Ok(untested)), PlanBuild::Untested(other.to_string()));
        assert_eq!(shown(Ok(StockBuild::Tested)), PlanBuild::Tested);
        let changed = Front2Error::Verification("vbmeta_a.bin changed".into());
        let PlanBuild::NotRead(reason) = shown(Err(changed)) else {
            panic!("a stored plan with a changed backup stays usable and tells the failure");
        };
        assert!(reason.contains("vbmeta_a.bin changed"), "{reason}");
    }

    #[test]
    fn a_backup_that_refuses_an_in_place_install_is_told_in_plain_words() {
        let reason = "LUN 4: boot_a has the active flag".to_string();
        for slot in [BootSlot::A, BootSlot::B] {
            let told =
                told_if_in_place::<()>(slot, slot, Err(Front2Error::Unsupported(reason.clone())))
                    .expect_err("a backup that is not tidy for an in-place install is not used");
            assert!(matches!(told, Front2Error::Plan(_)), "{told}");
            let text = told.to_string();
            assert!(text.contains(IN_PLACE_REFUSED), "{text}");
            assert!(text.contains(&reason), "{text}");
        }

        let kept = told_if_in_place::<()>(
            BootSlot::A,
            BootSlot::B,
            Err(Front2Error::Unsupported("other".into())),
        )
        .expect_err("a slot A failure stays as it is");
        assert!(matches!(kept, Front2Error::Unsupported(_)), "{kept}");

        let switch = told_if_in_place::<()>(
            BootSlot::B,
            BootSlot::A,
            Err(Front2Error::Unsupported("other".into())),
        )
        .expect_err("a slot B to slot A failure stays as it is");
        assert!(matches!(switch, Front2Error::Unsupported(_)), "{switch}");

        assert!(told_if_in_place(BootSlot::B, BootSlot::B, Ok(7)).is_ok());
    }

    #[test]
    fn a_backup_with_unsupported_slots_is_told_in_plain_words() {
        let reason = "LUN 4: boot_a is not active".to_string();
        let told = told_as_unsupported_slots::<()>(Err(Front2Error::Unsupported(reason.clone())))
            .expect_err("the backup is not used");
        assert!(matches!(told, Front2Error::Plan(_)), "{told}");
        assert!(told.to_string().contains(BACKUP_SLOTS_UNSUPPORTED));
        assert!(told.to_string().contains(&reason));

        let changed = Err(Front2Error::Verification("changed".into()));
        let other =
            told_as_unsupported_slots::<()>(changed).expect_err("other failures stay as they are");
        assert!(matches!(other, Front2Error::Verification(_)), "{other}");
        assert!(told_as_unsupported_slots(Ok(())).is_ok());
    }

    #[test]
    fn letter_1354_two_active_slots_are_not_told_as_an_inactive_slot_a() {
        let reason = "LUN 4: boot_b is already active".to_string();
        let told = told_as_unsupported_slots::<()>(Err(Front2Error::Unsupported(reason.clone())))
            .expect_err("the backup is not used");
        let text = told.to_string();
        assert!(text.contains(&reason), "{text}");
        assert!(text.contains(BACKUP_SLOTS_UNSUPPORTED), "{text}");
        assert!(!text.contains("A에서 B로"), "{text}");
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

        let fixture = InstallFixture::new(BootSlot::A, BootSlot::B);
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

        let fixture = InstallFixture::new(BootSlot::A, BootSlot::B);
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

    fn stored_candidate(source_slot: BootSlot) -> GsiCandidate {
        use front2::ImageRecord;
        use front2::gsi::{BackupPartition, SignedVbmeta};
        use front2::super_image::SuperVerification;

        let image = ImageRecord {
            file: "image.img".to_string(),
            bytes: 1,
            sha256: "00".to_string(),
        };
        let vbmeta = SignedVbmeta {
            image: image.clone(),
            algorithm: "SHA256_RSA4096".to_string(),
            key_spec: "testkey_rsa4096".to_string(),
            rollback_index: 0,
            rollback_index_location: 0,
            flags: 0,
            required_libavb_version_minor: 0,
            descriptors: 0,
        };
        let original_super = BackupPartition {
            name: "super".to_string(),
            lun: 0,
            path: PathBuf::new(),
            bytes: 1,
            sha256: "00".to_string(),
        };
        let super_verification = SuperVerification {
            metadata_sha256: "00".to_string(),
            logical_partitions: Vec::new(),
            allowed_super_ranges: Vec::new(),
            outside_ranges_equal_bytes: 0,
            candidate_bytes: 1,
            candidate_sha256: "00".to_string(),
        };
        GsiCandidate {
            schema: front2::gsi::gsi_candidate::CANDIDATE_SCHEMA,
            stock_slot: BootSlot::B,
            target_slot: BootSlot::A,
            source_slot,
            gsi_bytes: 1,
            gsi_sha256: "00".to_string(),
            original_super,
            super_image: image,
            super_verification,
            logical_view: Vec::new(),
            physical_images: Vec::new(),
            chained_vbmeta: vbmeta.clone(),
            top_vbmeta: vbmeta,
            avb_verification: Vec::new(),
            first_conversion_only: true,
            fec_recomputed: false,
            device_boot_verified: false,
            stored_rollback_verified: false,
        }
    }

    #[test]
    fn letter_1674_a_stored_plan_shows_the_source_slot_of_its_candidate() {
        use crate::install_support::InstallFixture;
        use front2::gsi::gsi_candidate::CANDIDATE_FILE;

        let fixture = InstallFixture::new(BootSlot::B, BootSlot::A);
        let mut plan = InstallPlan::build(&fixture.inputs).expect("plan");
        for source_slot in [BootSlot::A, BootSlot::B] {
            let directory = fixture
                .directory
                .path()
                .join(format!("candidate-{}", source_slot.suffix()));
            std::fs::create_dir(&directory).expect("candidate folder");
            let record = serde_json::to_vec(&stored_candidate(source_slot)).expect("record");
            std::fs::write(directory.join(CANDIDATE_FILE), record).expect("store the record");
            plan.candidate_directory = directory;

            let shown = stored_source_slot(&plan).expect("the source slot of the candidate");
            assert_eq!(shown, source_slot);
        }

        plan.candidate_directory = fixture.directory.path().join("no-candidate");
        assert!(stored_source_slot(&plan).is_err());
    }
}
