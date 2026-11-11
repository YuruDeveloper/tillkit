use crate::device_shell::DeviceShell;
use crate::device_state::DeviceState;
use crate::digest::sha256_file;
use crate::error::{Front2Error, Result};
use crate::kernel::hash_descriptor::HashDescriptor;
use crate::kernel::kernel_avb_summary::measured_hash_partitions;
use crate::kernel::kernel_set::{BOOT_BASE, BOOT_FILE, KernelSet, VBMETA_FILE};
use crate::kernel::kernel_store::KernelStore;
use crate::kernel::kernel_verification::verify_kernel_images;
use crate::kernel::reference_partition::{REFERENCE_BASES, ReferencePartition};
use crate::pair_kind::PairKind;
use crate::pair_request::PairRequest;
use crate::partition_target::{PartitionTarget, device_sha256};
use crate::profile;
use crate::root_access::{RootAccess, quote};
use std::fs;
use std::path::Path;

pub fn read_current_kernel_set(
    shell: &mut dyn DeviceShell,
    state: &DeviceState,
    store: &KernelStore,
    request: PairRequest<'_>,
) -> Result<KernelSet> {
    let root = state
        .root_access
        .ok_or_else(|| Front2Error::Unsupported("root access is not available".into()))?;
    let slot_suffix = state.slot_suffix();
    if slot_suffix != "_a" && slot_suffix != "_b" {
        return Err(Front2Error::Unsupported(format!(
            "active slot suffix is {slot_suffix:?}"
        )));
    }
    let profile = store.profile();
    let block_bytes = profile.storage_block_bytes;
    let directory = store.begin(request.id)?;
    let remote = format!(
        "{}/front2-kernel-read-{}",
        profile.remote_stage_parent, request.id
    );
    let outcome = shell
        .run_checked(&root.command(&format!("mkdir {}", quote(&remote))))
        .and_then(|_| read_images(shell, root, slot_suffix, &remote, &directory, block_bytes))
        .and_then(|()| read_references(shell, root, slot_suffix, &directory, block_bytes));
    let cleanup = shell.run(&root.command(&format!("rm -rf {}", quote(&remote))));
    let committed = outcome
        .and_then(|references| verify_kernel_images(&directory, profile).map(|()| references))
        .and_then(|references| {
            store.commit(
                request,
                PairKind::DeviceReadback,
                slot_suffix,
                None,
                references,
            )
        })
        .and_then(|set| set.avb.require_trust(profile).map(|()| set));
    if committed.is_err() {
        let _ = fs::remove_dir_all(&directory);
    }
    let set = committed?;
    cleanup?;
    Ok(set)
}

fn read_images(
    shell: &mut dyn DeviceShell,
    root: RootAccess,
    slot_suffix: &str,
    remote: &str,
    directory: &Path,
    block_bytes: u64,
) -> Result<()> {
    for (base, file) in [(BOOT_BASE, BOOT_FILE), (profile::VBMETA_BASE, VBMETA_FILE)] {
        let label = profile::slot_label(base, slot_suffix);
        let target = PartitionTarget::resolve(shell, root, &label, block_bytes)?;
        read_partition(
            shell,
            root,
            &target,
            &format!("{remote}/{file}"),
            &directory.join(file),
            block_bytes,
        )?;
    }
    Ok(())
}

fn read_partition(
    shell: &mut dyn DeviceShell,
    root: RootAccess,
    target: &PartitionTarget,
    remote_file: &str,
    local_file: &Path,
    block_bytes: u64,
) -> Result<()> {
    let before = target.sha256(shell, root)?;
    let copy = format!(
        "dd if={} of={} bs={} count={} && chmod 644 {}",
        quote(&target.by_name),
        quote(remote_file),
        block_bytes,
        target.bytes / block_bytes,
        quote(remote_file)
    );
    shell.run_checked(&root.command(&copy))?;
    let staged = device_sha256(shell, root, remote_file)?;
    shell.pull(remote_file, local_file)?;
    let (local_bytes, local) = sha256_file(local_file)?;
    let after = target.sha256(shell, root)?;
    if local_bytes != target.bytes || before != staged || staged != local || local != after {
        return Err(Front2Error::Verification(format!(
            "{}: partition {before} / staged copy {staged} / local copy {local} ({local_bytes} of {} bytes) / partition after {after}",
            target.label, target.bytes
        )));
    }
    Ok(())
}

fn read_references(
    shell: &mut dyn DeviceShell,
    root: RootAccess,
    slot_suffix: &str,
    directory: &Path,
    block_bytes: u64,
) -> Result<Vec<ReferencePartition>> {
    let descriptors = HashDescriptor::read_all(&directory.join(VBMETA_FILE))?;
    let mut names: Vec<String> = descriptors
        .iter()
        .map(|descriptor| descriptor.partition.clone())
        .collect();
    names.sort();
    if names != measured_hash_partitions() {
        return Err(Front2Error::Verification(format!(
            "top vbmeta carries hash descriptors for {names:?}, expected {:?}",
            measured_hash_partitions()
        )));
    }
    REFERENCE_BASES
        .iter()
        .map(|base| {
            let descriptor = HashDescriptor::find(&descriptors, base)?;
            ReferencePartition::read(shell, root, slot_suffix, descriptor, block_bytes)
        })
        .collect()
}
