use crate::error::{Front2Error, Result};
use crate::gpt::{GptCopyKind, SECTOR_BYTES};
use crate::image_record::ImageRecord;
use crate::install::digest_check::DigestCheck;
use crate::install::install_plan::InstallPlan;
use crate::install::install_step::InstallStep;
use crate::install::layout::{BOOT_LUN_STEP_NAME, GPT_LUN_COUNT, gpt_step_name};
use crate::install::payload_reader::PayloadReader;
use crate::install::preflight_record::PreflightRecord;
use crate::install::resume_point::ResumePoint;
use crate::install::sector_storage::SectorStorage;
use crate::install::sector_transfer::read_sectors_sha256;
use crate::install::step_check::StepCheck;
use crate::install::step_judgment::StepJudgment;
use crate::install::stored_install_plan::StoredInstallPlan;
use crate::install::write_step::WriteStep;

pub fn run_preflight(
    plan: &StoredInstallPlan,
    storage: &mut dyn SectorStorage,
) -> (PreflightRecord, Result<()>) {
    let mut record = PreflightRecord::started(plan.plan_sha256());
    let outcome = check_all(plan, storage, &mut record);
    match &outcome {
        Ok(()) => record.passed = true,
        Err(err) => record.failure = Some(err.to_string()),
    }
    (record, outcome)
}

fn check_all(
    plan: &StoredInstallPlan,
    storage: &mut dyn SectorStorage,
    record: &mut PreflightRecord,
) -> Result<()> {
    check_sources(plan, record)?;
    check_restore_files(plan, record)?;
    check_storage(plan, storage, record)?;
    check_gpt_copies(plan, storage, record)?;
    check_steps(plan, storage, record)
}

fn require_match(subject: &str, check: &DigestCheck) -> Result<()> {
    if !check.matches() {
        return Err(Front2Error::Verification(format!(
            "preflight: {subject} {} has SHA-256 {}, the plan records {}",
            check.name, check.sha256, check.expected_sha256
        )));
    }
    Ok(())
}

fn check_sources(plan: &StoredInstallPlan, record: &mut PreflightRecord) -> Result<()> {
    for (_, step) in plan.plan().write_steps() {
        let sha256 = PayloadReader::open(plan, step)?.sha256()?;
        let check = DigestCheck::new(&step.name, &sha256, &step.sha256);
        record.sources.push(check.clone());
        require_match("the source of", &check)?;
    }
    Ok(())
}

fn check_restore_files(plan: &StoredInstallPlan, record: &mut PreflightRecord) -> Result<()> {
    let plan = plan.plan();
    for entry in plan.restore_set.iter().chain([&plan.data_restore]) {
        let actual = ImageRecord::from_file(&plan.backup_directory, &entry.file)?;
        let check = DigestCheck::new(&entry.name, &actual.sha256, &entry.sha256);
        record.restore_files.push(check.clone());
        if actual.bytes != entry.bytes {
            return Err(Front2Error::Verification(format!(
                "preflight: the backup file of {} is {} bytes, the plan records {} bytes",
                entry.name, actual.bytes, entry.bytes
            )));
        }
        require_match("the backup file of", &check)?;
    }
    Ok(())
}

fn check_storage(
    plan: &StoredInstallPlan,
    storage: &mut dyn SectorStorage,
    record: &mut PreflightRecord,
) -> Result<()> {
    for lun in 0..GPT_LUN_COUNT {
        let expected_sectors = plan.plan().lun_sectors(lun)?;
        let reported = storage.storage_info(lun)?;
        record.storage.push(reported);
        let same = reported.lun == lun
            && reported.total_sectors == expected_sectors
            && reported.sector_bytes == SECTOR_BYTES as u64;
        if !same {
            return Err(Front2Error::Verification(format!(
                "preflight: the device reports LUN {} with {} sectors of {} bytes, the backup GPT gives LUN {lun} with {expected_sectors} sectors of {SECTOR_BYTES} bytes",
                reported.lun, reported.total_sectors, reported.sector_bytes
            )));
        }
    }
    Ok(())
}

fn check_gpt_copies(
    plan: &StoredInstallPlan,
    storage: &mut dyn SectorStorage,
    record: &mut PreflightRecord,
) -> Result<()> {
    for copy in &plan.plan().gpt_copies {
        let name = gpt_step_name(copy.copy, copy.lun);
        if plan.plan().write_step(&name).is_ok() {
            continue;
        }
        let sha256 = read_sectors_sha256(storage, copy.lun, copy.start_sector, copy.sectors)?;
        let check = DigestCheck::new(&name, &sha256, &copy.sha256);
        record.gpt_copies.push(check.clone());
        require_match("the current", &check)?;
    }
    Ok(())
}

fn check_steps(
    plan: &StoredInstallPlan,
    storage: &mut dyn SectorStorage,
    record: &mut PreflightRecord,
) -> Result<()> {
    let plan = plan.plan();
    for (index, step) in plan.steps.iter().enumerate() {
        let check = match (step.as_write(), step.boot_lun()) {
            (Some(write), _) => check_write(storage, index, write)?,
            (None, Some(lun)) => check_boot_lun(plan, storage, index, lun)?,
            (None, None) => continue,
        };
        record.steps.push(check);
    }
    let restart_index = plan
        .steps
        .iter()
        .position(|step| matches!(step, InstallStep::Restart))
        .ok_or_else(|| Front2Error::Plan("the plan holds no restart step".into()))?;
    record.resume = Some(ResumePoint::judge(&record.steps, restart_index)?);
    Ok(())
}

fn check_write(
    storage: &mut dyn SectorStorage,
    index: usize,
    step: &WriteStep,
) -> Result<StepCheck> {
    let sha256 = read_sectors_sha256(storage, step.lun, step.start_sector, step.sectors)?;
    Ok(StepCheck {
        index,
        name: step.name.clone(),
        judgment: StepJudgment::of_write(&sha256, step),
        sha256,
    })
}

fn check_boot_lun(
    plan: &InstallPlan,
    storage: &mut dyn SectorStorage,
    index: usize,
    lun: u8,
) -> Result<StepCheck> {
    let new = plan.gpt_copy(plan.new_boot_lun, GptCopyKind::Main)?;
    let original = plan.gpt_copy(plan.original_boot_lun, GptCopyKind::Main)?;
    if lun != plan.new_boot_lun || new.sha256 == original.sha256 {
        return Err(Front2Error::Plan(format!(
            "the boot LUN step for LUN {lun} cannot be judged against this plan"
        )));
    }
    let sha256 = read_sectors_sha256(storage, plan.boot_wlun, new.start_sector, new.sectors)?;
    let judgment = if sha256 == new.sha256 {
        StepJudgment::Done
    } else if sha256 == original.sha256 {
        StepJudgment::NotYet
    } else {
        return Err(Front2Error::Verification(format!(
            "preflight: the GPT that is read through boot WLUN {} has SHA-256 {sha256}; the main GPT of LUN {} has {}, the main GPT of LUN {} has {}",
            plan.boot_wlun, plan.new_boot_lun, new.sha256, plan.original_boot_lun, original.sha256
        )));
    };
    Ok(StepCheck {
        index,
        name: BOOT_LUN_STEP_NAME.to_string(),
        sha256,
        judgment,
    })
}
