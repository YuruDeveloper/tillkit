use front2::profiles::toss_front2::PROFILE;
mod super_support;

use front2::Front2Error;
use front2::digest::{sha256_bytes, sha256_file};
use front2::gsi::{
    BackupManifest, GsiCandidate, GsiCandidateRequest, extract_logical_view,
    gsi_candidate::{CANDIDATE_FILE, SUPER_FILE, VIEW_DIRECTORY},
};
use front2::super_image::{
    SuperCandidateRequest, SuperLayoutPlan, SuperVerificationRequest, build_super_candidate,
    verify_super_candidate,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use super_support::{SyntheticSuper, gsi_image};

const GSI_BYTES: u64 = 4608 * 512;
const MANIFEST_FILE: &str = "sha256-manifest.json";

fn write_dump(backup: &Path, lun: u8, name: &str, data: &[u8]) -> Value {
    let directory = backup.join("partitions").join(format!("lun{lun}"));
    std::fs::create_dir_all(&directory).expect("create the LUN directory");
    std::fs::write(directory.join(format!("{name}.bin")), data).expect("write the dump");
    json!({
        "path": format!("partitions\\lun{lun}\\{name}.bin"),
        "bytes": data.len(),
        "sha256": sha256_bytes(data).to_ascii_uppercase(),
    })
}

fn write_manifest(backup: &Path, entries: &[Value]) {
    let text = serde_json::to_string_pretty(entries).expect("manifest text");
    std::fs::write(backup.join(MANIFEST_FILE), text).expect("write the manifest");
}

fn request<'a>(backup: &'a Path, gsi: &'a Path, output: &'a Path) -> GsiCandidateRequest<'a> {
    GsiCandidateRequest {
        profile: &PROFILE,
        backup_directory: backup,
        gsi_image: gsi,
        output_directory: output,
        rollback_evidence: &[],
    }
}

static NO_GSI: front2::DeviceProfile = front2::DeviceProfile {
    id: "no-gsi",
    name: "Device without the GSI install",
    identity: &[("ro.product.vendor.device", "no_gsi")],
    slot_suffix: "_b",
    device_state: "locked",
    verified_boot_state: "green",
    storage_block_bytes: 4096,
    top_vbmeta_algorithm: "SHA256_RSA4096",
    top_vbmeta_key_spec: "testkey_rsa4096",
    remote_stage_parent: "/data/local/tmp",
    supports_gsi: false,
};

#[test]
fn a_profile_without_the_gsi_install_is_refused_before_any_work() {
    let directory = tempfile::tempdir().expect("tempdir");
    let output = directory.path().join("candidate");
    let gsi = directory.path().join("gsi.img");
    let refused = GsiCandidate::build(&GsiCandidateRequest {
        profile: &NO_GSI,
        ..request(directory.path(), &gsi, &output)
    });
    assert!(
        matches!(refused, Err(Front2Error::Unsupported(_))),
        "{refused:?}"
    );
    assert!(!output.exists());
}

#[test]
fn the_manifest_gives_one_dump_for_each_partition_name() {
    let directory = tempfile::tempdir().expect("tempdir");
    let backup = directory.path();
    let boot = write_dump(backup, 4, "boot_a", b"boot image");
    let first_copy = write_dump(backup, 0, "twice", b"one");
    let second_copy = write_dump(backup, 1, "twice", b"two");
    let outside = json!({"path": "..\\partitions\\lun0\\outside.bin", "bytes": 1, "sha256": "00"});
    let wrong_lun = json!({"path": "partitions\\lunx\\letter.bin", "bytes": 1, "sha256": "00"});
    let short_digest = json!({"path": "partitions\\lun0\\short.bin", "bytes": 1, "sha256": "00"});
    write_manifest(
        backup,
        &[
            boot,
            first_copy,
            second_copy,
            outside,
            wrong_lun,
            short_digest,
        ],
    );
    let manifest = BackupManifest::load(backup).expect("manifest");

    let partition = manifest.verified_partition("boot_a").expect("boot_a");
    assert_eq!(partition.lun, 4);
    assert_eq!(partition.bytes, 10);
    assert_eq!(partition.sha256, sha256_bytes(b"boot image"));
    assert!(partition.path.ends_with("partitions/lun4/boot_a.bin"));

    for name in ["twice", "outside", "letter", "short", "absent"] {
        let error = manifest.partition(name).expect_err(name);
        assert!(matches!(error, Front2Error::Image(_)), "{name}: {error}");
    }
    for name in ["../boot_a", "Boot_A", ""] {
        let error = manifest.partition(name).expect_err(name);
        assert!(matches!(error, Front2Error::Plan(_)), "{name}: {error}");
    }
}

#[test]
fn a_changed_dump_is_rejected_and_an_existing_output_is_kept() {
    let directory = tempfile::tempdir().expect("tempdir");
    let backup = directory.path();
    write_manifest(backup, &[write_dump(backup, 4, "dtbo_a", b"device tree")]);
    let manifest = BackupManifest::load(backup).expect("manifest");
    let partition = manifest.verified_partition("dtbo_a").expect("dtbo_a");

    let output = tempfile::tempdir().expect("output");
    let record = partition
        .copy_to(output.path(), "dtbo.img")
        .expect("first copy");
    assert_eq!(record.sha256, partition.sha256);
    assert!(partition.copy_to(output.path(), "dtbo.img").is_err());
    assert_eq!(
        std::fs::read(output.path().join("dtbo.img")).expect("kept copy"),
        b"device tree"
    );

    std::fs::write(&partition.path, b"device trap").expect("change the dump");
    let error = manifest
        .verified_partition("dtbo_a")
        .expect_err("changed dump");
    assert!(matches!(error, Front2Error::Verification(_)), "{error}");
    let error = partition
        .copy_to(output.path(), "second.img")
        .expect_err("changed dump");
    assert!(matches!(error, Front2Error::Verification(_)), "{error}");
}

#[test]
fn the_logical_view_holds_the_partitions_of_the_verified_candidate() {
    let directory = tempfile::tempdir().expect("tempdir");
    let original = directory.path().join("super.bin");
    let source = SyntheticSuper::standard().image();
    std::fs::write(&original, &source).expect("write the super");
    let gsi = directory.path().join("gsi.img");
    std::fs::write(&gsi, gsi_image(GSI_BYTES)).expect("write the GSI");
    let gsi_sha256 = sha256_file(&gsi).expect("GSI digest").1;
    let candidate = directory.path().join(SUPER_FILE);
    let plan = SuperLayoutPlan::from_super_file(&original, GSI_BYTES).expect("plan");
    build_super_candidate(
        &SuperCandidateRequest {
            original_super: &original,
            original_super_sha256: &sha256_bytes(&source),
            gsi_image: &gsi,
            gsi_sha256: &gsi_sha256,
            output: &candidate,
        },
        &plan,
    )
    .expect("candidate");
    let verification = verify_super_candidate(&SuperVerificationRequest {
        candidate: &candidate,
        original_super: &original,
        gsi_bytes: GSI_BYTES,
        gsi_sha256: &gsi_sha256,
    })
    .expect("verification");

    let view = directory.path().join(VIEW_DIRECTORY);
    std::fs::create_dir(&view).expect("create the view");
    let records = extract_logical_view(&candidate, &view).expect("view");
    let names: Vec<&str> = records.iter().map(|record| record.file.as_str()).collect();
    assert_eq!(
        names,
        [
            "odm.img",
            "product.img",
            "system.img",
            "system_dlkm.img",
            "vendor.img",
            "vendor_dlkm.img"
        ]
    );
    for (record, verified) in records.iter().zip(&verification.logical_partitions) {
        assert_eq!(record.bytes, verified.bytes, "{}", record.file);
        assert_eq!(record.sha256, verified.sha256, "{}", record.file);
        record.verify(&view).expect("the file matches its record");
    }
    assert_eq!(
        std::fs::read(view.join("system.img")).expect("system view"),
        gsi_image(GSI_BYTES)
    );
    assert!(extract_logical_view(&candidate, &view).is_err());
}

#[test]
fn a_failed_candidate_leaves_no_output_and_keeps_an_existing_folder() {
    let directory = tempfile::tempdir().expect("tempdir");
    let backup = directory.path().join("backup");
    std::fs::create_dir(&backup).expect("create the backup");
    write_manifest(
        &backup,
        &[write_dump(&backup, 4, "vbmeta_b", b"not a vbmeta image")],
    );
    let gsi = directory.path().join("gsi.img");
    std::fs::write(&gsi, gsi_image(GSI_BYTES)).expect("write the GSI");

    let output = directory.path().join("candidate");
    let error = GsiCandidate::build(&request(&backup, &gsi, &output)).expect_err("no vbmeta_a");
    assert!(error.to_string().contains("vbmeta_a"), "{error}");
    assert!(!output.exists());

    std::fs::create_dir(&output).expect("create the existing folder");
    std::fs::write(output.join("kept.txt"), b"kept").expect("write the kept file");
    assert!(GsiCandidate::build(&request(&backup, &gsi, &output)).is_err());
    assert_eq!(
        std::fs::read(output.join("kept.txt")).expect("kept file"),
        b"kept"
    );
    assert!(GsiCandidate::load(&output).is_err());
}

fn environment_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn recorded_sha256(records: &Value, partition: &str) -> String {
    records
        .as_array()
        .expect("a list of records")
        .iter()
        .find(|record| record["partition"] == partition)
        .unwrap_or_else(|| panic!("no record for {partition}"))["sha256"]
        .as_str()
        .expect("a SHA-256 text")
        .to_string()
}

#[test]
fn real_backup_gives_the_images_that_started_the_device_when_available() {
    let inputs = (
        environment_path("FRONT2_REAL_BACKUP_DIR"),
        environment_path("FRONT2_REAL_EVIDENCE_DIR"),
        environment_path("FRONT2_REAL_GSI_IMAGE"),
        environment_path("FRONT2_REAL_WORK_DIR"),
    );
    let (Some(backup), Some(evidence), Some(gsi), Some(work)) = inputs else {
        println!("skipped: the real backup, evidence, GSI and work folder are not all given");
        return;
    };
    let scratch = tempfile::tempdir_in(&work).expect("scratch folder");
    let output = scratch.path().join("candidate");
    let candidate = GsiCandidate::build(&request(&backup, &gsi, &output)).expect("candidate");

    let recorded: Value = serde_json::from_slice(
        &std::fs::read(evidence.join("prepared-gsi/candidate-verification.json"))
            .expect("read the recorded verification"),
    )
    .expect("parse the recorded verification");
    let booted_top = sha256_file(&evidence.join("root-preparation/current-vbmeta_b.img"))
        .expect("the top vbmeta that started the device")
        .1;
    for image in candidate.images() {
        let expected = match image.partition.as_str() {
            "vbmeta_b" => booted_top.clone(),
            partition => recorded_sha256(&recorded["outputs"], partition),
        };
        assert_eq!(image.image.sha256, expected, "{}", image.partition);
        println!("{} {}", image.partition, image.image.sha256);
    }
    assert_eq!(candidate.images().len(), 8);
    for view in &candidate.logical_view {
        let partition = view.file.replace(".img", "_b");
        assert_eq!(
            view.sha256,
            recorded_sha256(&recorded["logical_partitions"], &partition),
            "{partition}"
        );
    }
    assert_eq!(candidate.logical_view.len(), 6);
    assert!(!candidate.avb_verification.is_empty());
    let (loaded, _) = GsiCandidate::load(&output).expect("load the saved candidate");
    assert_eq!(loaded.images(), candidate.images());
    loaded.verify_files(&output).expect("the files match");
    assert!(output.join(CANDIDATE_FILE).is_file());
}
