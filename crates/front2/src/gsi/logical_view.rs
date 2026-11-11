use crate::digest::hex;
use crate::error::{Front2Error, Result};
use crate::gsi::view_verification::view_file;
use crate::image_record::ImageRecord;
use crate::partition_target::is_safe_label;
use crate::profiles::toss_front2;
use crate::super_image::file_ranges::copy_exact;
use crate::super_image::{ByteRange, LpGeometry, LpMetadata, LpPartition, TARGET_SLOT};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom};
use std::path::Path;

pub fn extract_logical_view(super_image: &Path, directory: &Path) -> Result<Vec<ImageRecord>> {
    let mut source = File::open(super_image)?;
    let geometry = LpGeometry::read(&mut source)?;
    let metadata = LpMetadata::read_slot(&mut source, &geometry, TARGET_SLOT)?;
    let mut records = Vec::new();
    for partition in &metadata.partitions {
        records.push(extract(&mut source, &metadata, partition, directory)?);
    }
    Ok(records)
}

fn extract(
    source: &mut File,
    metadata: &LpMetadata,
    partition: &LpPartition,
    directory: &Path,
) -> Result<ImageRecord> {
    let name = partition.name.as_str();
    let base = name
        .strip_suffix(toss_front2::SUPPORTED_SLOT_SUFFIX)
        .filter(|base| is_safe_label(base))
        .ok_or_else(|| {
            Front2Error::Image(format!(
                "super metadata: partition {name:?} is not a plain slot B name"
            ))
        })?;
    let file = view_file(base);
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(&file))?;
    let mut hasher = Sha256::new();
    let mut bytes = 0u64;
    for extent in metadata.extents_of(partition)? {
        if !extent.is_linear_on_first_device() {
            return Err(Front2Error::Image(format!(
                "super metadata: partition {name} has an extent that is not linear"
            )));
        }
        let range = ByteRange::from_sectors(extent.target_sector, extent.sectors)?;
        source.seek(SeekFrom::Start(range.start))?;
        copy_exact(source, &mut output, range.bytes(), &mut hasher)?;
        bytes += range.bytes();
    }
    output.sync_all()?;
    Ok(ImageRecord {
        file,
        bytes,
        sha256: hex(&hasher.finalize()),
    })
}
