use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const RECORD_FILE: &str = "magisk-package.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageRecord {
    pub file_name: String,
    pub version: String,
    pub sha256: String,
}

impl PackageRecord {
    pub fn load(cache_dir: &Path) -> Result<Option<Self>> {
        let path = cache_dir.join(RECORD_FILE);
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
    }

    pub fn save(&self, cache_dir: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)? + "\n";
        fs::write(cache_dir.join(RECORD_FILE), text)?;
        Ok(())
    }
}
