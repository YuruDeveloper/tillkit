use crate::error::{Front2Error, Result};
use crate::gsi::raw_descriptor::RawDescriptor;
use crate::gsi::vbmeta_parameters::VbmetaParameters;
use avbtool_rs::crypto::{
    compute_hash_for_algorithm, load_key_from_spec, lookup_algorithm_by_name, round_to_multiple,
};
use avbtool_rs::image::{default_vbmeta_header, encode_header};

const BLOCK_ALIGNMENT: u64 = 64;

pub fn build_signed_blob(
    parameters: &VbmetaParameters,
    descriptors: &[RawDescriptor],
) -> Result<Vec<u8>> {
    let engine = |error: &dyn std::fmt::Display| Front2Error::Image(format!("vbmeta: {error}"));
    let algorithm =
        lookup_algorithm_by_name(&parameters.algorithm_name).map_err(|error| engine(&error))?;
    let key = load_key_from_spec(&parameters.key_spec).map_err(|error| engine(&error))?;
    let public_key = key.encode_public_key();
    if algorithm.signature_num_bytes == 0 || public_key.len() != algorithm.public_key_num_bytes {
        return Err(Front2Error::Image(format!(
            "key {} does not fit algorithm {}",
            parameters.key_spec, parameters.algorithm_name
        )));
    }
    let encoded: Vec<u8> = descriptors
        .iter()
        .flat_map(|descriptor| descriptor.bytes.iter().copied())
        .collect();
    let mut auxiliary = encoded.clone();
    auxiliary.extend_from_slice(&public_key);
    auxiliary.resize(
        round_to_multiple(auxiliary.len() as u64, BLOCK_ALIGNMENT) as usize,
        0,
    );
    let authentication_bytes = round_to_multiple(
        (algorithm.hash_num_bytes + algorithm.signature_num_bytes) as u64,
        BLOCK_ALIGNMENT,
    );

    let mut header = default_vbmeta_header();
    header.required_libavb_version_minor = parameters.required_libavb_version_minor;
    header.authentication_data_block_size = authentication_bytes;
    header.auxiliary_data_block_size = auxiliary.len() as u64;
    header.algorithm_type = algorithm.algorithm_type;
    header.hash_size = algorithm.hash_num_bytes as u64;
    header.signature_offset = algorithm.hash_num_bytes as u64;
    header.signature_size = algorithm.signature_num_bytes as u64;
    header.public_key_offset = encoded.len() as u64;
    header.public_key_size = public_key.len() as u64;
    header.public_key_metadata_offset = (encoded.len() + public_key.len()) as u64;
    header.descriptors_size = encoded.len() as u64;
    header.rollback_index = parameters.rollback_index;
    header.flags = parameters.flags;
    header.rollback_index_location = parameters.rollback_index_location;
    header.release_string = parameters.release_string.clone();

    let mut blob = encode_header(&header);
    let signed_data = [blob.as_slice(), auxiliary.as_slice()].concat();
    let digest =
        compute_hash_for_algorithm(algorithm, &signed_data).map_err(|error| engine(&error))?;
    let signature = key
        .sign(&signed_data, algorithm.name)
        .map_err(|error| engine(&error))?;
    let mut authentication = digest;
    authentication.extend_from_slice(&signature);
    authentication.resize(authentication_bytes as usize, 0);
    blob.extend_from_slice(&authentication);
    blob.extend_from_slice(&auxiliary);
    Ok(blob)
}
