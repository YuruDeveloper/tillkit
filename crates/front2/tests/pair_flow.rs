mod support;

use front2::candidate::build_signed_candidate;
use front2::current_pair::find_stored_current_pair;
use front2::pair_apply::{apply, check, reboot, stage};
use front2::pair_backup::read_current_pair;
use front2::{
    ApplyOutcome, ApplyState, Authorization, DeviceState, PairKind, PairManifest, PairRequest,
    PairStore, PlanRequest, Scope, SupportOverride, SupportReport, WritePlan, WritePurpose,
};
use std::path::Path;
use support::{
    BOOT_DIGEST_BYTE, FakeDevice, Fault, TOP_ROLLBACK_INDEX, patched_init_boot,
    write_readback_pair, write_readback_pair_with_boot_digest,
};

const READBACK: &str = "1000-readback";
const CANDIDATE: &str = "1001-candidate";
const OTHER_READBACK: &str = "1002-other-readback";
const OTHER_CANDIDATE: &str = "1003-other-candidate";

struct Bench {
    directory: tempfile::TempDir,
    store: PairStore,
    device: FakeDevice,
    state: DeviceState,
}

fn request(id: &str) -> PairRequest<'_> {
    PairRequest {
        id,
        created_unix: 1000,
        note: "test",
    }
}

fn bench() -> Bench {
    let directory = tempfile::tempdir().expect("tempdir");
    let source = directory.path().join("source");
    std::fs::create_dir(&source).expect("source directory");
    write_readback_pair(&source);
    let mut device = FakeDevice::front2(
        std::fs::read(source.join("init_boot.img")).expect("init_boot"),
        std::fs::read(source.join("vbmeta.img")).expect("vbmeta"),
    );
    let state = DeviceState::read(&mut device).expect("device state");
    let store = PairStore::new(directory.path().join("pairs"));
    Bench {
        directory,
        store,
        device,
        state,
    }
}

fn readback_and_candidate(bench: &mut Bench) -> (PairManifest, PairManifest) {
    let readback = read_current_pair(
        &mut bench.device,
        &bench.state,
        &bench.store,
        request(READBACK),
    )
    .expect("readback");
    let patched = bench.directory.path().join("patched.img");
    std::fs::write(&patched, patched_init_boot()).expect("patched image");
    let candidate = build_signed_candidate(&bench.store, READBACK, &patched, request(CANDIDATE))
        .expect("candidate");
    (readback, candidate)
}

fn staged_plan(bench: &mut Bench, new_pair: &str, old_pair: &str, id: &str) -> (WritePlan, String) {
    let plan = WritePlan::prepare(
        &mut bench.device,
        &bench.state,
        &bench.store,
        new_pair,
        old_pair,
        PlanRequest::strict(id, 2000),
    )
    .expect("plan");
    let plan_directory = bench.directory.path().join(id);
    let digest = plan.save(&plan_directory).expect("save plan");
    bench.device.plan = Some(plan.clone());
    stage(&mut bench.device, &bench.store, &plan, &plan_directory).expect("stage");
    (plan, digest)
}

fn approval(digest: &str, scopes: &[Scope]) -> Authorization {
    Authorization {
        plan_sha256: digest.to_string(),
        scopes: scopes.to_vec(),
        granted_unix: 3000,
    }
}

fn run_apply(bench: &mut Bench, plan: &WritePlan, digest: &str) -> ApplyOutcome {
    let approved = approval(digest, &[Scope::WritePair, Scope::Reboot]);
    apply(&mut bench.device, plan, digest, &approved, &mut || {}).expect("apply")
}

fn stage_directory_is_clean(path: &Path) -> bool {
    !path.exists()
}

#[test]
fn readback_candidate_apply_reboot_and_restore() {
    let mut bench = bench();
    let original_init_boot = bench.device.partition("init_boot_b").to_vec();
    let original_vbmeta = bench.device.partition("vbmeta_b").to_vec();
    let (readback, candidate) = readback_and_candidate(&mut bench);

    assert_eq!(readback.kind, PairKind::DeviceReadback);
    assert_eq!(readback.avb.top_rollback_index, TOP_ROLLBACK_INDEX);
    assert!(readback.avb.pair_is_consistent());
    assert!(bench.device.directories.is_empty(), "read stage removed");
    assert_eq!(candidate.kind, PairKind::SignedCandidate);
    assert_eq!(candidate.parent.as_deref(), Some(READBACK));
    assert_eq!(candidate.avb.top_rollback_index, TOP_ROLLBACK_INDEX);
    assert_eq!(candidate.avb.top_key_sha1, readback.avb.top_key_sha1);
    assert_eq!(candidate.avb.top_chain_partitions, ["vbmeta_system"]);
    assert_ne!(
        candidate.avb.init_boot_digest_in_top,
        readback.avb.init_boot_digest_in_top
    );
    assert_eq!(candidate.init_boot.bytes, readback.init_boot.bytes);
    assert_eq!(candidate.vbmeta.bytes, readback.vbmeta.bytes);

    let (plan, digest) = staged_plan(&mut bench, CANDIDATE, READBACK, "2000-apply");
    assert_eq!(plan.purpose, WritePurpose::ApplyCandidate);
    assert!(!plan.wipes_user_data && !plan.changes_active_slot && !plan.restore_tested_on_device);
    assert_eq!(plan.writes[0].target.label, "init_boot_b");
    assert_eq!(plan.writes[1].target.label, "vbmeta_b");
    check(&mut bench.device, &plan).expect("check");
    assert_eq!(bench.device.partition("init_boot_b"), original_init_boot);

    let mut outcome = run_apply(&mut bench, &plan, &digest);
    assert_eq!(outcome.state, ApplyState::VerifiedPendingReboot);
    assert!(outcome.ready_for_reboot());
    assert!(!bench.device.rebooted, "apply never reboots");
    assert_eq!(
        front2::digest::sha256_bytes(bench.device.partition("init_boot_b")),
        candidate.init_boot.sha256
    );
    assert_eq!(
        front2::digest::sha256_bytes(bench.device.partition("vbmeta_b")),
        candidate.vbmeta.sha256
    );
    let approved = approval(&digest, &[Scope::WritePair, Scope::Reboot]);
    reboot(&mut bench.device, &plan, &digest, &approved, &mut outcome).expect("reboot");
    assert!(bench.device.rebooted && outcome.reboot_requested);

    bench.device.rebooted = false;
    let (restore, restore_digest) = staged_plan(&mut bench, READBACK, CANDIDATE, "2001-restore");
    assert_eq!(restore.purpose, WritePurpose::RestoreReadback);
    let restored = run_apply(&mut bench, &restore, &restore_digest);
    assert!(restored.ready_for_reboot());
    assert_eq!(bench.device.partition("init_boot_b"), original_init_boot);
    assert_eq!(bench.device.partition("vbmeta_b"), original_vbmeta);
}

#[test]
fn corrupt_pull_leaves_no_pair_in_the_store() {
    let mut bench = bench();
    bench.device.fault = Fault::CorruptPull;
    let result = read_current_pair(
        &mut bench.device,
        &bench.state,
        &bench.store,
        request(READBACK),
    );
    assert!(result.is_err());
    assert!(stage_directory_is_clean(&bench.store.root().join(READBACK)));
    assert!(bench.store.list().expect("list").is_empty());
    assert!(bench.device.directories.is_empty(), "read stage removed");
}

#[test]
fn plan_refuses_a_device_that_does_not_hold_the_old_pair() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    bench.device.partition_mut("vbmeta_b")[100] ^= 1;
    let result = WritePlan::prepare(
        &mut bench.device,
        &bench.state,
        &bench.store,
        CANDIDATE,
        READBACK,
        PlanRequest::strict("2000-apply", 2000),
    );
    assert!(result.is_err());
}

#[test]
fn plan_refuses_unsupported_state_and_identical_pairs() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    let prepare = |bench: &mut Bench, state: &DeviceState, new_pair: &str| {
        WritePlan::prepare(
            &mut bench.device,
            state,
            &bench.store,
            new_pair,
            READBACK,
            PlanRequest::strict("2000-apply", 2000),
        )
    };
    for (key, value) in [
        ("ro.boot.slot_suffix", "_a"),
        ("ro.boot.vbmeta.device_state", "unlocked"),
        ("ro.soc.model", "SM8550"),
    ] {
        let mut state = bench.state.clone();
        state.properties.insert(key.to_string(), value.to_string());
        assert!(prepare(&mut bench, &state, CANDIDATE).is_err(), "{key}");
    }
    let state = bench.state.clone();
    assert!(prepare(&mut bench, &state, READBACK).is_err());
    assert!(prepare(&mut bench, &state, CANDIDATE).is_ok());
}

fn with_property(state: &DeviceState, key: &str, value: &str) -> DeviceState {
    let mut changed = state.clone();
    changed
        .properties
        .insert(key.to_string(), value.to_string());
    changed
}

fn accepted_differences(state: &DeviceState) -> SupportOverride {
    SupportOverride::accept_unmet(&SupportReport::evaluate(state)).expect("acceptable differences")
}

fn prepare_accepting(
    bench: &mut Bench,
    state: &DeviceState,
    support_override: SupportOverride,
) -> front2::Result<WritePlan> {
    let request = PlanRequest {
        id: "2000-apply",
        created_unix: 2000,
        support_override,
    };
    WritePlan::prepare(
        &mut bench.device,
        state,
        &bench.store,
        CANDIDATE,
        READBACK,
        request,
    )
}

#[test]
fn plan_records_the_differences_that_the_approval_accepts() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    let other_build = with_property(
        &bench.state,
        "ro.vendor.build.fingerprint",
        "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/476:user/dev-keys",
    );
    let accepted = accepted_differences(&other_build);
    assert_eq!(accepted.accepted.len(), 1);

    let plan = prepare_accepting(&mut bench, &other_build, accepted.clone()).expect("plan");
    assert_eq!(plan.support_override, accepted);
    assert_eq!(plan.profile, front2::profile::ID);
    let directory = bench.directory.path().join("2000-apply");
    let digest = plan.save(&directory).expect("save plan");
    let (loaded, loaded_digest) = WritePlan::load(&directory).expect("load plan");
    assert_eq!((loaded, loaded_digest), (plan, digest));

    let strict = SupportOverride::none();
    assert!(prepare_accepting(&mut bench, &other_build, strict).is_err());
}

#[test]
fn plan_refuses_an_approval_for_the_differences_of_another_device() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    let other_build = with_property(&bench.state, "ro.soc.model", "SM8550");
    let accepted = accepted_differences(&other_build);

    let supported = bench.state.clone();
    assert!(prepare_accepting(&mut bench, &supported, accepted.clone()).is_err());
    let other_value = with_property(&bench.state, "ro.soc.model", "SM8650");
    assert!(prepare_accepting(&mut bench, &other_value, accepted.clone()).is_err());
    let two_differences = with_property(&other_build, "ro.board.platform", "kalama");
    assert!(prepare_accepting(&mut bench, &two_differences, accepted).is_err());
}

#[test]
fn no_approval_accepts_a_missing_root_or_an_unknown_slot() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    let mut rootless = bench.state.clone();
    rootless.root_access = None;
    let slots = ["", "_c"].map(|slot| with_property(&bench.state, "ro.boot.slot_suffix", slot));
    for state in [rootless, slots[0].clone(), slots[1].clone()] {
        let report = SupportReport::evaluate(&state);
        assert!(SupportOverride::accept_unmet(&report).is_err());
        let forged = SupportOverride {
            accepted: report.unmet().into_iter().cloned().collect(),
        };
        assert!(prepare_accepting(&mut bench, &state, forged).is_err());
    }
    let other_slot = with_property(&bench.state, "ro.boot.slot_suffix", "_a");
    let report = SupportReport::evaluate(&other_slot);
    assert!(SupportOverride::accept_unmet(&report).is_ok());
}

#[test]
fn plan_refuses_pairs_from_a_baseline_with_another_boot_digest() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    let other = bench.store.begin(OTHER_READBACK).expect("begin");
    write_readback_pair_with_boot_digest(&other, BOOT_DIGEST_BYTE + 1);
    commit_other_readback(&bench);
    let patched = bench.directory.path().join("patched.img");
    build_signed_candidate(
        &bench.store,
        OTHER_READBACK,
        &patched,
        request(OTHER_CANDIDATE),
    )
    .expect("a candidate from the other baseline is a valid pair on its own");

    let mut prepare = |new_pair: &str| {
        WritePlan::prepare(
            &mut bench.device,
            &bench.state,
            &bench.store,
            new_pair,
            READBACK,
            PlanRequest::strict("2000-apply", 2000),
        )
    };
    assert!(prepare(OTHER_CANDIDATE).is_err());
    assert!(prepare(OTHER_READBACK).is_err());
    assert!(prepare(CANDIDATE).is_ok());
}

fn commit_other_readback(bench: &Bench) {
    let kind = PairKind::DeviceReadback;
    bench
        .store
        .commit(OTHER_READBACK, kind, 1002, "_b", None, "test")
        .expect("the manifest records the files as they are");
}

fn flip_byte(path: &Path, offset: usize) {
    let mut bytes = std::fs::read(path).expect("read image");
    bytes[offset] ^= 1;
    std::fs::write(path, bytes).expect("write image");
}

fn damage_pair(directory: &Path, damage: &str) {
    if damage == "payload" {
        return flip_byte(&directory.join("init_boot.img"), 5000);
    }
    let vbmeta = directory.join("vbmeta.img");
    let info = avbtool_rs::image::inspect_avb_image(&vbmeta).expect("inspect vbmeta");
    flip_byte(&vbmeta, 256 + info.header.signature_offset as usize);
}

#[test]
fn plan_refuses_a_readback_that_fails_avb_verification() {
    for damage in ["payload", "signature"] {
        let mut bench = bench();
        readback_and_candidate(&mut bench);
        let damaged = bench.store.begin(OTHER_READBACK).expect("begin");
        write_readback_pair(&damaged);
        damage_pair(&damaged, damage);
        commit_other_readback(&bench);
        let result = WritePlan::prepare(
            &mut bench.device,
            &bench.state,
            &bench.store,
            OTHER_READBACK,
            READBACK,
            PlanRequest::strict("2000-restore", 2000),
        );
        assert!(result.is_err(), "{damage}");
    }
}

#[test]
fn changed_staged_script_is_not_run() {
    let mut bench = bench();
    let original = bench.device.partition("init_boot_b").to_vec();
    readback_and_candidate(&mut bench);
    let (plan, digest) = staged_plan(&mut bench, CANDIDATE, READBACK, "2000-apply");
    check(&mut bench.device, &plan).expect("the staged script is unchanged");

    let script = format!(
        "{}/{}",
        plan.remote_stage,
        front2::apply_script::SCRIPT_FILE
    );
    bench.device.files.insert(script, b"changed".to_vec());
    bench.device.commands.clear();
    assert!(check(&mut bench.device, &plan).is_err());
    let approved = approval(&digest, &[Scope::WritePair]);
    assert!(apply(&mut bench.device, &plan, &digest, &approved, &mut || {}).is_err());
    let script_ran = bench
        .device
        .commands
        .iter()
        .any(|command| command.contains("--check") || command.contains("--apply"));
    assert!(!script_ran);
    assert_eq!(bench.device.partition("init_boot_b"), original);
}

#[test]
fn corrupt_upload_stops_before_the_device_script_runs() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    let plan = WritePlan::prepare(
        &mut bench.device,
        &bench.state,
        &bench.store,
        CANDIDATE,
        READBACK,
        PlanRequest::strict("2000-apply", 2000),
    )
    .expect("plan");
    let plan_directory = bench.directory.path().join("2000-apply");
    plan.save(&plan_directory).expect("save plan");
    bench.device.fault = Fault::CorruptPush;
    assert!(stage(&mut bench.device, &bench.store, &plan, &plan_directory).is_err());
    assert!(
        !bench
            .device
            .commands
            .iter()
            .any(|command| command.contains("--apply"))
    );
}

#[test]
fn approval_must_match_the_plan_and_the_scope() {
    let mut bench = bench();
    let original = bench.device.partition("init_boot_b").to_vec();
    readback_and_candidate(&mut bench);
    let (plan, digest) = staged_plan(&mut bench, CANDIDATE, READBACK, "2000-apply");
    let other_plan = approval(&"0".repeat(64), &[Scope::WritePair, Scope::Reboot]);
    let reboot_only = approval(&digest, &[Scope::Reboot]);
    for approved in [other_plan, reboot_only] {
        let result = apply(&mut bench.device, &plan, &digest, &approved, &mut || {});
        assert!(result.is_err());
    }
    assert_eq!(bench.device.partition("init_boot_b"), original);

    let mut outcome = run_apply(&mut bench, &plan, &digest);
    let write_only = approval(&digest, &[Scope::WritePair]);
    assert!(reboot(&mut bench.device, &plan, &digest, &write_only, &mut outcome).is_err());
    assert!(!bench.device.rebooted && !outcome.reboot_requested);
}

#[test]
fn failed_or_unfinished_writes_never_reach_reboot() {
    for (fault, state) in [
        (Fault::SecondWriteFails, ApplyState::RollbackVerified),
        (Fault::RollbackFails, ApplyState::RollbackFailedDoNotReboot),
        (
            Fault::NeverFinishes,
            ApplyState::Writing("WRITING_VBMETA".into()),
        ),
        (
            Fault::ReportsVerifiedWithStaleVbmeta,
            ApplyState::VerifiedPendingReboot,
        ),
    ] {
        let mut bench = bench();
        readback_and_candidate(&mut bench);
        let (plan, digest) = staged_plan(&mut bench, CANDIDATE, READBACK, "2000-apply");
        bench.device.fault = fault;
        let mut outcome = run_apply(&mut bench, &plan, &digest);
        assert_eq!(outcome.state, state, "{fault:?}");
        assert!(!outcome.ready_for_reboot(), "{fault:?}");
        let approved = approval(&digest, &[Scope::WritePair, Scope::Reboot]);
        assert!(reboot(&mut bench.device, &plan, &digest, &approved, &mut outcome).is_err());
        assert!(!bench.device.rebooted, "{fault:?}");
    }
}

#[test]
fn partition_change_after_the_write_blocks_reboot() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    let (plan, digest) = staged_plan(&mut bench, CANDIDATE, READBACK, "2000-apply");
    let mut outcome = run_apply(&mut bench, &plan, &digest);
    assert!(outcome.ready_for_reboot());
    bench.device.partition_mut("vbmeta_b")[200] ^= 1;
    let approved = approval(&digest, &[Scope::WritePair, Scope::Reboot]);
    assert!(reboot(&mut bench.device, &plan, &digest, &approved, &mut outcome).is_err());
    assert!(!bench.device.rebooted && !outcome.reboot_requested);
}

#[test]
fn saved_plan_reloads_with_the_same_digest_and_rejects_edits() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    let (plan, digest) = staged_plan(&mut bench, CANDIDATE, READBACK, "2000-apply");
    let plan_directory = bench.directory.path().join("2000-apply");
    let (loaded, loaded_digest) = WritePlan::load(&plan_directory).expect("load");
    assert_eq!(
        (loaded, loaded_digest.as_str()),
        (plan.clone(), digest.as_str())
    );

    let path = plan_directory.join(front2::write_plan::PLAN_FILE);
    let edited = std::fs::read_to_string(&path)
        .expect("plan text")
        .replace("/dev/block/sde62", "/dev/block/sde63");
    std::fs::write(&path, edited).expect("edit plan");
    assert!(WritePlan::load(&plan_directory).is_err());
}

#[test]
fn stored_current_pair_is_found_only_when_the_device_holds_it() {
    let mut bench = bench();
    let missing = find_stored_current_pair(&mut bench.device, &bench.state, &bench.store);
    assert!(missing.is_err(), "an empty store has no current pair");

    let (readback, candidate) = readback_and_candidate(&mut bench);
    let found = find_stored_current_pair(&mut bench.device, &bench.state, &bench.store)
        .expect("the readback is the current pair");
    assert_eq!(found.id, readback.id);

    let candidate_directory = bench.store.directory(&candidate.id).expect("directory");
    let candidate_init_boot =
        std::fs::read(candidate_directory.join("init_boot.img")).expect("candidate init_boot");
    *bench.device.partition_mut("init_boot_b") = candidate_init_boot;
    let mixed = find_stored_current_pair(&mut bench.device, &bench.state, &bench.store);
    assert!(
        mixed.is_err(),
        "a half-written device matches no stored pair"
    );

    let candidate_vbmeta =
        std::fs::read(candidate_directory.join("vbmeta.img")).expect("candidate vbmeta");
    *bench.device.partition_mut("vbmeta_b") = candidate_vbmeta;
    let found = find_stored_current_pair(&mut bench.device, &bench.state, &bench.store)
        .expect("the candidate is the current pair");
    assert_eq!(found.id, candidate.id);

    let mut rootless = bench.state.clone();
    rootless.root_access = None;
    assert!(find_stored_current_pair(&mut bench.device, &rootless, &bench.store).is_err());
}
