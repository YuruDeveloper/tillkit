use crate::avb_summary::AvbSummary;
use crate::error::Result;
use crate::image_record::ImageRecord;
use crate::pair_kind::PairKind;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MANIFEST_FILE: &str = "manifest.json";
pub const INIT_BOOT_FILE: &str = "init_boot.img";
pub const VBMETA_FILE: &str = "vbmeta.img";
pub const MANIFEST_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairManifest {
    pub schema: u32,
    pub id: String,
    pub kind: PairKind,
    pub created_unix: u64,
    pub slot_suffix: String,
    pub init_boot: ImageRecord,
    pub vbmeta: ImageRecord,
    pub avb: AvbSummary,
    pub parent: Option<String>,
    pub note: String,
}

impl PairManifest {
    pub fn describe(
        directory: &Path,
        id: &str,
        kind: PairKind,
        created_unix: u64,
        slot_suffix: &str,
        parent: Option<String>,
        note: &str,
    ) -> Result<Self> {
        Ok(Self {
            schema: MANIFEST_SCHEMA,
            id: id.to_string(),
            kind,
            created_unix,
            slot_suffix: slot_suffix.to_string(),
            init_boot: ImageRecord::from_file(directory, INIT_BOOT_FILE)?,
            vbmeta: ImageRecord::from_file(directory, VBMETA_FILE)?,
            avb: AvbSummary::inspect(
                &directory.join(INIT_BOOT_FILE),
                &directory.join(VBMETA_FILE),
            )?,
            parent,
            note: note.to_string(),
        })
    }

    pub fn verify_files(&self, directory: &Path) -> Result<()> {
        self.init_boot.verify(directory)?;
        self.vbmeta.verify(directory)
    }
}
