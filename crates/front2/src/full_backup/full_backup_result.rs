use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullBackupResult {
    pub directory: PathBuf,
    pub backup_bytes: u64,
    pub single_read_partitions: Vec<String>,
}
