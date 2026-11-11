use crate::device_profile::DeviceProfile;
use crate::error::{Front2Error, Result};
use crate::pair_kind::PairKind;
use crate::pair_manifest::{MANIFEST_FILE, MANIFEST_SCHEMA, PairManifest};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PairStore {
    root: PathBuf,
    profile: &'static DeviceProfile,
}

impl PairStore {
    pub fn new(root: impl Into<PathBuf>, profile: &'static DeviceProfile) -> Self {
        Self {
            root: root.into(),
            profile,
        }
    }

    pub fn profile(&self) -> &'static DeviceProfile {
        self.profile
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn directory(&self, id: &str) -> Result<PathBuf> {
        if !is_safe_id(id) {
            return Err(Front2Error::Store(format!("pair id {id:?}")));
        }
        Ok(self.root.join(id))
    }

    pub fn begin(&self, id: &str) -> Result<PathBuf> {
        let directory = self.directory(id)?;
        fs::create_dir_all(&self.root)?;
        fs::create_dir(&directory).map_err(|error| {
            Front2Error::Store(format!("create {}: {error}", directory.display()))
        })?;
        Ok(directory)
    }

    pub fn commit(
        &self,
        id: &str,
        kind: PairKind,
        created_unix: u64,
        slot_suffix: &str,
        parent: Option<String>,
        note: &str,
    ) -> Result<PairManifest> {
        let directory = self.directory(id)?;
        let manifest = PairManifest::describe(
            &directory,
            id,
            kind,
            created_unix,
            slot_suffix,
            parent,
            note,
        )?;
        let text = serde_json::to_string_pretty(&manifest)? + "\n";
        let path = directory.join(MANIFEST_FILE);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        std::io::Write::write_all(&mut file, text.as_bytes())?;
        Ok(manifest)
    }

    pub fn load(&self, id: &str) -> Result<PairManifest> {
        let directory = self.directory(id)?;
        let manifest: PairManifest =
            serde_json::from_slice(&fs::read(directory.join(MANIFEST_FILE))?)?;
        if manifest.schema != MANIFEST_SCHEMA || manifest.id != id {
            return Err(Front2Error::Store(format!(
                "manifest in {} has schema {} and id {:?}",
                directory.display(),
                manifest.schema,
                manifest.id
            )));
        }
        Ok(manifest)
    }

    pub fn load_verified(&self, id: &str) -> Result<PairManifest> {
        let manifest = self.load(id)?;
        manifest.verify_files(&self.directory(id)?)?;
        Ok(manifest)
    }

    pub fn list(&self) -> Result<Vec<PairManifest>> {
        let mut manifests = Vec::new();
        if !self.root.exists() {
            return Ok(manifests);
        }
        for entry in fs::read_dir(&self.root)? {
            let name = entry?.file_name();
            manifests.extend(name.to_str().and_then(|id| self.load(id).ok()));
        }
        manifests.sort_by(|left, right| {
            (right.created_unix, &right.id).cmp(&(left.created_unix, &left.id))
        });
        Ok(manifests)
    }
}

pub fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::toss_front2::PROFILE;

    #[test]
    fn ids_cannot_leave_the_store() {
        let store = PairStore::new("store", &PROFILE);
        assert!(store.directory("20261112-004000-readback").is_ok());
        for id in ["", "..", "a/b", "a\\b", "A", "a b"] {
            assert!(store.directory(id).is_err(), "{id:?}");
        }
    }

    #[test]
    fn listing_a_missing_store_is_empty() {
        let directory = tempfile::tempdir().expect("tempdir");
        let store = PairStore::new(directory.path().join("absent"), &PROFILE);
        assert!(store.list().expect("list").is_empty());
    }
}
