use super::init_boot_patch::patch_init_boot;
use super::magisk_package::MagiskPackage;
use crate::candidate::build_signed_candidate;
use crate::error::Result;
use crate::pair_manifest::{INIT_BOOT_FILE, PairManifest};
use crate::pair_request::PairRequest;
use crate::pair_store::PairStore;
use std::path::Path;

pub fn build_magisk_candidate(
    store: &PairStore,
    original_id: &str,
    package: &MagiskPackage,
    preinit_device: &str,
    work_dir: &Path,
    request: PairRequest<'_>,
    log: &mut Vec<String>,
) -> Result<PairManifest> {
    store.load_verified(original_id)?;
    let stock = store.directory(original_id)?.join(INIT_BOOT_FILE);
    let patched = patch_init_boot(&stock, package, preinit_device, work_dir, log)?;
    build_signed_candidate(store, original_id, &patched, request)
}
