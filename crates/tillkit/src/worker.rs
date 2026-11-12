use crate::adb_shell::AdbShell;
use crate::clock::now_unix;
use crate::data_paths::DataPaths;
use crate::diagnosis::Diagnosis;
use crate::message::Message;
use crate::pair_session::PairSession;
use crate::plan_review::PlanReview;
use crate::plan_source::PlanSource;
use front2::candidate::build_signed_candidate;
use front2::current_pair::{NO_STORED_PAIR, find_stored_current_pair};
use front2::kernel::current_kernel_set::NO_STORED_KERNEL_SET;
use front2::kernel::{KernelPlan, KernelStore, read_current_kernel_set};
use front2::pair_backup;
use front2::profiles;
use front2::{
    ApplyOutcome, Authorization, DeviceState, Front2Error, PairKind, PairManifest, PairRequest,
    PairStore, Result, SupportOverride, WritePlan, pair_apply,
};
use iced::Task;
use std::path::Path;
use std::time::Duration;

const STATE_POLL_INTERVAL: Duration = Duration::from_secs(1);
pub const NO_BACKUP: &str = "no backup holds the images that the device has now";

pub fn is_no_backup(error: &str) -> bool {
    let store = |text: &str| Front2Error::Store(text.into()).to_string();
    error == store(NO_BACKUP)
        || error.starts_with(&store(NO_STORED_PAIR))
        || error.starts_with(&store(NO_STORED_KERNEL_SET))
}

pub fn run_blocking<T, F>(
    work: F,
    done: fn(std::result::Result<T, String>) -> Message,
) -> Task<Message>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    Task::perform(
        async move {
            match tokio::task::spawn_blocking(work).await {
                Ok(result) => result.map_err(|error| error.to_string()),
                Err(error) => Err(format!("the worker stopped: {error}")),
            }
        },
        done,
    )
}

pub fn read_device() -> Result<Diagnosis> {
    let mut shell = AdbShell::connect()?;
    let state = DeviceState::read(&mut shell)?;
    Ok(Diagnosis::evaluate(state))
}

pub fn back_up(paths: &DataPaths) -> Result<()> {
    let mut shell = AdbShell::connect()?;
    let state = DeviceState::read(&mut shell)?;
    let created_unix = now_unix();
    let id = format!("readback-{created_unix}");
    let request = || PairRequest {
        id: &id,
        created_unix,
        note: "",
    };
    let profile = Diagnosis::profile_for(&state);
    let pairs = PairStore::new(paths.pairs(), profile);
    pair_backup::read_current_pair(&mut shell, &state, &pairs, request())?;
    let kernel_sets = KernelStore::new(paths.kernel_sets(), profile);
    read_current_kernel_set(&mut shell, &state, &kernel_sets, request())?;
    Ok(())
}

pub fn is_backup_of(pair: &PairManifest, current: &PairManifest) -> bool {
    pair.kind == PairKind::DeviceReadback
        && pair.slot_suffix == current.slot_suffix
        && pair.init_boot.sha256 == current.init_boot.sha256
        && pair.vbmeta.sha256 == current.vbmeta.sha256
}

pub fn current_backup(
    shell: &mut AdbShell,
    state: &DeviceState,
    store: &PairStore,
) -> Result<PairManifest> {
    let current = find_stored_current_pair(shell, state, store)?;
    store
        .list()?
        .into_iter()
        .find(|pair| is_backup_of(pair, &current))
        .ok_or_else(|| Front2Error::Store(NO_BACKUP.into()))
}

pub fn prepare_root_write(
    paths: &DataPaths,
    patched_init_boot: &Path,
    support_override: SupportOverride,
) -> Result<PlanReview> {
    let session = PairSession::open(paths, support_override)?;
    let candidate_id = format!("candidate-{}", session.created_unix);
    let candidate = build_signed_candidate(
        &session.store,
        &session.backup.id,
        patched_init_boot,
        session.request(&candidate_id),
    )?;
    session.plan(paths, &candidate)
}

fn load_reviewed(review: &PlanReview) -> Result<WritePlan> {
    let (plan, sha256, source) = load_stored(review)?;
    if sha256 != review.sha256 || plan != review.plan || source != review.source {
        return Err(Front2Error::Plan(
            "the plan file changed after the review".into(),
        ));
    }
    Ok(plan)
}

fn load_stored(review: &PlanReview) -> Result<(WritePlan, String, PlanSource)> {
    let profile = profiles::find(&review.plan.profile)?;
    if review.source == PlanSource::Pair {
        let (plan, sha256) = WritePlan::load(&review.directory, profile)?;
        return Ok((plan, sha256, PlanSource::Pair));
    }
    let (kernel_plan, sha256) = KernelPlan::load(&review.directory, profile)?;
    let source = PlanSource::Kernel(Box::new(kernel_plan.facts));
    Ok((kernel_plan.plan, sha256, source))
}

pub fn stage_plan(paths: &DataPaths, review: &PlanReview) -> Result<()> {
    let plan = load_reviewed(review)?;
    let mut shell = AdbShell::connect()?;
    let store = PairStore::new(
        review.source.store_root(paths),
        profiles::find(&plan.profile)?,
    );
    pair_apply::stage(&mut shell, &store, &plan, &review.directory)?;
    pair_apply::check(&mut shell, &plan)
}

pub fn write_pair(review: &PlanReview, authorization: &Authorization) -> Result<ApplyOutcome> {
    let plan = load_reviewed(review)?;
    let mut shell = AdbShell::connect()?;
    let mut wait = || std::thread::sleep(STATE_POLL_INTERVAL);
    let Some(facts) = review.source.kernel_facts() else {
        return pair_apply::apply(&mut shell, &plan, &review.sha256, authorization, &mut wait);
    };
    let kernel_plan = KernelPlan {
        plan,
        facts: facts.clone(),
    };
    kernel_plan.apply(&mut shell, &review.sha256, authorization, &mut wait)
}

pub fn reboot(
    review: &PlanReview,
    authorization: &Authorization,
    mut outcome: ApplyOutcome,
) -> Result<ApplyOutcome> {
    let plan = load_reviewed(review)?;
    let mut shell = AdbShell::connect()?;
    if let Some(facts) = review.source.kernel_facts() {
        let block_bytes = profiles::find(&plan.profile)?.storage_block_bytes;
        facts.require_references_unchanged(&mut shell, plan.root_access, block_bytes)?;
    }
    pair_apply::reboot(
        &mut shell,
        &plan,
        &review.sha256,
        authorization,
        &mut outcome,
    )?;
    Ok(outcome)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    const MANIFEST: &str = r#"{
        "schema": 1,
        "id": "readback-1",
        "kind": "DeviceReadback",
        "created_unix": 1,
        "slot_suffix": "_b",
        "init_boot": { "file": "init_boot.img", "bytes": 8, "sha256": "11" },
        "vbmeta": { "file": "vbmeta.img", "bytes": 8, "sha256": "33" },
        "avb": {
            "top_algorithm": "SHA256_RSA4096",
            "top_key_sha1": "",
            "top_rollback_index": 0,
            "top_rollback_index_location": 0,
            "top_flags": 0,
            "top_hash_partitions": [],
            "top_chain_partitions": [],
            "init_boot_digest_in_top": "55",
            "init_boot_footer_digest": "55",
            "init_boot_footer_rollback_index": 0,
            "init_boot_payload_bytes": 4
        },
        "parent": null,
        "note": ""
    }"#;

    pub fn pair(id: &str, kind: PairKind) -> PairManifest {
        let mut pair: PairManifest = serde_json::from_str(MANIFEST).expect("manifest");
        pair.id = id.into();
        pair.kind = kind;
        pair
    }

    #[test]
    fn a_missing_backup_is_told_from_other_store_errors() {
        let missing = Front2Error::Store(NO_BACKUP.into()).to_string();
        assert!(is_no_backup(&missing));
        let no_pair = format!("{NO_STORED_PAIR}: init_boot 11, vbmeta 22");
        assert!(is_no_backup(&Front2Error::Store(no_pair).to_string()));
        let no_kernel_set = format!("{NO_STORED_KERNEL_SET}: boot 11, vbmeta 22");
        assert!(is_no_backup(&Front2Error::Store(no_kernel_set).to_string()));
        let unsupported = Front2Error::Unsupported(NO_STORED_PAIR.into()).to_string();
        assert!(!is_no_backup(&unsupported));
        let other = Front2Error::Store("the manifest is not valid".into()).to_string();
        assert!(!is_no_backup(&other));
        assert!(!is_no_backup(NO_BACKUP), "the bare text is no store error");
        assert!(!is_no_backup(""));
    }

    #[test]
    fn the_backup_of_a_written_candidate_is_the_readback_with_the_same_images() {
        let current = pair("candidate-2", PairKind::SignedCandidate);
        assert!(
            !is_backup_of(&current, &current),
            "a candidate is no backup"
        );
        let readback = pair("readback-3", PairKind::DeviceReadback);
        assert!(is_backup_of(&readback, &current));

        let mut other_slot = readback.clone();
        other_slot.slot_suffix = "_a".into();
        assert!(!is_backup_of(&other_slot, &current));
        let mut earlier = readback.clone();
        earlier.init_boot.sha256 = "00".into();
        assert!(!is_backup_of(&earlier, &current));
        let mut other_vbmeta = readback;
        other_vbmeta.vbmeta.sha256 = "00".into();
        assert!(!is_backup_of(&other_vbmeta, &current));
    }
}
