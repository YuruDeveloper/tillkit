use serde::{Deserialize, Serialize};

const PARTITIONS_DIRECTORY: &str = "partitions";
const LUN_PREFIX: &str = "lun";
const DUMP_EXTENSION: &str = ".bin";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupEntry {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

impl BackupEntry {
    pub fn partition_lun(&self, name: &str) -> Option<u8> {
        let parts: Vec<&str> = self.path.split(['\\', '/']).collect();
        let [directory, lun, file] = parts.as_slice() else {
            return None;
        };
        if *directory != PARTITIONS_DIRECTORY || file.strip_suffix(DUMP_EXTENSION) != Some(name) {
            return None;
        }
        lun.strip_prefix(LUN_PREFIX)?.parse().ok()
    }

    pub fn relative_path(name: &str, lun: u8) -> String {
        format!("{PARTITIONS_DIRECTORY}/{LUN_PREFIX}{lun}/{name}{DUMP_EXTENSION}")
    }
}
