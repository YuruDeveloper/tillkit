use crate::digest::is_sha256_hex;
use crate::error::{Front2Error, Result};
use crate::gsi::backup_entry::BackupEntry;
use crate::gsi::backup_partition::BackupPartition;
use crate::partition_target::is_safe_label;
use std::fs;
use std::path::{Path, PathBuf};

pub const BACKUP_MANIFEST_FILE: &str = "sha256-manifest.json";

#[derive(Debug, Clone)]
pub struct BackupManifest {
    directory: PathBuf,
    entries: Vec<BackupEntry>,
}

impl BackupManifest {
    pub fn load(directory: &Path) -> Result<Self> {
        let bytes = fs::read(directory.join(BACKUP_MANIFEST_FILE))?;
        Ok(Self {
            directory: directory.to_path_buf(),
            entries: serde_json::from_slice(&bytes)?,
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn partition(&self, name: &str) -> Result<BackupPartition> {
        let failure = |reason: String| {
            Front2Error::Image(format!(
                "{}: {reason}",
                self.directory.join(BACKUP_MANIFEST_FILE).display()
            ))
        };
        if !is_safe_label(name) {
            return Err(Front2Error::Plan(format!("partition name {name:?}")));
        }
        let mut found = self
            .entries
            .iter()
            .filter_map(|entry| entry.partition_lun(name).map(|lun| (lun, entry)));
        let (Some((lun, entry)), None) = (found.next(), found.next()) else {
            return Err(failure(format!(
                "there is not exactly one dump of partition {name}"
            )));
        };
        let sha256 = entry.sha256.to_ascii_lowercase();
        if !is_sha256_hex(&sha256) {
            return Err(failure(format!(
                "the SHA-256 of partition {name} is not 64 hexadecimal digits"
            )));
        }
        Ok(BackupPartition {
            name: name.to_string(),
            lun,
            path: self.directory.join(BackupEntry::relative_path(name, lun)),
            bytes: entry.bytes,
            sha256,
        })
    }

    pub fn verified_partition(&self, name: &str) -> Result<BackupPartition> {
        let partition = self.partition(name)?;
        partition.verify()?;
        Ok(partition)
    }
}
