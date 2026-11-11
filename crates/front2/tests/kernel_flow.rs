use front2::profiles::toss_front2::PROFILE;
mod kernel_support;
mod support;

use front2::digest::sha256_bytes;
use front2::kernel::kernel_plan_facts::{
    BOOT_FAILURE_STATEMENT, FACTS_FILE, SAME_KMI_STATEMENT, SLOT_FALLBACK_STATEMENT,
};
use front2::kernel::kernel_set::SET_FILE;
use front2::kernel::{
    BootImageHeader, KernelPlan, KernelRelease, KernelSet, KernelStore, build_kernel_candidate,
    find_stored_current_kernel_set, read_current_kernel_set, verify_kernel_images,
};
use front2::pair_apply::{apply, check, reboot, stage};
use front2::{
    ApplyOutcome, ApplyState, Authorization, DeviceState, PairKind, PairRequest, PlanRequest,
    RootAccess, Scope, WritePlan, WritePurpose,
};
use kernel_support::{
    BOOT_PARTITION_BYTES, CURRENT_KERNEL_BYTES, CURRENT_RELEASE, DTBO_DESCRIPTOR_BYTES,
    FORMAT_STRING, KMI, KernelDevice, NEW_RELEASE, SyntheticKernel, TOP_ROLLBACK_INDEX, banner,
    current_kernel, front2_device, reference_descriptors, write_top_vbmeta,
};
use std::path::{Path, PathBuf};

const READBACK: &str = "1000-readback";
const CANDIDATE: &str = "1001-candidate";
const NEW_KERNEL_BYTES: usize = 200_000;

struct Bench {
    directory: tempfile::TempDir,
    store: KernelStore,
    device: KernelDevice,
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
    let mut device = front2_device(&source);
    let state = DeviceState::read(&mut device).expect("device state");
    let store = KernelStore::new(directory.path().join("kernel-sets"), &PROFILE);
    Bench {
        directory,
        store,
        device,
        state,
    }
}

fn new_kernel() -> SyntheticKernel {
    SyntheticKernel::with_release(NEW_RELEASE, NEW_KERNEL_BYTES, 0x3c)
}

fn read(bench: &mut Bench) -> front2::Result<KernelSet> {
    read_current_kernel_set(
        &mut bench.device,
        &bench.state,
        &bench.store,
        request(READBACK),
    )
}

fn build(bench: &Bench, image: &[u8]) -> front2::Result<KernelSet> {
    let path = bench.directory.path().join("new-boot.img");
    std::fs::write(&path, image).expect("new boot image");
    build_kernel_candidate(&bench.store, READBACK, &path, request(CANDIDATE))
}

fn readback_and_candidate(bench: &mut Bench) -> (KernelSet, KernelSet) {
    let readback = read(bench).expect("readback");
    let candidate = build(bench, &new_kernel().boot_image()).expect("candidate");
    (readback, candidate)
}

fn prepare(
    bench: &mut Bench,
    new_set: &str,
    old_set: &str,
    id: &str,
) -> front2::Result<KernelPlan> {
    KernelPlan::prepare(
        &mut bench.device,
        &bench.state,
        &bench.store,
        new_set,
        old_set,
        PlanRequest::strict(id, 2000),
    )
}

fn staged_plan(bench: &mut Bench, new_set: &str, old_set: &str, id: &str) -> (KernelPlan, String) {
    let kernel_plan = prepare(bench, new_set, old_set, id).expect("plan");
    let plan_directory = bench.directory.path().join(id);
    let digest = kernel_plan.save(&plan_directory).expect("save plan");
    bench.device.inner.plan = Some(kernel_plan.plan.clone());
    stage(
        &mut bench.device,
        &bench.store.pair_store(),
        &kernel_plan.plan,
        &plan_directory,
    )
    .expect("stage");
    (kernel_plan, digest)
}

fn approval(digest: &str) -> Authorization {
    Authorization {
        plan_sha256: digest.to_string(),
        scopes: vec![Scope::WritePair, Scope::Reboot],
        granted_unix: 3000,
    }
}

fn run_apply(bench: &mut Bench, plan: &WritePlan, digest: &str) -> ApplyOutcome {
    apply(
        &mut bench.device,
        plan,
        digest,
        &approval(digest),
        &mut || {},
    )
    .expect("apply")
}

fn rejection(bench: &Bench, image: &[u8]) -> String {
    let error = build(bench, image).expect_err("the candidate must be refused");
    assert!(
        !bench.store.root().join(CANDIDATE).exists(),
        "a refused candidate leaves no folder"
    );
    error.to_string()
}

fn with_header_field(mut image: Vec<u8>, offset: usize, value: u32) -> Vec<u8> {
    image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    image
}

#[test]
fn readback_candidate_apply_reboot_and_restore() {
    let mut bench = bench();
    let original_boot = bench.device.partition("boot_b").to_vec();
    let original_vbmeta = bench.device.partition("vbmeta_b").to_vec();
    let (readback, candidate) = readback_and_candidate(&mut bench);

    assert_eq!(readback.kind, PairKind::DeviceReadback);
    assert_eq!(readback.kernel.release, CURRENT_RELEASE);
    assert_eq!(readback.kernel.kmi, KMI);
    assert_eq!(
        readback.boot_header.kernel_size as usize,
        CURRENT_KERNEL_BYTES
    );
    assert_eq!(readback.boot.bytes as usize, BOOT_PARTITION_BYTES);
    assert_eq!(readback.avb.top_rollback_index, TOP_ROLLBACK_INDEX);
    assert_eq!(readback.avb.top_algorithm, "SHA256_RSA4096");
    assert_eq!(readback.avb.boot_footer_algorithm, "NONE");
    assert_eq!(readback.avb.top_chain_partitions, ["vbmeta_system"]);
    assert!(readback.avb.boot_is_consistent());
    let labels: Vec<&str> = readback
        .references
        .iter()
        .map(|reference| reference.label.as_str())
        .collect();
    assert_eq!(
        labels,
        ["vendor_boot_b", "dtbo_b", "init_boot_b", "recovery_b"]
    );
    assert!(
        bench.device.inner.directories.is_empty(),
        "read stage removed"
    );

    assert_eq!(candidate.kind, PairKind::SignedCandidate);
    assert_eq!(candidate.parent.as_deref(), Some(READBACK));
    assert_eq!(candidate.kernel.release, NEW_RELEASE);
    assert_eq!(candidate.kernel.kmi, KMI);
    assert_eq!(candidate.boot_header.kernel_size as usize, NEW_KERNEL_BYTES);
    assert_eq!(candidate.boot.bytes, readback.boot.bytes);
    assert_eq!(candidate.vbmeta.bytes, readback.vbmeta.bytes);
    assert_eq!(candidate.avb.top_rollback_index, TOP_ROLLBACK_INDEX);
    assert_eq!(candidate.avb.top_key_sha1, readback.avb.top_key_sha1);
    assert_eq!(candidate.references, readback.references);
    assert_ne!(
        candidate.avb.boot_digest_in_top,
        readback.avb.boot_digest_in_top
    );

    let (kernel_plan, digest) = staged_plan(&mut bench, CANDIDATE, READBACK, "2000-apply");
    let plan = kernel_plan.plan.clone();
    assert_eq!(plan.purpose, WritePurpose::ApplyCandidate);
    assert_eq!(plan.writes[0].target.label, "boot_b");
    assert_eq!(plan.writes[1].target.label, "vbmeta_b");
    assert_eq!(plan.writes[0].staged_new, "boot.new.img");
    assert_eq!(plan.writes[0].state_while_writing, "WRITING_BOOT");
    assert_eq!(plan.top_rollback_index, TOP_ROLLBACK_INDEX);
    assert!(!plan.wipes_user_data && !plan.changes_active_slot && !plan.restore_tested_on_device);
    let script = plan.script().expect("script");
    assert!(script.contains("/dev/block/by-name/boot_b"));
    assert!(
        !script.contains("init_boot"),
        "only boot and vbmeta are written"
    );
    let facts = &kernel_plan.facts;
    assert_eq!(facts.written_partitions, ["boot_b", "vbmeta_b"]);
    assert_eq!(
        (facts.kmi.as_str(), facts.new_release.as_str()),
        (KMI, NEW_RELEASE)
    );
    assert!(!facts.same_kmi_guarantees_boot && !facts.slot_a_fallback);
    assert!(!facts.adb_available_after_boot_failure);
    assert!(!facts.boot_failure_recovery_tested_on_device);
    assert_eq!(
        facts.statements,
        [
            SAME_KMI_STATEMENT,
            BOOT_FAILURE_STATEMENT,
            SLOT_FALLBACK_STATEMENT
        ]
    );
    let plan_directory = bench.directory.path().join("2000-apply");
    let (loaded, loaded_digest) = KernelPlan::load(&plan_directory, &PROFILE).expect("load plan");
    assert_eq!((&loaded, &loaded_digest), (&kernel_plan, &digest));

    check(&mut bench.device, &plan).expect("check");
    assert_eq!(bench.device.partition("boot_b"), original_boot);
    let mut outcome = run_apply(&mut bench, &plan, &digest);
    assert_eq!(outcome.state, ApplyState::VerifiedPendingReboot);
    assert!(outcome.ready_for_reboot());
    assert!(!bench.device.inner.rebooted, "apply never reboots");
    assert_eq!(
        sha256_bytes(bench.device.partition("boot_b")),
        candidate.boot.sha256
    );
    assert_eq!(
        sha256_bytes(bench.device.partition("vbmeta_b")),
        candidate.vbmeta.sha256
    );
    for reference in &candidate.references {
        reference
            .require_unchanged(
                &mut bench.device,
                RootAccess::Su,
                PROFILE.storage_block_bytes,
            )
            .expect("the reference partitions are untouched by the write");
    }
    let approved = approval(&digest);
    reboot(&mut bench.device, &plan, &digest, &approved, &mut outcome).expect("reboot");
    assert!(bench.device.inner.rebooted && outcome.reboot_requested);

    bench.device.inner.rebooted = false;
    let (restore, restore_digest) = staged_plan(&mut bench, READBACK, CANDIDATE, "2001-restore");
    assert_eq!(restore.plan.purpose, WritePurpose::RestoreReadback);
    let restored = run_apply(&mut bench, &restore.plan, &restore_digest);
    assert!(restored.ready_for_reboot());
    assert_eq!(bench.device.partition("boot_b"), original_boot);
    assert_eq!(bench.device.partition("vbmeta_b"), original_vbmeta);
}

#[test]
fn banner_after_the_format_string_is_chosen() {
    let kernel = current_kernel().build();
    let format_offset = find(&kernel, FORMAT_STRING.as_bytes()).expect("format string");
    let banner_offset = find(&kernel, banner(CURRENT_RELEASE).as_bytes()).expect("banner");
    assert!(format_offset < banner_offset);
    let release = KernelRelease::from_kernel(&kernel).expect("release");
    assert_eq!(release.release, CURRENT_RELEASE);
    assert_eq!(release.kmi, KMI);

    let mut repeated = current_kernel();
    repeated.releases.push(CURRENT_RELEASE.to_string());
    assert_eq!(
        KernelRelease::from_kernel(&repeated.build()).expect("repeated banner"),
        release
    );
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[test]
fn kernels_without_one_well_formed_banner_are_refused() {
    let mut only_format_string = current_kernel();
    only_format_string.releases.clear();
    let mut two_releases = current_kernel();
    two_releases.releases.push(NEW_RELEASE.to_string());
    let mut compressed = current_kernel();
    compressed.arm64_magic = false;
    let with_release = |release: &str| SyntheticKernel::with_release(release, 100_000, 0x3c);
    for (kernel, reason) in [
        (only_format_string, "no Linux banner"),
        (two_releases, "different releases"),
        (compressed, "ARM64 Image magic"),
        (with_release("5.15.41"), "not in the form"),
        (with_release("5.15.41-android13"), "not in the form"),
        (with_release("5.15-android13-8"), "not in the form"),
        (with_release("5.15.41-android13-8abc"), "not in the form"),
        (with_release("5.15.41-androidT-8"), "not in the form"),
    ] {
        let error = KernelRelease::from_kernel(&kernel.build()).expect_err(reason);
        assert!(error.to_string().contains(reason), "{error}");
    }
}

#[test]
fn candidate_refuses_another_kmi() {
    let mut bench = bench();
    read(&mut bench).expect("readback");
    for release in [
        "5.15.41-android13-9-g0c4c3f721017-ab475",
        "5.15.41-android14-8-g0c4c3f721017-ab475",
        "5.10.41-android13-8-g0c4c3f721017-ab475",
        "6.15.41-android13-8-g0c4c3f721017-ab475",
    ] {
        let image = SyntheticKernel::with_release(release, NEW_KERNEL_BYTES, 0x3c).boot_image();
        let error = rejection(&bench, &image);
        assert!(error.contains("has KMI"), "{release}: {error}");
    }
    build(&bench, &new_kernel().boot_image()).expect("the same KMI is accepted");
}

#[test]
fn candidate_refuses_kernels_without_a_usable_banner() {
    let mut bench = bench();
    read(&mut bench).expect("readback");
    let mut only_format_string = new_kernel();
    only_format_string.releases.clear();
    let mut compressed = new_kernel();
    compressed.arm64_magic = false;
    for (kernel, reason) in [
        (only_format_string, "no Linux banner"),
        (compressed, "ARM64 Image magic"),
    ] {
        let error = rejection(&bench, &kernel.boot_image());
        assert!(error.contains(reason), "{error}");
    }
}

#[test]
fn candidate_refuses_other_boot_image_shapes() {
    let mut bench = bench();
    read(&mut bench).expect("readback");
    let image = || new_kernel().boot_image();
    let oversized = SyntheticKernel::with_release(NEW_RELEASE, 990_000, 0x3c).boot_image();
    assert!(oversized.len() < BOOT_PARTITION_BYTES);
    let mut beyond_partition = image();
    beyond_partition.resize(BOOT_PARTITION_BYTES + 4096, 0);
    let mut truncated = image();
    truncated.truncate(4096 + NEW_KERNEL_BYTES - 1);
    let mut no_magic = image();
    no_magic[0] = b'X';
    for (candidate, reason) in [
        (with_header_field(image(), 12, 4096), "ramdisk"),
        (
            with_header_field(image(), 1580, 4096),
            "boot signature block",
        ),
        (with_header_field(image(), 8, 0), "no kernel"),
        (with_header_field(image(), 40, 3), "version 4"),
        (with_header_field(image(), 20, 1580), "version 4"),
        (with_header_field(image(), 16, 1), "header fields"),
        (with_header_field(image(), 44, 0x6f6f_6261), "header fields"),
        (oversized, "AVB footer limit"),
        (beyond_partition, "the partition is"),
        (truncated, "payload"),
        (no_magic, "no Android boot image header"),
        (vec![0u8; 16], "no Android boot image header"),
    ] {
        let error = rejection(&bench, &candidate);
        assert!(error.contains(reason), "expected {reason:?}: {error}");
    }
}

#[test]
fn candidate_identical_to_the_current_boot_is_refused() {
    let mut bench = bench();
    read(&mut bench).expect("readback");
    let partition_image = bench.device.partition("boot_b").to_vec();
    for image in [partition_image, current_kernel().boot_image()] {
        let error = rejection(&bench, &image);
        assert!(error.contains("same kernel"), "{error}");
    }
}

#[test]
fn candidate_must_start_from_a_device_readback() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    let path = bench.directory.path().join("new-boot.img");
    let result = build_kernel_candidate(&bench.store, CANDIDATE, &path, request("1002-second"));
    assert!(result.is_err());
    assert!(!bench.store.root().join("1002-second").exists());
}

fn assert_read_refused(bench: &mut Bench, case: &str) {
    assert!(read(bench).is_err(), "{case}");
    assert!(!bench.store.root().join(READBACK).exists(), "{case}");
    assert!(bench.store.list().expect("list").is_empty(), "{case}");
    assert!(bench.device.inner.directories.is_empty(), "{case}");
}

#[test]
fn readback_refuses_a_top_descriptor_that_does_not_match_boot() {
    let mut changed_kernel = bench();
    changed_kernel.device.partition_mut("boot_b")[4096 + 5000] ^= 1;
    assert_read_refused(&mut changed_kernel, "kernel byte changed under the footer");

    let mut donor = bench();
    let (_, candidate) = readback_and_candidate(&mut donor);
    let candidate_boot = donor
        .store
        .directory(&candidate.id)
        .expect("directory")
        .join("boot.img");
    let mut half_written = bench();
    *half_written.device.partition_mut("boot_b") = std::fs::read(candidate_boot).expect("boot");
    assert_read_refused(&mut half_written, "new boot with the old top vbmeta");
}

#[test]
fn readback_refuses_a_reference_partition_that_does_not_match_its_descriptor() {
    for label in ["vendor_boot_b", "dtbo_b", "init_boot_b", "recovery_b"] {
        let mut bench = bench();
        bench.device.partition_mut(label)[100] ^= 1;
        assert_read_refused(&mut bench, label);
    }
    let mut last_covered = bench();
    last_covered.device.partition_mut("dtbo_b")[DTBO_DESCRIPTOR_BYTES - 1] ^= 1;
    assert_read_refused(&mut last_covered, "last covered dtbo byte");

    let mut outside = bench();
    outside.device.partition_mut("dtbo_b")[DTBO_DESCRIPTOR_BYTES] ^= 1;
    let readback = read(&mut outside).expect("a byte after the covered range is not in the digest");
    let dtbo = &readback.references[1];
    assert_eq!(
        dtbo.sha256,
        sha256_bytes(outside.device.partition("dtbo_b"))
    );
    assert_eq!(dtbo.descriptor_image_bytes as usize, DTBO_DESCRIPTOR_BYTES);
}

#[test]
fn readback_refuses_a_top_vbmeta_with_other_hash_partitions() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut device = front2_device(directory.path());
    let mut descriptors = reference_descriptors();
    descriptors.pop();
    let vbmeta = directory.path().join("short-vbmeta.img");
    write_top_vbmeta(&vbmeta, &directory.path().join("boot.img"), descriptors);
    *device.partition_mut("vbmeta_b") = std::fs::read(&vbmeta).expect("vbmeta");
    let state = DeviceState::read(&mut device).expect("device state");
    let store = KernelStore::new(directory.path().join("kernel-sets"), &PROFILE);
    let result = read_current_kernel_set(&mut device, &state, &store, request(READBACK));
    let error = result.expect_err("a top vbmeta without the recovery hash is not the measured one");
    assert!(error.to_string().contains("hash descriptors"), "{error}");
    assert!(!store.root().join(READBACK).exists());
}

#[test]
fn plan_refuses_a_changed_reference_partition() {
    for label in ["vendor_boot_b", "dtbo_b", "init_boot_b", "recovery_b"] {
        let mut bench = bench();
        readback_and_candidate(&mut bench);
        let last = bench.device.partition(label).len() - 1;
        bench.device.partition_mut(label)[last] ^= 1;
        let error = prepare(&mut bench, CANDIDATE, READBACK, "2000-apply")
            .expect_err("a changed reference partition blocks the plan");
        assert!(error.to_string().contains(label), "{error}");
    }
}

#[test]
fn the_stored_current_set_is_found_only_when_the_device_holds_it() {
    let mut bench = bench();
    let missing = find_stored_current_kernel_set(&mut bench.device, &bench.state, &bench.store)
        .expect_err("the store is empty");
    assert!(
        missing.to_string().contains("no stored kernel set"),
        "{missing}"
    );

    let (readback, candidate) = readback_and_candidate(&mut bench);
    let found = find_stored_current_kernel_set(&mut bench.device, &bench.state, &bench.store)
        .expect("the readback matches the device");
    assert_eq!(found.id, readback.id);

    let (plan, digest) = staged_plan(&mut bench, CANDIDATE, READBACK, "2000-apply");
    run_apply(&mut bench, &plan.plan, &digest);
    let found = find_stored_current_kernel_set(&mut bench.device, &bench.state, &bench.store)
        .expect("the candidate matches the device after the write");
    assert_eq!(found.id, candidate.id);

    bench.device.partition_mut("boot_b")[4096] ^= 1;
    assert!(find_stored_current_kernel_set(&mut bench.device, &bench.state, &bench.store).is_err());
}

#[test]
fn the_plan_facts_refuse_a_reference_partition_that_changed_after_the_plan() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    let plan = prepare(&mut bench, CANDIDATE, READBACK, "2000-apply").expect("plan");
    plan.facts
        .require_references_unchanged(
            &mut bench.device,
            RootAccess::Su,
            PROFILE.storage_block_bytes,
        )
        .expect("nothing changed");
    assert_eq!(plan.facts.reference_partitions.len(), 4);

    bench.device.partition_mut("recovery_b")[0] ^= 1;
    let error = plan
        .facts
        .require_references_unchanged(
            &mut bench.device,
            RootAccess::Su,
            PROFILE.storage_block_bytes,
        )
        .expect_err("recovery_b changed");
    assert!(error.to_string().contains("recovery_b"), "{error}");
}

#[test]
fn letter_1168_apply_refuses_a_reference_partition_that_changed_after_the_stage() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    let (plan, digest) = staged_plan(&mut bench, CANDIDATE, READBACK, "2000-apply");
    let boot_before = bench.device.partition("boot_b").to_vec();
    let vbmeta_before = bench.device.partition("vbmeta_b").to_vec();

    bench.device.partition_mut("recovery_b")[0] ^= 1;
    let error = plan
        .apply(&mut bench.device, &digest, &approval(&digest), &mut || {})
        .expect_err("recovery_b changed after the stage");
    assert!(error.to_string().contains("recovery_b"), "{error}");
    assert!(bench.device.partition("boot_b") == boot_before.as_slice());
    assert!(bench.device.partition("vbmeta_b") == vbmeta_before.as_slice());

    bench.device.partition_mut("recovery_b")[0] ^= 1;
    let outcome = plan
        .apply(&mut bench.device, &digest, &approval(&digest), &mut || {})
        .expect("the references hold the planned content again");
    assert_eq!(outcome.state, ApplyState::VerifiedPendingReboot);
    assert!(outcome.readback_matches_new_pair);
    assert!(bench.device.partition("boot_b") != boot_before.as_slice());
}

#[test]
fn plan_refuses_a_device_that_does_not_hold_the_old_set() {
    for label in ["boot_b", "vbmeta_b"] {
        let mut bench = bench();
        readback_and_candidate(&mut bench);
        let last = bench.device.partition(label).len() / 2;
        bench.device.partition_mut(label)[last] ^= 1;
        let error = prepare(&mut bench, CANDIDATE, READBACK, "2000-apply")
            .expect_err("the device no longer holds the old set");
        assert!(error.to_string().contains(label), "{error}");
    }
}

#[test]
fn plan_refuses_unsupported_state_and_identical_sets() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    for (key, value) in [
        ("ro.boot.slot_suffix", "_a"),
        ("ro.boot.vbmeta.device_state", "unlocked"),
        ("ro.soc.model", "SM8550"),
    ] {
        let supported = bench.state.clone();
        bench
            .state
            .properties
            .insert(key.to_string(), value.to_string());
        assert!(
            prepare(&mut bench, CANDIDATE, READBACK, "2000-apply").is_err(),
            "{key}"
        );
        bench.state = supported;
    }
    assert!(prepare(&mut bench, READBACK, READBACK, "2000-apply").is_err());
    assert!(prepare(&mut bench, CANDIDATE, READBACK, "../escape").is_err());
    assert!(prepare(&mut bench, CANDIDATE, READBACK, "2000-apply").is_ok());
}

#[test]
fn letter_1143_plan_compares_the_running_kernel_with_the_kernel_sets() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    assert_eq!(bench.state.kernel_release.as_deref(), Some(CURRENT_RELEASE));

    for running in [
        None,
        Some("5.15.0-fake"),
        Some("5.10.41-android13-8-g0c4c3f721017-ab475"),
        Some("5.15.41-android14-8-g0c4c3f721017-ab475"),
        Some("5.15.41-android13-9-g0c4c3f721017-ab475"),
    ] {
        bench.state.kernel_release = running.map(str::to_string);
        assert!(
            prepare(&mut bench, CANDIDATE, READBACK, "2000-apply").is_err(),
            "{running:?}"
        );
    }

    bench.state.kernel_release = Some(NEW_RELEASE.to_string());
    let other_running = prepare(&mut bench, CANDIDATE, READBACK, "2000-apply").expect("same KMI");
    assert!(!other_running.facts.old_set_is_running);
    assert_eq!(
        other_running.facts.running_release.as_deref(),
        Some(NEW_RELEASE)
    );

    bench.state.kernel_release = Some(CURRENT_RELEASE.to_string());
    let plan = prepare(&mut bench, CANDIDATE, READBACK, "2000-apply").expect("plan");
    assert!(plan.facts.old_set_is_running);
}

fn edit_text(path: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(path).expect("read text");
    assert!(text.contains(from), "{from:?} is in {}", path.display());
    std::fs::write(path, text.replace(from, to)).expect("write text");
}

#[test]
fn plan_refuses_an_edited_or_damaged_kernel_set() {
    let mut edited = bench();
    readback_and_candidate(&mut edited);
    let set_file = edited.store.root().join(CANDIDATE).join(SET_FILE);
    edit_text(&set_file, NEW_RELEASE, CURRENT_RELEASE);
    assert!(prepare(&mut edited, CANDIDATE, READBACK, "2000-apply").is_err());

    let mut damaged = bench();
    readback_and_candidate(&mut damaged);
    let boot = damaged.store.root().join(CANDIDATE).join("boot.img");
    let mut bytes = std::fs::read(&boot).expect("boot");
    bytes[4096 + 5000] ^= 1;
    std::fs::write(&boot, bytes).expect("boot");
    assert!(prepare(&mut damaged, CANDIDATE, READBACK, "2000-apply").is_err());
    assert!(verify_kernel_images(&damaged.store.root().join(CANDIDATE), &PROFILE).is_err());
}

#[test]
fn saved_plan_facts_reject_edits() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    let (_, digest) = staged_plan(&mut bench, CANDIDATE, READBACK, "2000-apply");
    let plan_directory = bench.directory.path().join("2000-apply");
    let (_, loaded_digest) = KernelPlan::load(&plan_directory, &PROFILE).expect("load");
    assert_eq!(loaded_digest, digest);
    let facts: PathBuf = plan_directory.join(FACTS_FILE);
    edit_text(
        &facts,
        "\"slot_a_fallback\": false",
        "\"slot_a_fallback\": true",
    );
    assert!(KernelPlan::load(&plan_directory, &PROFILE).is_err());
    edit_text(
        &facts,
        "\"slot_a_fallback\": true",
        "\"slot_a_fallback\": false",
    );
    KernelPlan::load(&plan_directory, &PROFILE).expect("the restored facts load again");
    edit_text(&facts, "does not guarantee", "guarantees");
    assert!(KernelPlan::load(&plan_directory, &PROFILE).is_err());
}

#[test]
fn kernel_set_folder_is_not_a_pair() {
    let mut bench = bench();
    readback_and_candidate(&mut bench);
    let pair_store = bench.store.pair_store();
    assert!(pair_store.list().expect("list").is_empty());
    assert!(pair_store.load(READBACK).is_err());
    assert_eq!(
        pair_store.directory(CANDIDATE).expect("directory"),
        bench.store.directory(CANDIDATE).expect("directory")
    );
    let ids: Vec<String> = bench
        .store
        .list()
        .expect("list")
        .into_iter()
        .map(|set| set.id)
        .collect();
    assert_eq!(ids, [CANDIDATE, READBACK]);
}

#[test]
fn candidate_from_a_real_boot_backup_when_available() {
    let Some(backup) = std::env::var_os("FRONT2_REAL_BACKUP_DIR").map(PathBuf::from) else {
        eprintln!("skipped: FRONT2_REAL_BACKUP_DIR is not set");
        return;
    };
    let directory = tempfile::tempdir().expect("tempdir");
    let store = KernelStore::new(directory.path().join("kernel-sets"), &PROFILE);
    let readback_directory = store.begin(READBACK).expect("begin");
    let boot = readback_directory.join("boot.img");
    std::fs::copy(backup.join("partitions/lun4/boot_a.bin"), &boot).expect("copy boot");
    let image = std::fs::read(&boot).expect("read boot");
    let header = BootImageHeader::parse(&image).expect("header");
    header.require_kernel_only().expect("kernel only");
    let release = KernelRelease::from_kernel(header.kernel(&image).expect("kernel"));
    println!("header: {header:?}");
    println!("release: {release:?}");
    assert_eq!(release.expect("release").kmi, KMI);

    let vbmeta = readback_directory.join("vbmeta.img");
    let real_top = std::env::var_os("FRONT2_REAL_TOP_VBMETA").map(PathBuf::from);
    match &real_top {
        Some(top) => drop(std::fs::copy(top, &vbmeta).expect("copy top vbmeta")),
        None => write_top_vbmeta(&vbmeta, &boot, reference_descriptors()),
    }
    println!("top vbmeta is the real signed one: {}", real_top.is_some());
    verify_kernel_images(&readback_directory, &PROFILE).expect("the readback verifies");
    let readback = store
        .commit(
            request(READBACK),
            PairKind::DeviceReadback,
            "_b",
            None,
            Vec::new(),
        )
        .expect("commit readback");
    readback.avb.require_trust(&PROFILE).expect("front2 trust");
    println!("readback avb: {:#?}", readback.avb);

    let mut changed = image[..header.payload_bytes() as usize].to_vec();
    changed[4096 + 0x2000] ^= 1;
    let new_boot = directory.path().join("new-boot.img");
    std::fs::write(&new_boot, changed).expect("new boot");
    let candidate =
        build_kernel_candidate(&store, READBACK, &new_boot, request(CANDIDATE)).expect("candidate");
    println!("candidate avb: {:#?}", candidate.avb);
    println!("candidate boot: {:?}", candidate.boot);
    println!("candidate vbmeta: {:?}", candidate.vbmeta);
    assert_eq!(candidate.kernel, readback.kernel);
    assert_eq!(candidate.boot_header, readback.boot_header);
    assert_eq!(
        candidate.avb.top_rollback_index,
        readback.avb.top_rollback_index
    );
    assert_eq!(
        candidate.avb.boot_payload_bytes,
        readback.avb.boot_payload_bytes
    );
    assert_ne!(candidate.kernel_sha256, readback.kernel_sha256);
}
