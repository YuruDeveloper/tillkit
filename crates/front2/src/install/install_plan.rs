use crate::digest::sha256_bytes;
use crate::error::{Front2Error, Result};
use crate::gpt::{GptCopyKind, SECTOR_BYTES};
use crate::image_record::ImageRecord;
use crate::install::gpt_copy_record::GptCopyRecord;
use crate::install::install_inputs::InstallInputs;
use crate::install::install_step::InstallStep;
use crate::install::layout::{BOOT_WLUN, NEW_BOOT_LUN, ORIGINAL_BOOT_LUN, PLAN_FILE, PLAN_SCHEMA};
use crate::install::plan_assembly;
use crate::install::plan_payload::PlanPayload;
use crate::install::plan_validation;
use crate::install::planned_target::PlannedTarget;
use crate::install::restore_entry::RestoreEntry;
use crate::install::step_source::StepSource;
use crate::install::stored_install_plan::StoredInstallPlan;
use crate::install::write_step::WriteStep;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallPlan {
    pub schema: u32,
    pub sector_bytes: u64,
    pub first_conversion_only: bool,
    pub wipes_user_data: bool,
    pub preserves_slot_a_fallback: bool,
    pub restore_tested_on_device: bool,
    pub stored_rollback_source_confirmed: bool,
    pub boot_lun_persistence_confirmed: bool,
    pub original_boot_lun: u8,
    pub new_boot_lun: u8,
    pub boot_wlun: u8,
    pub candidate_directory: PathBuf,
    pub backup_directory: PathBuf,
    pub gpt_copies: Vec<GptCopyRecord>,
    pub steps: Vec<InstallStep>,
    pub restore_set: Vec<RestoreEntry>,
    pub data_restore: RestoreEntry,
    #[serde(skip)]
    payloads: Vec<PlanPayload>,
}

impl InstallPlan {
    pub fn build(inputs: &InstallInputs) -> Result<Self> {
        let gpt_copies = plan_assembly::gpt_copy_records(inputs)?;
        let targets = plan_assembly::planned_targets(inputs)?;
        let data_restore = plan_assembly::userdata_restore(inputs)?;
        let mut plan = Self {
            schema: PLAN_SCHEMA,
            sector_bytes: SECTOR_BYTES as u64,
            first_conversion_only: true,
            wipes_user_data: true,
            preserves_slot_a_fallback: false,
            restore_tested_on_device: false,
            stored_rollback_source_confirmed: false,
            boot_lun_persistence_confirmed: false,
            original_boot_lun: ORIGINAL_BOOT_LUN,
            new_boot_lun: NEW_BOOT_LUN,
            boot_wlun: BOOT_WLUN,
            candidate_directory: inputs.candidate_directory.clone(),
            backup_directory: inputs.backup_directory.clone(),
            gpt_copies,
            steps: Vec::new(),
            restore_set: Vec::new(),
            data_restore,
            payloads: Vec::new(),
        };
        for target in targets {
            plan.push_target(target);
        }
        plan.steps.push(InstallStep::Restart);
        plan.validate()?;
        Ok(plan)
    }

    pub fn validate(&self) -> Result<()> {
        plan_validation::validate(self)
    }

    pub fn save(&self, directory: &Path) -> Result<String> {
        self.validate()?;
        let text = serde_json::to_string_pretty(self)? + "\n";
        fs::create_dir_all(directory)?;
        for (_, step) in self.write_steps() {
            self.save_plan_file(directory, step)?;
        }
        write_new(&directory.join(PLAN_FILE), text.as_bytes())?;
        Ok(sha256_bytes(text.as_bytes()))
    }

    pub fn load(directory: &Path) -> Result<StoredInstallPlan> {
        let bytes = fs::read(directory.join(PLAN_FILE))?;
        let plan: Self = serde_json::from_slice(&bytes)?;
        plan.validate()?;
        for (_, step) in plan.write_steps() {
            verify_plan_file(directory, step)?;
        }
        Ok(StoredInstallPlan::new(
            plan,
            directory,
            sha256_bytes(&bytes),
        ))
    }

    pub fn write_steps(&self) -> Vec<(usize, &WriteStep)> {
        self.steps
            .iter()
            .enumerate()
            .filter_map(|(i, step)| step.as_write().map(|write| (i, write)))
            .collect()
    }

    pub fn write_step(&self, name: &str) -> Result<&WriteStep> {
        self.steps
            .iter()
            .filter_map(InstallStep::as_write)
            .find(|step| step.name == name)
            .ok_or_else(|| Front2Error::Plan(format!("the plan holds no write step named {name}")))
    }

    pub fn gpt_copy(&self, lun: u8, kind: GptCopyKind) -> Result<&GptCopyRecord> {
        self.gpt_copies
            .iter()
            .find(|record| record.lun == lun && record.copy == kind)
            .ok_or_else(|| {
                Front2Error::Plan(format!(
                    "the plan holds no {} GPT copy of LUN {lun}",
                    kind.label()
                ))
            })
    }

    pub fn lun_sectors(&self, lun: u8) -> Result<u64> {
        let backup = self.gpt_copy(lun, GptCopyKind::Backup)?;
        backup
            .start_sector
            .checked_add(backup.sectors)
            .ok_or_else(|| {
                Front2Error::Plan(format!("the backup GPT copy of LUN {lun} is out of range"))
            })
    }

    fn push_target(&mut self, target: PlannedTarget) {
        if target.step.requires_data_wipe_approval {
            self.steps.push(boot_lun_step(self.new_boot_lun));
        }
        self.restore_set.push(target.restore);
        self.payloads.extend(target.payload);
        self.steps.push(InstallStep::Write(target.step));
    }

    fn save_plan_file(&self, directory: &Path, step: &WriteStep) -> Result<()> {
        let Some(file) = plan_file_of(step) else {
            return Ok(());
        };
        let payload = self
            .payloads
            .iter()
            .find(|payload| payload.file == file)
            .ok_or_else(|| {
                Front2Error::Plan(format!(
                    "the plan holds no bytes for {file}; a loaded plan cannot be saved again"
                ))
            })?;
        if payload.data.len() as u64 != step.bytes || sha256_bytes(&payload.data) != step.sha256 {
            return Err(Front2Error::Verification(format!(
                "the bytes for {file} differ from the step {}",
                step.name
            )));
        }
        write_new(&directory.join(file), &payload.data)
    }
}

fn boot_lun_step(lun: u8) -> InstallStep {
    InstallStep::SetBootLun { lun }
}

fn plan_file_of(step: &WriteStep) -> Option<&str> {
    match &step.source {
        StepSource::PlanFile { file } => Some(file),
        _ => None,
    }
}

fn verify_plan_file(directory: &Path, step: &WriteStep) -> Result<()> {
    let Some(file) = plan_file_of(step) else {
        return Ok(());
    };
    let record = ImageRecord {
        file: file.to_string(),
        bytes: step.bytes,
        sha256: step.sha256.clone(),
    };
    record.verify(directory)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
