use super::init_boot_patch::patch_init_boot;
use super::patch_inputs::PatchInputs;
use crate::candidate::build_signed_candidate;
use crate::error::{Front2Error, Result};
use crate::pair_manifest::{INIT_BOOT_FILE, PairManifest};
use crate::pair_request::PairRequest;
use crate::pair_store::PairStore;
use crate::root_access::RootAccess;

pub fn build_magisk_candidate(
    store: &PairStore,
    original_id: &str,
    root_access: Option<RootAccess>,
    inputs: PatchInputs<'_>,
    request: PairRequest<'_>,
    log: &mut Vec<String>,
) -> Result<PairManifest> {
    if root_access.is_none() {
        return Err(Front2Error::Unsupported(
            "root access is not available, and the pair write needs it".into(),
        ));
    }
    store.load_verified(original_id)?;
    let stock = store.directory(original_id)?.join(INIT_BOOT_FILE);
    let patched = patch_init_boot(&stock, inputs, log)?;
    build_signed_candidate(store, original_id, &patched, request)
}
