use crate::error::{Front2Error, Result};
use crate::gsi::raw_descriptor::RawDescriptor;
use crate::top_signature::verify_top_signature;
use avbtool_rs::crypto::extract_public_key;
use avbtool_rs::info::AvbImageInfo;
use std::path::{Path, PathBuf};

const VBMETA_HEADER_BYTES: u64 = 256;

#[derive(Debug, Clone)]
pub struct VbmetaSource {
    pub path: PathBuf,
    pub info: AvbImageInfo,
    pub descriptors: Vec<RawDescriptor>,
    pub public_key: Vec<u8>,
    pub file_bytes: u64,
}

impl VbmetaSource {
    pub fn read(path: &Path) -> Result<Self> {
        let failure = |reason: String| Front2Error::Image(format!("{}: {reason}", path.display()));
        let info = avbtool_rs::image::inspect_avb_image(path)
            .map_err(|error| failure(error.to_string()))?;
        let blob = avbtool_rs::image::load_vbmeta_blob(path)
            .map_err(|error| failure(error.to_string()))?;
        let header = &info.header;
        let key_start =
            VBMETA_HEADER_BYTES + header.authentication_data_block_size + header.public_key_offset;
        let public_key = usize::try_from(key_start)
            .ok()
            .zip(usize::try_from(header.public_key_size).ok())
            .and_then(|(start, size)| blob.get(start..start.checked_add(size)?))
            .ok_or_else(|| failure("the public key is outside the vbmeta blob".into()))?
            .to_vec();
        Ok(Self {
            path: path.to_path_buf(),
            descriptors: RawDescriptor::read_all(path)?,
            public_key,
            file_bytes: std::fs::metadata(path)?.len(),
            info,
        })
    }

    pub fn require_signed_with(&self, key_spec: &str, algorithm_name: &str) -> Result<()> {
        let expected = extract_public_key(key_spec)
            .map_err(|error| Front2Error::Image(format!("key {key_spec}: {error}")))?;
        if self.public_key != expected || self.info.algorithm_name != algorithm_name {
            return Err(Front2Error::Unsupported(format!(
                "{} is not signed with {key_spec} and {algorithm_name}; its algorithm is {}",
                self.path.display(),
                self.info.algorithm_name
            )));
        }
        verify_top_signature(&self.path, key_spec)
    }

    pub fn require_no_public_key_metadata(&self) -> Result<()> {
        if self.info.header.public_key_metadata_size != 0 {
            return Err(Front2Error::Unsupported(format!(
                "{} has public key metadata",
                self.path.display()
            )));
        }
        Ok(())
    }
}
