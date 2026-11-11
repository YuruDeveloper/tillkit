use crate::error::{Front2Error, Result};
use crate::gpt::GptCopyKind;
use crate::install::boot_lun_record::BootLunRecord;
use crate::install::install_plan::InstallPlan;
use crate::install::sector_storage::SectorStorage;
use crate::install::sector_transfer::read_sectors_sha256;

pub fn select_boot_lun(
    plan: &InstallPlan,
    storage: &mut dyn SectorStorage,
    lun: u8,
) -> (BootLunRecord, Result<()>) {
    let mut record = BootLunRecord::requested(lun, plan.boot_wlun);
    let outcome = select_and_judge(plan, storage, &mut record);
    (record, outcome)
}

fn select_and_judge(
    plan: &InstallPlan,
    storage: &mut dyn SectorStorage,
    record: &mut BootLunRecord,
) -> Result<()> {
    let new = plan.gpt_copy(plan.new_boot_lun, GptCopyKind::Main)?;
    let original = plan.gpt_copy(plan.original_boot_lun, GptCopyKind::Main)?;
    if record.requested_lun != plan.new_boot_lun || new.sha256 == original.sha256 {
        return Err(Front2Error::Plan(format!(
            "the boot LUN step for LUN {} cannot be judged against this plan",
            record.requested_lun
        )));
    }
    storage.set_boot_lun(record.requested_lun)?;
    record.command_accepted = true;
    let readback = read_sectors_sha256(storage, plan.boot_wlun, new.start_sector, new.sectors)?;
    record.matches_new_lun_gpt = readback == new.sha256;
    record.matches_original_lun_gpt = readback == original.sha256;
    record.passed = record.matches_new_lun_gpt && !record.matches_original_lun_gpt;
    record.readback_sha256 = Some(readback.clone());
    if !record.passed {
        return Err(Front2Error::Verification(format!(
            "the boot WLUN {} gives a GPT with SHA-256 {readback} after the boot LUN command; the main GPT of LUN {} has {}, the main GPT of LUN {} has {}",
            plan.boot_wlun, plan.new_boot_lun, new.sha256, plan.original_boot_lun, original.sha256
        )));
    }
    Ok(())
}
