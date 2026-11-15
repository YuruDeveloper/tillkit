use crate::digest::{sha256_bytes, sha256_file};
use crate::error::{Front2Error, Result};
use crate::gpt::BootSlot;
use crate::gsi::backup_manifest::BackupManifest;
use crate::gsi::backup_partition::BackupPartition;
use crate::gsi::candidate_image::CandidateImage;
use crate::gsi::gsi_candidate_request::GsiCandidateRequest;
use crate::gsi::logical_view::extract_logical_view;
use crate::gsi::signed_vbmeta::SignedVbmeta;
use crate::gsi::stock_system::require_stock_front2_system;
use crate::gsi::stock_top::require_stock_front2_top;
use crate::gsi::system_vbmeta::build_system_vbmeta;
use crate::gsi::top_vbmeta::build_top_vbmeta;
use crate::gsi::vbmeta_source::VbmetaSource;
use crate::gsi::vbmeta_template::VbmetaTemplate;
use crate::gsi::view_verification::{verify_view, view_file};
use crate::image_record::ImageRecord;
use crate::profile;
use crate::profiles::toss_front2;
use crate::super_image::{
    SuperCandidateRequest, SuperLayoutPlan, SuperVerification, SuperVerificationRequest,
    build_super_candidate, verify_super_candidate,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;

pub const CANDIDATE_FILE: &str = "candidate.json";
pub const CANDIDATE_SCHEMA: u32 = 1;
pub const SUPER_FILE: &str = "super.img";
pub const VIEW_DIRECTORY: &str = "view";
pub const SUPER_PARTITION: &str = "super";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "GsiCandidateRecord", into = "GsiCandidateRecord")]
pub struct GsiCandidate {
    pub schema: u32,
    pub stock_slot: BootSlot,
    pub target_slot: BootSlot,
    pub source_slot: BootSlot,
    pub gsi_bytes: u64,
    pub gsi_sha256: String,
    pub original_super: BackupPartition,
    pub super_image: ImageRecord,
    pub super_verification: SuperVerification,
    pub logical_view: Vec<ImageRecord>,
    pub physical_images: Vec<CandidateImage>,
    pub chained_vbmeta: SignedVbmeta,
    pub top_vbmeta: SignedVbmeta,
    pub avb_verification: Vec<String>,
    pub first_conversion_only: bool,
    pub fec_recomputed: bool,
    pub device_boot_verified: bool,
    pub stored_rollback_verified: bool,
}

#[derive(Serialize, Deserialize)]
struct GsiCandidateRecord {
    schema: u32,
    #[serde(default, skip_serializing_if = "is_slot_a")]
    stock_slot: BootSlot,
    #[serde(default = "default_target_slot", skip_serializing_if = "is_slot_b")]
    target_slot: BootSlot,
    #[serde(
        default,
        deserialize_with = "deserialize_present_slot",
        skip_serializing_if = "Option::is_none"
    )]
    source_slot: Option<BootSlot>,
    gsi_bytes: u64,
    gsi_sha256: String,
    original_super: BackupPartition,
    super_image: ImageRecord,
    super_verification: SuperVerification,
    logical_view: Vec<ImageRecord>,
    physical_images: Vec<CandidateImage>,
    chained_vbmeta: SignedVbmeta,
    top_vbmeta: SignedVbmeta,
    avb_verification: Vec<String>,
    first_conversion_only: bool,
    fec_recomputed: bool,
    device_boot_verified: bool,
    stored_rollback_verified: bool,
}

impl From<GsiCandidateRecord> for GsiCandidate {
    fn from(record: GsiCandidateRecord) -> Self {
        let source_slot = record.source_slot.unwrap_or(record.stock_slot);
        Self {
            schema: record.schema,
            stock_slot: record.stock_slot,
            target_slot: record.target_slot,
            source_slot,
            gsi_bytes: record.gsi_bytes,
            gsi_sha256: record.gsi_sha256,
            original_super: record.original_super,
            super_image: record.super_image,
            super_verification: record.super_verification,
            logical_view: record.logical_view,
            physical_images: record.physical_images,
            chained_vbmeta: record.chained_vbmeta,
            top_vbmeta: record.top_vbmeta,
            avb_verification: record.avb_verification,
            first_conversion_only: record.first_conversion_only,
            fec_recomputed: record.fec_recomputed,
            device_boot_verified: record.device_boot_verified,
            stored_rollback_verified: record.stored_rollback_verified,
        }
    }
}

impl From<GsiCandidate> for GsiCandidateRecord {
    fn from(candidate: GsiCandidate) -> Self {
        let source_slot =
            (candidate.source_slot != candidate.stock_slot).then_some(candidate.source_slot);
        Self {
            schema: candidate.schema,
            stock_slot: candidate.stock_slot,
            target_slot: candidate.target_slot,
            source_slot,
            gsi_bytes: candidate.gsi_bytes,
            gsi_sha256: candidate.gsi_sha256,
            original_super: candidate.original_super,
            super_image: candidate.super_image,
            super_verification: candidate.super_verification,
            logical_view: candidate.logical_view,
            physical_images: candidate.physical_images,
            chained_vbmeta: candidate.chained_vbmeta,
            top_vbmeta: candidate.top_vbmeta,
            avb_verification: candidate.avb_verification,
            first_conversion_only: candidate.first_conversion_only,
            fec_recomputed: candidate.fec_recomputed,
            device_boot_verified: candidate.device_boot_verified,
            stored_rollback_verified: candidate.stored_rollback_verified,
        }
    }
}

impl GsiCandidate {
    pub fn build(request: &GsiCandidateRequest<'_>) -> Result<Self> {
        request.profile.require_gsi()?;
        if request.target_slot == BootSlot::A {
            return Err(Front2Error::Unsupported(
                "the install target slot A is not supported until the boot switch and the super rebuild are complete".into(),
            ));
        }
        let manifest = BackupManifest::load(request.backup_directory)?;
        fs::create_dir(request.output_directory)?;
        let built = assemble(request, &manifest).and_then(|candidate| {
            candidate.save(request.output_directory)?;
            Ok(candidate)
        });
        if built.is_err() {
            let _ = fs::remove_dir_all(request.output_directory);
        }
        built
    }

    pub fn load(directory: &Path) -> Result<(Self, String)> {
        let bytes = fs::read(directory.join(CANDIDATE_FILE))?;
        let candidate: Self = serde_json::from_slice(&bytes)?;
        if candidate.schema != CANDIDATE_SCHEMA {
            return Err(Front2Error::Plan(format!(
                "GSI candidate schema {}",
                candidate.schema
            )));
        }
        Ok((candidate, sha256_bytes(&bytes)))
    }

    pub fn images(&self) -> Vec<CandidateImage> {
        let target = |base: &str| profile::slot_label(base, self.target_slot.suffix());
        let mut images: Vec<CandidateImage> = self
            .physical_images
            .iter()
            .map(|physical| CandidateImage {
                image: in_view(&physical.image),
                ..physical.clone()
            })
            .collect();
        images.push(CandidateImage {
            partition: target(toss_front2::CHAINED_VBMETA_BASE),
            image: in_view(&self.chained_vbmeta.image),
            backup_source: None,
        });
        images.push(CandidateImage {
            partition: target(profile::VBMETA_BASE),
            image: in_view(&self.top_vbmeta.image),
            backup_source: None,
        });
        images.push(CandidateImage {
            partition: SUPER_PARTITION.to_string(),
            image: self.super_image.clone(),
            backup_source: None,
        });
        images
    }

    pub fn verify_files(&self, directory: &Path) -> Result<()> {
        for candidate_image in self.images() {
            candidate_image.image.verify(directory)?;
        }
        Ok(())
    }

    fn save(&self, directory: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)? + "\n";
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(CANDIDATE_FILE))?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        Ok(())
    }
}

fn deserialize_present_slot<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<BootSlot>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    BootSlot::deserialize(deserializer).map(Some)
}

fn is_slot_a(slot: &BootSlot) -> bool {
    matches!(slot, BootSlot::A)
}

fn is_slot_b(slot: &BootSlot) -> bool {
    matches!(slot, BootSlot::B)
}

fn default_target_slot() -> BootSlot {
    BootSlot::B
}

fn in_view(image: &ImageRecord) -> ImageRecord {
    ImageRecord {
        file: format!("{VIEW_DIRECTORY}/{}", image.file),
        ..image.clone()
    }
}

fn assemble(request: &GsiCandidateRequest<'_>, manifest: &BackupManifest) -> Result<GsiCandidate> {
    let source_slot = request.stock_slot;
    let source_name = |base: &str| profile::slot_label(base, source_slot.suffix());
    let target_name = |base: &str| profile::slot_label(base, request.target_slot.suffix());
    let stock_top = manifest.verified_partition(&source_name(profile::VBMETA_BASE))?;
    let original_top = manifest.verified_partition(&target_name(profile::VBMETA_BASE))?;
    let stock_chained =
        manifest.verified_partition(&source_name(toss_front2::CHAINED_VBMETA_BASE))?;
    let original_chained =
        manifest.verified_partition(&target_name(toss_front2::CHAINED_VBMETA_BASE))?;
    let stock_top_vbmeta = VbmetaSource::read(&stock_top.path)?;
    require_stock_front2_top(&stock_top_vbmeta)?
        .require_accepted(request.accepted_untested_build)?;
    require_stock_front2_system(&VbmetaSource::read(&stock_chained.path)?)?;

    let original_super = manifest.partition(SUPER_PARTITION)?;
    let (gsi_bytes, gsi_sha256) = sha256_file(request.gsi_image)?;
    let directory = request.output_directory;
    let super_path = directory.join(SUPER_FILE);
    let plan = SuperLayoutPlan::from_super_file(&original_super.path, gsi_bytes)?;
    build_super_candidate(
        &SuperCandidateRequest {
            original_super: &original_super.path,
            original_super_sha256: &original_super.sha256,
            gsi_image: request.gsi_image,
            gsi_sha256: &gsi_sha256,
            output: &super_path,
        },
        &plan,
    )?;
    let super_verification = verify_super_candidate(&SuperVerificationRequest {
        candidate: &super_path,
        original_super: &original_super.path,
        gsi_bytes,
        gsi_sha256: &gsi_sha256,
    })?;

    let view = directory.join(VIEW_DIRECTORY);
    fs::create_dir(&view)?;
    let logical_view = extract_logical_view(&super_path, &view)?;
    require_view_matches(&logical_view, &super_verification)?;
    let mut physical_images = Vec::new();
    for base in toss_front2::PHYSICAL_BOOT_PARTITIONS {
        let source = manifest.partition(&source_name(base))?;
        physical_images.push(CandidateImage {
            partition: target_name(base),
            image: source.copy_to(&view, &view_file(base))?,
            backup_source: Some(source.name.clone()),
        });
    }
    let chained_vbmeta = build_system_vbmeta(
        &VbmetaTemplate {
            stock_slot_a: &stock_chained.path,
            original_slot_b: &original_chained.path,
        },
        &view.join(view_file(toss_front2::GSI_SYSTEM_PARTITION)),
        &view.join(view_file(toss_front2::CHAINED_VBMETA_BASE)),
    )?;
    let floor = rollback_floor(
        manifest,
        request.stock_slot,
        source_slot,
        stock_top_vbmeta.info.header.rollback_index,
    )?;
    let top_vbmeta = build_top_vbmeta(
        &VbmetaTemplate {
            stock_slot_a: &stock_top.path,
            original_slot_b: &original_top.path,
        },
        request.rollback_evidence,
        floor,
        &view.join(view_file(profile::VBMETA_BASE)),
    )?;
    let avb_verification = verify_view(&view)?;
    Ok(GsiCandidate {
        schema: CANDIDATE_SCHEMA,
        stock_slot: request.stock_slot,
        target_slot: request.target_slot,
        source_slot,
        gsi_bytes,
        gsi_sha256,
        original_super,
        super_image: ImageRecord::from_file(directory, SUPER_FILE)?,
        super_verification,
        logical_view,
        physical_images,
        chained_vbmeta,
        top_vbmeta,
        avb_verification,
        first_conversion_only: true,
        fec_recomputed: false,
        device_boot_verified: false,
        stored_rollback_verified: false,
    })
}

fn require_view_matches(view: &[ImageRecord], verification: &SuperVerification) -> Result<()> {
    let same = view.len() == verification.logical_partitions.len()
        && view
            .iter()
            .zip(&verification.logical_partitions)
            .all(|(image, record)| image.bytes == record.bytes && image.sha256 == record.sha256);
    if !same {
        return Err(Front2Error::Verification(
            "the extracted logical partitions differ from the verified super candidate".into(),
        ));
    }
    Ok(())
}

fn rollback_floor(
    manifest: &BackupManifest,
    boot_slot: BootSlot,
    source_slot: BootSlot,
    source_rollback_index: u64,
) -> Result<Option<u64>> {
    if boot_slot == source_slot {
        return Ok(None);
    }
    let boot_vbmeta = manifest.verified_partition(&profile::slot_label(
        profile::VBMETA_BASE,
        boot_slot.suffix(),
    ))?;
    let boot_rollback_index = VbmetaSource::read(&boot_vbmeta.path)?
        .info
        .header
        .rollback_index;
    Ok(raised_floor(boot_rollback_index, source_rollback_index))
}

fn raised_floor(boot_rollback_index: u64, source_rollback_index: u64) -> Option<u64> {
    (boot_rollback_index > source_rollback_index).then_some(boot_rollback_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image_record() -> ImageRecord {
        ImageRecord {
            file: "image.img".to_string(),
            bytes: 1,
            sha256: "00".to_string(),
        }
    }

    fn signed_vbmeta() -> SignedVbmeta {
        SignedVbmeta {
            image: image_record(),
            algorithm: "SHA256_RSA4096".to_string(),
            key_spec: "testkey_rsa4096".to_string(),
            rollback_index: 0,
            rollback_index_location: 0,
            flags: 0,
            required_libavb_version_minor: 0,
            descriptors: 0,
        }
    }

    fn backup_partition() -> BackupPartition {
        BackupPartition {
            name: SUPER_PARTITION.to_string(),
            lun: 0,
            path: std::path::PathBuf::new(),
            bytes: 1,
            sha256: "00".to_string(),
        }
    }

    fn super_verification() -> SuperVerification {
        SuperVerification {
            metadata_sha256: "00".to_string(),
            logical_partitions: Vec::new(),
            allowed_super_ranges: Vec::new(),
            outside_ranges_equal_bytes: 0,
            candidate_bytes: 1,
            candidate_sha256: "00".to_string(),
        }
    }

    fn sample(target_slot: BootSlot) -> GsiCandidate {
        GsiCandidate {
            schema: CANDIDATE_SCHEMA,
            stock_slot: BootSlot::A,
            target_slot,
            source_slot: BootSlot::A,
            gsi_bytes: 1,
            gsi_sha256: "00".to_string(),
            original_super: backup_partition(),
            super_image: image_record(),
            super_verification: super_verification(),
            logical_view: Vec::new(),
            physical_images: Vec::new(),
            chained_vbmeta: signed_vbmeta(),
            top_vbmeta: signed_vbmeta(),
            avb_verification: Vec::new(),
            first_conversion_only: true,
            fec_recomputed: false,
            device_boot_verified: false,
            stored_rollback_verified: false,
        }
    }

    #[test]
    fn the_target_slot_is_omitted_for_slot_b_and_reads_back_as_slot_b_when_absent() {
        let value = serde_json::to_value(sample(BootSlot::B)).expect("slot B value");
        assert_eq!(value.get("target_slot"), None);
        let loaded: GsiCandidate =
            serde_json::from_value(value).expect("read the slot B candidate");
        assert_eq!(loaded.target_slot, BootSlot::B);
    }

    #[test]
    fn the_target_slot_is_recorded_for_slot_a() {
        let value = serde_json::to_value(sample(BootSlot::A)).expect("slot A value");
        assert_eq!(value["target_slot"], serde_json::Value::from("a"));
    }

    fn omits_and_recovers_the_source_slot(stock: BootSlot) {
        let candidate = GsiCandidate {
            stock_slot: stock,
            source_slot: stock,
            ..sample(BootSlot::B)
        };
        let value = serde_json::to_value(&candidate).expect("the candidate value");
        assert_eq!(value.get("source_slot"), None, "{value}");
        let loaded: GsiCandidate = serde_json::from_value(value).expect("read the candidate back");
        assert_eq!(loaded.source_slot, stock);
    }

    #[test]
    fn the_source_slot_is_omitted_when_it_matches_the_stock_slot_and_recovers_from_it() {
        omits_and_recovers_the_source_slot(BootSlot::A);
        omits_and_recovers_the_source_slot(BootSlot::B);
    }

    #[test]
    fn the_source_slot_is_recorded_when_it_is_not_the_stock_slot() {
        let candidate = GsiCandidate {
            stock_slot: BootSlot::B,
            source_slot: BootSlot::A,
            ..sample(BootSlot::B)
        };
        let value = serde_json::to_value(&candidate).expect("the candidate value");
        assert_eq!(value["source_slot"], serde_json::Value::from("a"));
        let loaded: GsiCandidate = serde_json::from_value(value).expect("read the candidate back");
        assert_eq!(loaded.source_slot, BootSlot::A);
        assert_eq!(loaded.stock_slot, BootSlot::B);
    }

    #[test]
    fn the_floor_raises_the_signed_index_only_above_the_source_value() {
        assert_eq!(raised_floor(1675555200, 0), Some(1675555200));
        assert_eq!(raised_floor(0, 0), None);
        assert_eq!(raised_floor(0, 1675555200), None);
    }

    #[test]
    fn an_explicit_null_source_slot_is_rejected() {
        let mut value = serde_json::to_value(sample(BootSlot::B)).expect("the candidate value");
        value
            .as_object_mut()
            .expect("a JSON object")
            .insert("source_slot".to_string(), serde_json::Value::Null);
        let loaded: std::result::Result<GsiCandidate, _> = serde_json::from_value(value);
        assert!(loaded.is_err(), "{loaded:?}");
    }

    #[test]
    fn the_images_take_the_vbmeta_names_from_the_target_slot() {
        let names = |slot| {
            sample(slot)
                .images()
                .into_iter()
                .map(|image| image.partition)
                .collect::<Vec<String>>()
        };
        let slot_a = names(BootSlot::A);
        assert!(slot_a.contains(&"vbmeta_a".to_string()), "{slot_a:?}");
        assert!(
            slot_a.contains(&"vbmeta_system_a".to_string()),
            "{slot_a:?}"
        );
        let slot_b = names(BootSlot::B);
        assert!(slot_b.contains(&"vbmeta_b".to_string()), "{slot_b:?}");
        assert!(
            slot_b.contains(&"vbmeta_system_b".to_string()),
            "{slot_b:?}"
        );
    }
}
