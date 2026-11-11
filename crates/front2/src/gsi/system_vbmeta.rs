use crate::error::{Front2Error, Result};
use crate::gsi::raw_descriptor::RawDescriptor;
use crate::gsi::signed_vbmeta::SignedVbmeta;
use crate::gsi::vbmeta_parameters::VbmetaParameters;
use crate::gsi::vbmeta_source::VbmetaSource;
use crate::gsi::vbmeta_template::VbmetaTemplate;
use crate::profiles::toss_front2;
use avbtool_rs::info::DescriptorInfo;
use std::path::Path;

pub fn build_system_vbmeta(
    template: &VbmetaTemplate<'_>,
    gsi: &Path,
    output: &Path,
) -> Result<SignedVbmeta> {
    let stock = VbmetaSource::read(template.stock_slot_a)?;
    let original = VbmetaSource::read(template.original_slot_b)?;
    let gsi_source = VbmetaSource::read(gsi)?;
    original.require_signed_with(
        toss_front2::CHAINED_VBMETA_KEY_SPEC,
        toss_front2::CHAINED_VBMETA_ALGORITHM,
    )?;
    original.require_no_public_key_metadata()?;
    require_same_rollback(&stock, &original)?;

    let mut descriptors: Vec<RawDescriptor> = stock
        .descriptors
        .iter()
        .filter(|descriptor| is_retained_product(&descriptor.info))
        .cloned()
        .collect();
    require_one_hashtree(
        &descriptors,
        toss_front2::RETAINED_PRODUCT_PARTITION,
        &stock.path,
    )?;
    require_only_hashtree_and_properties(&gsi_source)?;
    require_one_hashtree(
        &gsi_source.descriptors,
        toss_front2::GSI_SYSTEM_PARTITION,
        &gsi_source.path,
    )?;
    descriptors.extend(gsi_source.descriptors.iter().cloned());

    let required_minor = [&stock, &original, &gsi_source]
        .iter()
        .map(|source| source.info.header.required_libavb_version_minor)
        .max()
        .unwrap_or(0);
    let parameters = VbmetaParameters {
        algorithm_name: toss_front2::CHAINED_VBMETA_ALGORITHM.to_string(),
        key_spec: toss_front2::CHAINED_VBMETA_KEY_SPEC.to_string(),
        rollback_index: original.info.header.rollback_index,
        rollback_index_location: original.info.header.rollback_index_location,
        flags: original.info.header.flags,
        required_libavb_version_minor: required_minor,
        release_string: toss_front2::GSI_VBMETA_RELEASE_STRING.to_string(),
    };
    SignedVbmeta::write(output, original.file_bytes, &parameters, &descriptors)
}

fn is_retained_product(descriptor: &DescriptorInfo) -> bool {
    match descriptor {
        DescriptorInfo::Hashtree { partition_name, .. } => {
            partition_name == toss_front2::RETAINED_PRODUCT_PARTITION
        }
        DescriptorInfo::Property { key, .. } => {
            key.starts_with(toss_front2::RETAINED_PRODUCT_PROPERTY_PREFIX)
        }
        _ => false,
    }
}

fn require_same_rollback(stock: &VbmetaSource, original: &VbmetaSource) -> Result<()> {
    let stock_header = &stock.info.header;
    let original_header = &original.info.header;
    if stock_header.rollback_index != original_header.rollback_index
        || stock_header.rollback_index_location != original_header.rollback_index_location
    {
        return Err(Front2Error::Unsupported(format!(
            "vbmeta_system rollback index is {} at location {} on slot A and {} at location {} on slot B; no tested rule selects between them",
            stock_header.rollback_index,
            stock_header.rollback_index_location,
            original_header.rollback_index,
            original_header.rollback_index_location
        )));
    }
    if stock_header.flags != 0 || original_header.flags != 0 {
        return Err(Front2Error::Unsupported(format!(
            "vbmeta_system flags are {} on slot A and {} on slot B, only 0 is supported",
            stock_header.flags, original_header.flags
        )));
    }
    Ok(())
}

fn require_only_hashtree_and_properties(gsi: &VbmetaSource) -> Result<()> {
    let supported = gsi.descriptors.iter().all(|descriptor| {
        matches!(
            descriptor.info,
            DescriptorInfo::Hashtree { .. } | DescriptorInfo::Property { .. }
        )
    });
    if !supported {
        return Err(Front2Error::Unsupported(format!(
            "{} has a descriptor that is not a hashtree or a property",
            gsi.path.display()
        )));
    }
    Ok(())
}

fn require_one_hashtree(descriptors: &[RawDescriptor], partition: &str, path: &Path) -> Result<()> {
    let names: Vec<&str> = descriptors
        .iter()
        .filter_map(|descriptor| match &descriptor.info {
            DescriptorInfo::Hashtree { partition_name, .. } => Some(partition_name.as_str()),
            _ => None,
        })
        .collect();
    if names != [partition] {
        return Err(Front2Error::Unsupported(format!(
            "{} must give one hashtree for {partition}, it gives {names:?}",
            path.display()
        )));
    }
    Ok(())
}
