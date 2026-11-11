use crate::device_shell::DeviceShell;
use crate::device_state::DeviceState;
use crate::error::{Front2Error, Result};
use crate::kernel::kernel_set::KernelSet;
use crate::kernel::reference_partition::ReferencePartition;
use crate::root_access::RootAccess;
use crate::write_plan::WritePlan;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const FACTS_FILE: &str = "kernel-plan-facts.json";
pub const FACTS_SCHEMA: u32 = 1;
pub const SAME_KMI_STATEMENT: &str =
    "The same KMI identifier does not guarantee that the new kernel boots on this device.";
pub const BOOT_FAILURE_STATEMENT: &str = "If the device does not boot after the write, ADB is not available. The only recovery is an EDL rewrite of the two partitions, and that rewrite was not tested on a device.";
pub const SLOT_FALLBACK_STATEMENT: &str = "There is no fallback to slot A.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelPlanFacts {
    pub schema: u32,
    pub plan_id: String,
    pub script_sha256: String,
    pub new_set: String,
    pub old_set: String,
    pub kmi: String,
    pub new_release: String,
    pub old_release: String,
    pub running_release: Option<String>,
    pub old_set_is_running: bool,
    pub written_partitions: Vec<String>,
    pub reference_partitions: Vec<ReferencePartition>,
    pub same_kmi_guarantees_boot: bool,
    pub adb_available_after_boot_failure: bool,
    pub boot_failure_recovery_tested_on_device: bool,
    pub slot_a_fallback: bool,
    pub statements: Vec<String>,
}

impl KernelPlanFacts {
    pub fn describe(
        plan: &WritePlan,
        new: &KernelSet,
        old: &KernelSet,
        state: &DeviceState,
    ) -> Self {
        Self {
            schema: FACTS_SCHEMA,
            plan_id: plan.id.clone(),
            script_sha256: plan.script_sha256.clone(),
            new_set: new.id.clone(),
            old_set: old.id.clone(),
            kmi: new.kernel.kmi.clone(),
            new_release: new.kernel.release.clone(),
            old_release: old.kernel.release.clone(),
            running_release: state.kernel_release.clone(),
            old_set_is_running: state.kernel_release.as_deref()
                == Some(old.kernel.release.as_str()),
            written_partitions: plan
                .writes
                .iter()
                .map(|write| write.target.label.clone())
                .collect(),
            reference_partitions: old.references.clone(),
            same_kmi_guarantees_boot: false,
            adb_available_after_boot_failure: false,
            boot_failure_recovery_tested_on_device: false,
            slot_a_fallback: false,
            statements: fixed_statements(),
        }
    }

    pub fn require_matches(&self, plan: &WritePlan) -> Result<()> {
        let bound_to_plan = self.schema == FACTS_SCHEMA
            && self.plan_id == plan.id
            && self.script_sha256 == plan.script_sha256
            && self.new_set == plan.new_pair
            && self.old_set == plan.old_pair;
        let facts_intact = !self.same_kmi_guarantees_boot
            && !self.adb_available_after_boot_failure
            && !self.boot_failure_recovery_tested_on_device
            && !self.slot_a_fallback
            && self.statements == fixed_statements();
        if !bound_to_plan || !facts_intact {
            return Err(Front2Error::Plan(format!(
                "the kernel plan facts for plan {:?} do not match plan {:?} or were edited",
                self.plan_id, plan.id
            )));
        }
        Ok(())
    }

    pub fn require_references_unchanged(
        &self,
        shell: &mut dyn DeviceShell,
        root: RootAccess,
    ) -> Result<()> {
        for reference in &self.reference_partitions {
            reference.require_unchanged(shell, root)?;
        }
        Ok(())
    }

    pub fn save(&self, directory: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)? + "\n";
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(FACTS_FILE))?;
        std::io::Write::write_all(&mut file, text.as_bytes())?;
        Ok(())
    }

    pub fn load(directory: &Path) -> Result<Self> {
        Ok(serde_json::from_slice(&fs::read(
            directory.join(FACTS_FILE),
        )?)?)
    }
}

fn fixed_statements() -> Vec<String> {
    [
        SAME_KMI_STATEMENT,
        BOOT_FAILURE_STATEMENT,
        SLOT_FALLBACK_STATEMENT,
    ]
    .iter()
    .map(|statement| statement.to_string())
    .collect()
}
