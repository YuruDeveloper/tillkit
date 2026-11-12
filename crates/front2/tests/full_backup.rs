mod full_backup_support;
mod gpt_support;
mod install_support;

use front2::full_backup::{FullBackupPlan, validate_full_backup};
use front2::gpt::GptCopyKind;
use front2::gsi::{BackupEntry, BackupManifest, CandidateImage, install_inputs_from_images};
use front2::install::{InstallPlan, SectorStorage};
use full_backup_support::BackupFixture;
use gpt_support::{LAST_LBA, SECTOR, SyntheticGpt};
use std::fs;

const MANIFEST: &str = "sha256-manifest.json";

#[test]
fn letter_1292_full_backup_is_read_only_and_builds_install_inputs() {
    let mut fixture = BackupFixture::with_super_sectors(1025);
    let output = fixture.output();
    let plan = FullBackupPlan::read(&mut fixture.storage).expect("backup plan");
    assert!(!output.exists());
    assert!(
        fixture
            .storage
            .reads
            .keys()
            .all(|(_, sector)| *sector == 0 || *sector == LAST_LBA - 4)
    );
    fixture.storage.read_bytes = 0;
    let result = plan
        .write(&mut fixture.storage, &output)
        .expect("backup files");
    assert_eq!(result.single_read_partitions, ["super", "userdata"]);
    assert_eq!(result.directory, output);
    assert_eq!(fixture.storage.read_bytes, plan.read_bytes());
    assert_eq!(result.backup_bytes, plan.backup_bytes());
    validate_full_backup(&output).expect("complete backup");
    let manifest = BackupManifest::load(&output).expect("manifest");
    let entries: Vec<BackupEntry> =
        serde_json::from_slice(&fs::read(output.join(MANIFEST)).unwrap()).unwrap();
    assert_eq!(
        entries.iter().map(|entry| entry.bytes).sum::<u64>(),
        plan.backup_bytes()
    );
    verify_every_file(&output, &entries);
    let images: Vec<CandidateImage> = fixture
        .inputs
        .images
        .iter()
        .map(|image| CandidateImage {
            partition: image.partition.clone(),
            image: image.candidate.clone(),
            backup_source: None,
        })
        .collect();
    let inputs = install_inputs_from_images(
        &images,
        &manifest.partition("super").unwrap().sha256,
        &fixture.inputs.candidate_directory,
        &output,
    )
    .expect("install inputs from full backup");
    InstallPlan::build(&inputs).expect("install plan from full backup");
    let super_location = inputs.partition_map.locate("super").unwrap();
    assert_eq!(
        fixture.storage.reads[&(super_location.lun, super_location.start_sector)],
        1
    );
    assert_eq!(
        fixture.storage.reads[&(super_location.lun, super_location.start_sector + 1024)],
        1
    );
}

fn verify_every_file(directory: &std::path::Path, entries: &[BackupEntry]) {
    for entry in entries {
        let actual = front2::digest::sha256_file(&directory.join(&entry.path)).unwrap();
        assert_eq!(actual, (entry.bytes, entry.sha256.clone()));
    }
}

#[test]
fn doc_1066_an_existing_folder_is_refused_without_device_reads_or_file_changes() {
    let mut fixture = BackupFixture::new();
    let output = fixture.output();
    let plan = FullBackupPlan::read(&mut fixture.storage).unwrap();
    fs::create_dir(&output).unwrap();
    fs::write(output.join(MANIFEST), b"keep this file").unwrap();
    let operations = fixture.storage.operations;
    assert!(plan.write(&mut fixture.storage, &output).is_err());
    assert_eq!(fixture.storage.operations, operations);
    assert_eq!(fs::read(output.join(MANIFEST)).unwrap(), b"keep this file");
}

#[test]
fn letter_1292_non_4096_sectors_and_incorrect_lun_are_refused() {
    let mut fixture = BackupFixture::new();
    fixture.storage.sector_bytes = 512;
    assert!(FullBackupPlan::read(&mut fixture.storage).is_err());
    fixture.storage.sector_bytes = 4096;
    fixture.storage.reported_lun = Some(5);
    assert!(FullBackupPlan::read(&mut fixture.storage).is_err());
}

#[test]
fn letter_1292_gpt_past_the_storage_end_is_refused_before_out_of_range_reads() {
    let mut fixture = BackupFixture::new();
    fixture.storage.total_sectors = LAST_LBA;
    let error = FullBackupPlan::read(&mut fixture.storage)
        .unwrap_err()
        .to_string();
    assert!(error.contains("geometry differ"), "{error}");
    assert_eq!(fixture.storage.reads.len(), 1);
}

#[test]
fn letter_1292_gpt_crc_and_pair_mismatch_are_refused() {
    let mut fixture = BackupFixture::new();
    let mut main = fixture.storage.inner.content(0, 0, 6);
    main[SECTOR + 16] ^= 1;
    fixture.storage.inner.load(0, 0, &main);
    assert!(
        FullBackupPlan::read(&mut fixture.storage)
            .unwrap_err()
            .to_string()
            .contains("CRC32")
    );
    let mut fixture = BackupFixture::new();
    let other = SyntheticGpt::empty().with_entry("cdt", 1);
    fixture.storage.inner.load(3, LAST_LBA - 4, &other.backup());
    assert!(
        FullBackupPlan::read(&mut fixture.storage)
            .unwrap_err()
            .to_string()
            .contains("do not agree")
    );
}

#[test]
fn letter_1292_duplicate_partition_names_are_refused() {
    let mut fixture = BackupFixture::new();
    let other = SyntheticGpt::empty().with_entry("boot_a", 0);
    fixture.storage.inner.load(3, 0, &other.main());
    fixture.storage.inner.load(3, LAST_LBA - 4, &other.backup());
    assert!(
        FullBackupPlan::read(&mut fixture.storage)
            .unwrap_err()
            .to_string()
            .contains("partition name")
    );
}

#[test]
fn letter_1292_every_storage_failure_leaves_no_complete_manifest() {
    let mut fixture = BackupFixture::with_super_sectors(1025);
    let output = fixture.output();
    FullBackupPlan::read(&mut fixture.storage)
        .unwrap()
        .write(&mut fixture.storage, &output)
        .unwrap();
    let operation_count = fixture.storage.operations;
    println!("backup storage failure points: {operation_count}");
    for failure_at in 1..=operation_count {
        assert_failure_leaves_no_manifest(failure_at);
    }
}

fn assert_failure_leaves_no_manifest(failure_at: usize) {
    let mut fixture = BackupFixture::with_super_sectors(1025);
    let output = fixture.output();
    fixture.storage.failure_at = Some(failure_at);
    let result = FullBackupPlan::read(&mut fixture.storage)
        .and_then(|plan| plan.write(&mut fixture.storage, &output));
    assert!(result.is_err(), "failure at {failure_at}");
    assert!(!output.join(MANIFEST).exists(), "failure at {failure_at}");
    assert!(validate_full_backup(&output).is_err());
}

#[test]
fn letter_1292_a_second_read_with_a_different_hash_leaves_no_manifest() {
    let mut fixture = BackupFixture::new();
    let output = fixture.output();
    let location = fixture.inputs.partition_map.locate("boot_a").unwrap();
    fixture.storage.corrupt_second_read = Some((location.lun, location.start_sector));
    let plan = FullBackupPlan::read(&mut fixture.storage).unwrap();
    let error = plan
        .write(&mut fixture.storage, &output)
        .unwrap_err()
        .to_string();
    assert!(error.contains("second read differs"), "{error}");
    assert!(!output.join(MANIFEST).exists());
}

#[test]
fn doc_1066_a_changed_gpt_after_preparation_leaves_no_manifest() {
    let mut fixture = BackupFixture::new();
    let output = fixture.output();
    let plan = FullBackupPlan::read(&mut fixture.storage).unwrap();
    let table = BackupFixture::extra_table().with_flags("fsg", 1);
    fixture.storage.inner.load(5, 0, &table.main());
    fixture.storage.inner.load(5, LAST_LBA - 4, &table.backup());
    let error = plan
        .write(&mut fixture.storage, &output)
        .unwrap_err()
        .to_string();
    assert!(error.contains("changed after preparation"), "{error}");
    assert!(!output.join(MANIFEST).exists());
}

#[test]
fn letter_1292_short_gpt_and_partition_reads_are_refused() {
    let mut fixture = BackupFixture::new();
    fixture.storage.short_read_at = Some(2);
    assert!(FullBackupPlan::read(&mut fixture.storage).is_err());
    let mut fixture = BackupFixture::new();
    let output = fixture.output();
    let plan = FullBackupPlan::read(&mut fixture.storage).unwrap();
    fixture.storage.short_read_at = Some(fixture.storage.operations * 2 + 1);
    assert!(plan.write(&mut fixture.storage, &output).is_err());
    assert!(!output.join(MANIFEST).exists());
}

#[test]
fn doc_1066_completion_check_refuses_empty_partial_missing_and_duplicate_entries() {
    let mut fixture = BackupFixture::new();
    let output = fixture.output();
    FullBackupPlan::read(&mut fixture.storage)
        .unwrap()
        .write(&mut fixture.storage, &output)
        .unwrap();
    let manifest_path = output.join(MANIFEST);
    let bytes = fs::read(&manifest_path).unwrap();
    fs::write(&manifest_path, b"[").unwrap();
    assert!(validate_full_backup(&output).is_err());
    fs::write(&manifest_path, b"[]").unwrap();
    assert!(validate_full_backup(&output).is_err());
    let mut entries: Vec<BackupEntry> = serde_json::from_slice(&bytes).unwrap();
    entries.push(entries[0].clone());
    fs::write(&manifest_path, serde_json::to_vec(&entries).unwrap()).unwrap();
    assert!(validate_full_backup(&output).is_err());
    fs::write(&manifest_path, &bytes).unwrap();
    fs::remove_file(output.join("partitions/lun5/fsg.bin")).unwrap();
    assert!(validate_full_backup(&output).is_err());
}

#[test]
fn letter_1292_missing_required_partitions_and_reserved_file_names_are_refused() {
    assert_bad_extra_table(BackupFixture::extra_table().without("dtbo_a"));
    assert_bad_extra_table(BackupFixture::extra_table().with_entry("nul", 0));
    assert_bad_extra_table(BackupFixture::extra_table().with_entry("gpt_main5", 0));
}

fn assert_bad_extra_table(table: SyntheticGpt) {
    let mut fixture = BackupFixture::new();
    fixture.storage.inner.load(5, 0, &table.main());
    fixture.storage.inner.load(5, LAST_LBA - 4, &table.backup());
    assert!(FullBackupPlan::read(&mut fixture.storage).is_err());
}

#[test]
fn letter_1292_a_table_crc_error_is_refused() {
    let mut fixture = BackupFixture::new();
    let mut bytes = fixture.storage.inner.read_sectors(0, 0, 6).unwrap();
    bytes[GptCopyKind::Main.table_offset()] ^= 1;
    fixture.storage.inner.load(0, 0, &bytes);
    assert!(
        FullBackupPlan::read(&mut fixture.storage)
            .unwrap_err()
            .to_string()
            .contains("table CRC32")
    );
}
