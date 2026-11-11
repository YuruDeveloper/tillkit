use crate::error::{Front2Error, Result};
use crate::profile;
use avbtool_rs::crypto::extract_public_key;
use avbtool_rs::verify::{ExpectedChainPartition, VerifyImageOptions, verify_image};
use std::path::Path;

pub const VIEW_IMAGE_EXTENSION: &str = "img";

pub fn view_file(partition_base: &str) -> String {
    format!("{partition_base}.{VIEW_IMAGE_EXTENSION}")
}

pub fn verify_view(directory: &Path) -> Result<Vec<String>> {
    let failure =
        |reason: String| Front2Error::Verification(format!("{}: {reason}", directory.display()));
    let key = |key_spec: &str| {
        extract_public_key(key_spec).map_err(|error| failure(format!("key {key_spec}: {error}")))
    };
    let top_key = key(profile::TOP_VBMETA_KEY_SPEC)?;
    let chained_key = key(profile::CHAINED_VBMETA_KEY_SPEC)?;
    let top = verify_image(
        &directory.join(view_file(profile::VBMETA_BASE)),
        &VerifyImageOptions {
            key_blob: Some(top_key),
            expected_chain_partitions: vec![ExpectedChainPartition {
                partition_name: profile::CHAINED_VBMETA_BASE.to_string(),
                rollback_index_location: profile::CHAINED_ROLLBACK_INDEX_LOCATION,
                public_key: chained_key.clone(),
            }],
            follow_chain_partitions: false,
            accept_zeroed_hashtree: false,
        },
    )
    .map_err(|error| failure(error.to_string()))?;
    let chained = verify_image(
        &directory.join(view_file(profile::CHAINED_VBMETA_BASE)),
        &VerifyImageOptions {
            key_blob: Some(chained_key),
            expected_chain_partitions: Vec::new(),
            follow_chain_partitions: false,
            accept_zeroed_hashtree: false,
        },
    )
    .map_err(|error| failure(error.to_string()))?;
    Ok([top.messages, chained.messages].concat())
}
