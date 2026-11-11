mod gpt_support;
mod install_support;

use front2::digest::sha256_bytes;
use front2::gpt::{GptCopy, GptCopyKind, PartitionMap};
use front2::install::{
    CHUNK_SECTORS, DataWipeApproval, DigestCheck, InstallExecutor, InstallPlan, InstallStep,
    PLAN_FILE, RestartApproval, StepSource, StepState, StoredInstallPlan, WriteApproval, WriteStep,
};
use front2::{Front2Error, Result};
use gpt_support::{PARTITION_SECTORS, SECTOR};
use install_support::{InstallFixture, MockCall, pattern};
use serde_json::Value;
use std::cell::RefCell;
use std::fmt::Debug;
use std::fs;
use std::path::{Path, PathBuf};

const LARGE_SUPER_SECTORS: u64 = CHUNK_SECTORS + 6;
const OTHER_PLAN_SHA256: &str = "00000000000000000000000000000000000000000000000000000000000000ff";
const WRITE_STEP_COUNT: usize = 14;
const STEPS_BEFORE_THE_DATA_WIPE: usize = 14;
const STOPPED: &str = "takes no more commands";

type PlanChange = fn(&mut Value);

fn rejection<Outcome: Debug>(result: Result<Outcome>, fragment: &str) -> Front2Error {
    let err = result.expect_err(fragment);
    assert!(
        err.to_string().contains(fragment),
        "unexpected message: {err}"
    );
    err
}

fn approvals(plan_sha256: &str) -> (WriteApproval, DataWipeApproval, RestartApproval) {
    (
        WriteApproval {
            plan_sha256: plan_sha256.to_string(),
            granted_unix: 1,
        },
        DataWipeApproval {
            plan_sha256: plan_sha256.to_string(),
            granted_unix: 2,
        },
        RestartApproval {
            plan_sha256: plan_sha256.to_string(),
            granted_unix: 3,
        },
    )
}

fn count(calls: &RefCell<Vec<MockCall>>, predicate: fn(&MockCall) -> bool) -> usize {
    calls.borrow().iter().filter(|call| predicate(call)).count()
}

fn writes_into(calls: &RefCell<Vec<MockCall>>, step: &WriteStep) -> usize {
    calls
        .borrow()
        .iter()
        .filter(|call| call.writes_into(step.lun, step.start_sector, step.sectors))
        .count()
}

fn source_bytes(plan: &StoredInstallPlan, step: &WriteStep) -> Vec<u8> {
    match &step.source {
        StepSource::CandidateFile { file } => {
            fs::read(plan.plan().candidate_directory.join(file)).expect("candidate file")
        }
        StepSource::PlanFile { file } => fs::read(plan.directory().join(file)).expect("plan file"),
        StepSource::ZeroFill => vec![0u8; step.bytes as usize],
    }
}

fn step_named(plan: &StoredInstallPlan, name: &str) -> WriteStep {
    plan.plan().write_step(name).expect("write step").clone()
}

fn change_plan_file(directory: &Path, change: impl FnOnce(&mut Value)) {
    let path = directory.join(PLAN_FILE);
    let mut value: Value = serde_json::from_slice(&fs::read(&path).expect("plan")).expect("JSON");
    change(&mut value);
    fs::write(&path, serde_json::to_vec_pretty(&value).expect("text")).expect("changed plan");
}

fn assert_refuses_everything(
    executor: &mut InstallExecutor<'_>,
    plan_sha256: &str,
    calls: &RefCell<Vec<MockCall>>,
) {
    let (write, data_wipe, restart) = approvals(plan_sha256);
    let before = calls.borrow().clone();
    rejection(executor.run_preflight(), STOPPED);
    rejection(executor.run_write_steps(&write), STOPPED);
    rejection(executor.run_data_wipe_step(&write, &data_wipe), STOPPED);
    rejection(executor.request_restart(&restart), STOPPED);
    assert_eq!(*calls.borrow(), before);
    assert_eq!(count(calls, MockCall::is_reset), 0);
}

#[test]
fn the_whole_flow_writes_every_step_and_restarts_only_with_the_restart_approval() {
    let mut fixture = InstallFixture::with_super_sectors(LARGE_SUPER_SECTORS);
    let plan = fixture.stored_plan();
    let (write, data_wipe, restart) = approvals(plan.plan_sha256());
    let misc = step_named(&plan, "misc");
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);

    executor.run_preflight().expect("preflight");
    assert_eq!(count(&fixture.calls, MockCall::changes_the_device), 0);
    let preflight = executor.progress().preflight.clone().expect("record");
    assert!(preflight.passed);
    assert_eq!(preflight.restore_files.len(), WRITE_STEP_COUNT + 1);
    assert!(preflight.restore_files.iter().all(DigestCheck::matches));

    executor.run_write_steps(&write).expect("write steps");
    assert_eq!(
        executor.progress().verified_steps(),
        STEPS_BEFORE_THE_DATA_WIPE
    );
    assert!(executor.progress().boot_lun_passed());
    assert_eq!(writes_into(&fixture.calls, &misc), 0);
    assert_eq!(count(&fixture.calls, MockCall::is_boot_lun_command), 1);
    assert_eq!(count(&fixture.calls, MockCall::is_reset), 0);

    executor
        .run_data_wipe_step(&write, &data_wipe)
        .expect("data wipe request");
    assert_eq!(writes_into(&fixture.calls, &misc), 1);
    assert_eq!(count(&fixture.calls, MockCall::is_reset), 0);

    executor.request_restart(&restart).expect("restart");
    let progress = executor.progress().clone();
    assert!(progress.restart_sent);
    assert_eq!(progress.halted, None);
    assert_eq!(progress.steps.len(), WRITE_STEP_COUNT + 1);
    assert!(
        progress
            .steps
            .iter()
            .all(|record| record.state == StepState::Verified)
    );
    assert_eq!(count(&fixture.calls, MockCall::is_reset), 1);
    assert_eq!(fixture.calls.borrow().last(), Some(&MockCall::Reset));

    for (i, step) in plan.plan().write_steps() {
        let content = fixture
            .storage
            .content(step.lun, step.start_sector, step.sectors);
        assert!(content == source_bytes(&plan, step), "{}", step.name);
        assert!(progress.write_verified(i, step), "{}", step.name);
    }
    let largest_call = fixture
        .calls
        .borrow()
        .iter()
        .map(MockCall::sectors)
        .max()
        .expect("calls");
    assert_eq!(largest_call, CHUNK_SECTORS);
    let super_step = step_named(&plan, "super");
    assert_eq!(super_step.sectors, LARGE_SUPER_SECTORS);
    assert_eq!(writes_into(&fixture.calls, &super_step), 2);
    serde_json::to_string(&progress).expect("the progress record serializes");
}

#[test]
fn the_misc_step_changes_only_the_first_sector() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let (write, data_wipe, _) = approvals(plan.plan_sha256());
    let location = fixture.location("misc");
    let original = pattern("misc-original", PARTITION_SECTORS as usize * SECTOR);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    executor.run_write_steps(&write).expect("write steps");
    executor
        .run_data_wipe_step(&write, &data_wipe)
        .expect("data wipe request");
    let content = fixture
        .storage
        .content(location.lun, location.start_sector, location.sectors);
    assert_eq!(&content[..13], b"boot-recovery");
    assert_eq!(&content[64..85], b"recovery\n--wipe_data\n");
    assert!(content[2048..] == original[2048..]);
    assert!(content[..2048] != original[..2048]);
}

#[test]
fn the_plan_holds_the_fixed_steps_and_facts_and_loads_again() {
    let fixture = InstallFixture::standard();
    let directory = fixture.plan_directory();
    let plan = InstallPlan::build(&fixture.inputs).expect("plan");
    let sha256 = plan.save(&directory).expect("save");
    let stored = InstallPlan::load(&directory).expect("load");
    assert_eq!(stored.plan_sha256(), sha256);
    assert_eq!(
        sha256,
        sha256_bytes(&fs::read(directory.join(PLAN_FILE)).expect("plan file"))
    );
    let value = serde_json::to_value(stored.plan()).expect("value");
    assert_eq!(value, serde_json::to_value(&plan).expect("built value"));
    let labels: Vec<String> = stored.plan().steps.iter().map(InstallStep::label).collect();
    assert_eq!(
        labels,
        [
            "write:boot_b",
            "write:dtbo_b",
            "write:init_boot_b",
            "write:recovery_b",
            "write:vendor_boot_b",
            "write:vbmeta_system_b",
            "write:vbmeta_b",
            "write:super",
            "write:metadata",
            "write:gpt_backup0",
            "write:gpt_backup4",
            "write:gpt_main0",
            "write:gpt_main4",
            "set_boot_lun:2",
            "write:misc",
            "restart",
        ]
    );
    for (fact, expected) in [
        ("wipes_user_data", true),
        ("first_conversion_only", true),
        ("preserves_slot_a_fallback", false),
        ("restore_tested_on_device", false),
        ("stored_rollback_source_confirmed", false),
        ("boot_lun_persistence_confirmed", false),
    ] {
        assert_eq!(value[fact], expected, "{fact}");
    }
    assert_eq!(value["original_boot_lun"], 1);
    assert_eq!(value["new_boot_lun"], 2);
    assert_eq!(value["steps"][8]["source"]["kind"], "zero_fill");
    assert_eq!(value["steps"][14]["requires_data_wipe_approval"], true);
    assert_eq!(stored.plan().restore_set.len(), WRITE_STEP_COUNT);
    assert_eq!(stored.plan().gpt_copies.len(), 12);
}

#[test]
fn the_plan_takes_positions_from_the_partition_map_and_records_the_restore_set() {
    let fixture = InstallFixture::standard();
    let stored = fixture.stored_plan();
    for name in ["boot_b", "vbmeta_system_b", "super", "metadata"] {
        let step = step_named(&stored, name);
        let location = fixture.location(name);
        assert_eq!(
            (step.lun, step.start_sector, step.sectors),
            (location.lun, location.start_sector, location.sectors),
            "{name}"
        );
    }
    let misc = step_named(&stored, "misc");
    let misc_location = fixture.location("misc");
    assert_eq!(
        (misc.lun, misc.start_sector, misc.sectors),
        (misc_location.lun, misc_location.start_sector, 1)
    );
    let restore = &stored.plan().restore_set;
    let misc_restore = restore.iter().find(|entry| entry.name == "misc");
    let misc_restore = misc_restore.expect("misc restore entry");
    assert_eq!(misc_restore.sectors, PARTITION_SECTORS);
    assert_eq!(misc_restore.file, "partitions/lun0/misc.bin");
    assert_eq!(misc_restore.sha256, fixture.inputs.misc_original.sha256);
    let boot_restore = restore.iter().find(|entry| entry.name == "boot_b");
    let boot_restore = boot_restore.expect("boot_b restore entry");
    assert_eq!(boot_restore.file, "partitions/lun4/boot_b.bin");
    assert_eq!(
        boot_restore.sha256,
        step_named(&stored, "boot_b").expected_before_sha256
    );
    let gpt_restore = restore.iter().find(|entry| entry.name == "gpt_main4");
    assert_eq!(
        gpt_restore.expect("gpt_main4 restore entry").file,
        "gpt/gpt_main4.bin"
    );
}

#[test]
fn a_saved_plan_is_never_written_over() {
    let fixture = InstallFixture::standard();
    let directory = fixture.plan_directory();
    let plan = InstallPlan::build(&fixture.inputs).expect("plan");
    plan.save(&directory).expect("first save");
    let err = plan.save(&directory).expect_err("second save");
    assert!(matches!(err, Front2Error::Io(_)), "{err}");
    let stored = InstallPlan::load(&directory).expect("load");
    let other = fixture.directory.path().join("other");
    rejection(stored.plan().save(&other), "cannot be saved again");
}

#[test]
fn an_image_larger_than_its_partition_is_rejected() {
    let mut fixture = InstallFixture::standard();
    let bytes = (PARTITION_SECTORS as usize + 1) * SECTOR;
    fixture.replace_candidate("dtbo_b", &pattern("larger", bytes));
    let err = rejection(
        InstallPlan::build(&fixture.inputs),
        "larger than the partition",
    );
    assert!(matches!(err, Front2Error::Plan(_)));
}

#[test]
fn an_image_smaller_than_its_partition_is_rejected() {
    let mut fixture = InstallFixture::standard();
    let bytes = (PARTITION_SECTORS as usize - 1) * SECTOR;
    fixture.replace_candidate("vbmeta_b", &pattern("smaller", bytes));
    let err = rejection(
        InstallPlan::build(&fixture.inputs),
        "smaller than the partition",
    );
    assert!(matches!(err, Front2Error::Plan(_)));
}

#[test]
fn steps_that_cover_the_same_sectors_are_rejected() {
    let fixture = InstallFixture::standard();
    fixture.stored_plan();
    let directory = fixture.plan_directory();
    change_plan_file(&directory, |plan| {
        plan["steps"][1]["start_sector"] = plan["steps"][0]["start_sector"].clone();
    });
    let err = rejection(
        InstallPlan::load(&directory),
        "the steps boot_b and dtbo_b cover the same sectors of LUN 4",
    );
    assert!(matches!(err, Front2Error::Plan(_)));
}

#[test]
fn letter_1144_the_plan_records_the_userdata_backup_that_the_data_wipe_needs() {
    let fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let restore = &plan.plan().data_restore;
    let location = fixture.location("userdata");
    assert_eq!(restore.name, "userdata");
    assert_eq!(restore.file, "partitions/lun0/userdata.bin");
    assert_eq!(
        (restore.lun, restore.start_sector, restore.sectors),
        (location.lun, location.start_sector, location.sectors)
    );
    assert_eq!(restore.sha256, fixture.inputs.userdata_original.sha256);
    assert_eq!(plan.plan().restore_set.len(), WRITE_STEP_COUNT);

    let changes: [PlanChange; 3] = [
        |plan| plan["data_restore"]["name"] = Value::from("misc"),
        |plan| plan["data_restore"]["start_sector"] = plan["steps"][7]["start_sector"].clone(),
        |plan| plan["data_restore"]["sectors"] = Value::from(1u64),
    ];
    for change in changes {
        let fixture = InstallFixture::standard();
        fixture.stored_plan();
        change_plan_file(&fixture.plan_directory(), change);
        rejection(
            InstallPlan::load(&fixture.plan_directory()),
            "the data restore entry",
        );
    }

    let mut smaller = InstallFixture::standard();
    smaller.inputs.userdata_original.bytes -= 1;
    rejection(
        InstallPlan::build(&smaller.inputs),
        "smaller than the partition",
    );

    let missing = InstallFixture::standard();
    let file = missing
        .inputs
        .backup_directory
        .join(&missing.inputs.userdata_original.file);
    fs::remove_file(file).expect("remove the userdata backup");
    assert!(InstallPlan::build(&missing.inputs).is_err());
}

#[test]
fn a_changed_plan_file_is_rejected_when_it_is_loaded() {
    let changes: [(PlanChange, &str); 5] = [
        (
            |plan| plan["restore_tested_on_device"] = Value::Bool(true),
            "restore_tested_on_device",
        ),
        (
            |plan| plan["wipes_user_data"] = Value::Bool(false),
            "wipes_user_data",
        ),
        (
            |plan| plan["steps"].as_array_mut().expect("steps").swap(13, 14),
            "the fixed order",
        ),
        (
            |plan| plan["steps"][14]["requires_data_wipe_approval"] = Value::Bool(false),
            "data wipe approval flag",
        ),
        (
            |plan| plan["steps"][11]["start_sector"] = Value::from(1u64),
            "gpt_main0",
        ),
    ];
    for (change, fragment) in changes {
        let fixture = InstallFixture::standard();
        fixture.stored_plan();
        change_plan_file(&fixture.plan_directory(), change);
        rejection(InstallPlan::load(&fixture.plan_directory()), fragment);
    }
}

#[test]
fn a_plan_file_with_an_unknown_field_or_a_changed_payload_is_rejected() {
    let fixture = InstallFixture::standard();
    fixture.stored_plan();
    let directory = fixture.plan_directory();
    let payload = directory.join("gpt_main0-slot-b.bin");
    let mut bytes = fs::read(&payload).expect("payload");
    bytes[0] ^= 0x01;
    fs::write(&payload, &bytes).expect("changed payload");
    let err = rejection(InstallPlan::load(&directory), "gpt_main0-slot-b.bin");
    assert!(matches!(err, Front2Error::Verification(_)));

    let changes: [PlanChange; 5] = [
        |plan| plan["approved"] = Value::Bool(true),
        |plan| plan["steps"][0]["skip_readback"] = Value::Bool(true),
        |plan| plan["steps"][0]["source"]["offset"] = Value::from(1u64),
        |plan| plan["restore_set"][0]["tested"] = Value::Bool(true),
        |plan| plan["gpt_copies"][0]["checked"] = Value::Bool(true),
    ];
    for change in changes {
        let fixture = InstallFixture::standard();
        fixture.stored_plan();
        change_plan_file(&fixture.plan_directory(), change);
        let err = InstallPlan::load(&fixture.plan_directory()).expect_err("unknown field");
        assert!(matches!(err, Front2Error::Json(_)), "{err}");
    }
}

#[test]
fn identical_gpt_copies_on_lun_1_and_lun_2_are_rejected_when_the_plan_is_built() {
    let fixture = InstallFixture::with_identical_boot_lun_tables();
    let err = rejection(
        InstallPlan::build(&fixture.inputs),
        "the main GPT copies of LUN 1 and LUN 2 are identical",
    );
    assert!(matches!(err, Front2Error::Plan(_)));
}

#[test]
fn inputs_that_do_not_come_from_the_backup_are_rejected() {
    let mut fixture = InstallFixture::standard();
    let backup = fixture.inputs.backup_directory.clone();
    let main0 = fs::read(backup.join("gpt/gpt_main0.bin")).expect("main 0");
    let main4 = fs::read(backup.join("gpt/gpt_main4.bin")).expect("main 4");
    fixture.inputs.partition_map =
        PartitionMap::from_main_copies(&[(0, &main0), (4, &main4)]).expect("partial map");
    rejection(
        InstallPlan::build(&fixture.inputs),
        "the partition map differs",
    );

    let mut fixture = InstallFixture::standard();
    fixture.inputs.misc_first_sector[100] ^= 0x01;
    rejection(
        InstallPlan::build(&fixture.inputs),
        "not the data wipe request",
    );

    let mut fixture = InstallFixture::standard();
    let inputs = &mut fixture.inputs;
    std::mem::swap(&mut inputs.activation_lun0, &mut inputs.activation_lun4);
    rejection(
        InstallPlan::build(&fixture.inputs),
        "no slot activation that was prepared for LUN 0",
    );

    let mut fixture = InstallFixture::standard();
    fixture.inputs.images.remove(2);
    rejection(
        InstallPlan::build(&fixture.inputs),
        "exactly one image for init_boot_b",
    );

    let mut fixture = InstallFixture::standard();
    fixture.inputs.gpt_backups.pop();
    rejection(InstallPlan::build(&fixture.inputs), "GPT backup copies");
}

#[test]
fn writes_are_refused_without_a_passed_preflight() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let (write, data_wipe, restart) = approvals(plan.plan_sha256());
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    let fragment = "no passed preflight";
    rejection(executor.run_write_steps(&write), fragment);
    rejection(executor.run_data_wipe_step(&write, &data_wipe), fragment);
    rejection(executor.request_restart(&restart), fragment);
    assert!(fixture.calls.borrow().is_empty());
}

#[test]
fn preflight_stops_on_a_gpt_copy_that_differs_from_the_backup() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let mut header = fixture.storage.content(3, 1, 1);
    header[200] ^= 0x01;
    fixture.storage.load(3, 1, &header);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    let err = rejection(executor.run_preflight(), "preflight: the current gpt_main3");
    assert!(matches!(err, Front2Error::Verification(_)));
    let record = executor.progress().preflight.clone().expect("record");
    assert!(!record.passed);
    assert!(record.failure.is_some());
    assert_refuses_everything(&mut executor, plan.plan_sha256(), &fixture.calls);
    assert_eq!(count(&fixture.calls, MockCall::changes_the_device), 0);
}

#[test]
fn preflight_stops_when_a_target_does_not_hold_the_backup_content() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let location = fixture.location("vbmeta_b");
    let mut sector = fixture
        .storage
        .content(location.lun, location.start_sector + 3, 1);
    sector[0] ^= 0x80;
    fixture
        .storage
        .load(location.lun, location.start_sector + 3, &sector);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    rejection(
        executor.run_preflight(),
        "preflight: the current content of vbmeta_b",
    );
    assert_refuses_everything(&mut executor, plan.plan_sha256(), &fixture.calls);
    assert_eq!(count(&fixture.calls, MockCall::changes_the_device), 0);
}

#[test]
fn preflight_stops_when_the_boot_wlun_does_not_give_the_gpt_of_lun_1() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    fixture.storage.start_with_boot_lun(2);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    rejection(
        executor.run_preflight(),
        "preflight: the GPT that is read through boot WLUN 176",
    );
    assert_refuses_everything(&mut executor, plan.plan_sha256(), &fixture.calls);
    assert_eq!(count(&fixture.calls, MockCall::changes_the_device), 0);
}

#[test]
fn preflight_stops_on_a_candidate_file_that_changed_after_the_plan() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let bytes = PARTITION_SECTORS as usize * SECTOR;
    fixture.overwrite_candidate_file("boot_b", &pattern("changed", bytes));
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    rejection(executor.run_preflight(), "preflight: the source of boot_b");
    assert!(fixture.calls.borrow().is_empty());
    assert_refuses_everything(&mut executor, plan.plan_sha256(), &fixture.calls);
}

#[test]
fn letter_1157_a_candidate_changed_after_the_preflight_is_refused_before_its_write() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let (write, _, _) = approvals(plan.plan_sha256());
    let unchanged = step_named(&plan, "boot_b");
    let changed = step_named(&plan, "dtbo_b");
    let StepSource::CandidateFile { file } = &changed.source else {
        panic!("dtbo_b comes from a candidate file");
    };
    let path = plan.plan().candidate_directory.join(file);
    let mut data = fs::read(&path).expect("candidate file");
    data[0] ^= 0x01;
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    fs::write(&path, &data).expect("changed candidate file");

    let err = rejection(
        executor.run_write_steps(&write),
        "dtbo_b: the source has SHA-256",
    );
    assert!(matches!(err, Front2Error::Verification(_)));
    let progress = executor.progress().clone();
    assert_eq!(progress.verified_steps(), 1);
    assert_eq!(progress.steps[1].name, "dtbo_b");
    assert_eq!(progress.steps[1].state, StepState::Failed);
    assert_refuses_everything(&mut executor, plan.plan_sha256(), &fixture.calls);
    assert_eq!(writes_into(&fixture.calls, &unchanged), 1);
    assert_eq!(writes_into(&fixture.calls, &changed), 0);
    assert_eq!(count(&fixture.calls, MockCall::is_write), 1);
}

#[test]
fn letter_1157_preflight_stops_on_a_backup_file_with_a_changed_byte() {
    let mut fixture = InstallFixture::standard();
    let original = &fixture.inputs.images[0].original;
    assert_eq!(fixture.inputs.images[0].partition, "boot_b");
    let path = fixture.inputs.backup_directory.join(&original.file);
    let mut data = fs::read(&path).expect("backup file");
    data[0] ^= 0x01;
    fs::write(&path, &data).expect("changed backup file");
    let plan = fixture.stored_plan();
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    let err = rejection(
        executor.run_preflight(),
        "preflight: the backup file of boot_b has SHA-256",
    );
    assert!(matches!(err, Front2Error::Verification(_)));
    assert!(fixture.calls.borrow().is_empty());
    assert_refuses_everything(&mut executor, plan.plan_sha256(), &fixture.calls);
}

#[test]
fn letter_1157_preflight_stops_on_a_userdata_backup_file_with_another_length() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let path = fixture
        .inputs
        .backup_directory
        .join(&fixture.inputs.userdata_original.file);
    let mut data = fs::read(&path).expect("backup file");
    data.pop();
    fs::write(&path, &data).expect("shorter backup file");
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    rejection(
        executor.run_preflight(),
        "preflight: the backup file of userdata is",
    );
    assert!(fixture.calls.borrow().is_empty());
    assert_refuses_everything(&mut executor, plan.plan_sha256(), &fixture.calls);
}

#[test]
fn a_write_that_is_acknowledged_but_ignored_stops_the_install() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let (write, _, _) = approvals(plan.plan_sha256());
    let failing = step_named(&plan, "recovery_b");
    let next = step_named(&plan, "vendor_boot_b");
    fixture
        .storage
        .ignore_writes_that_cover(failing.lun, failing.start_sector);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    let err = rejection(executor.run_write_steps(&write), "recovery_b: the readback");
    assert!(matches!(err, Front2Error::Verification(_)));
    let progress = executor.progress().clone();
    assert_eq!(progress.verified_steps(), 3);
    assert_eq!(progress.steps.len(), 4);
    assert_eq!(progress.steps[3].name, "recovery_b");
    assert_eq!(progress.steps[3].state, StepState::Failed);
    assert!(progress.halted.is_some());
    assert_refuses_everything(&mut executor, plan.plan_sha256(), &fixture.calls);
    assert_eq!(writes_into(&fixture.calls, &failing), 1);
    assert_eq!(writes_into(&fixture.calls, &next), 0);
    assert_eq!(count(&fixture.calls, MockCall::is_write), 4);
    assert_eq!(count(&fixture.calls, MockCall::is_boot_lun_command), 0);
}

#[test]
fn a_write_with_one_wrong_byte_stops_the_install() {
    let mut fixture = InstallFixture::with_super_sectors(LARGE_SUPER_SECTORS);
    let plan = fixture.stored_plan();
    let (write, _, _) = approvals(plan.plan_sha256());
    let failing = step_named(&plan, "super");
    let next = step_named(&plan, "metadata");
    fixture
        .storage
        .corrupt_writes_that_cover(failing.lun, failing.start_sector + CHUNK_SECTORS + 2);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    rejection(executor.run_write_steps(&write), "super: the readback");
    assert_eq!(executor.progress().verified_steps(), 7);
    assert_refuses_everything(&mut executor, plan.plan_sha256(), &fixture.calls);
    assert_eq!(writes_into(&fixture.calls, &failing), 2);
    assert_eq!(writes_into(&fixture.calls, &next), 0);
    let written = fixture
        .storage
        .content(failing.lun, failing.start_sector, failing.sectors);
    let expected = source_bytes(&plan, &failing);
    let different = (0..written.len())
        .filter(|offset| written[*offset] != expected[*offset])
        .count();
    assert_eq!(different, 1);
}

#[test]
fn a_short_readback_stops_the_install() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let (write, _, _) = approvals(plan.plan_sha256());
    let failing = step_named(&plan, "dtbo_b");
    let next = step_named(&plan, "init_boot_b");
    fixture
        .storage
        .shorten_reads_after_a_write_to(failing.lun, failing.start_sector);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    let err = rejection(
        executor.run_write_steps(&write),
        "the read of 16 sectors at sector",
    );
    assert!(matches!(err, Front2Error::Verification(_)));
    assert_eq!(executor.progress().verified_steps(), 1);
    assert_refuses_everything(&mut executor, plan.plan_sha256(), &fixture.calls);
    assert_eq!(writes_into(&fixture.calls, &next), 0);
}

#[test]
fn a_read_error_during_the_readback_stops_the_install() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let (write, _, _) = approvals(plan.plan_sha256());
    let failing = step_named(&plan, "vbmeta_system_b");
    let next = step_named(&plan, "vbmeta_b");
    fixture
        .storage
        .fail_reads_after_a_write_to(failing.lun, failing.start_sector);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    let err = rejection(executor.run_write_steps(&write), "mock: read error");
    assert!(matches!(err, Front2Error::Shell(_)));
    let progress = executor.progress().clone();
    assert_eq!(progress.verified_steps(), 5);
    assert_eq!(progress.steps[5].state, StepState::Failed);
    assert_refuses_everything(&mut executor, plan.plan_sha256(), &fixture.calls);
    assert_eq!(writes_into(&fixture.calls, &failing), 1);
    assert_eq!(writes_into(&fixture.calls, &next), 0);
}

#[test]
fn a_boot_wlun_that_still_gives_lun_1_after_the_command_stops_the_install() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let (write, _, _) = approvals(plan.plan_sha256());
    let misc = step_named(&plan, "misc");
    fixture.storage.ignore_the_boot_lun_command();
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    let err = rejection(
        executor.run_write_steps(&write),
        "after the boot LUN command",
    );
    assert!(matches!(err, Front2Error::Verification(_)));
    let progress = executor.progress().clone();
    let record = progress.boot_lun.clone().expect("boot LUN record");
    assert!(record.command_accepted);
    assert!(record.matches_original_lun_gpt);
    assert!(!record.matches_new_lun_gpt);
    assert!(!record.passed);
    assert_eq!(progress.verified_steps(), 13);
    assert_refuses_everything(&mut executor, plan.plan_sha256(), &fixture.calls);
    assert_eq!(count(&fixture.calls, MockCall::is_boot_lun_command), 1);
    assert_eq!(writes_into(&fixture.calls, &misc), 0);
}

#[test]
fn the_misc_step_stays_closed_without_the_data_wipe_approval() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let (write, data_wipe, restart) = approvals(plan.plan_sha256());
    let (_, other_data_wipe, _) = approvals(OTHER_PLAN_SHA256);
    let misc = step_named(&plan, "misc");
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    executor.run_write_steps(&write).expect("write steps");
    rejection(
        executor.run_write_steps(&write),
        "no step that needs only the write approval is left",
    );
    rejection(
        executor.run_data_wipe_step(&write, &other_data_wipe),
        "the user data wipe approval is for plan",
    );
    rejection(
        executor.request_restart(&restart),
        "the restart needs a matching readback of every write step",
    );
    assert_eq!(writes_into(&fixture.calls, &misc), 0);
    assert_eq!(count(&fixture.calls, MockCall::is_reset), 0);
    assert_eq!(executor.progress().halted, None);
    executor
        .run_data_wipe_step(&write, &data_wipe)
        .expect("data wipe request");
    assert_eq!(writes_into(&fixture.calls, &misc), 1);
}

#[test]
fn the_restart_stays_closed_without_a_restart_approval_for_this_plan() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let (write, data_wipe, restart) = approvals(plan.plan_sha256());
    let (_, _, other_restart) = approvals(OTHER_PLAN_SHA256);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    executor.run_write_steps(&write).expect("write steps");
    executor
        .run_data_wipe_step(&write, &data_wipe)
        .expect("data wipe request");
    assert_eq!(count(&fixture.calls, MockCall::is_reset), 0);
    rejection(
        executor.request_restart(&other_restart),
        "the restart approval is for plan",
    );
    assert_eq!(count(&fixture.calls, MockCall::is_reset), 0);
    executor.request_restart(&restart).expect("restart");
    assert_eq!(count(&fixture.calls, MockCall::is_reset), 1);
    rejection(executor.request_restart(&restart), STOPPED);
    rejection(executor.run_write_steps(&write), STOPPED);
    assert_eq!(count(&fixture.calls, MockCall::is_reset), 1);
}

#[test]
fn approvals_for_another_plan_are_refused() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let (write, data_wipe, _) = approvals(plan.plan_sha256());
    let (other_write, _, _) = approvals(OTHER_PLAN_SHA256);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    let fragment = "the write approval is for plan";
    rejection(executor.run_write_steps(&other_write), fragment);
    assert_eq!(count(&fixture.calls, MockCall::changes_the_device), 0);
    executor.run_write_steps(&write).expect("write steps");
    let before = count(&fixture.calls, MockCall::is_write);
    rejection(
        executor.run_data_wipe_step(&other_write, &data_wipe),
        fragment,
    );
    assert_eq!(count(&fixture.calls, MockCall::is_write), before);
    assert_eq!(executor.progress().halted, None);
}

#[test]
fn a_restart_after_a_failed_step_is_refused() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let (write, data_wipe, restart) = approvals(plan.plan_sha256());
    let misc = step_named(&plan, "misc");
    fixture
        .storage
        .ignore_writes_that_cover(misc.lun, misc.start_sector);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    executor.run_write_steps(&write).expect("write steps");
    rejection(
        executor.run_data_wipe_step(&write, &data_wipe),
        "misc: the readback",
    );
    rejection(executor.request_restart(&restart), STOPPED);
    assert_refuses_everything(&mut executor, plan.plan_sha256(), &fixture.calls);
    assert_eq!(count(&fixture.calls, MockCall::is_reset), 0);
}

#[test]
fn the_executor_sends_no_restart_on_its_own_or_when_it_is_dropped() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let (write, data_wipe, _) = approvals(plan.plan_sha256());
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    executor.run_write_steps(&write).expect("write steps");
    executor
        .run_data_wipe_step(&write, &data_wipe)
        .expect("data wipe request");
    let before = fixture.calls.borrow().clone();
    drop(executor);
    assert_eq!(*fixture.calls.borrow(), before);
    assert_eq!(count(&fixture.calls, MockCall::is_reset), 0);

    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let (write, _, _) = approvals(plan.plan_sha256());
    let failing = step_named(&plan, "boot_b");
    fixture
        .storage
        .ignore_writes_that_cover(failing.lun, failing.start_sector);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    rejection(executor.run_write_steps(&write), "boot_b: the readback");
    let before = fixture.calls.borrow().clone();
    drop(executor);
    assert_eq!(*fixture.calls.borrow(), before);
    assert_eq!(count(&fixture.calls, MockCall::is_reset), 0);
    assert_eq!(count(&fixture.calls, MockCall::is_write), 1);
}

#[test]
fn a_failed_restart_command_is_recorded_and_not_sent_again() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let (write, data_wipe, restart) = approvals(plan.plan_sha256());
    fixture.storage.fail_the_reset_command();
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    executor.run_write_steps(&write).expect("write steps");
    executor
        .run_data_wipe_step(&write, &data_wipe)
        .expect("data wipe request");
    rejection(executor.request_restart(&restart), "mock: reset error");
    assert!(!executor.progress().restart_sent);
    assert!(executor.progress().halted.is_some());
    rejection(executor.request_restart(&restart), STOPPED);
    assert_eq!(count(&fixture.calls, MockCall::is_reset), 1);
}

fn real_backup_directory() -> Option<PathBuf> {
    let directory = std::env::var_os("FRONT2_REAL_BACKUP_DIR").map(PathBuf::from);
    if directory.is_none() {
        eprintln!("skipped: FRONT2_REAL_BACKUP_DIR is not set");
    }
    directory
}

#[test]
fn real_backup_gpt_copies_fit_the_assumptions_of_the_install_plan_when_available() {
    let Some(backup) = real_backup_directory() else {
        return;
    };
    let read = |kind: GptCopyKind, lun: u8| {
        let file = format!("gpt_{}{lun}.bin", kind.label());
        fs::read(backup.join("gpt").join(file)).expect("real GPT copy")
    };
    for lun in 0u8..6 {
        let main = GptCopy::parse(GptCopyKind::Main, &read(GptCopyKind::Main, lun)).expect("main");
        let backup_copy =
            GptCopy::parse(GptCopyKind::Backup, &read(GptCopyKind::Backup, lun)).expect("backup");
        main.require_pair_with(&backup_copy, &format!("LUN {lun}"))
            .expect("the copies pair");
        let lun_sectors = backup_copy.start_sector() + backup_copy.sectors();
        assert_eq!(lun_sectors, main.alternate_lba() + 1);
        println!(
            "LUN {lun}: main and backup GPT copies pair, the backup copy ends at sector {lun_sectors}"
        );
    }
    assert!(read(GptCopyKind::Main, 1) != read(GptCopyKind::Main, 2));
    println!("the main GPT copies of LUN 1 and LUN 2 differ");
}
