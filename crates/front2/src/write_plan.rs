use crate::apply_script;
use crate::device_profile::DeviceProfile;
use crate::device_shell::DeviceShell;
use crate::device_state::DeviceState;
use crate::digest::sha256_bytes;
use crate::error::{Front2Error, Result};
use crate::image_record::ImageRecord;
use crate::pair_kind::PairKind;
use crate::pair_manifest::{PairManifest, VBMETA_FILE};
use crate::pair_store::{PairStore, is_safe_id};
use crate::pair_verification::verify_pair_images;
use crate::partition_target::PartitionTarget;
use crate::plan_request::PlanRequest;
use crate::planned_write::PlannedWrite;
use crate::profile;
use crate::profiles;
use crate::root_access::RootAccess;
use crate::support_override::SupportOverride;
use crate::support_report::SupportReport;
use crate::top_descriptors::require_unchanged_except_init_boot;
use crate::write_purpose::WritePurpose;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const PLAN_FILE: &str = "write-plan.json";
pub const PLAN_SCHEMA: u32 = 2;
pub const POWER_LOSS_RECOVERY: &str =
    "EDL restore from a full device backup. This restore was not tested on a device.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WritePlan {
    pub schema: u32,
    pub id: String,
    pub created_unix: u64,
    pub purpose: WritePurpose,
    pub profile: String,
    pub support_override: SupportOverride,
    pub new_pair: String,
    pub old_pair: String,
    pub slot_suffix: String,
    pub device_state: String,
    pub verified_boot_state: String,
    pub root_access: RootAccess,
    pub remote_stage: String,
    pub writes: Vec<PlannedWrite>,
    pub top_rollback_index: u64,
    pub script_sha256: String,
    pub wipes_user_data: bool,
    pub changes_active_slot: bool,
    pub changes_bootloader_lock: bool,
    pub restore_tested_on_device: bool,
    pub power_loss_recovery: String,
}

impl WritePlan {
    pub fn prepare(
        shell: &mut dyn DeviceShell,
        state: &DeviceState,
        store: &PairStore,
        new_pair: &str,
        old_pair: &str,
        request: PlanRequest,
    ) -> Result<Self> {
        let id = request.id;
        if !is_safe_id(id) {
            return Err(Front2Error::Plan(format!("plan id {id:?}")));
        }
        let profile = store.profile();
        SupportReport::evaluate(profile, state).require_allowed_with(&request.support_override)?;
        let root = state
            .root_access
            .ok_or_else(|| Front2Error::Unsupported("root access is not available".into()))?;
        let new = store.load_verified(new_pair)?;
        let old = store.load_verified(old_pair)?;
        require_compatible(profile, &new, &old, state.slot_suffix())?;
        let new_directory = store.directory(new_pair)?;
        let old_directory = store.directory(old_pair)?;
        verify_pair_images(&new_directory, profile)?;
        verify_pair_images(&old_directory, profile)?;
        require_unchanged_except_init_boot(
            &new_directory.join(VBMETA_FILE),
            &old_directory.join(VBMETA_FILE),
        )?;
        let writes = vec![
            plan_write(
                shell,
                root,
                profile.storage_block_bytes,
                state,
                profile::INIT_BOOT_BASE,
                &new.init_boot,
                &old.init_boot,
            )?,
            plan_write(
                shell,
                root,
                profile.storage_block_bytes,
                state,
                profile::VBMETA_BASE,
                &new.vbmeta,
                &old.vbmeta,
            )?,
        ];
        let mut plan = Self {
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
        Ok(plan)
    }

    pub fn script(&self) -> Result<String> {
        let script = apply_script::render(self, profiles::find(&self.profile)?);
        if sha256_bytes(script.as_bytes()) != self.script_sha256 {
            return Err(Front2Error::Plan(
                "the apply script does not match the SHA-256 in the plan".into(),
            ));
        }
        Ok(script)
    }

    pub fn save(&self, directory: &Path) -> Result<String> {
        fs::create_dir_all(directory)?;
        let text = serde_json::to_string_pretty(self)? + "\n";
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(PLAN_FILE))?;
        std::io::Write::write_all(&mut file, text.as_bytes())?;
        Ok(sha256_bytes(text.as_bytes()))
    }

    pub fn load(directory: &Path, profile: &DeviceProfile) -> Result<(Self, String)> {
        let bytes = fs::read(directory.join(PLAN_FILE))?;
        let plan: Self = serde_json::from_slice(&bytes)?;
        if plan.schema != PLAN_SCHEMA {
            return Err(Front2Error::Plan(format!("plan schema {}", plan.schema)));
        }
        profile.require_id(&plan.profile)?;
        plan.support_override.require_acceptable()?;
        plan.script()?;
        Ok((plan, sha256_bytes(&bytes)))
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
    require_device_holds(&target, &current, new_image, old_image)?;
    Ok(PlannedWrite {
        target,
        new_image: new_image.clone(),
        old_image: old_image.clone(),
        staged_new: format!("{base}.new.img"),
        staged_old: format!("{base}.old.img"),
        state_while_writing: format!("WRITING_{}", base.to_ascii_uppercase()),
    })
}

fn require_compatible(
    profile: &DeviceProfile,
    new: &PairManifest,
    old: &PairManifest,
    slot_suffix: &str,
) -> Result<()> {
    new.avb.require_trust(profile)?;
    old.avb.require_trust(profile)?;
    if new.slot_suffix != slot_suffix || old.slot_suffix != slot_suffix {
        return Err(Front2Error::Plan(format!(
            "pairs are for slots {:?} and {:?}, the device runs {slot_suffix:?}",
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
    if new.init_boot.sha256 == old.init_boot.sha256 && new.vbmeta.sha256 == old.vbmeta.sha256 {
        return Err(Front2Error::Plan(
            "the new pair is identical to the pair on the device".into(),
        ));
    }
    Ok(())
}

fn require_device_holds(
    target: &PartitionTarget,
    current_sha256: &str,
    new_image: &ImageRecord,
    old_image: &ImageRecord,
) -> Result<()> {
    if target.bytes != new_image.bytes || target.bytes != old_image.bytes {
        return Err(Front2Error::Plan(format!(
            "{} is {} bytes, the images are {} and {} bytes",
            target.label, target.bytes, new_image.bytes, old_image.bytes
        )));
    }
    if current_sha256 != old_image.sha256 {
        return Err(Front2Error::Plan(format!(
            "{} holds SHA-256 {current_sha256}, the old pair records {}; read the current pair again",
            target.label, old_image.sha256
        )));
    }
    Ok(())
}
