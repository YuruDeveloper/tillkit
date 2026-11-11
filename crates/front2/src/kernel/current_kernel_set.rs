use crate::device_shell::DeviceShell;
use crate::device_state::DeviceState;
use crate::error::{Front2Error, Result};
use crate::kernel::kernel_set::{BOOT_BASE, KernelSet};
use crate::kernel::kernel_store::KernelStore;
use crate::partition_target::PartitionTarget;
use crate::profile;
use crate::root_access::RootAccess;

pub fn find_stored_current_kernel_set(
    shell: &mut dyn DeviceShell,
    state: &DeviceState,
    store: &KernelStore,
) -> Result<KernelSet> {
    let root = state
        .root_access
        .ok_or_else(|| Front2Error::Unsupported("root access is not available".into()))?;
    let boot = current_sha256(shell, root, state, BOOT_BASE)?;
    let vbmeta = current_sha256(shell, root, state, profile::VBMETA_BASE)?;
    store
        .list()?
        .into_iter()
        .find(|set| {
            set.slot_suffix == state.slot_suffix()
                && set.boot.sha256 == boot
                && set.vbmeta.sha256 == vbmeta
        })
        .ok_or_else(|| {
            Front2Error::Store(format!(
                "no stored kernel set matches the device: boot {boot}, vbmeta {vbmeta}"
            ))
        })
}

fn current_sha256(
    shell: &mut dyn DeviceShell,
    root: RootAccess,
    state: &DeviceState,
    base: &str,
) -> Result<String> {
    let label = profile::slot_label(base, state.slot_suffix());
    PartitionTarget::resolve(shell, root, &label)?.sha256(shell, root)
}
