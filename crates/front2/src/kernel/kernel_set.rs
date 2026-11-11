use crate::digest::sha256_bytes;
use crate::error::{Front2Error, Result};
use crate::image_record::ImageRecord;
use crate::kernel::boot_image_header::BootImageHeader;
use crate::kernel::kernel_avb_summary::KernelAvbSummary;
use crate::kernel::kernel_release::KernelRelease;
use crate::kernel::reference_partition::ReferencePartition;
use crate::pair_kind::PairKind;
use crate::pair_request::PairRequest;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const SET_FILE: &str = "kernel-set.json";
pub const BOOT_FILE: &str = "boot.img";
pub const VBMETA_FILE: &str = "vbmeta.img";
pub const BOOT_BASE: &str = "boot";
pub const SET_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelSet {
    pub schema: u32,
    pub id: String,
    pub kind: PairKind,
    pub created_unix: u64,
    pub slot_suffix: String,
    pub boot: ImageRecord,
    pub vbmeta: ImageRecord,
    pub boot_header: BootImageHeader,
    pub kernel: KernelRelease,
    pub kernel_sha256: String,
    pub avb: KernelAvbSummary,
    pub references: Vec<ReferencePartition>,
    pub parent: Option<String>,
    pub note: String,
}

impl KernelSet {
    pub fn describe(
        directory: &Path,
        request: PairRequest<'_>,
        kind: PairKind,
        slot_suffix: &str,
        parent: Option<String>,
        references: Vec<ReferencePartition>,
    ) -> Result<Self> {
        let boot_path = directory.join(BOOT_FILE);
        let image = fs::read(&boot_path)?;
        let boot_header = in_boot(BootImageHeader::parse(&image))?;
        in_boot(boot_header.require_kernel_only())?;
        let kernel = in_boot(boot_header.kernel(&image))?;
        Ok(Self {
            schema: SET_SCHEMA,
            id: request.id.to_string(),
            kind,
            created_unix: request.created_unix,
            slot_suffix: slot_suffix.to_string(),
            boot: ImageRecord::from_file(directory, BOOT_FILE)?,
            vbmeta: ImageRecord::from_file(directory, VBMETA_FILE)?,
            boot_header,
            kernel: KernelRelease::from_kernel(kernel)?,
            kernel_sha256: sha256_bytes(kernel),
            avb: KernelAvbSummary::inspect(&boot_path, &directory.join(VBMETA_FILE))?,
            references,
            parent,
            note: request.note.to_string(),
        })
    }

    pub fn request(&self) -> PairRequest<'_> {
        PairRequest {
            id: &self.id,
            created_unix: self.created_unix,
            note: &self.note,
        }
    }

    pub fn verify_files(&self, directory: &Path) -> Result<()> {
        self.boot.verify(directory)?;
        self.vbmeta.verify(directory)?;
        let described = Self::describe(
            directory,
            self.request(),
            self.kind,
            &self.slot_suffix,
            self.parent.clone(),
            self.references.clone(),
        )?;
        if described != *self {
            return Err(Front2Error::Verification(format!(
                "{} does not describe the images next to it",
                directory.join(SET_FILE).display()
            )));
        }
        Ok(())
    }
}

fn in_boot<T>(result: Result<T>) -> Result<T> {
    result.map_err(|error| match error {
        Front2Error::Image(reason) => Front2Error::Image(format!("boot image {reason}")),
        other => other,
    })
}
