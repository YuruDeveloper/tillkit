use crate::error::{Front2Error, Result};
use crate::gpt::{GptCopyKind, SECTOR_BYTES};
use crate::install::digest_check::DigestCheck;
use crate::install::layout::GPT_LUN_COUNT;
use crate::install::payload_reader::PayloadReader;
use crate::install::preflight_record::PreflightRecord;
use crate::install::sector_storage::SectorStorage;
use crate::install::sector_transfer::read_sectors_sha256;
use crate::install::stored_install_plan::StoredInstallPlan;

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
    check_storage(plan, storage, record)?;
    check_gpt_copies(plan, storage, record)?;
    check_boot_wlun(plan, storage, record)?;
    check_current_targets(plan, storage, record)
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
        let name = format!("gpt_{}{}", copy.copy.label(), copy.lun);
        let sha256 = read_sectors_sha256(storage, copy.lun, copy.start_sector, copy.sectors)?;
        let check = DigestCheck::new(&name, &sha256, &copy.sha256);
        record.gpt_copies.push(check.clone());
        require_match("the current", &check)?;
    }
    Ok(())
}

fn check_boot_wlun(
    plan: &StoredInstallPlan,
    storage: &mut dyn SectorStorage,
    record: &mut PreflightRecord,
) -> Result<()> {
    let plan = plan.plan();
    let original = plan.gpt_copy(plan.original_boot_lun, GptCopyKind::Main)?;
    let name = format!("boot WLUN {}", plan.boot_wlun);
    let sha256 = read_sectors_sha256(
        storage,
        plan.boot_wlun,
        original.start_sector,
        original.sectors,
    )?;
    let check = DigestCheck::new(&name, &sha256, &original.sha256);
    record.boot_wlun = Some(check.clone());
    require_match("the GPT that is read through", &check)
}

fn check_current_targets(
    plan: &StoredInstallPlan,
    storage: &mut dyn SectorStorage,
    record: &mut PreflightRecord,
) -> Result<()> {
    for (_, step) in plan.plan().write_steps() {
        let sha256 = read_sectors_sha256(storage, step.lun, step.start_sector, step.sectors)?;
        let check = DigestCheck::new(&step.name, &sha256, &step.expected_before_sha256);
        record.current_targets.push(check.clone());
        require_match("the current content of", &check)?;
    }
    Ok(())
}
