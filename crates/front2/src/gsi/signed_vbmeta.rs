use crate::error::{Front2Error, Result};
use crate::gsi::raw_descriptor::RawDescriptor;
use crate::gsi::vbmeta_blob::build_signed_blob;
use crate::gsi::vbmeta_parameters::VbmetaParameters;
use crate::gsi::vbmeta_source::VbmetaSource;
use crate::image_record::ImageRecord;
use crate::top_signature::verify_top_signature;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedVbmeta {
    pub image: ImageRecord,
    pub algorithm: String,
    pub key_spec: String,
    pub rollback_index: u64,
    pub rollback_index_location: u32,
    pub flags: u32,
    pub required_libavb_version_minor: u32,
    pub descriptors: usize,
}

impl SignedVbmeta {
    pub fn write(
        output: &Path,
        partition_bytes: u64,
        parameters: &VbmetaParameters,
        descriptors: &[RawDescriptor],
    ) -> Result<Self> {
        let failure =
            |reason: String| Front2Error::Verification(format!("{}: {reason}", output.display()));
        let mut blob = build_signed_blob(parameters, descriptors)?;
        if blob.len() as u64 > partition_bytes {
            return Err(Front2Error::Image(format!(
                "{} needs {} bytes, the partition has {partition_bytes}",
                output.display(),
                blob.len()
            )));
        }
        blob.resize(partition_bytes as usize, 0);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?;
        file.write_all(&blob)?;
        file.sync_all()?;
        drop(file);

        let written = VbmetaSource::read(output)?;
        if written.descriptors != descriptors {
            return Err(failure(
                "the descriptors differ from the descriptors that were signed".into(),
            ));
        }
        let header = &written.info.header;
        let kept = header.rollback_index == parameters.rollback_index
            && header.rollback_index_location == parameters.rollback_index_location
            && header.flags == parameters.flags
            && header.required_libavb_version_minor == parameters.required_libavb_version_minor
            && written.info.algorithm_name == parameters.algorithm_name;
        if !kept {
            return Err(failure("the header differs from the parameters".into()));
        }
        verify_top_signature(output, &parameters.key_spec)?;
        let directory = output
            .parent()
            .ok_or_else(|| failure("the output has no parent directory".into()))?;
        let file_name = output
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| failure("the output has no file name".into()))?;
        Ok(Self {
            image: ImageRecord::from_file(directory, file_name)?,
            algorithm: parameters.algorithm_name.clone(),
            key_spec: parameters.key_spec.clone(),
            rollback_index: parameters.rollback_index,
            rollback_index_location: parameters.rollback_index_location,
            flags: parameters.flags,
            required_libavb_version_minor: parameters.required_libavb_version_minor,
            descriptors: descriptors.len(),
        })
    }
}
