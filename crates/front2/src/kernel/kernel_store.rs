use crate::device_profile::DeviceProfile;
use crate::error::{Front2Error, Result};
use crate::kernel::kernel_set::{KernelSet, SET_FILE, SET_SCHEMA};
use crate::kernel::reference_partition::ReferencePartition;
use crate::pair_kind::PairKind;
use crate::pair_request::PairRequest;
use crate::pair_store::{PairStore, is_safe_id};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct KernelStore {
    root: PathBuf,
    profile: &'static DeviceProfile,
}

impl KernelStore {
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

    pub fn pair_store(&self) -> PairStore {
        PairStore::new(self.root.clone(), self.profile)
    }

    pub fn directory(&self, id: &str) -> Result<PathBuf> {
        if !is_safe_id(id) {
            return Err(Front2Error::Store(format!("kernel set id {id:?}")));
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
        request: PairRequest<'_>,
        kind: PairKind,
        slot_suffix: &str,
        parent: Option<String>,
        references: Vec<ReferencePartition>,
    ) -> Result<KernelSet> {
        let directory = self.directory(request.id)?;
        let set = KernelSet::describe(&directory, request, kind, slot_suffix, parent, references)?;
        let text = serde_json::to_string_pretty(&set)? + "\n";
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(SET_FILE))?;
        std::io::Write::write_all(&mut file, text.as_bytes())?;
        Ok(set)
    }

    pub fn load(&self, id: &str) -> Result<KernelSet> {
        let directory = self.directory(id)?;
        let set: KernelSet = serde_json::from_slice(&fs::read(directory.join(SET_FILE))?)?;
        if set.schema != SET_SCHEMA || set.id != id {
            return Err(Front2Error::Store(format!(
                "kernel set in {} has schema {} and id {:?}",
                directory.display(),
                set.schema,
                set.id
            )));
        }
        Ok(set)
    }

    pub fn load_verified(&self, id: &str) -> Result<KernelSet> {
        let set = self.load(id)?;
        set.verify_files(&self.directory(id)?)?;
        Ok(set)
    }

    pub fn list(&self) -> Result<Vec<KernelSet>> {
        let mut sets = Vec::new();
        if !self.root.exists() {
            return Ok(sets);
        }
        for entry in fs::read_dir(&self.root)? {
            let name = entry?.file_name();
            sets.extend(name.to_str().and_then(|id| self.load(id).ok()));
        }
        sets.sort_by(|left, right| {
            (right.created_unix, &right.id).cmp(&(left.created_unix, &left.id))
        });
        Ok(sets)
    }
}
