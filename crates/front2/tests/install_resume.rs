mod gpt_support;
mod install_support;

use front2::install::{
    CHUNK_SECTORS, DataWipeApproval, InstallExecutor, InstallStep, PreflightRecord,
    RestartApproval, StepJudgment, StepSource, StepState, StoredInstallPlan, WriteApproval,
    WriteStep,
};
use front2::{Front2Error, Result};
use gpt_support::{LAST_LBA, PARTITION_SECTORS, SECTOR};
use install_support::{InstallFixture, MockCall, MockStorage, pattern};
use std::cell::RefCell;
use std::fmt::Debug;
use std::fs;

const LARGE_SUPER_SECTORS: u64 = CHUNK_SECTORS + 6;
const STOPPED: &str = "takes no more commands";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Interruption {
    Before,
    Within,
    After,
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

fn rejection<Outcome: Debug>(result: Result<Outcome>, fragment: &str) -> Front2Error {
    let err = result.expect_err(fragment);
    assert!(
        err.to_string().contains(fragment),
        "unexpected message: {err}"
    );
    err
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

fn step_index(plan: &StoredInstallPlan, name: &str) -> usize {
    plan.plan()
        .steps
        .iter()
        .position(|step| step.as_write().is_some_and(|write| write.name == name))
        .expect("write step")
}

fn boot_lun_index(plan: &StoredInstallPlan) -> usize {
    plan.plan()
        .steps
        .iter()
        .position(|step| step.boot_lun().is_some())
        .expect("boot LUN step")
}

fn restart_index(plan: &StoredInstallPlan) -> usize {
    plan.plan().steps.len() - 1
}

fn apply_step(storage: &mut MockStorage, plan: &StoredInstallPlan, step: &InstallStep) {
    if let Some(lun) = step.boot_lun() {
        storage.start_with_boot_lun(lun);
    }
    if let Some(write) = step.as_write() {
        storage.load(write.lun, write.start_sector, &source_bytes(plan, write));
    }
}

fn apply_steps_before(storage: &mut MockStorage, plan: &StoredInstallPlan, index: usize) {
    for step in &plan.plan().steps[..index] {
        apply_step(storage, plan, step);
    }
}

fn written_part(step: &WriteStep) -> u64 {
    if step.sectors > CHUNK_SECTORS {
        CHUNK_SECTORS
    } else {
        step.sectors / 2
    }
}

fn apply_part_of(storage: &mut MockStorage, plan: &StoredInstallPlan, step: &WriteStep) {
    let sectors = written_part(step);
    let bytes = sectors as usize * SECTOR;
    storage.load(
        step.lun,
        step.start_sector,
        &source_bytes(plan, step)[..bytes],
    );
}

fn change_one_byte(storage: &mut MockStorage, lun: u8, sector: u64) {
    let mut data = storage.content(lun, sector, 1);
    data[200] ^= 0x01;
    storage.load(lun, sector, &data);
}

fn next_step<'plan>(
    executor: &InstallExecutor<'_>,
    plan: &'plan StoredInstallPlan,
) -> Option<&'plan InstallStep> {
    plan.plan().steps.get(executor.progress().verified_steps())
}

fn needs_only_the_write_approval(step: &InstallStep) -> bool {
    match step.as_write() {
        Some(write) => !write.requires_data_wipe_approval,
        None => step.boot_lun().is_some(),
    }
}

fn is_the_data_wipe(step: &InstallStep) -> bool {
    step.as_write()
        .is_some_and(|write| write.requires_data_wipe_approval)
}

fn finish(executor: &mut InstallExecutor<'_>, plan: &StoredInstallPlan) {
    let (write, data_wipe, restart) = approvals(plan.plan_sha256());
    if next_step(executor, plan).is_some_and(needs_only_the_write_approval) {
        executor
            .run_write_steps(&write, &data_wipe)
            .expect("write steps");
    }
    if next_step(executor, plan).is_some_and(is_the_data_wipe) {
        executor
            .run_data_wipe_step(&write, &data_wipe)
            .expect("data wipe request");
    }
    executor.request_restart(&restart).expect("restart");
}

fn preflight_of(executor: &InstallExecutor<'_>) -> PreflightRecord {
    executor.progress().preflight.clone().expect("record")
}

fn judgment_of(record: &PreflightRecord, index: usize) -> StepJudgment {
    record
        .steps
        .iter()
        .find(|check| check.index == index)
        .expect("step check")
        .judgment
}

fn assert_installed(calls: &RefCell<Vec<MockCall>>, executor: &InstallExecutor<'_>) {
    let progress = executor.progress();
    assert!(progress.restart_sent);
    assert_eq!(progress.halted, None);
    assert!(progress.boot_lun_passed());
    assert_eq!(count(calls, MockCall::is_reset), 1);
    assert!(
        progress
            .steps
            .iter()
            .all(|record| record.state == StepState::Verified)
    );
}

fn assert_device_holds_the_plan(fixture: &InstallFixture, plan: &StoredInstallPlan) {
    for (_, step) in plan.plan().write_steps() {
        let content = fixture
            .storage
            .content(step.lun, step.start_sector, step.sectors);
        assert!(content == source_bytes(plan, step), "{}", step.name);
    }
}

fn assert_failed_without_change(
    executor: &mut InstallExecutor<'_>,
    calls: &RefCell<Vec<MockCall>>,
) {
    let record = preflight_of(executor);
    assert!(!record.passed);
    assert!(record.failure.is_some());
    assert_eq!(record.resume, None);
    assert!(executor.progress().steps.is_empty());
    let (write, data_wipe, restart) = approvals(&record.plan_sha256);
    rejection(executor.run_write_steps(&write, &data_wipe), STOPPED);
    rejection(executor.request_restart(&restart), STOPPED);
    assert_eq!(count(calls, MockCall::changes_the_device), 0);
}

fn holds_the_plan_content(storage: &MockStorage, plan: &StoredInstallPlan, index: usize) -> bool {
    plan.plan().steps[index].as_write().is_some_and(|write| {
        storage.content(write.lun, write.start_sector, write.sectors) == source_bytes(plan, write)
    })
}

fn expected_next_index(plan: &StoredInstallPlan, index: usize, step_complete: bool) -> usize {
    let next = if step_complete { index + 1 } else { index };
    next.min(restart_index(plan))
}

fn prepare(
    fixture: &mut InstallFixture,
    plan: &StoredInstallPlan,
    index: usize,
    interruption: Interruption,
) {
    let step = &plan.plan().steps[index];
    let applied = if interruption == Interruption::After {
        index + 1
    } else {
        index
    };
    apply_steps_before(&mut fixture.storage, plan, applied);
    if interruption == Interruption::Within {
        let write = step.as_write().expect("a write step");
        apply_part_of(&mut fixture.storage, plan, write);
    }
}

fn resume_case(index: usize, interruption: Interruption) {
    let case = format!("step {index}, {interruption:?}");
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    prepare(&mut fixture, &plan, index, interruption);
    let step_complete = interruption == Interruption::After
        || holds_the_plan_content(&fixture.storage, &plan, index);
    let next_index = expected_next_index(&plan, index, step_complete);

    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect(&case);
    let record = preflight_of(&executor);
    let resume = record.resume.clone().expect("resume point");
    assert_eq!(resume.next_index, next_index, "{case}");
    let first_install = next_index == 0 && judgment_of(&record, 0) == StepJudgment::NotYet;
    assert_eq!(resume.first_install, first_install, "{case}");
    assert_eq!(executor.progress().verified_steps(), next_index, "{case}");
    if interruption == Interruption::Within {
        let judgment = judgment_of(&record, index);
        assert_eq!(judgment == StepJudgment::Done, step_complete, "{case}");
        let unknown = judgment == StepJudgment::UnknownContent;
        assert_eq!(resume.unknown_content.is_some(), unknown, "{case}");
    }
    let calls_of_the_preflight = fixture.calls.borrow().len();
    assert_eq!(count(&fixture.calls, MockCall::changes_the_device), 0);

    finish(&mut executor, &plan);
    assert_installed(&fixture.calls, &executor);
    let calls_after_the_preflight = fixture.calls.borrow()[calls_of_the_preflight..].to_vec();
    let after = RefCell::new(calls_after_the_preflight);
    for (i, step) in plan.plan().write_steps() {
        assert!(executor.progress().write_verified(i, step), "{case}");
        let written_again = writes_into(&after, step) > 0;
        assert_eq!(written_again, i >= next_index, "{case}: {}", step.name);
    }
    let boot_lun_sent = count(&after, MockCall::is_boot_lun_command) == 1;
    assert_eq!(boot_lun_sent, boot_lun_index(&plan) >= next_index, "{case}");
    drop(executor);
    assert_device_holds_the_plan(&fixture, &plan);
}

#[test]
fn an_install_that_stopped_before_within_or_after_any_step_continues_to_the_restart() {
    let plan = InstallFixture::standard().stored_plan();
    for (index, step) in plan.plan().steps.iter().enumerate() {
        if matches!(step, InstallStep::Restart) {
            continue;
        }
        resume_case(index, Interruption::Before);
        resume_case(index, Interruption::After);
        if step.as_write().is_some_and(|write| written_part(write) > 0) {
            resume_case(index, Interruption::Within);
        }
    }
}

#[test]
fn a_super_write_that_stopped_after_its_first_chunk_is_of_unknown_content_and_is_written_again() {
    let mut fixture = InstallFixture::with_super_sectors(LARGE_SUPER_SECTORS);
    let plan = fixture.stored_plan();
    let index = step_index(&plan, "super");
    let super_step = plan.plan().write_step("super").expect("super").clone();
    apply_steps_before(&mut fixture.storage, &plan, index);
    apply_part_of(&mut fixture.storage, &plan, &super_step);

    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    let record = preflight_of(&executor);
    let resume = record.resume.clone().expect("resume point");
    assert_eq!(judgment_of(&record, index), StepJudgment::UnknownContent);
    assert_eq!(resume.next_index, index);
    assert_eq!(resume.next_name, "super");
    assert_eq!(resume.unknown_content.as_deref(), Some("super"));
    assert!(!resume.first_install);
    serde_json::to_string(&record).expect("the record serializes");

    finish(&mut executor, &plan);
    assert_installed(&fixture.calls, &executor);
    assert_eq!(writes_into(&fixture.calls, &super_step), 2);
    drop(executor);
    assert_device_holds_the_plan(&fixture, &plan);
}

#[test]
fn a_done_step_after_a_step_that_is_not_done_fails_the_preflight() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let super_index = step_index(&plan, "super");
    apply_step(&mut fixture.storage, &plan, &plan.plan().steps[super_index]);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    rejection(
        executor.run_preflight(),
        "preflight: the step super is done, but the step boot_b before it is not done yet",
    );
    assert_failed_without_change(&mut executor, &fixture.calls);
}

#[test]
fn a_second_step_of_unknown_content_fails_the_preflight() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let first = step_index(&plan, "init_boot_b");
    apply_steps_before(&mut fixture.storage, &plan, first);
    for name in ["init_boot_b", "vendor_boot_b"] {
        let step = plan.plan().write_step(name).expect("step").clone();
        apply_part_of(&mut fixture.storage, &plan, &step);
    }
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    rejection(
        executor.run_preflight(),
        "preflight: the step vendor_boot_b is of unknown content, but the step init_boot_b before it is of unknown content",
    );
    assert_failed_without_change(&mut executor, &fixture.calls);
}

#[test]
fn a_boot_wlun_that_gives_neither_boot_lun_fails_the_preflight() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    apply_steps_before(&mut fixture.storage, &plan, boot_lun_index(&plan));
    fixture.storage.start_with_boot_lun(3);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    let err = rejection(
        executor.run_preflight(),
        "preflight: the GPT that is read through boot WLUN 176 has SHA-256",
    );
    assert!(matches!(err, Front2Error::Verification(_)));
    assert_failed_without_change(&mut executor, &fixture.calls);
}

#[test]
fn a_read_error_fails_the_preflight_and_is_not_judged_as_unknown_content() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    let index = step_index(&plan, "recovery_b");
    apply_steps_before(&mut fixture.storage, &plan, index);
    let location = fixture.location("recovery_b");
    fixture
        .storage
        .fail_reads_that_cover(location.lun, location.start_sector + 1);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    let err = rejection(executor.run_preflight(), "mock: read error");
    assert!(matches!(err, Front2Error::Shell(_)));
    let record = preflight_of(&executor);
    assert!(record.steps.iter().all(|check| check.index < index));
    assert_failed_without_change(&mut executor, &fixture.calls);
}

#[test]
fn a_changed_gpt_copy_that_no_step_writes_fails_the_preflight_of_a_resume() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    apply_steps_before(&mut fixture.storage, &plan, boot_lun_index(&plan));
    change_one_byte(&mut fixture.storage, 2, LAST_LBA - 4);
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    rejection(
        executor.run_preflight(),
        "preflight: the current gpt_backup2",
    );
    let record = preflight_of(&executor);
    assert!(record.steps.is_empty());
    assert_failed_without_change(&mut executor, &fixture.calls);
}

#[test]
fn the_gpt_copies_that_the_plan_writes_are_judged_as_steps_and_not_against_the_backup() {
    let mut fixture = InstallFixture::standard();
    let plan = fixture.stored_plan();
    apply_steps_before(&mut fixture.storage, &plan, boot_lun_index(&plan));
    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    let record = preflight_of(&executor);
    let checked: Vec<&str> = record
        .gpt_copies
        .iter()
        .map(|check| check.name.as_str())
        .collect();
    for name in ["gpt_main0", "gpt_main4", "gpt_backup0", "gpt_backup4"] {
        assert!(!checked.contains(&name), "{name}");
        assert_eq!(
            judgment_of(&record, step_index(&plan, name)),
            StepJudgment::Done
        );
    }
    assert_eq!(checked.len(), plan.plan().gpt_copies.len() - 4);
}

fn fixture_with_the_backup_content_as_the_dtbo_candidate() -> InstallFixture {
    let mut fixture = InstallFixture::standard();
    let original = pattern("dtbo_b-original", PARTITION_SECTORS as usize * SECTOR);
    fixture.replace_candidate("dtbo_b", &original);
    fixture
}

#[test]
fn doc_1066_a_first_install_with_a_step_that_writes_the_backup_content_passes() {
    let mut fixture = fixture_with_the_backup_content_as_the_dtbo_candidate();
    let plan = fixture.stored_plan();
    let dtbo = step_index(&plan, "dtbo_b");
    let step = plan.plan().write_step("dtbo_b").expect("dtbo_b");
    assert_eq!(step.sha256, step.expected_before_sha256);

    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    let record = preflight_of(&executor);
    let resume = record.resume.clone().expect("resume point");
    assert_eq!(judgment_of(&record, dtbo), StepJudgment::Neutral);
    assert!(resume.first_install);
    assert_eq!(resume.next_index, 0);
    assert_eq!(executor.progress().verified_steps(), 0);

    finish(&mut executor, &plan);
    assert_installed(&fixture.calls, &executor);
    drop(executor);
    assert_device_holds_the_plan(&fixture, &plan);
}

#[test]
fn doc_1066_a_step_that_writes_the_backup_content_before_the_resume_point_counts_as_verified() {
    let mut fixture = fixture_with_the_backup_content_as_the_dtbo_candidate();
    let plan = fixture.stored_plan();
    let dtbo = step_index(&plan, "dtbo_b");
    apply_steps_before(&mut fixture.storage, &plan, dtbo);

    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    let resume = preflight_of(&executor).resume.expect("resume point");
    assert_eq!(resume.next_index, dtbo + 1);
    assert!(!resume.first_install);
    let dtbo_step = plan.plan().write_step("dtbo_b").expect("dtbo_b").clone();
    assert!(executor.progress().write_verified(dtbo, &dtbo_step));

    finish(&mut executor, &plan);
    assert_installed(&fixture.calls, &executor);
    assert_eq!(writes_into(&fixture.calls, &dtbo_step), 0);
}

#[test]
fn doc_1066_a_step_that_writes_the_backup_content_is_of_unknown_content_when_the_device_differs() {
    let mut fixture = fixture_with_the_backup_content_as_the_dtbo_candidate();
    let plan = fixture.stored_plan();
    let dtbo = step_index(&plan, "dtbo_b");
    let location = fixture.location("dtbo_b");
    apply_steps_before(&mut fixture.storage, &plan, dtbo);
    change_one_byte(&mut fixture.storage, location.lun, location.start_sector);

    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    executor.run_preflight().expect("preflight");
    let record = preflight_of(&executor);
    let resume = record.resume.clone().expect("resume point");
    assert_eq!(judgment_of(&record, dtbo), StepJudgment::UnknownContent);
    assert_eq!(resume.next_index, dtbo);
    assert_eq!(resume.unknown_content.as_deref(), Some("dtbo_b"));
    finish(&mut executor, &plan);
    assert_installed(&fixture.calls, &executor);
}

#[test]
fn doc_1066_a_changed_step_with_the_backup_content_fails_a_first_install_as_a_second_unknown_step()
{
    let mut fixture = fixture_with_the_backup_content_as_the_dtbo_candidate();
    let plan = fixture.stored_plan();
    let location = fixture.location("dtbo_b");
    change_one_byte(&mut fixture.storage, location.lun, location.start_sector);

    let mut executor = InstallExecutor::new(&plan, &mut fixture.storage);
    rejection(
        executor.run_preflight(),
        "preflight: the step dtbo_b is of unknown content, but the step boot_b before it is not done yet",
    );
    let record = preflight_of(&executor);
    assert_eq!(
        judgment_of(&record, step_index(&plan, "dtbo_b")),
        StepJudgment::UnknownContent
    );
    assert_failed_without_change(&mut executor, &fixture.calls);
}
