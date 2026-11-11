use crate::apply_outcome::ApplyOutcome;
use crate::apply_script;
use crate::authorization::Authorization;
use crate::device_profile::DeviceProfile;
use crate::device_shell::DeviceShell;
use crate::device_state::DeviceState;
use crate::digest::sha256_bytes;
use crate::error::{Front2Error, Result};
use crate::image_record::ImageRecord;
use crate::kernel::boot_descriptors::require_unchanged_except_boot;
use crate::kernel::kernel_plan_facts::KernelPlanFacts;
use crate::kernel::kernel_release::KernelRelease;
use crate::kernel::kernel_set::{BOOT_BASE, KernelSet, VBMETA_FILE};
use crate::kernel::kernel_store::KernelStore;
use crate::kernel::kernel_verification::verify_kernel_images;
use crate::pair_apply;
use crate::pair_kind::PairKind;
use crate::pair_store::is_safe_id;
use crate::partition_target::PartitionTarget;
use crate::plan_request::PlanRequest;
use crate::planned_write::PlannedWrite;
use crate::profile;
use crate::profiles;
use crate::root_access::RootAccess;
use crate::scope::Scope;
use crate::support_report::SupportReport;
use crate::write_plan::{PLAN_SCHEMA, POWER_LOSS_RECOVERY, WritePlan};
use crate::write_purpose::WritePurpose;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelPlan {
    pub plan: WritePlan,
    pub facts: KernelPlanFacts,
}

impl KernelPlan {
    pub fn prepare(
        shell: &mut dyn DeviceShell,
        state: &DeviceState,
        store: &KernelStore,
        new_set: &str,
        old_set: &str,
        request: PlanRequest,
    ) -> Result<Self> {
        let id = request.id;
        if !is_safe_id(id) {
            return Err(Front2Error::Plan(format!("plan id {id:?}")));
        }
        let profile = store.profile();
        let block_bytes = profile.storage_block_bytes;
        SupportReport::evaluate(profile, state).require_allowed_with(&request.support_override)?;
        let root = state
            .root_access
            .ok_or_else(|| Front2Error::Unsupported("root access is not available".into()))?;
        let new = store.load_verified(new_set)?;
        let old = store.load_verified(old_set)?;
        let running = running_kernel(state)?;
        require_compatible(profile, &new, &old, state.slot_suffix())?;
        require_running_kmi(&running, &old)?;
        let new_directory = store.directory(new_set)?;
        let old_directory = store.directory(old_set)?;
        verify_kernel_images(&new_directory, profile)?;
        verify_kernel_images(&old_directory, profile)?;
        require_unchanged_except_boot(
            &new_directory.join(VBMETA_FILE),
            &old_directory.join(VBMETA_FILE),
        )?;
        let writes = vec![
            plan_write(
                shell,
                root,
                block_bytes,
                state,
                BOOT_BASE,
                &new.boot,
                &old.boot,
            )?,
            plan_write(
                shell,
                root,
                block_bytes,
                state,
                profile::VBMETA_BASE,
                &new.vbmeta,
                &old.vbmeta,
            )?,
        ];
        for reference in &old.references {
            reference.require_unchanged(shell, root, block_bytes)?;
        }
        let mut plan = WritePlan {
            schema: PLAN_SCHEMA,
            id: id.to_string(),
            created_unix: request.created_unix,
            purpose: purpose_of(new.kind),
            profile: profile.id.to_string(),
            support_override: request.support_override,
            new_pair: new.id.clone(),
            old_pair: old.id.clone(),
            slot_suffix: state.slot_suffix().to_string(),
            device_state: state.property("ro.boot.vbmeta.device_state").to_string(),
            verified_boot_state: state.property("ro.boot.verifiedbootstate").to_string(),
            root_access: root,
            remote_stage: format!("{}/front2-write-{id}", profile.remote_stage_parent),
            writes,
            top_rollback_index: new.avb.top_rollback_index,
            script_sha256: String::new(),
            wipes_user_data: false,
            changes_active_slot: false,
            changes_bootloader_lock: false,
            restore_tested_on_device: false,
            power_loss_recovery: POWER_LOSS_RECOVERY.to_string(),
        };
        plan.script_sha256 = sha256_bytes(apply_script::render(&plan, profile).as_bytes());
        let facts = KernelPlanFacts::describe(&plan, &new, &old, state);
        Ok(Self { plan, facts })
    }

    pub fn save(&self, directory: &Path) -> Result<String> {
        self.facts.require_matches(&self.plan)?;
        let plan_sha256 = self.plan.save(directory)?;
        self.facts.save(directory)?;
        Ok(plan_sha256)
    }

    pub fn load(directory: &Path, profile: &DeviceProfile) -> Result<(Self, String)> {
        let (plan, plan_sha256) = WritePlan::load(directory, profile)?;
        let facts = KernelPlanFacts::load(directory)?;
        facts.require_matches(&plan)?;
        Ok((Self { plan, facts }, plan_sha256))
    }

    pub fn apply(
        &self,
        shell: &mut dyn DeviceShell,
        plan_sha256: &str,
        authorization: &Authorization,
        wait: &mut dyn FnMut(),
    ) -> Result<ApplyOutcome> {
        authorization.require(plan_sha256, Scope::WritePair)?;
        self.facts.require_matches(&self.plan)?;
        let block_bytes = profiles::find(&self.plan.profile)?.storage_block_bytes;
        self.facts
            .require_references_unchanged(shell, self.plan.root_access, block_bytes)?;
        pair_apply::apply(shell, &self.plan, plan_sha256, authorization, wait)
    }
}

fn purpose_of(kind: PairKind) -> WritePurpose {
    match kind {
        PairKind::DeviceReadback => WritePurpose::RestoreReadback,
        PairKind::SignedCandidate => WritePurpose::ApplyCandidate,
    }
}

fn plan_write(
    shell: &mut dyn DeviceShell,
    root: RootAccess,
    block_bytes: u64,
    state: &DeviceState,
    base: &str,
    new_image: &ImageRecord,
    old_image: &ImageRecord,
) -> Result<PlannedWrite> {
    let label = profile::slot_label(base, state.slot_suffix());
    let target = PartitionTarget::resolve(shell, root, &label, block_bytes)?;
    let current = target.sha256(shell, root)?;
    if target.bytes != new_image.bytes || target.bytes != old_image.bytes {
        return Err(Front2Error::Plan(format!(
            "{} is {} bytes, the images are {} and {} bytes",
            target.label, target.bytes, new_image.bytes, old_image.bytes
        )));
    }
    if current != old_image.sha256 {
        return Err(Front2Error::Plan(format!(
            "{} holds SHA-256 {current}, the old kernel set records {}; read the current kernel set again",
            target.label, old_image.sha256
        )));
    }
    Ok(PlannedWrite {
        target,
        new_image: new_image.clone(),
        old_image: old_image.clone(),
        staged_new: format!("{base}.new.img"),
        staged_old: format!("{base}.old.img"),
        state_while_writing: format!("WRITING_{}", base.to_ascii_uppercase()),
    })
}

fn running_kernel(state: &DeviceState) -> Result<KernelRelease> {
    let release = state.kernel_release.as_deref().ok_or_else(|| {
        Front2Error::Unsupported(
            "the device did not report the release of the running kernel".into(),
        )
    })?;
    KernelRelease::parse(release)
}

fn require_running_kmi(running: &KernelRelease, old: &KernelSet) -> Result<()> {
    if running.kmi != old.kernel.kmi {
        return Err(Front2Error::Plan(format!(
            "the device runs kernel {} with KMI {}, the kernel sets have KMI {}",
            running.release, running.kmi, old.kernel.kmi
        )));
    }
    Ok(())
}

fn require_compatible(
    profile: &DeviceProfile,
    new: &KernelSet,
    old: &KernelSet,
    slot_suffix: &str,
) -> Result<()> {
    new.avb.require_trust(profile)?;
    old.avb.require_trust(profile)?;
    if new.slot_suffix != slot_suffix || old.slot_suffix != slot_suffix {
        return Err(Front2Error::Plan(format!(
            "kernel sets are for slots {:?} and {:?}, the device runs {slot_suffix:?}",
            new.slot_suffix, old.slot_suffix
        )));
    }
    if new.avb.top_rollback_index != old.avb.top_rollback_index
        || new.avb.top_rollback_index_location != old.avb.top_rollback_index_location
        || new.avb.top_key_sha1 != old.avb.top_key_sha1
    {
        return Err(Front2Error::Plan(format!(
            "top vbmeta rollback index {} at location {} would become {} at location {}, or the key would change",
            old.avb.top_rollback_index,
            old.avb.top_rollback_index_location,
            new.avb.top_rollback_index,
            new.avb.top_rollback_index_location
        )));
    }
    if new.kernel.kmi != old.kernel.kmi {
        return Err(Front2Error::Plan(format!(
            "the new kernel set has KMI {}, the kernel set on the device has KMI {}",
            new.kernel.kmi, old.kernel.kmi
        )));
    }
    if new.references != old.references {
        return Err(Front2Error::Plan(
            "the kernel sets record different vendor_boot, dtbo, init_boot or recovery partitions"
                .into(),
        ));
    }
    if new.boot.sha256 == old.boot.sha256 && new.vbmeta.sha256 == old.vbmeta.sha256 {
        return Err(Front2Error::Plan(
            "the new kernel set is identical to the kernel set on the device".into(),
        ));
    }
    Ok(())
}
