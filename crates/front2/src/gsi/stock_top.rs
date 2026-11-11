use crate::error::{Front2Error, Result};
use crate::gsi::vbmeta_source::VbmetaSource;
use crate::profiles::toss_front2;
use avbtool_rs::info::DescriptorInfo;

pub fn require_stock_front2_top(stock: &VbmetaSource) -> Result<()> {
    let failure =
        |reason: String| Front2Error::Unsupported(format!("{}: {reason}", stock.path.display()));
    let header = &stock.info.header;
    if stock.info.algorithm_name != toss_front2::TOP_VBMETA_ALGORITHM
        || header.flags != 0
        || header.rollback_index_location != 0
    {
        return Err(failure(format!(
            "algorithm {}, flags {} and rollback index location {} are not {}, 0 and 0",
            stock.info.algorithm_name,
            header.flags,
            header.rollback_index_location,
            toss_front2::TOP_VBMETA_ALGORITHM
        )));
    }
    let mut hashes: Vec<&str> = stock
        .descriptors
        .iter()
        .filter_map(|descriptor| hash_partition(&descriptor.info))
        .collect();
    hashes.sort_unstable();
    if hashes != toss_front2::PHYSICAL_BOOT_PARTITIONS {
        return Err(failure(format!(
            "hash descriptors are {hashes:?}, expected {:?}",
            toss_front2::PHYSICAL_BOOT_PARTITIONS
        )));
    }
    let fingerprints: Vec<&[u8]> = stock
        .descriptors
        .iter()
        .filter_map(|descriptor| boot_fingerprint(&descriptor.info))
        .collect();
    if fingerprints != [toss_front2::VENDOR_BUILD_FINGERPRINT.as_bytes()] {
        return Err(failure(format!(
            "property {} is not exactly one {}",
            toss_front2::BOOT_FINGERPRINT_PROPERTY,
            toss_front2::VENDOR_BUILD_FINGERPRINT
        )));
    }
    Ok(())
}

fn hash_partition(descriptor: &DescriptorInfo) -> Option<&str> {
    match descriptor {
        DescriptorInfo::Hash { partition_name, .. } => Some(partition_name.as_str()),
        _ => None,
    }
}

fn boot_fingerprint(descriptor: &DescriptorInfo) -> Option<&[u8]> {
    match descriptor {
        DescriptorInfo::Property { key, value }
            if key == toss_front2::BOOT_FINGERPRINT_PROPERTY =>
        {
            Some(value.as_slice())
        }
        _ => None,
    }
}
