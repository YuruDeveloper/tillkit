use crate::digest::{hex, sha256_file};
use crate::error::{Front2Error, Result};
use crate::image_record::ImageRecord;
use crate::super_image::file_ranges::copy_all;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupPartition {
    pub name: String,
    pub lun: u8,
    #[serde(skip)]
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
}

impl BackupPartition {
    pub fn verify(&self) -> Result<()> {
        let (bytes, sha256) = sha256_file(&self.path)?;
        self.require_recorded(bytes, &sha256)
    }

    pub fn copy_to(&self, directory: &Path, file: &str) -> Result<ImageRecord> {
        let mut source = File::open(&self.path)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(file))?;
        let mut hasher = Sha256::new();
        let bytes = copy_all(&mut source, &mut output, &mut hasher)?;
        output.sync_all()?;
        let sha256 = hex(&hasher.finalize());
        self.require_recorded(bytes, &sha256)?;
        Ok(ImageRecord {
            file: file.to_string(),
            bytes,
            sha256,
        })
    }

    fn require_recorded(&self, bytes: u64, sha256: &str) -> Result<()> {
        if bytes != self.bytes || sha256 != self.sha256 {
            return Err(Front2Error::Verification(format!(
                "backup of {} is {bytes} bytes with SHA-256 {sha256}, the manifest records {} bytes with SHA-256 {}",
                self.name, self.bytes, self.sha256
            )));
        }
        Ok(())
    }
}
