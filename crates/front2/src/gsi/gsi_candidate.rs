use crate::digest::{sha256_bytes, sha256_file};
use crate::error::{Front2Error, Result};
use crate::gsi::backup_manifest::BackupManifest;
use crate::gsi::backup_partition::BackupPartition;
use crate::gsi::candidate_image::CandidateImage;
use crate::gsi::gsi_candidate_request::GsiCandidateRequest;
use crate::gsi::logical_view::extract_logical_view;
use crate::gsi::signed_vbmeta::SignedVbmeta;
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
pub const STOCK_SLOT_SUFFIX: &str = "_a";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GsiCandidate {
    pub schema: u32,
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

impl GsiCandidate {
    pub fn build(request: &GsiCandidateRequest<'_>) -> Result<Self> {
        request.profile.require_gsi()?;
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
        let slot_b = |base: &str| profile::slot_label(base, toss_front2::SUPPORTED_SLOT_SUFFIX);
        let mut images: Vec<CandidateImage> = self
            .physical_images
            .iter()
            .map(|physical| CandidateImage {
                image: in_view(&physical.image),
                ..physical.clone()
            })
            .collect();
        images.push(CandidateImage {
            partition: slot_b(toss_front2::CHAINED_VBMETA_BASE),
            image: in_view(&self.chained_vbmeta.image),
            backup_source: None,
        });
        images.push(CandidateImage {
            partition: slot_b(profile::VBMETA_BASE),
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

fn in_view(image: &ImageRecord) -> ImageRecord {
    ImageRecord {
        file: format!("{VIEW_DIRECTORY}/{}", image.file),
        ..image.clone()
    }
}

fn assemble(request: &GsiCandidateRequest<'_>, manifest: &BackupManifest) -> Result<GsiCandidate> {
    let stock_name = |base: &str| profile::slot_label(base, STOCK_SLOT_SUFFIX);
    let target_name = |base: &str| profile::slot_label(base, toss_front2::SUPPORTED_SLOT_SUFFIX);
    let stock_top = manifest.verified_partition(&stock_name(profile::VBMETA_BASE))?;
    let original_top = manifest.verified_partition(&target_name(profile::VBMETA_BASE))?;
    let stock_chained =
        manifest.verified_partition(&stock_name(toss_front2::CHAINED_VBMETA_BASE))?;
    let original_chained =
        manifest.verified_partition(&target_name(toss_front2::CHAINED_VBMETA_BASE))?;
    require_stock_front2_top(&VbmetaSource::read(&stock_top.path)?)?;

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
        let source = manifest.partition(&stock_name(base))?;
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
    let top_vbmeta = build_top_vbmeta(
        &VbmetaTemplate {
            stock_slot_a: &stock_top.path,
            original_slot_b: &original_top.path,
        },
        request.rollback_evidence,
        &view.join(view_file(profile::VBMETA_BASE)),
    )?;
    let avb_verification = verify_view(&view)?;
    Ok(GsiCandidate {
        schema: CANDIDATE_SCHEMA,
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
