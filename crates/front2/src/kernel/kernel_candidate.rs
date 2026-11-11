use crate::device_profile::DeviceProfile;
use crate::error::{Front2Error, Result};
use crate::kernel::boot_descriptors::require_unchanged_except_boot;
use crate::kernel::boot_image_header::BootImageHeader;
use crate::kernel::kernel_payload::candidate_payload_bytes;
use crate::kernel::kernel_set::{BOOT_BASE, BOOT_FILE, KernelSet, VBMETA_FILE};
use crate::kernel::kernel_store::KernelStore;
use crate::kernel::kernel_verification::verify_kernel_images;
use crate::pair_kind::PairKind;
use crate::pair_request::PairRequest;
use ltbox_patch::avb;
use std::fs;
use std::path::Path;

const STOCK_FOOTER_ALGORITHM: &str = "NONE";

pub fn build_kernel_candidate(
    store: &KernelStore,
    original_id: &str,
    new_boot: &Path,
    request: PairRequest<'_>,
) -> Result<KernelSet> {
    let original = store.load_verified(original_id)?;
    if original.kind != PairKind::DeviceReadback {
        return Err(Front2Error::Plan(format!(
            "kernel set {original_id} is {:?}, a candidate must start from a device readback",
            original.kind
        )));
    }
    let profile = store.profile();
    original.avb.require_trust(profile)?;
    let source = store.directory(original_id)?;
    let directory = store.begin(request.id)?;
    let built = sign(&source, new_boot, &directory, profile)
        .and_then(|()| {
            store.commit(
                request,
                PairKind::SignedCandidate,
                &original.slot_suffix,
                Some(original.id.clone()),
                original.references.clone(),
            )
        })
        .and_then(|candidate| verify(&original, candidate, &source, &directory, profile));
    if built.is_err() {
        let _ = fs::remove_dir_all(&directory);
    }
    built
}

fn sign(source: &Path, new_boot: &Path, directory: &Path, profile: &DeviceProfile) -> Result<()> {
    let original_boot = source.join(BOOT_FILE);
    let stock = avb::extract_image_avb_info(&original_boot)?;
    if stock.partition_name.as_deref() != Some(BOOT_BASE)
        || stock.algorithm != STOCK_FOOTER_ALGORITHM
        || stock.flags != 0
    {
        return Err(Front2Error::Image(format!(
            "boot footer names {:?} with algorithm {} and flags {}",
            stock.partition_name, stock.algorithm, stock.flags
        )));
    }
    let original_bytes = fs::read(&original_boot)?;
    let new_bytes = fs::read(new_boot)?;
    let maximum = avb::max_hash_footer_image_size(original_bytes.len() as u64)?;
    let payload = candidate_payload_bytes(&original_bytes, &new_bytes, maximum)?;
    let boot = directory.join(BOOT_FILE);
    fs::write(&boot, &new_bytes[..payload as usize])?;
    avb::add_hash_footer(&boot, &stock, None, Some(stock.rollback_index))?;
    avb::rebuild_root_vbmeta_owner(
        &directory.join(VBMETA_FILE),
        &source.join(VBMETA_FILE),
        &boot,
        BOOT_BASE,
        profile.top_vbmeta_key_spec,
    )?;
    Ok(())
}

fn verify(
    original: &KernelSet,
    candidate: KernelSet,
    source: &Path,
    directory: &Path,
    profile: &DeviceProfile,
) -> Result<KernelSet> {
    let failure = |reason: String| Front2Error::Verification(format!("candidate: {reason}"));
    if candidate.boot.bytes != original.boot.bytes
        || candidate.vbmeta.bytes != original.vbmeta.bytes
    {
        return Err(failure(format!(
            "image sizes {} and {} differ from the partition sizes {} and {}",
            candidate.boot.bytes,
            candidate.vbmeta.bytes,
            original.boot.bytes,
            original.vbmeta.bytes
        )));
    }
    candidate.avb.require_trust(profile)?;
    let unchanged = candidate.avb.top_key_sha1 == original.avb.top_key_sha1
        && candidate.avb.top_rollback_index == original.avb.top_rollback_index
        && candidate.avb.top_rollback_index_location == original.avb.top_rollback_index_location
        && candidate.avb.top_flags == original.avb.top_flags
        && candidate.avb.top_chain_partitions == original.avb.top_chain_partitions
        && candidate.avb.top_hash_partitions == original.avb.top_hash_partitions
        && candidate.avb.boot_footer_algorithm == original.avb.boot_footer_algorithm
        && candidate.avb.boot_footer_rollback_index == original.avb.boot_footer_rollback_index;
    if !unchanged {
        return Err(failure(
            "AVB metadata other than the boot digest changed".into(),
        ));
    }
    if candidate.avb.boot_digest_in_top == original.avb.boot_digest_in_top {
        return Err(failure(
            "boot digest is the same as in the original set".into(),
        ));
    }
    let expected_header = BootImageHeader {
        kernel_size: candidate.boot_header.kernel_size,
        ..original.boot_header
    };
    if candidate.boot_header != expected_header || candidate.kernel.kmi != original.kernel.kmi {
        return Err(failure(format!(
            "boot header {:?} with KMI {} does not continue {:?} with KMI {}",
            candidate.boot_header, candidate.kernel.kmi, original.boot_header, original.kernel.kmi
        )));
    }
    require_unchanged_except_boot(&directory.join(VBMETA_FILE), &source.join(VBMETA_FILE))?;
    verify_kernel_images(directory, profile)?;
    Ok(candidate)
}
