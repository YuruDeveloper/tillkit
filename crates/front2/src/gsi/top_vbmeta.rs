use crate::error::{Front2Error, Result};
use crate::gsi::raw_descriptor::RawDescriptor;
use crate::gsi::rollback_evidence::RollbackEvidence;
use crate::gsi::signed_vbmeta::SignedVbmeta;
use crate::gsi::vbmeta_parameters::VbmetaParameters;
use crate::gsi::vbmeta_source::VbmetaSource;
use crate::gsi::vbmeta_template::VbmetaTemplate;
use crate::profiles::toss_front2;
use avbtool_rs::crypto::extract_public_key;
use avbtool_rs::info::DescriptorInfo;
use std::path::Path;

pub fn build_top_vbmeta(
    template: &VbmetaTemplate<'_>,
    evidence: &[RollbackEvidence],
    output: &Path,
) -> Result<SignedVbmeta> {
    let stock = VbmetaSource::read(template.stock_slot_a)?;
    let original = VbmetaSource::read(template.original_slot_b)?;
    original.require_signed_with(
        toss_front2::TOP_VBMETA_KEY_SPEC,
        toss_front2::TOP_VBMETA_ALGORITHM,
    )?;
    original.require_no_public_key_metadata()?;
    require_supported_headers(&stock, &original)?;
    let rollback_index = stock.info.header.rollback_index;
    require_no_higher_evidence(rollback_index, &original, evidence)?;

    let chained_key =
        extract_public_key(toss_front2::CHAINED_VBMETA_KEY_SPEC).map_err(|error| {
            Front2Error::Image(format!(
                "key {}: {error}",
                toss_front2::CHAINED_VBMETA_KEY_SPEC
            ))
        })?;
    let mut chains = 0usize;
    let mut descriptors = Vec::with_capacity(stock.descriptors.len());
    for descriptor in &stock.descriptors {
        let DescriptorInfo::ChainPartition {
            rollback_index_location,
            partition_name,
            flags,
            ..
        } = &descriptor.info
        else {
            descriptors.push(descriptor.clone());
            continue;
        };
        require_expected_chain(partition_name, *rollback_index_location, &stock.path)?;
        chains += 1;
        descriptors.push(RawDescriptor::chain_partition(
            partition_name,
            *rollback_index_location,
            &chained_key,
            *flags,
        ));
    }
    if chains != 1 {
        return Err(Front2Error::Unsupported(format!(
            "{} has {chains} chain descriptors, one is necessary",
            stock.path.display()
        )));
    }

    let parameters = VbmetaParameters {
        algorithm_name: toss_front2::TOP_VBMETA_ALGORITHM.to_string(),
        key_spec: toss_front2::TOP_VBMETA_KEY_SPEC.to_string(),
        rollback_index,
        rollback_index_location: original.info.header.rollback_index_location,
        flags: original.info.header.flags,
        required_libavb_version_minor: stock
            .info
            .header
            .required_libavb_version_minor
            .max(original.info.header.required_libavb_version_minor),
        release_string: toss_front2::GSI_VBMETA_RELEASE_STRING.to_string(),
    };
    SignedVbmeta::write(output, original.file_bytes, &parameters, &descriptors)
}

fn require_supported_headers(stock: &VbmetaSource, original: &VbmetaSource) -> Result<()> {
    let stock_header = &stock.info.header;
    let original_header = &original.info.header;
    if stock_header.rollback_index_location != original_header.rollback_index_location {
        return Err(Front2Error::Unsupported(format!(
            "top vbmeta rollback index location is {} on slot A and {} on slot B",
            stock_header.rollback_index_location, original_header.rollback_index_location
        )));
    }
    if stock_header.flags != 0 || original_header.flags != 0 {
        return Err(Front2Error::Unsupported(format!(
            "top vbmeta flags are {} on slot A and {} on slot B, only 0 is supported",
            stock_header.flags, original_header.flags
        )));
    }
    Ok(())
}

fn require_no_higher_evidence(
    selected: u64,
    original: &VbmetaSource,
    evidence: &[RollbackEvidence],
) -> Result<()> {
    let original_value = original.info.header.rollback_index;
    if original_value > selected {
        return Err(Front2Error::Unsupported(format!(
            "slot B top vbmeta has rollback index {original_value}, slot A has {selected}; the tested conversion used the slot A value and it must not be lower"
        )));
    }
    if let Some(higher) = evidence.iter().find(|item| item.value > selected) {
        return Err(Front2Error::Unsupported(format!(
            "{} gives rollback index {}, slot A top vbmeta gives {selected}; the values do not agree",
            higher.source, higher.value
        )));
    }
    Ok(())
}

fn require_expected_chain(partition_name: &str, location: u32, path: &Path) -> Result<()> {
    if partition_name != toss_front2::CHAINED_VBMETA_BASE
        || location != toss_front2::CHAINED_ROLLBACK_INDEX_LOCATION
    {
        return Err(Front2Error::Unsupported(format!(
            "{} chains {partition_name} at rollback index location {location}, only {} at location {} is supported",
            path.display(),
            toss_front2::CHAINED_VBMETA_BASE,
            toss_front2::CHAINED_ROLLBACK_INDEX_LOCATION
        )));
    }
    Ok(())
}
