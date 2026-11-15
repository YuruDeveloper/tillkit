mod gpt_support;
mod install_support;

use front2::Result;
use front2::gpt::BootSlot;
use front2::install::{
    DataWipeApproval, InstallExecutor, InstallPlan, InstallStep, PLAN_FILE, RestartApproval,
    StepSource, StoredInstallPlan, WriteApproval, WriteStep,
};
use install_support::{InstallFixture, MockCall};
use serde_json::Value;
use std::cell::RefCell;
use std::fmt::Debug;
use std::fs;

const STOCK_B_LABELS: [&str; 12] = [
    "write:boot_b",
    "write:dtbo_b",
    "write:init_boot_b",
    "write:recovery_b",
    "write:vendor_boot_b",
    "write:vbmeta_system_b",
    "write:vbmeta_b",
    "write:super",
    "write:metadata",
    "keep_boot_lun:2",
    "write:misc",
    "restart",
];
const STOCK_B_WRITE_STEPS: usize = 10;
const STOCK_B_WRITTEN_REGIONS: [&str; 5] =
    ["vbmeta_system_b", "vbmeta_b", "super", "metadata", "misc"];
const SKIPPED_REGIONS: [&str; 5] = [
    "boot_b",
    "dtbo_b",
    "init_boot_b",
    "recovery_b",
    "vendor_boot_b",
];

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

fn step_named(plan: &StoredInstallPlan, name: &str) -> WriteStep {
    plan.plan().write_step(name).expect("write step").clone()
}

fn candidate_bytes(plan: &StoredInstallPlan, step: &WriteStep) -> Vec<u8> {
    match &step.source {
        StepSource::CandidateFile { file } => {
            fs::read(plan.plan().candidate_directory.join(file)).expect("candidate file")
        }
        _ => panic!("the step {} does not come from a candidate file", step.name),
    }
}

fn rejection<Outcome: Debug>(result: Result<Outcome>, fragment: &str) {
    let err = result.expect_err(fragment);
    assert!(
        err.to_string().contains(fragment),
        "unexpected message: {err}"
    );
}

#[test]
fn the_stock_b_plan_keeps_the_boot_lun_and_omits_the_gpt_steps() {
    let fixture = InstallFixture::stock_b();
    let stored = fixture.stored_plan();
    let labels: Vec<String> = stored.plan().steps.iter().map(InstallStep::label).collect();
    assert_eq!(labels, STOCK_B_LABELS);
    assert!(
        !stored
            .plan()
            .steps
            .iter()
            .any(|step| matches!(step, InstallStep::SetBootLun { .. }))
    );
    assert_eq!(stored.plan().restore_set.len(), STOCK_B_WRITE_STEPS);
    assert_eq!(stored.plan().write_steps().len(), STOCK_B_WRITE_STEPS);
    assert_eq!(stored.plan().gpt_copies.len(), 12);
    assert_eq!(stored.plan().new_boot_lun, 2);
    assert_eq!(stored.plan().original_boot_lun, 1);
}

#[test]
fn the_stock_a_plan_omits_the_stock_slot_field_and_the_stock_b_plan_records_it() {
    let slot_a = InstallFixture::standard();
    let plan_a = InstallPlan::build(&slot_a.inputs).expect("slot A plan");
    let value_a = serde_json::to_value(&plan_a).expect("slot A value");
    assert_eq!(value_a.get("stock_slot"), None);

    let slot_b = InstallFixture::stock_b();
    let plan_b = InstallPlan::build(&slot_b.inputs).expect("slot B plan");
    let value_b = serde_json::to_value(&plan_b).expect("slot B value");
    assert_eq!(value_b["stock_slot"], Value::from("b"));
}

#[test]
fn the_target_b_plan_omits_the_target_slot_field_and_a_target_a_plan_records_it() {
    let fixture = InstallFixture::standard();
    let mut plan = InstallPlan::build(&fixture.inputs).expect("plan");

    let value_b = serde_json::to_value(&plan).expect("target B value");
    assert_eq!(value_b.get("target_slot"), None);
    let loaded_b: InstallPlan = serde_json::from_value(value_b).expect("target B loads");
    assert_eq!(loaded_b.target_slot, BootSlot::B);

    plan.target_slot = BootSlot::A;
    let value_a = serde_json::to_value(&plan).expect("target A value");
    assert_eq!(value_a["target_slot"], Value::from("a"));
    let loaded_a: InstallPlan = serde_json::from_value(value_a).expect("target A loads");
    assert_eq!(loaded_a.target_slot, BootSlot::A);
}

#[test]
fn an_install_plan_for_target_slot_a_switches_the_boot_lun_and_validates() {
    let fixture = InstallFixture::target_slot_a();
    let plan = InstallPlan::build(&fixture.inputs).expect("kk plan");
    plan.validate().expect("the kk plan validates");
    assert_eq!(plan.stock_slot, BootSlot::B);
    assert_eq!(plan.target_slot, BootSlot::A);
    assert_eq!(plan.original_boot_lun, 2);
    assert_eq!(plan.new_boot_lun, 1);
    let labels: Vec<String> = plan.steps.iter().map(InstallStep::label).collect();
    assert!(labels.contains(&"write:boot_a".to_string()));
    assert!(labels.contains(&"write:vbmeta_a".to_string()));
    assert!(labels.contains(&"set_boot_lun:1".to_string()));
    assert!(labels.iter().any(|label| label.starts_with("write:gpt_")));
    assert_eq!(labels.last().map(String::as_str), Some("restart"));
}

#[test]
fn an_install_plan_file_for_target_slot_a_round_trips_through_a_save_and_load() {
    let fixture = InstallFixture::target_slot_a();
    let stored = fixture.stored_plan();
    assert_eq!(stored.plan().stock_slot, BootSlot::B);
    assert_eq!(stored.plan().target_slot, BootSlot::A);

    let directory = fixture.plan_directory();
    let path = directory.join(PLAN_FILE);
    let value: Value =
        serde_json::from_slice(&fs::read(&path).expect("read the plan file")).expect("parse");
    assert_eq!(value["stock_slot"], Value::from("b"));
    assert_eq!(value["target_slot"], Value::from("a"));

    let reloaded = InstallPlan::load(&directory).expect("the kk plan file loads");
    assert_eq!(reloaded.plan().stock_slot, BootSlot::B);
    assert_eq!(reloaded.plan().target_slot, BootSlot::A);
}

#[test]
fn a_stock_b_first_install_writes_five_regions_and_sends_no_boot_lun_command() {
    let mut fixture = InstallFixture::stock_b();
    let plan = fixture.stored_plan();
    let (write, data_wipe, restart) = approvals(plan.plan_sha256());
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);

    executor.run_preflight().expect("preflight");
    let preflight = executor.progress().preflight.clone().expect("record");
    let resume = preflight.resume.clone().expect("resume point");
    assert!(resume.first_install);
    assert_eq!(resume.next_name, "vbmeta_system_b");
    assert_eq!(executor.progress().verified_steps(), SKIPPED_REGIONS.len());
    assert_eq!(count(&fixture.calls, MockCall::changes_the_device), 0);

    executor
        .run_write_steps(&write, &data_wipe)
        .expect("write steps");
    assert!(executor.progress().boot_lun_passed());
    assert_eq!(count(&fixture.calls, MockCall::is_boot_lun_command), 0);
    for name in SKIPPED_REGIONS {
        let step = step_named(&plan, name);
        assert_eq!(writes_into(&fixture.calls, &step), 0, "{name}");
    }
    for name in ["vbmeta_system_b", "vbmeta_b", "super", "metadata"] {
        let step = step_named(&plan, name);
        assert_eq!(writes_into(&fixture.calls, &step), 1, "{name}");
    }

    executor
        .run_data_wipe_step(&write, &data_wipe)
        .expect("data wipe request");
    let misc = step_named(&plan, "misc");
    assert_eq!(writes_into(&fixture.calls, &misc), 1);

    executor.request_restart(&restart).expect("restart");
    let progress = executor.progress().clone();
    assert!(progress.restart_sent);
    assert_eq!(progress.halted, None);
    assert_eq!(count(&fixture.calls, MockCall::is_reset), 1);
    let boot_lun = progress.boot_lun.clone().expect("boot LUN record");
    assert!(boot_lun.passed);
    assert!(!boot_lun.command_accepted);
    assert!(boot_lun.matches_new_lun_gpt);
    assert!(count(&fixture.calls, MockCall::is_write) >= STOCK_B_WRITTEN_REGIONS.len());
}

#[test]
fn a_stock_b_preflight_stops_when_the_boot_wlun_gives_the_original_lun() {
    let mut fixture = InstallFixture::stock_b();
    let plan = fixture.stored_plan();
    fixture.storage.start_with_boot_lun(1);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    rejection(
        executor.run_preflight(),
        "the GPT that is read through boot WLUN",
    );
    assert_eq!(count(&fixture.calls, MockCall::changes_the_device), 0);
    assert!(
        !executor
            .progress()
            .preflight
            .clone()
            .expect("record")
            .passed
    );
}

#[test]
fn a_stock_b_resume_after_the_first_region_keeps_the_boot_lun_neutral() {
    let mut fixture = InstallFixture::stock_b();
    let plan = fixture.stored_plan();
    let first = step_named(&plan, "vbmeta_system_b");
    let data = candidate_bytes(&plan, &first);
    fixture.storage.load(first.lun, first.start_sector, &data);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    let resume = executor
        .progress()
        .preflight
        .clone()
        .expect("record")
        .resume
        .clone()
        .expect("resume point");
    assert!(!resume.first_install);
    assert_eq!(resume.next_name, "vbmeta_b");
    assert_eq!(count(&fixture.calls, MockCall::changes_the_device), 0);
}

#[test]
fn a_stock_b_preflight_checks_every_gpt_copy_against_the_backup() {
    let mut fixture = InstallFixture::stock_b();
    let plan = fixture.stored_plan();
    let mut header = fixture.storage.content(4, 1, 1);
    header[200] ^= 0x01;
    fixture.storage.load(4, 1, &header);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    rejection(executor.run_preflight(), "preflight: the current gpt_main4");
    assert_eq!(count(&fixture.calls, MockCall::changes_the_device), 0);
}
