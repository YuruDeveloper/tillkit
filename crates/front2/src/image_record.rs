use crate::digest::sha256_file;
use crate::error::{Front2Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageRecord {
    pub file: String,
    pub bytes: u64,
    pub sha256: String,
}

impl ImageRecord {
    pub fn from_file(directory: &Path, file: &str) -> Result<Self> {
        let (bytes, sha256) = sha256_file(&directory.join(file))?;
        Ok(Self {
            file: file.to_string(),
            bytes,
            sha256,
        })
    }

    pub fn verify(&self, directory: &Path) -> Result<()> {
        let actual = Self::from_file(directory, &self.file)?;
        if actual != *self {
            return Err(Front2Error::Verification(format!(
                "{} is {} bytes with SHA-256 {}, recorded {} bytes with SHA-256 {}",
                directory.join(&self.file).display(),
                actual.bytes,
                actual.sha256,
                self.bytes,
                self.sha256
            )));
        }
        Ok(())
    }
}
