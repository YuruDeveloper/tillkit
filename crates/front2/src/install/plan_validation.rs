use crate::digest::is_sha256_hex;
use crate::error::{Front2Error, Result};
use crate::gpt::{BootSlot, GptCopyKind, SECTOR_BYTES};
use crate::install::gpt_copy_record::GptCopyRecord;
use crate::install::install_plan::InstallPlan;
use crate::install::install_step::InstallStep;
use crate::install::layout::{
    BOOT_WLUN, GPT_LUN_COUNT, GPT_STEP_ORDER, KEEP_BOOT_LUN_STEP_NAME, METADATA_PARTITION,
    MISC_PARTITION, PLAN_SCHEMA, SET_BOOT_LUN_STEP_NAME, USERDATA_PARTITION, boot_lun_pair,
    gpt_step_name, require_plain_file_name, require_relative_file, sectors_to_bytes,
    write_step_names,
};
use crate::install::restore_entry::RestoreEntry;
use crate::install::step_source::StepSource;
use crate::install::write_step::WriteStep;

pub fn validate(plan: &InstallPlan) -> Result<()> {
    require_supported_target(plan)?;
    require_fixed_facts(plan)?;
    require_gpt_copies(plan)?;
    require_step_order(plan)?;
    for (_, step) in plan.write_steps() {
        require_valid_write(plan, step)?;
    }
    require_separate_ranges(plan)?;
    require_gpt_steps(plan)?;
    require_restore_set(plan)?;
    require_data_restore(plan)
}

fn require_supported_target(plan: &InstallPlan) -> Result<()> {
    if plan.target_slot == BootSlot::A {
        return Err(Front2Error::Unsupported(
            "the install target slot A is not supported until the boot switch and the super rebuild are complete".into(),
        ));
    }
    Ok(())
}

fn require_fixed_facts(plan: &InstallPlan) -> Result<()> {
    let (expected_original, expected_new) = boot_lun_pair(plan.target_slot);
    let wrong: Vec<&str> = [
        (plan.schema == PLAN_SCHEMA, "schema"),
        (plan.sector_bytes == SECTOR_BYTES as u64, "sector_bytes"),
        (plan.first_conversion_only, "first_conversion_only"),
        (plan.wipes_user_data, "wipes_user_data"),
        (!plan.preserves_slot_a_fallback, "preserves_slot_a_fallback"),
        (!plan.restore_tested_on_device, "restore_tested_on_device"),
        (
            !plan.stored_rollback_source_confirmed,
            "stored_rollback_source_confirmed",
        ),
        (
            !plan.boot_lun_persistence_confirmed,
            "boot_lun_persistence_confirmed",
        ),
        (
            plan.original_boot_lun == expected_original,
            "original_boot_lun",
        ),
        (plan.new_boot_lun == expected_new, "new_boot_lun"),
        (plan.boot_wlun == BOOT_WLUN, "boot_wlun"),
        (
            plan.candidate_directory.is_absolute(),
            "candidate_directory",
        ),
        (plan.backup_directory.is_absolute(), "backup_directory"),
    ]
    .into_iter()
    .filter_map(|(holds, name)| (!holds).then_some(name))
    .collect();
    if !wrong.is_empty() {
        return Err(Front2Error::Plan(format!(
            "the install plan holds values that are not supported in {}",
            wrong.join(", ")
        )));
    }
    Ok(())
}

fn require_gpt_copies(plan: &InstallPlan) -> Result<()> {
    let expected: Vec<(u8, GptCopyKind)> = (0..GPT_LUN_COUNT)
        .flat_map(|lun| [(lun, GptCopyKind::Main), (lun, GptCopyKind::Backup)])
        .collect();
    let actual: Vec<(u8, GptCopyKind)> = plan
        .gpt_copies
        .iter()
        .map(|record| (record.lun, record.copy))
        .collect();
    if actual != expected {
        return Err(Front2Error::Plan(format!(
            "the plan does not list the main and backup GPT copies of LUN 0 to {} in order",
            GPT_LUN_COUNT - 1
        )));
    }
    for record in &plan.gpt_copies {
        require_valid_gpt_record(record)?;
    }
    let original = plan.gpt_copy(plan.original_boot_lun, GptCopyKind::Main)?;
    let new = plan.gpt_copy(plan.new_boot_lun, GptCopyKind::Main)?;
    if original.sha256 == new.sha256 {
        return Err(Front2Error::Plan(format!(
            "the main GPT copies of LUN {} and LUN {} are identical, a read of the boot WLUN cannot tell the boot LUN",
            plan.original_boot_lun, plan.new_boot_lun
        )));
    }
    Ok(())
}

fn require_valid_gpt_record(record: &GptCopyRecord) -> Result<()> {
    require_relative_file(&record.file)?;
    let dump_bytes = record.copy.dump_bytes();
    let valid = record.bytes == dump_bytes as u64
        && record.sectors == (dump_bytes / SECTOR_BYTES) as u64
        && (record.copy == GptCopyKind::Backup || record.start_sector == 0)
        && record.start_sector.checked_add(record.sectors).is_some()
        && is_sha256_hex(&record.sha256);
    if !valid {
        return Err(Front2Error::Plan(format!(
            "the record of the {} GPT copy of LUN {} is not valid",
            record.copy.label(),
            record.lun
        )));
    }
    Ok(())
}

fn require_step_order(plan: &InstallPlan) -> Result<()> {
    let actual: Vec<String> = plan.steps.iter().map(InstallStep::label).collect();
    let expected = expected_step_labels(plan.stock_slot, plan.target_slot, plan.new_boot_lun);
    if actual != expected {
        return Err(Front2Error::Plan(format!(
            "the plan steps are {actual:?}, the fixed order is {expected:?}"
        )));
    }
    Ok(())
}

fn expected_step_labels(
    boot_slot: BootSlot,
    target_slot: BootSlot,
    new_boot_lun: u8,
) -> Vec<String> {
    let mut labels = Vec::new();
    for name in write_step_names(boot_slot, target_slot) {
        if name == MISC_PARTITION {
            labels.push(boot_lun_label(boot_slot, target_slot, new_boot_lun));
        }
        labels.push(format!("write:{name}"));
    }
    labels.push("restart".to_string());
    labels
}

fn boot_lun_label(boot_slot: BootSlot, target_slot: BootSlot, new_boot_lun: u8) -> String {
    if boot_slot != target_slot {
        format!("{SET_BOOT_LUN_STEP_NAME}:{new_boot_lun}")
    } else {
        format!("{KEEP_BOOT_LUN_STEP_NAME}:{new_boot_lun}")
    }
}

fn require_valid_write(plan: &InstallPlan, step: &WriteStep) -> Result<()> {
    require_valid_source(&step.source)?;
    let lun_sectors = plan.lun_sectors(step.lun)?;
    let problems: Vec<&str> = [
        (step.sectors > 0, "sector count"),
        (
            sectors_to_bytes(step.sectors) == Some(step.bytes),
            "byte count",
        ),
        (is_sha256_hex(&step.sha256), "SHA-256"),
        (
            is_sha256_hex(&step.expected_before_sha256),
            "expected SHA-256 before the write",
        ),
        (
            step.end_sector().is_some_and(|end| end <= lun_sectors),
            "sector range",
        ),
        (
            step.requires_data_wipe_approval == (step.name == MISC_PARTITION),
            "data wipe approval flag",
        ),
        (
            (step.source == StepSource::ZeroFill) == (step.name == METADATA_PARTITION),
            "source kind",
        ),
    ]
    .into_iter()
    .filter_map(|(holds, name)| (!holds).then_some(name))
    .collect();
    if !problems.is_empty() {
        return Err(Front2Error::Plan(format!(
            "the step {} on LUN {} of {lun_sectors} sectors is not valid in {}",
            step.name,
            step.lun,
            problems.join(", ")
        )));
    }
    Ok(())
}

fn require_valid_source(source: &StepSource) -> Result<()> {
    match source {
        StepSource::CandidateFile { file } => require_relative_file(file),
        StepSource::PlanFile { file } => require_plain_file_name(file),
        StepSource::ZeroFill => Ok(()),
    }
}

fn require_separate_ranges(plan: &InstallPlan) -> Result<()> {
    let steps: Vec<&WriteStep> = plan
        .write_steps()
        .into_iter()
        .map(|(_, step)| step)
        .collect();
    for (i, first) in steps.iter().enumerate() {
        require_no_overlap(first, &steps[i + 1..])?;
    }
    Ok(())
}

fn require_no_overlap(first: &WriteStep, later: &[&WriteStep]) -> Result<()> {
    match later.iter().find(|second| first.overlaps(second)) {
        Some(second) => Err(Front2Error::Plan(format!(
            "the steps {} and {} cover the same sectors of LUN {}",
            first.name, second.name, first.lun
        ))),
        None => Ok(()),
    }
}

fn require_gpt_steps(plan: &InstallPlan) -> Result<()> {
    if plan.stock_slot == plan.target_slot {
        return Ok(());
    }
    for (kind, lun) in GPT_STEP_ORDER {
        let name = gpt_step_name(kind, lun);
        let step = plan.write_step(&name)?;
        let record = plan.gpt_copy(lun, kind)?;
        let same = step.lun == lun
            && step.start_sector == record.start_sector
            && step.sectors == record.sectors
            && step.expected_before_sha256 == record.sha256;
        if !same {
            return Err(Front2Error::Plan(format!(
                "the step {name} does not cover the {} GPT copy of LUN {lun} that the plan records",
                kind.label()
            )));
        }
    }
    Ok(())
}

fn require_restore_set(plan: &InstallPlan) -> Result<()> {
    let steps = plan.write_steps();
    if plan.restore_set.len() != steps.len() {
        return Err(Front2Error::Plan(format!(
            "the restore set holds {} entries for {} write steps",
            plan.restore_set.len(),
            steps.len()
        )));
    }
    for ((_, step), restore) in steps.into_iter().zip(&plan.restore_set) {
        require_restore_entry(step, restore)?;
    }
    Ok(())
}

fn require_data_restore(plan: &InstallPlan) -> Result<()> {
    let restore = &plan.data_restore;
    require_relative_file(&restore.file)?;
    let end = restore.start_sector.checked_add(restore.sectors);
    let inside = end.is_some_and(|end| {
        plan.lun_sectors(restore.lun)
            .is_ok_and(|total| end <= total)
    });
    let written = plan.write_steps().into_iter().any(|(_, step)| {
        step.lun == restore.lun
            && step.start_sector < end.unwrap_or(u64::MAX)
            && restore.start_sector < step.start_sector.saturating_add(step.sectors)
    });
    let valid = restore.name == USERDATA_PARTITION
        && restore.sectors > 0
        && inside
        && !written
        && sectors_to_bytes(restore.sectors) == Some(restore.bytes)
        && is_sha256_hex(&restore.sha256);
    if !valid {
        return Err(Front2Error::Plan(format!(
            "the data restore entry {} is not a whole userdata partition outside the write steps",
            restore.name
        )));
    }
    Ok(())
}

fn require_restore_entry(step: &WriteStep, restore: &RestoreEntry) -> Result<()> {
    require_relative_file(&restore.file)?;
    let whole_range = restore.sectors == step.sectors;
    let valid = restore.name == step.name
        && restore.lun == step.lun
        && restore.start_sector == step.start_sector
        && restore.sectors >= step.sectors
        && sectors_to_bytes(restore.sectors) == Some(restore.bytes)
        && is_sha256_hex(&restore.sha256)
        && (!whole_range || restore.sha256 == step.expected_before_sha256);
    if !valid {
        return Err(Front2Error::Plan(format!(
            "the restore entry {} does not fit the step {}",
            restore.name, step.name
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_step_labels_of_a_switch_to_slot_a_set_the_boot_lun_and_hold_the_gpt_steps() {
        let labels = expected_step_labels(BootSlot::B, BootSlot::A, 1);
        assert!(labels.contains(&"write:boot_a".to_string()));
        assert!(labels.contains(&"set_boot_lun:1".to_string()));
        assert!(labels.iter().any(|label| label.starts_with("write:gpt_")));
        assert_eq!(labels.last().map(String::as_str), Some("restart"));
    }

    #[test]
    fn the_step_labels_of_a_stock_slot_b_install_keep_the_boot_lun_and_omit_the_gpt_steps() {
        let labels = expected_step_labels(BootSlot::B, BootSlot::B, 2);
        assert!(labels.contains(&"keep_boot_lun:2".to_string()));
        assert!(!labels.iter().any(|label| label.starts_with("write:gpt_")));
    }

    #[test]
    fn the_boot_lun_label_sets_on_a_transition_and_keeps_without_one() {
        assert_eq!(
            boot_lun_label(BootSlot::A, BootSlot::B, 2),
            "set_boot_lun:2"
        );
        assert_eq!(
            boot_lun_label(BootSlot::B, BootSlot::A, 1),
            "set_boot_lun:1"
        );
        assert_eq!(
            boot_lun_label(BootSlot::B, BootSlot::B, 2),
            "keep_boot_lun:2"
        );
    }
}
