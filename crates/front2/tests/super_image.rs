mod super_support;

use front2::Front2Error;
use front2::digest::{sha256_bytes, sha256_file};
use front2::super_image::{
    ByteRange, LpGeometry, LpMetadata, PartitionOverlap, PartitionSummary, PlannedExtent,
    RebuiltMetadata, RetainedPartition, SuperCandidateReport, SuperCandidateRequest,
    SuperLayoutPlan, SuperVerification, SuperVerificationRequest, build_super_candidate,
    verify_super_candidate,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use super_support::{
    ALIGNMENT_BYTES, GROUP_MAXIMUM_BYTES, MAXIMUM_SIZE, SUPER_BYTES, SyntheticSuper, gsi_image,
};
use tempfile::TempDir;

const SECTOR: u64 = 512;
const GSI_BYTES: u64 = 4608 * SECTOR;
const RETAINED_BYTES: u64 = 1224 * SECTOR;
const FREE_ALIGNED_SECTORS: u64 = 12928;
const TARGET_SLOT: u32 = 1;
const REAL_GSI_BYTES: u64 = 3_102_658_560;
const BACKUP_GEOMETRY_OFFSET: usize = 8192;

fn written(synthetic: &SyntheticSuper) -> (TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("super.bin");
    synthetic.write(&path);
    (directory, path)
}

fn written_image(image: &[u8]) -> (TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("super.bin");
    std::fs::write(&path, image).expect("write the damaged super");
    (directory, path)
}

fn planned(extents: &[(u64, u64)]) -> Vec<PlannedExtent> {
    extents
        .iter()
        .map(|(target_sector, sectors)| PlannedExtent {
            target_sector: *target_sector,
            sectors: *sectors,
            bytes: sectors * SECTOR,
        })
        .collect()
}

fn built_candidate() -> (TempDir, PathBuf, PathBuf, SuperCandidateReport, String) {
    let (directory, original) = written(&SyntheticSuper::standard());
    let gsi = directory.path().join("gsi.img");
    std::fs::write(&gsi, gsi_image(GSI_BYTES)).expect("write the GSI");
    let (_, gsi_sha256) = sha256_file(&gsi).expect("GSI digest");
    let (_, original_sha256) = sha256_file(&original).expect("original digest");
    let candidate = directory.path().join("candidate.img");
    let plan = SuperLayoutPlan::from_super_file(&original, GSI_BYTES).expect("plan");
    let request = SuperCandidateRequest {
        original_super: &original,
        original_super_sha256: &original_sha256,
        gsi_image: &gsi,
        gsi_sha256: &gsi_sha256,
        output: &candidate,
    };
    let report = build_super_candidate(&request, &plan).expect("candidate");
    (directory, original, candidate, report, gsi_sha256)
}

fn verified(
    candidate: &Path,
    original: &Path,
    gsi_sha256: &str,
) -> front2::Result<SuperVerification> {
    verify_super_candidate(&SuperVerificationRequest {
        candidate,
        original_super: original,
        gsi_bytes: GSI_BYTES,
        gsi_sha256,
    })
}

fn flip(path: &Path, offset: u64) {
    let mut bytes = std::fs::read(path).expect("read the file to damage");
    bytes[offset as usize] ^= 0x01;
    std::fs::write(path, bytes).expect("write the damaged file");
}

fn plan_error(synthetic: &SyntheticSuper, gsi_bytes: u64) -> Front2Error {
    let (_directory, path) = written(synthetic);
    SuperLayoutPlan::from_super_file(&path, gsi_bytes).expect_err("the plan must be rejected")
}

fn image_error(image: &[u8]) -> String {
    let (_directory, path) = written_image(image);
    let error = SuperLayoutPlan::from_super_file(&path, GSI_BYTES).expect_err("damaged super");
    assert!(matches!(error, Front2Error::Image(_)), "{error}");
    error.to_string()
}

#[test]
fn plan_places_the_gsi_in_the_free_aligned_ranges_of_slot_b() {
    let (_directory, path) = written(&SyntheticSuper::standard());
    let plan = SuperLayoutPlan::from_super_file(&path, GSI_BYTES).expect("plan");
    let retained = [
        ("odm_b", 64),
        ("product_b", 384),
        ("system_dlkm_b", 8),
        ("vendor_b", 512),
        ("vendor_dlkm_b", 256),
    ]
    .map(|(name, sectors)| RetainedPartition {
        name: name.to_string(),
        bytes: sectors * SECTOR,
    });
    let overlaps = [
        ("system_a", 1536),
        ("system_ext_a", 2048),
        ("system_a-cow", 128),
    ]
    .map(|(partition, sectors)| PartitionOverlap {
        partition: partition.to_string(),
        overlap_bytes: sectors * SECTOR,
    });
    assert_eq!(
        plan.allocation,
        planned(&[(2176, 1024), (3584, 3072), (7424, 512)])
    );
    assert_eq!(plan.retained, retained);
    assert_eq!(plan.original_a_overlaps, overlaps);
    assert_eq!(plan.gsi_bytes, GSI_BYTES);
    assert_eq!(plan.super_bytes, SUPER_BYTES);
    assert_eq!(plan.alignment_bytes, ALIGNMENT_BYTES);
    assert_eq!(plan.alignment_offset_bytes, 0);
    assert_eq!(plan.used_bytes, RETAINED_BYTES + GSI_BYTES);
    assert_eq!(plan.group_maximum_bytes, GROUP_MAXIMUM_BYTES);
}

#[test]
fn plan_accepts_a_gsi_that_exactly_fills_the_group() {
    let (_directory, path) = written(&SyntheticSuper::standard());
    let gsi_bytes = GROUP_MAXIMUM_BYTES - RETAINED_BYTES;
    let plan = SuperLayoutPlan::from_super_file(&path, gsi_bytes).expect("plan");
    assert_eq!(plan.used_bytes, GROUP_MAXIMUM_BYTES);
    assert_eq!(
        plan.allocation
            .iter()
            .map(|extent| extent.bytes)
            .sum::<u64>(),
        gsi_bytes
    );
}

#[test]
fn plan_rejects_a_gsi_size_that_is_not_a_positive_multiple_of_4096() {
    for gsi_bytes in [0, 512, 4097, GSI_BYTES + 2048] {
        let error = plan_error(&SyntheticSuper::standard(), gsi_bytes);
        assert!(
            matches!(error, Front2Error::Plan(_)),
            "{gsi_bytes}: {error}"
        );
        assert!(error.to_string().contains("multiple of 4096"), "{error}");
    }
}

#[test]
fn plan_rejects_a_gsi_larger_than_the_free_space() {
    let gsi_bytes = (FREE_ALIGNED_SECTORS + 8) * SECTOR;
    let error = plan_error(&SyntheticSuper::standard(), gsi_bytes);
    assert!(matches!(error, Front2Error::Plan(_)), "{error}");
    assert!(error.to_string().contains("8 sectors short"), "{error}");
}

#[test]
fn plan_rejects_a_gsi_larger_than_the_group_maximum() {
    let gsi_bytes = GROUP_MAXIMUM_BYTES - RETAINED_BYTES + 4096;
    let error = plan_error(&SyntheticSuper::standard(), gsi_bytes);
    assert!(matches!(error, Front2Error::Plan(_)), "{error}");
    assert!(error.to_string().contains("allows"), "{error}");
}

#[test]
fn plan_rejects_a_retained_extent_that_is_not_linear() {
    let mut synthetic = SyntheticSuper::standard();
    synthetic.slots[TARGET_SLOT as usize]
        .partition_mut("vendor_b")
        .extents[0]
        .2 = 1;
    let error = plan_error(&synthetic, GSI_BYTES);
    assert!(matches!(error, Front2Error::Plan(_)), "{error}");
    assert!(error.to_string().contains("vendor_b"), "{error}");

    synthetic.slots[TARGET_SLOT as usize]
        .partition_mut("vendor_b")
        .extents[0]
        .2 = 2;
    let error = plan_error(&synthetic, GSI_BYTES);
    assert!(matches!(error, Front2Error::Image(_)), "{error}");
}

#[test]
fn plan_rejects_a_system_extent_that_is_not_linear() {
    let mut synthetic = SyntheticSuper::standard();
    synthetic.slots[TARGET_SLOT as usize]
        .partition_mut("system_b")
        .extents[1]
        .2 = 1;
    let error = plan_error(&synthetic, GSI_BYTES);
    assert!(matches!(error, Front2Error::Plan(_)), "{error}");
    assert!(error.to_string().contains("system_b"), "{error}");
}

#[test]
fn plan_rejects_two_block_devices() {
    let mut synthetic = SyntheticSuper::standard();
    synthetic.slots[TARGET_SLOT as usize].block_device_count = 2;
    let error = plan_error(&synthetic, GSI_BYTES);
    assert!(matches!(error, Front2Error::Image(_)), "{error}");
    assert!(error.to_string().contains("2 block devices"), "{error}");
}

#[test]
fn plan_rejects_a_missing_dynamic_group_or_retained_partition() {
    let mut synthetic = SyntheticSuper::standard();
    synthetic.slots[TARGET_SLOT as usize].groups[1].0 = "qti_dynamic_partitions_x".to_string();
    let error = plan_error(&synthetic, GSI_BYTES);
    assert!(
        error
            .to_string()
            .contains("no group named qti_dynamic_partitions_b"),
        "{error}"
    );

    let mut synthetic = SyntheticSuper::standard();
    synthetic.slots[TARGET_SLOT as usize]
        .partition_mut("odm_b")
        .name = "odm_x".to_string();
    let error = plan_error(&synthetic, GSI_BYTES);
    assert!(error.to_string().contains("odm_b"), "{error}");
}

#[test]
fn damaged_checksums_are_rejected() {
    let image = SyntheticSuper::standard().image();
    let primary = SyntheticSuper::primary_offset(TARGET_SLOT) as usize;
    let backup = SyntheticSuper::backup_offset(TARGET_SLOT) as usize;

    let mut geometry = image.clone();
    geometry[4096 + 40] ^= 0x01;
    assert!(image_error(&geometry).contains("geometry"));

    let mut header = image.clone();
    header[primary + 130] ^= 0x01;
    header[backup + 130] ^= 0x01;
    assert!(image_error(&header).contains("header: the checksum"));

    let mut tables = image.clone();
    tables[primary + 256 + 40] ^= 0x01;
    tables[backup + 256 + 40] ^= 0x01;
    assert!(image_error(&tables).contains("tables: the checksum"));

    let mut magic = image;
    magic[primary] ^= 0x01;
    magic[backup] ^= 0x01;
    assert!(image_error(&magic).contains("header"));
}

#[test]
fn primary_and_backup_metadata_must_be_identical() {
    let mut image = SyntheticSuper::standard().image();
    let backup = SyntheticSuper::backup_offset(TARGET_SLOT) as usize;
    image[backup + MAXIMUM_SIZE as usize - 1] ^= 0x01;
    assert!(image_error(&image).contains("primary and backup copies of slot 1 differ"));
}

#[test]
fn rebuilt_metadata_keeps_six_partitions_and_replaces_system() {
    let (_directory, path) = written(&SyntheticSuper::standard());
    let plan = SuperLayoutPlan::from_super_file(&path, GSI_BYTES).expect("plan");
    let rebuilt = RebuiltMetadata::from_super_file(&path, &plan).expect("rebuilt");
    let decoded = LpMetadata::decode(&rebuilt.blob).expect("decode the rebuilt metadata");
    let mut file = std::fs::File::open(&path).expect("open");
    let geometry = LpGeometry::read(&mut file).expect("geometry");
    let original = LpMetadata::read_slot(&mut file, &geometry, TARGET_SLOT).expect("slot B");
    let expected = [
        ("odm_b", 5, 64, 1),
        ("product_b", 5, 384, 2),
        ("system_b", 1, 4608, 3),
        ("system_dlkm_b", 5, 8, 1),
        ("vendor_b", 5, 512, 1),
        ("vendor_dlkm_b", 5, 256, 1),
    ]
    .map(
        |(name, attributes, sectors, extent_count)| PartitionSummary {
            name: name.to_string(),
            attributes,
            bytes: sectors * SECTOR,
            extent_count,
        },
    );
    assert_eq!(rebuilt.partitions, expected);
    assert_eq!(rebuilt.blob.len(), MAXIMUM_SIZE as usize);
    assert_eq!(rebuilt.candidate_bytes, u64::from(MAXIMUM_SIZE));
    assert_eq!(rebuilt.candidate_sha256, sha256_bytes(&rebuilt.blob));
    assert_eq!(rebuilt.source_metadata_sha256, original.sha256);
    assert_eq!(
        rebuilt.primary_offset,
        SyntheticSuper::primary_offset(TARGET_SLOT)
    );
    assert_eq!(
        rebuilt.backup_offset,
        SyntheticSuper::backup_offset(TARGET_SLOT)
    );
    assert_eq!(rebuilt.used_bytes, RETAINED_BYTES + GSI_BYTES);
    assert_eq!(rebuilt.group_maximum_bytes, GROUP_MAXIMUM_BYTES);

    let group_names: Vec<&str> = decoded
        .groups
        .iter()
        .map(|group| group.name.as_str())
        .collect();
    assert_eq!(group_names, ["default", "qti_dynamic_partitions_b"]);
    assert_eq!(decoded.block_devices, original.block_devices);
    assert_eq!(decoded.header[128..], original.header[128..]);
    assert_eq!(decoded.extents.len(), 9);
    assert!(
        decoded
            .partitions
            .iter()
            .all(|partition| partition.group_index == 1)
    );
    let system = decoded.partition("system_b").expect("system_b");
    let system_extents: Vec<(u64, u64, u32, u32)> = decoded
        .extents_of(system)
        .expect("system extents")
        .iter()
        .map(|extent| {
            (
                extent.target_sector,
                extent.sectors,
                extent.target_type,
                extent.target_source,
            )
        })
        .collect();
    assert_eq!(
        system_extents,
        [(2176, 1024, 0, 0), (3584, 3072, 0, 0), (7424, 512, 0, 0)]
    );
    let product = decoded.partition("product_b").expect("product_b");
    let original_product = original.partition("product_b").expect("original product_b");
    assert_eq!(
        decoded.extents_of(product).expect("product extents"),
        original
            .extents_of(original_product)
            .expect("original product extents")
    );
}

#[test]
fn rebuilt_metadata_rejects_a_plan_for_another_super() {
    let (_directory, path) = written(&SyntheticSuper::standard());
    let plan = SuperLayoutPlan::from_super_file(&path, GSI_BYTES).expect("plan");

    let mut other_size = plan.clone();
    other_size.super_bytes += 4096;
    assert!(RebuiltMetadata::from_super_file(&path, &other_size).is_err());

    let mut overlapping = plan.clone();
    overlapping.allocation[0].target_sector = 2048;
    assert!(RebuiltMetadata::from_super_file(&path, &overlapping).is_err());

    let mut unaligned = plan.clone();
    unaligned.allocation[0].target_sector += 8;
    assert!(RebuiltMetadata::from_super_file(&path, &unaligned).is_err());

    let mut shorter = plan;
    shorter.allocation.pop();
    assert!(RebuiltMetadata::from_super_file(&path, &shorter).is_err());
}

#[test]
fn candidate_is_built_and_passes_the_independent_verification() {
    let (_directory, original, candidate, report, gsi_sha256) = built_candidate();
    let original_bytes = std::fs::read(&original).expect("original");
    let candidate_bytes = std::fs::read(&candidate).expect("candidate");
    let gsi = gsi_image(GSI_BYTES);
    let primary = SyntheticSuper::primary_offset(TARGET_SLOT);
    let backup = SyntheticSuper::backup_offset(TARGET_SLOT);
    let slot_bytes = u64::from(MAXIMUM_SIZE);
    let allowed = [
        (primary, primary + slot_bytes),
        (backup, backup + slot_bytes),
        (2176 * SECTOR, 3200 * SECTOR),
        (3584 * SECTOR, 6656 * SECTOR),
        (7424 * SECTOR, 7936 * SECTOR),
    ]
    .map(|(start, end)| ByteRange { start, end });
    let outside_bytes = SUPER_BYTES - 2 * slot_bytes - GSI_BYTES;

    assert_eq!(original_bytes, SyntheticSuper::standard().image());
    assert_eq!(candidate_bytes.len() as u64, SUPER_BYTES);
    assert_eq!(report.allowed_super_ranges, allowed);
    assert_eq!(report.outside_ranges_equal_bytes, outside_bytes);
    assert_eq!(report.candidate_bytes, SUPER_BYTES);
    assert_eq!(report.candidate_sha256, sha256_bytes(&candidate_bytes));
    assert_eq!(report.original_super_sha256, sha256_bytes(&original_bytes));
    assert!(report.original_super_unchanged);
    assert_eq!(report.gsi_sha256, gsi_sha256);

    let mut written_system = Vec::new();
    for range in &allowed[2..] {
        written_system
            .extend_from_slice(&candidate_bytes[range.start as usize..range.end as usize]);
    }
    assert!(
        written_system == gsi,
        "system_b extents hold the GSI in order"
    );
    for offset in [primary, backup] {
        let slot = &candidate_bytes[offset as usize..(offset + slot_bytes) as usize];
        assert!(slot == report.metadata.blob, "rebuilt metadata at {offset}");
    }
    let changed_outside = (0..SUPER_BYTES as usize)
        .filter(|index| {
            !allowed
                .iter()
                .any(|range| range.start as usize <= *index && *index < range.end as usize)
        })
        .any(|index| original_bytes[index] != candidate_bytes[index]);
    assert!(!changed_outside);

    let verification = verified(&candidate, &original, &gsi_sha256).expect("verification");
    assert_eq!(verification.allowed_super_ranges, allowed);
    assert_eq!(verification.outside_ranges_equal_bytes, outside_bytes);
    assert_eq!(verification.candidate_sha256, report.candidate_sha256);
    assert_eq!(
        verification.metadata_sha256,
        report.metadata.candidate_sha256
    );
    assert_eq!(verification.logical_partitions.len(), 6);
    let system = &verification.logical_partitions[2];
    assert_eq!(system.partition, "system_b");
    assert_eq!(system.bytes, GSI_BYTES);
    assert_eq!(system.sha256, gsi_sha256);
    assert!(!system.original_preserved);
    let odm = &verification.logical_partitions[0];
    assert_eq!(odm.partition, "odm_b");
    assert_eq!(
        odm.sha256,
        sha256_bytes(&original_bytes[2048 * 512..2112 * 512])
    );
    assert!(odm.original_preserved);

    let report_json = serde_json::to_value(&report).expect("serialize the report");
    assert!(report_json["metadata"].get("blob").is_none());
    assert_eq!(report_json["candidate_sha256"], report.candidate_sha256);
    serde_json::to_string(&verification).expect("serialize the verification");
}

#[test]
fn candidate_refuses_an_existing_output_and_wrong_digests() {
    let (directory, original) = written(&SyntheticSuper::standard());
    let gsi = directory.path().join("gsi.img");
    std::fs::write(&gsi, gsi_image(GSI_BYTES)).expect("write the GSI");
    let (_, gsi_sha256) = sha256_file(&gsi).expect("GSI digest");
    let (_, original_sha256) = sha256_file(&original).expect("original digest");
    let plan = SuperLayoutPlan::from_super_file(&original, GSI_BYTES).expect("plan");
    let wrong = "0".repeat(64);
    let output = directory.path().join("candidate.img");
    let request = SuperCandidateRequest {
        original_super: &original,
        original_super_sha256: &original_sha256,
        gsi_image: &gsi,
        gsi_sha256: &gsi_sha256,
        output: &output,
    };

    let wrong_gsi = SuperCandidateRequest {
        gsi_sha256: &wrong,
        ..request
    };
    assert!(build_super_candidate(&wrong_gsi, &plan).is_err());
    assert!(!output.exists());

    let wrong_original = SuperCandidateRequest {
        original_super_sha256: &wrong,
        ..request
    };
    assert!(build_super_candidate(&wrong_original, &plan).is_err());
    assert!(!output.exists(), "a failed candidate is removed");

    let larger_plan = SuperLayoutPlan::from_super_file(&original, GSI_BYTES + 4096).expect("plan");
    assert!(build_super_candidate(&request, &larger_plan).is_err());
    assert!(!output.exists());

    std::fs::write(&output, b"keep").expect("write an existing output");
    assert!(build_super_candidate(&request, &plan).is_err());
    assert_eq!(std::fs::read(&output).expect("existing output"), b"keep");
    assert_eq!(
        sha256_file(&original).expect("original digest").1,
        original_sha256
    );
}

#[test]
fn verification_detects_one_changed_byte_outside_the_allowed_ranges() {
    let unused_data_sector = 12000 * SECTOR;
    let retained_odm = 2048 * SECTOR + 7;
    let slot_a_metadata = SyntheticSuper::primary_offset(0) + 300;
    for offset in [100, slot_a_metadata, retained_odm, unused_data_sector] {
        let (_directory, original, candidate, _report, gsi_sha256) = built_candidate();
        flip(&candidate, offset);
        let error = verified(&candidate, &original, &gsi_sha256).expect_err("changed byte");
        assert!(
            matches!(error, Front2Error::Verification(_)),
            "{offset}: {error}"
        );
        assert!(
            error.to_string().contains(&format!("at offset {offset}")),
            "{offset}: {error}"
        );
    }
}

#[test]
fn verification_detects_one_changed_byte_of_the_system_content() {
    for offset in [2176 * SECTOR, 5000 * SECTOR + 9, 7936 * SECTOR - 1] {
        let (_directory, original, candidate, _report, gsi_sha256) = built_candidate();
        flip(&candidate, offset);
        let error = verified(&candidate, &original, &gsi_sha256).expect_err("changed system");
        assert!(
            matches!(error, Front2Error::Verification(_)),
            "{offset}: {error}"
        );
        assert!(
            error.to_string().contains("system_b holds"),
            "{offset}: {error}"
        );
    }
}

#[test]
fn verification_detects_changed_metadata_and_a_wrong_gsi_digest() {
    let (_directory, original, candidate, _report, gsi_sha256) = built_candidate();
    assert!(verified(&candidate, &original, &"0".repeat(64)).is_err());
    assert!(verified(&original, &original, &gsi_sha256).is_err());

    flip(
        &candidate,
        SyntheticSuper::backup_offset(TARGET_SLOT) + 4000,
    );
    let error = verified(&candidate, &original, &gsi_sha256).expect_err("changed metadata");
    assert!(error.to_string().contains("primary and backup"), "{error}");

    let (_directory, original, candidate, _report, gsi_sha256) = built_candidate();
    flip(&candidate, 4096 + 44);
    let error = verified(&candidate, &original, &gsi_sha256).expect_err("changed geometry");
    assert!(error.to_string().contains("geometry"), "{error}");
}

fn with_checksum(record: &mut [u8], checksum_offset: usize) {
    record[checksum_offset..checksum_offset + 32].fill(0);
    let checksum = Sha256::digest(&*record);
    record[checksum_offset..checksum_offset + 32].copy_from_slice(&checksum);
}

#[test]
fn letter_1130_item_1_a_damaged_backup_geometry_is_rejected_before_the_candidate() {
    let image = SyntheticSuper::standard().image();
    let (_directory, valid) = written_image(&image);
    let plan = SuperLayoutPlan::from_super_file(&valid, GSI_BYTES).expect("plan");

    let mut damaged = image.clone();
    damaged[BACKUP_GEOMETRY_OFFSET + 40] ^= 0x01;
    let mut other_size = image.clone();
    let backup = &mut other_size[BACKUP_GEOMETRY_OFFSET..BACKUP_GEOMETRY_OFFSET + 52];
    backup[40..44].copy_from_slice(&(MAXIMUM_SIZE * 2).to_le_bytes());
    with_checksum(backup, 8);
    let mut changed_padding = image;
    changed_padding[BACKUP_GEOMETRY_OFFSET + 4095] ^= 0x01;

    for source in [damaged, other_size, changed_padding] {
        assert!(image_error(&source).contains("primary and backup copies differ"));
        let (directory, original) = written_image(&source);
        let error = RebuiltMetadata::from_super_file(&original, &plan).expect_err("metadata");
        assert!(matches!(error, Front2Error::Image(_)), "{error}");
        let gsi = directory.path().join("gsi.img");
        std::fs::write(&gsi, gsi_image(GSI_BYTES)).expect("write the GSI");
        let candidate = directory.path().join("candidate.img");
        let request = SuperCandidateRequest {
            original_super: &original,
            original_super_sha256: &sha256_bytes(&source),
            gsi_image: &gsi,
            gsi_sha256: &sha256_file(&gsi).expect("GSI digest").1,
            output: &candidate,
        };
        let error = build_super_candidate(&request, &plan).expect_err("candidate");
        assert!(error.to_string().contains("geometry"), "{error}");
        assert!(!candidate.exists());
    }
}

#[test]
fn letter_1130_item_1_the_verification_rejects_a_changed_backup_geometry() {
    let (_directory, original, candidate, _report, gsi_sha256) = built_candidate();
    verified(&candidate, &original, &gsi_sha256).expect("the candidate with two equal copies");
    flip(&candidate, BACKUP_GEOMETRY_OFFSET as u64 + 3);
    let error = verified(&candidate, &original, &gsi_sha256).expect_err("changed backup");
    assert!(
        error
            .to_string()
            .contains("primary and backup copies differ"),
        "{error}"
    );
}

#[test]
fn letter_1130_item_2_the_verification_rejects_a_changed_header_tail() {
    let (_directory, original, candidate, _report, gsi_sha256) = built_candidate();
    verified(&candidate, &original, &gsi_sha256).expect("the unchanged candidate");
    let mut image = std::fs::read(&candidate).expect("read the candidate");
    for offset in [
        SyntheticSuper::primary_offset(TARGET_SLOT) as usize,
        SyntheticSuper::backup_offset(TARGET_SLOT) as usize,
    ] {
        let header = &mut image[offset..offset + 256];
        header[128] ^= 0x01;
        with_checksum(header, 12);
    }
    std::fs::write(&candidate, image).expect("write the changed candidate");
    let error = verified(&candidate, &original, &gsi_sha256).expect_err("changed header tail");
    assert!(matches!(error, Front2Error::Verification(_)), "{error}");
    assert!(
        error.to_string().contains("the header from byte 128"),
        "{error}"
    );
}

fn json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("read the evidence")).expect("parse JSON")
}

fn parsed<Record: serde::de::DeserializeOwned>(value: &Value) -> Record {
    serde_json::from_value(value.clone()).expect("evidence record")
}

#[test]
fn real_super_matches_the_recorded_evidence_when_available() {
    let directories = (
        std::env::var_os("FRONT2_REAL_BACKUP_DIR").map(PathBuf::from),
        std::env::var_os("FRONT2_REAL_EVIDENCE_DIR").map(PathBuf::from),
    );
    let (Some(backup), Some(evidence)) = directories else {
        eprintln!("skipped: FRONT2_REAL_BACKUP_DIR and FRONT2_REAL_EVIDENCE_DIR are not both set");
        return;
    };
    let super_image = backup.join("partitions").join("lun0").join("super.bin");
    let layout = json(
        &evidence
            .join("slot-analysis")
            .join("proposed-super-layout.json"),
    );
    let prepared = evidence.join("prepared-layout");
    let verification = json(&prepared.join("metadata-verification.json"));
    let reference = std::fs::read(prepared.join("slot1-metadata-dry-run.bin")).expect("reference");
    assert_eq!(layout["assumed_gsi_bytes"], REAL_GSI_BYTES);

    let plan = SuperLayoutPlan::from_super_file(&super_image, REAL_GSI_BYTES).expect("plan");
    assert_eq!(
        plan.allocation,
        parsed::<Vec<PlannedExtent>>(&layout["allocation"])
    );
    assert_eq!(
        plan.retained,
        parsed::<Vec<RetainedPartition>>(&layout["retained"])
    );
    assert_eq!(
        plan.original_a_overlaps,
        parsed::<Vec<PartitionOverlap>>(&layout["original_a_overlaps"])
    );
    assert_eq!(layout["super_bytes"], plan.super_bytes);
    assert_eq!(layout["alignment_bytes"], plan.alignment_bytes);
    assert_eq!(
        layout["source_alignment_offset_preserved"],
        plan.alignment_offset_bytes
    );
    assert_eq!(layout["used_bytes"], plan.used_bytes);
    assert_eq!(layout["group_maximum_bytes"], plan.group_maximum_bytes);
    println!(
        "plan equals proposed-super-layout.json: {} extents, {} retained, {} overlaps",
        plan.allocation.len(),
        plan.retained.len(),
        plan.original_a_overlaps.len()
    );

    let rebuilt = RebuiltMetadata::from_super_file(&super_image, &plan).expect("rebuilt");
    assert_eq!(verification["candidate_sha256"], rebuilt.candidate_sha256);
    assert_eq!(sha256_bytes(&rebuilt.blob), rebuilt.candidate_sha256);
    assert!(rebuilt.blob == reference, "blob equals the reference file");
    assert_eq!(
        verification["source_metadata_sha256"],
        rebuilt.source_metadata_sha256
    );
    assert_eq!(verification["candidate_bytes"], rebuilt.candidate_bytes);
    assert_eq!(verification["primary_offset"], rebuilt.primary_offset);
    assert_eq!(verification["backup_offset"], rebuilt.backup_offset);
    assert_eq!(verification["used_bytes"], rebuilt.used_bytes);
    assert_eq!(
        verification["group_maximum_bytes"],
        rebuilt.group_maximum_bytes
    );
    assert_eq!(
        rebuilt.partitions,
        parsed::<Vec<PartitionSummary>>(&verification["partitions"])
    );
    println!(
        "rebuilt metadata SHA-256 {} equals candidate_sha256 and slot1-metadata-dry-run.bin ({} bytes)",
        rebuilt.candidate_sha256,
        reference.len()
    );
}
