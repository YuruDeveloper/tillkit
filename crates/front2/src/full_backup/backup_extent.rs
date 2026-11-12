use crate::error::{Front2Error, Result};
use crate::gpt::{PartitionLocation, SECTOR_BYTES};
use crate::gsi::BackupEntry;
use crate::partition_target::is_safe_label;

#[derive(Debug, Clone)]
pub(super) struct BackupExtent {
    pub name: String,
    pub location: PartitionLocation,
    pub bytes: u64,
}

impl BackupExtent {
    pub fn new(name: &str, location: PartitionLocation) -> Result<Self> {
        require_file_name(name)?;
        let bytes = location.sectors.checked_mul(SECTOR_BYTES as u64);
        let Some(bytes) = bytes.filter(|bytes| *bytes > 0) else {
            return Err(Front2Error::Image(format!(
                "backup size of {name} is out of range"
            )));
        };
        Ok(Self {
            name: name.to_string(),
            location,
            bytes,
        })
    }

    pub fn single_read(&self) -> bool {
        matches!(self.name.as_str(), "super" | "userdata")
    }

    pub fn entry(&self, sha256: String) -> BackupEntry {
        BackupEntry {
            path: BackupEntry::relative_path(&self.name, self.location.lun),
            bytes: self.bytes,
            sha256,
        }
    }
}

fn require_file_name(name: &str) -> Result<()> {
    let reserved = matches!(name, "con" | "prn" | "aux" | "nul")
        || reserved_number(name, "com")
        || reserved_number(name, "lpt");
    if !is_safe_label(name)
        || reserved
        || name.starts_with("gpt_main")
        || name.starts_with("gpt_backup")
    {
        return Err(Front2Error::Image(format!(
            "backup partition name {name:?} is not supported"
        )));
    }
    Ok(())
}

fn reserved_number(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|suffix| suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9'))
}
