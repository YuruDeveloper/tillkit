use crate::device_shell::DeviceShell;
use crate::device_state::DeviceState;
use crate::error::{Front2Error, Result};
use crate::pair_manifest::PairManifest;
use crate::pair_store::PairStore;
use crate::partition_target::PartitionTarget;
use crate::profile;
use crate::root_access::RootAccess;

pub fn find_stored_current_pair(
    shell: &mut dyn DeviceShell,
    state: &DeviceState,
    store: &PairStore,
) -> Result<PairManifest> {
    let root = state
        .root_access
        .ok_or_else(|| Front2Error::Unsupported("root access is not available".into()))?;
    let block_bytes = store.profile().storage_block_bytes;
    let init_boot = current_sha256(shell, root, state, profile::INIT_BOOT_BASE, block_bytes)?;
    let vbmeta = current_sha256(shell, root, state, profile::VBMETA_BASE, block_bytes)?;
    store
        .list()?
        .into_iter()
        .find(|pair| {
            pair.slot_suffix == state.slot_suffix()
                && pair.init_boot.sha256 == init_boot
                && pair.vbmeta.sha256 == vbmeta
        })
        .ok_or_else(|| {
            Front2Error::Store(format!(
                "no stored pair matches the device: init_boot {init_boot}, vbmeta {vbmeta}"
            ))
        })
}

fn current_sha256(
    shell: &mut dyn DeviceShell,
    root: RootAccess,
    state: &DeviceState,
    base: &str,
    block_bytes: u64,
) -> Result<String> {
    let label = profile::slot_label(base, state.slot_suffix());
    PartitionTarget::resolve(shell, root, &label, block_bytes)?.sha256(shell, root)
}
