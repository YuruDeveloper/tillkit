use crate::device_profile::DeviceProfile;
use crate::error::{Front2Error, Result};
use crate::init_boot_payload::payload_bytes;
use crate::pair_kind::PairKind;
use crate::pair_manifest::{INIT_BOOT_FILE, PairManifest, VBMETA_FILE};
use crate::pair_request::PairRequest;
use crate::pair_store::PairStore;
use crate::pair_verification::verify_pair_images;
use crate::profile;
use crate::top_descriptors::require_unchanged_except_init_boot;
use ltbox_patch::avb;
use std::fs;
use std::path::Path;

pub fn build_signed_candidate(
    store: &PairStore,
    original_id: &str,
    patched_init_boot: &Path,
    request: PairRequest<'_>,
) -> Result<PairManifest> {
    let original = store.load_verified(original_id)?;
    if original.kind != PairKind::DeviceReadback {
        return Err(Front2Error::Plan(format!(
            "pair {original_id} is {:?}, a candidate must start from a device readback",
            original.kind
        )));
    }
    let profile = store.profile();
    original.avb.require_trust(profile)?;
    let source = store.directory(original_id)?;
    let directory = store.begin(request.id)?;
    let built = sign(&source, patched_init_boot, &directory, profile)
        .and_then(|()| {
            store.commit(
                request.id,
                PairKind::SignedCandidate,
                request.created_unix,
                &original.slot_suffix,
                Some(original.id.clone()),
                request.note,
            )
        })
        .and_then(|candidate| verify(&original, candidate, &source, &directory, profile));
    if built.is_err() {
        let _ = fs::remove_dir_all(&directory);
    }
    built
}

fn sign(
    source: &Path,
    patched_init_boot: &Path,
    directory: &Path,
    device: &DeviceProfile,
) -> Result<()> {
    let original_init_boot = source.join(INIT_BOOT_FILE);
    let original_vbmeta = source.join(VBMETA_FILE);
    let stock = avb::extract_image_avb_info(&original_init_boot)?;
    if stock.partition_name.as_deref() != Some(profile::INIT_BOOT_BASE)
        || stock.algorithm != "NONE"
        || stock.flags != 0
    {
        return Err(Front2Error::Image(format!(
            "init_boot footer names {:?} with algorithm {} and flags {}",
            stock.partition_name, stock.algorithm, stock.flags
        )));
    }
    let original_bytes = fs::read(&original_init_boot)?;
    let patched_bytes = fs::read(patched_init_boot)?;
    let maximum = avb::max_hash_footer_image_size(original_bytes.len() as u64)?;
    let payload = payload_bytes(&original_bytes, &patched_bytes, maximum)?;
    let init_boot = directory.join(INIT_BOOT_FILE);
    fs::write(&init_boot, &patched_bytes[..payload as usize])?;
    avb::add_hash_footer(&init_boot, &stock, None, Some(stock.rollback_index))?;
    avb::rebuild_root_vbmeta_owner(
        &directory.join(VBMETA_FILE),
        &original_vbmeta,
        &init_boot,
        profile::INIT_BOOT_BASE,
        device.top_vbmeta_key_spec,
    )?;
    Ok(())
}

fn verify(
    original: &PairManifest,
    candidate: PairManifest,
    source: &Path,
    directory: &Path,
    device: &DeviceProfile,
) -> Result<PairManifest> {
    let failure = |reason: String| Front2Error::Verification(format!("candidate: {reason}"));
    if candidate.init_boot.bytes != original.init_boot.bytes
        || candidate.vbmeta.bytes != original.vbmeta.bytes
    {
        return Err(failure(format!(
            "image sizes {} and {} differ from the partition sizes {} and {}",
            candidate.init_boot.bytes,
            candidate.vbmeta.bytes,
            original.init_boot.bytes,
            original.vbmeta.bytes
        )));
    }
    candidate.avb.require_trust(device)?;
    let unchanged = candidate.avb.top_key_sha1 == original.avb.top_key_sha1
        && candidate.avb.top_rollback_index == original.avb.top_rollback_index
        && candidate.avb.top_rollback_index_location == original.avb.top_rollback_index_location
        && candidate.avb.top_flags == original.avb.top_flags
        && candidate.avb.top_chain_partitions == original.avb.top_chain_partitions
        && candidate.avb.init_boot_footer_rollback_index
            == original.avb.init_boot_footer_rollback_index
        && sorted(&candidate.avb.top_hash_partitions) == sorted(&original.avb.top_hash_partitions);
    if !unchanged {
        return Err(failure(
            "top vbmeta metadata other than the init_boot digest changed".into(),
        ));
    }
    if candidate.avb.init_boot_digest_in_top == original.avb.init_boot_digest_in_top {
        return Err(failure(
            "init_boot digest is the same as in the original pair".into(),
        ));
    }
    require_unchanged_except_init_boot(&directory.join(VBMETA_FILE), &source.join(VBMETA_FILE))?;
    verify_pair_images(directory, device)?;
    Ok(candidate)
}

fn sorted(names: &[String]) -> Vec<&String> {
    let mut sorted: Vec<&String> = names.iter().collect();
    sorted.sort();
    sorted
}
