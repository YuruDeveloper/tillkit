use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CHOICE_FILE: &str = "tillkit-folder.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderChoice {
    pub data_folder: PathBuf,
}

impl FolderChoice {
    pub fn new(data_folder: impl Into<PathBuf>) -> Self {
        Self {
            data_folder: data_folder.into(),
        }
    }

    pub fn read(program_folder: &Path) -> Result<Option<Self>, String> {
        let bytes = match std::fs::read(program_folder.join(CHOICE_FILE)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| error.to_string())
    }

    pub fn write(&self, program_folder: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(program_folder.join(CHOICE_FILE), bytes).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_choice_is_read_again() {
        let directory = tempfile::tempdir().expect("tempdir");
        let choice = FolderChoice::new(directory.path().join("backups"));
        choice.write(directory.path()).expect("write");
        assert_eq!(FolderChoice::read(directory.path()), Ok(Some(choice)));
    }

    #[test]
    fn doc_1262_an_absent_file_is_no_choice_and_a_broken_file_is_an_error() {
        let directory = tempfile::tempdir().expect("tempdir");
        assert_eq!(FolderChoice::read(directory.path()), Ok(None));
        std::fs::write(directory.path().join(CHOICE_FILE), b"{ not json").expect("file");
        assert!(FolderChoice::read(directory.path()).is_err());
    }

    #[test]
    fn a_folder_that_cannot_hold_the_file_gives_an_error() {
        let directory = tempfile::tempdir().expect("tempdir");
        let absent = directory.path().join("absent");
        assert!(FolderChoice::new("backups").write(&absent).is_err());
    }
}
