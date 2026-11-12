use crate::data_paths::DataPaths;
use front2::full_backup::validate_full_backup;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const FOLDER_PREFIX: &str = "backup-";

pub fn refusal(folder: &Path) -> Option<String> {
    validate_full_backup(folder)
        .err()
        .map(|error| error.to_string())
}

pub fn new_folder(paths: &DataPaths, created_unix: u64) -> PathBuf {
    paths
        .full_backups()
        .join(format!("{FOLDER_PREFIX}{created_unix}"))
}

pub fn newest(paths: &DataPaths) -> Option<PathBuf> {
    newest_where(paths, |folder| refusal(folder).is_none())
}

fn newest_where(paths: &DataPaths, is_complete: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let entries = std::fs::read_dir(paths.full_backups()).ok()?;
    entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| Some((created_unix(&entry.file_name())?, entry.path())))
        .filter(|(_, folder)| is_complete(folder))
        .max_by_key(|(created, _)| *created)
        .map(|(_, folder)| folder)
}

fn created_unix(name: &OsStr) -> Option<u64> {
    name.to_str()?.strip_prefix(FOLDER_PREFIX)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARK: &str = "complete";

    fn folder(paths: &DataPaths, name: &str, complete: bool) -> PathBuf {
        let folder = paths.full_backups().join(name);
        std::fs::create_dir_all(&folder).expect("folder");
        if complete {
            std::fs::write(folder.join(MARK), b"").expect("mark");
        }
        folder
    }

    #[test]
    fn the_newest_complete_backup_of_the_data_folder_is_found() {
        let directory = tempfile::tempdir().expect("tempdir");
        let paths = DataPaths::new(directory.path());
        let marked = |folder: &Path| folder.join(MARK).is_file();
        assert_eq!(newest_where(&paths, marked), None, "no store yet");
        folder(&paths, "backup-900", true);
        let newest_complete = folder(&paths, "backup-1000", true);
        folder(&paths, "backup-2000", false);
        folder(&paths, "other-3000", true);
        assert_eq!(newest_where(&paths, marked), Some(newest_complete));
        assert_eq!(newest(&paths), None, "a mark is no full backup");
        assert_eq!(new_folder(&paths, 7), paths.full_backups().join("backup-7"));
    }
}
