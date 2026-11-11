use crate::error::{Front2Error, Result};
use avbtool_rs::crypto::{
    AvbPublicKey, compute_hash_for_algorithm, extract_public_key, lookup_algorithm_by_type,
};
use std::path::Path;

const VBMETA_HEADER_BYTES: usize = 256;

pub fn verify_top_signature(vbmeta: &Path, expected_key_spec: &str) -> Result<()> {
    let failure = |reason: &str| {
        Front2Error::Verification(format!("{} signature: {reason}", vbmeta.display()))
    };
    let engine = |error: &dyn std::fmt::Display| failure(&error.to_string());
    let info = avbtool_rs::image::inspect_avb_image(vbmeta).map_err(|error| engine(&error))?;
    let blob = avbtool_rs::image::load_vbmeta_blob(vbmeta).map_err(|error| engine(&error))?;
    let header = &info.header;
    let algorithm =
        lookup_algorithm_by_type(header.algorithm_type).map_err(|error| engine(&error))?;
    let authentication_end = VBMETA_HEADER_BYTES + header.authentication_data_block_size as usize;
    let auxiliary_end = authentication_end + header.auxiliary_data_block_size as usize;
    let authentication = blob
        .get(VBMETA_HEADER_BYTES..authentication_end)
        .ok_or_else(|| failure("authentication block is truncated"))?;
    let auxiliary = blob
        .get(authentication_end..auxiliary_end)
        .ok_or_else(|| failure("auxiliary block is truncated"))?;
    let recorded_digest = slice(authentication, header.hash_offset, header.hash_size)
        .ok_or_else(|| failure("digest is outside the authentication block"))?;
    let signature = slice(
        authentication,
        header.signature_offset,
        header.signature_size,
    )
    .ok_or_else(|| failure("signature is outside the authentication block"))?;
    let embedded_key = slice(auxiliary, header.public_key_offset, header.public_key_size)
        .ok_or_else(|| failure("public key is outside the auxiliary block"))?;

    let expected_key = extract_public_key(expected_key_spec).map_err(|error| engine(&error))?;
    if embedded_key != expected_key.as_slice() {
        return Err(failure("embedded public key is not the expected key"));
    }
    let signed_data = [&blob[..VBMETA_HEADER_BYTES], auxiliary].concat();
    let digest =
        compute_hash_for_algorithm(algorithm, &signed_data).map_err(|error| engine(&error))?;
    if digest.as_slice() != recorded_digest {
        return Err(failure("digest does not match the signed data"));
    }
    let public_key = AvbPublicKey::decode(embedded_key).map_err(|error| engine(&error))?;
    let verified = public_key
        .verify(algorithm, signature, &signed_data)
        .map_err(|error| engine(&error))?;
    if !verified {
        return Err(failure("RSA verification rejected the signature"));
    }
    Ok(())
}

fn slice(block: &[u8], offset: u64, size: u64) -> Option<&[u8]> {
    let start = usize::try_from(offset).ok()?;
    let end = start.checked_add(usize::try_from(size).ok()?)?;
    block.get(start..end)
}
