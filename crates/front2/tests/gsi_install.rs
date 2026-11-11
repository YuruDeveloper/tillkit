use front2::profiles::toss_front2::PROFILE;
mod gpt_support;
mod install_support;

use front2::digest::{sha256_bytes, sha256_file};
use front2::gsi::{CandidateImage, GsiCandidate, install_inputs, install_inputs_from_images};
use front2::install::{InstallInputs, InstallPlan, InstallStep};
use front2::{Front2Error, Result};
use install_support::InstallFixture;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

const LUN_COUNT: u8 = 6;
const MANIFEST_FILE: &str = "sha256-manifest.json";

fn manifest_entries(backup: &Path) -> Vec<Value> {
    let mut entries = Vec::new();
    for lun in 0..LUN_COUNT {
        let folder = format!("partitions/lun{lun}");
        fs::create_dir_all(backup.join(&folder)).expect("LUN folder");
        for kind in ["main", "backup"] {
            let name = format!("gpt_{kind}{lun}.bin");
            fs::copy(
                backup.join("gpt").join(&name),
                backup.join(&folder).join(&name),
            )
            .expect("copy the GPT dump");
        }
        let mut names: Vec<String> = fs::read_dir(backup.join(&folder))
            .expect("read the LUN folder")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .into_string()
                    .expect("name")
            })
            .collect();
        names.sort();
        for name in names {
            let data = fs::read(backup.join(&folder).join(&name)).expect("read the dump");
            entries.push(json!({
                "path": format!("partitions\\lun{lun}\\{name}"),
                "bytes": data.len(),
                "sha256": sha256_bytes(&data).to_ascii_uppercase(),
            }));
        }
    }
    entries
}

fn write_manifest(backup: &Path, entries: &[Value]) {
    let text = serde_json::to_string_pretty(entries).expect("manifest text");
    fs::write(backup.join(MANIFEST_FILE), text).expect("write the manifest");
}

fn candidate_images(fixture: &InstallFixture) -> Vec<CandidateImage> {
    fixture
        .inputs
        .images
        .iter()
        .map(|image| CandidateImage {
            partition: image.partition.clone(),
            image: image.candidate.clone(),
            backup_source: None,
        })
        .collect()
}

fn original_super_sha256(fixture: &InstallFixture) -> String {
    fixture
        .inputs
        .images
        .iter()
        .find(|image| image.partition == "super")
        .expect("the fixture holds the super image")
        .original
        .sha256
        .clone()
}

fn inputs_of(fixture: &InstallFixture) -> Result<InstallInputs> {
    install_inputs_from_images(
        &candidate_images(fixture),
        &original_super_sha256(fixture),
        &fixture.inputs.candidate_directory,
        &fixture.inputs.backup_directory,
    )
}

fn prepared_fixture() -> InstallFixture {
    let fixture = InstallFixture::standard();
    let backup = &fixture.inputs.backup_directory;
    write_manifest(backup, &manifest_entries(backup));
    fixture
}

#[test]
fn the_backup_and_the_candidate_images_give_the_inputs_of_the_install_plan() {
    let fixture = prepared_fixture();
    let inputs = inputs_of(&fixture).expect("inputs");
    let plan = InstallPlan::build(&inputs).expect("plan");
    let expected = InstallPlan::build(&fixture.inputs).expect("the plan of the fixture");

    assert_eq!(plan.steps, expected.steps);
    assert_eq!(plan.data_restore, expected.data_restore);
    assert_eq!(inputs.partition_map, fixture.inputs.partition_map);
    assert_eq!(inputs.misc_first_sector, fixture.inputs.misc_first_sector);
    assert_eq!(inputs.gpt_backups.len(), 12);
    let gpt_file = &plan.gpt_copies[0].file;
    assert_eq!(gpt_file, "partitions/lun0/gpt_main0.bin");
    for (restore, expected) in plan.restore_set.iter().zip(&expected.restore_set) {
        assert_eq!(restore.name, expected.name);
        assert_eq!(restore.sha256, expected.sha256);
    }
}

#[test]
fn a_candidate_from_another_backup_is_rejected() {
    let fixture = prepared_fixture();
    let error = install_inputs_from_images(
        &candidate_images(&fixture),
        &"0f".repeat(32),
        &fixture.inputs.candidate_directory,
        &fixture.inputs.backup_directory,
    )
    .expect_err("another super image");
    assert!(matches!(error, Front2Error::Plan(_)), "{error}");
    assert!(
        error.to_string().contains("the candidate was made from"),
        "{error}"
    );
}

#[test]
fn a_backup_without_one_verified_dump_of_each_gpt_copy_is_rejected() {
    let fixture = prepared_fixture();
    let backup = &fixture.inputs.backup_directory;
    let entries = manifest_entries(backup);

    let without: Vec<Value> = entries
        .iter()
        .filter(|entry| entry["path"] != "partitions\\lun3\\gpt_main3.bin")
        .cloned()
        .collect();
    write_manifest(backup, &without);
    let error = inputs_of(&fixture).expect_err("no dump of gpt_main3");
    assert!(error.to_string().contains("gpt_main3"), "{error}");

    write_manifest(backup, &entries);
    let dump = backup.join("partitions/lun4/gpt_backup4.bin");
    let mut bytes = fs::read(&dump).expect("read the GPT dump");
    bytes[0] ^= 1;
    fs::write(&dump, bytes).expect("change the GPT dump");
    let error = inputs_of(&fixture).expect_err("changed dump");
    assert!(matches!(error, Front2Error::Verification(_)), "{error}");

    let other = prepared_fixture();
    let userdata = other
        .inputs
        .backup_directory
        .join("partitions/lun0/userdata.bin");
    fs::remove_file(userdata).expect("remove the userdata dump");
    let inputs = inputs_of(&other).expect("the manifest still records userdata");
    assert!(InstallPlan::build(&inputs).is_err());
}

fn environment_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn recorded_writes(recorded: &Value) -> Vec<&Value> {
    recorded["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .filter(|step| step["kind"] == "write_and_readback")
        .collect()
}

#[test]
fn real_candidate_gives_the_install_plan_that_worked_on_the_device_when_available() {
    let paths = (
        environment_path("FRONT2_REAL_GSI_CANDIDATE_DIR"),
        environment_path("FRONT2_REAL_BACKUP_DIR"),
        environment_path("FRONT2_REAL_EVIDENCE_DIR"),
    );
    let (Some(candidate_directory), Some(backup), Some(evidence)) = paths else {
        println!("skipped: the real candidate folder, backup and evidence are not all given");
        return;
    };
    let (candidate, _) = GsiCandidate::load(&candidate_directory).expect("candidate");
    let inputs =
        install_inputs(&candidate, &candidate_directory, &backup, &PROFILE).expect("inputs");
    let plan = InstallPlan::build(&inputs).expect("plan");

    let recorded: Value = serde_json::from_slice(
        &fs::read(evidence.join("prepared-gsi/install-plan-proposal.json"))
            .expect("read the recorded plan"),
    )
    .expect("parse the recorded plan");
    let booted_top = sha256_file(&evidence.join("root-preparation/current-vbmeta_b.img"))
        .expect("the top vbmeta that started the device")
        .1;
    let writes = plan.write_steps();
    let recorded_steps = recorded_writes(&recorded);
    assert_eq!(writes.len(), recorded_steps.len());
    for ((_, step), expected) in writes.iter().zip(recorded_steps) {
        assert_eq!(step.name, expected["partition"], "{}", step.name);
        assert_eq!(u64::from(step.lun), expected["lun"], "{}", step.name);
        assert_eq!(step.start_sector, expected["start_sector"], "{}", step.name);
        assert_eq!(step.sectors, expected["sectors"], "{}", step.name);
        let expected_sha256 = match step.name.as_str() {
            "vbmeta_b" => booted_top.as_str(),
            _ => expected["sha256"].as_str().expect("a SHA-256 text"),
        };
        assert_eq!(step.sha256, expected_sha256, "{}", step.name);
        println!(
            "{} {} {} {}",
            step.name, step.lun, step.start_sector, step.sha256
        );
    }
    assert!(matches!(
        plan.steps[writes[writes.len() - 1].0 - 1],
        InstallStep::SetBootLun { lun: 2 }
    ));

    let userdata = recorded["restore_files"]
        .as_array()
        .expect("restore files")
        .iter()
        .find(|entry| entry["partition"] == "userdata")
        .expect("the recorded userdata entry");
    let restore = &plan.data_restore;
    assert_eq!(restore.start_sector, userdata["start_sector"]);
    assert_eq!(restore.sectors, userdata["sectors"]);
    assert_eq!(restore.bytes, userdata["bytes"]);
    assert_eq!(restore.sha256, userdata["sha256"].as_str().expect("text"));

    let directory = tempfile::tempdir().expect("tempdir");
    let plan_directory = directory.path().join("plan");
    let sha256 = plan.save(&plan_directory).expect("save the plan");
    let stored = InstallPlan::load(&plan_directory).expect("load the plan");
    assert_eq!(stored.plan_sha256(), sha256);
}
