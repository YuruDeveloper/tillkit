mod gpt_support;

use front2::digest::sha256_bytes;
use front2::gpt::{
    GptCopy, GptCopyKind, PartitionLocation, PartitionMap, SlotActivation, misc_wipe_sector,
    require_current_copy_matches, require_matching_table_areas,
};
use front2::{Front2Error, Result};
use gpt_support::{
    ENTRY_BYTES, FIRST_PARTITION_LBA, LAST_LBA, PARTITION_SECTORS, SECTOR, SyntheticGpt, seal,
};
use serde_json::{Value, json};
use std::fmt::Debug;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const LUN: u8 = 4;
const MAIN_HEADER: usize = SECTOR;
const MAIN_TABLE: usize = SECTOR * 2;
const BACKUP_HEADER: usize = SECTOR * 4;
const SLOT_ENTRIES: usize = 6;
const EXPECTED_PAIRS: [(&str, &str, u64, u64); 3] = [
    ("boot_a", "boot_b", 0x0072 << 48, 0x003f << 48),
    ("vbmeta_a", "vbmeta_b", 0x107b << 48, 0x107f << 48),
    ("dtbo_a", "dtbo_b", 0x007b << 48, 0x007f << 48),
];

fn activate(gpt: &SyntheticGpt) -> Result<SlotActivation> {
    SlotActivation::prepare(LUN, &gpt.main(), &gpt.backup())
}

fn rejection<Outcome: Debug>(result: Result<Outcome>, fragment: &str) -> Front2Error {
    let err = result.expect_err(fragment);
    assert!(
        err.to_string().contains(fragment),
        "unexpected message: {err}"
    );
    err
}

fn parsed_output(activation: &SlotActivation, kind: GptCopyKind) -> GptCopy {
    let output = activation.output(kind).expect("output");
    GptCopy::parse(kind, &output.data).expect("the output is a valid GPT copy")
}

fn differing_offsets(left: &[u8], right: &[u8]) -> Vec<usize> {
    assert_eq!(left.len(), right.len());
    (0..left.len())
        .filter(|offset| left[*offset] != right[*offset])
        .collect()
}

fn allowed_offsets(kind: GptCopyKind) -> Vec<usize> {
    let header = kind.header_offset();
    let mut allowed: Vec<usize> = (header + 16..header + 20)
        .chain(header + 88..header + 92)
        .collect();
    for i in 0..SLOT_ENTRIES {
        let entry = kind.table_offset() + i * ENTRY_BYTES;
        allowed.extend(entry..entry + 16);
        allowed.extend(entry + 48..entry + 56);
    }
    allowed
}

fn assert_switched(before: &GptCopy, after: &GptCopy, name: &str, partner: &str, flags: u64) {
    let old = before.entry(name).expect("entry before");
    let new = after.entry(name).expect("entry after");
    let old_partner = before.entry(partner).expect("partner before");
    assert_eq!(new.flags, flags, "{name}");
    assert_eq!(new.type_guid, old_partner.type_guid, "{name}");
    assert_ne!(new.type_guid, old.type_guid, "{name}");
    assert_eq!(
        (new.first_lba, new.last_lba, new.table_offset),
        (old.first_lba, old.last_lba, old.table_offset),
        "{name}"
    );
}

#[test]
fn activation_reports_both_copies_with_positions_and_digests() {
    let gpt = SyntheticGpt::standard();
    let activation = activate(&gpt).expect("activation");
    let main = activation.output(GptCopyKind::Main).expect("main output");
    let backup = activation
        .output(GptCopyKind::Backup)
        .expect("backup output");
    assert_eq!(activation.lun, LUN);
    assert_eq!(activation.outputs.len(), 2);
    assert_eq!(
        (main.lun, main.start_sector, main.sectors, main.bytes),
        (LUN, 0, 6, 24576)
    );
    assert_eq!(
        (
            backup.lun,
            backup.start_sector,
            backup.sectors,
            backup.bytes
        ),
        (LUN, LAST_LBA - 4, 5, 20480)
    );
    assert_eq!(main.sha256, sha256_bytes(&main.data));
    assert_eq!(backup.sha256, sha256_bytes(&backup.data));
    assert_eq!(main.restore_sha256, sha256_bytes(&gpt.main()));
    assert_eq!(backup.restore_sha256, sha256_bytes(&gpt.backup()));
    assert_ne!(main.sha256, main.restore_sha256);
    assert_ne!(backup.sha256, backup.restore_sha256);
}

#[test]
fn activation_swaps_type_guids_and_moves_flags() {
    let gpt = SyntheticGpt::standard();
    let activation = activate(&gpt).expect("activation");
    let before = GptCopy::parse(GptCopyKind::Main, &gpt.main()).expect("original");
    let after = parsed_output(&activation, GptCopyKind::Main);
    let after_backup = parsed_output(&activation, GptCopyKind::Backup);
    assert_eq!(after.table(), after_backup.table());
    assert_eq!(after.entries(), after_backup.entries());
    for (name_a, name_b, flags_a, flags_b) in EXPECTED_PAIRS {
        assert_switched(&before, &after, name_a, name_b, flags_a);
        assert_switched(&before, &after, name_b, name_a, flags_b);
    }
    assert_eq!(after.entry("misc"), before.entry("misc"));
    assert_eq!(after.entries().len(), before.entries().len());
}

#[test]
fn activation_changes_only_type_guids_flags_and_checksums() {
    let gpt = SyntheticGpt::standard();
    let activation = activate(&gpt).expect("activation");
    for (kind, original) in [
        (GptCopyKind::Main, gpt.main()),
        (GptCopyKind::Backup, gpt.backup()),
    ] {
        let output = activation.output(kind).expect("output");
        let allowed = allowed_offsets(kind);
        let changed = differing_offsets(&original, &output.data);
        let outside: Vec<&usize> = changed
            .iter()
            .filter(|offset| !allowed.contains(offset))
            .collect();
        assert!(outside.is_empty(), "{kind:?}: {outside:?}");
        let slot_table = kind.table_offset()..kind.table_offset() + SLOT_ENTRIES * ENTRY_BYTES;
        let type_guid_bytes = changed
            .iter()
            .filter(|offset| slot_table.contains(offset))
            .filter(|offset| (**offset - slot_table.start) % ENTRY_BYTES < 16)
            .count();
        assert_eq!(type_guid_bytes, SLOT_ENTRIES * 16, "{kind:?}");
    }
}

#[test]
fn activation_lists_changes_in_table_order() {
    let activation = activate(&SyntheticGpt::standard()).expect("activation");
    let actual = serde_json::to_value(&activation.changes).expect("changes");
    let expected = json!([
        {
            "lun": 4,
            "a": "boot_a",
            "b": "boot_b",
            "flags_before_a": "0077000000000000",
            "flags_before_b": "00c3000000000000",
            "flags_after_a": "0072000000000000",
            "flags_after_b": "003f000000000000",
            "type_guids_swapped": true
        },
        {
            "lun": 4,
            "a": "vbmeta_a",
            "b": "vbmeta_b",
            "flags_before_a": "107f000000000000",
            "flags_before_b": "107b000000000000",
            "flags_after_a": "107b000000000000",
            "flags_after_b": "107f000000000000",
            "type_guids_swapped": true
        },
        {
            "lun": 4,
            "a": "dtbo_a",
            "b": "dtbo_b",
            "flags_before_a": "007f000000000000",
            "flags_before_b": "007b000000000000",
            "flags_after_a": "007b000000000000",
            "flags_after_b": "007f000000000000",
            "type_guids_swapped": true
        }
    ]);
    assert_eq!(actual, expected);
}

#[test]
fn activation_serializes_without_the_image_bytes() {
    let activation = activate(&SyntheticGpt::standard()).expect("activation");
    let value = serde_json::to_value(&activation).expect("serialize");
    let main = &value["outputs"][0];
    let backup = &value["outputs"][1];
    assert_eq!(value["lun"], LUN);
    assert_eq!(main["copy"], "main");
    assert_eq!(backup["copy"], "backup");
    assert_eq!(main["start_sector"], 0);
    assert_eq!(backup["start_sector"], LAST_LBA - 4);
    assert_eq!(main["bytes"], 24576);
    assert!(main.get("data").is_none());
    assert!(backup.get("data").is_none());
    assert_eq!(value["changes"].as_array().map(Vec::len), Some(3));
}

#[test]
fn a_damaged_header_checksum_is_rejected() {
    let gpt = SyntheticGpt::standard();
    let mut main = gpt.main();
    main[MAIN_HEADER + 56] ^= 0xFF;
    let err = rejection(
        SlotActivation::prepare(LUN, &main, &gpt.backup()),
        "header CRC32",
    );
    assert!(matches!(err, Front2Error::Image(_)));
    let mut backup = gpt.backup();
    backup[BACKUP_HEADER + 16] ^= 0x01;
    rejection(
        SlotActivation::prepare(LUN, &gpt.main(), &backup),
        "header CRC32",
    );
}

#[test]
fn a_damaged_table_checksum_is_rejected() {
    let gpt = SyntheticGpt::standard();
    let mut main = gpt.main();
    main[MAIN_TABLE + 20] ^= 0xFF;
    let err = rejection(
        SlotActivation::prepare(LUN, &main, &gpt.backup()),
        "table CRC32",
    );
    assert!(matches!(err, Front2Error::Image(_)));
    let mut backup = gpt.backup();
    backup[ENTRY_BYTES + 48] ^= 0x04;
    rejection(
        SlotActivation::prepare(LUN, &gpt.main(), &backup),
        "table CRC32",
    );
}

#[test]
fn a_backup_table_that_differs_from_the_main_table_is_rejected() {
    let gpt = SyntheticGpt::standard();
    let other = SyntheticGpt::standard().with_flags("misc", 1);
    let err = rejection(
        SlotActivation::prepare(LUN, &gpt.main(), &other.backup()),
        "do not agree in tables",
    );
    assert!(matches!(err, Front2Error::Verification(_)));
}

#[test]
fn a_slot_a_partition_without_a_slot_b_partner_is_rejected() {
    let gpt = SyntheticGpt::standard().without("dtbo_b");
    let err = rejection(activate(&gpt), "dtbo_a has no partner named dtbo_b");
    assert!(matches!(err, Front2Error::Unsupported(_)));
}

#[test]
fn a_boot_a_partition_without_the_active_flag_is_rejected() {
    let gpt = SyntheticGpt::standard().with_flags("boot_a", 0x0073 << 48);
    let err = rejection(activate(&gpt), "boot_a is not active");
    assert!(matches!(err, Front2Error::Unsupported(_)));
}

#[test]
fn a_pair_with_no_active_flag_is_switched_to_the_same_table_as_a_pair_with_the_flag() {
    let flagged = activate(&SyntheticGpt::standard()).expect("flagged pair");
    let unflagged_gpt = SyntheticGpt::standard().with_flags("vbmeta_a", 0x107b << 48);
    let unflagged = activate(&unflagged_gpt).expect("pair with no active flag");
    for kind in [GptCopyKind::Main, GptCopyKind::Backup] {
        let expected = flagged.output(kind).expect("flagged output");
        let actual = unflagged.output(kind).expect("unflagged output");
        assert_eq!(actual.data, expected.data, "{kind:?}");
        assert_ne!(actual.restore_sha256, expected.restore_sha256, "{kind:?}");
    }
    let change = &unflagged.changes[1];
    assert_eq!(change.a, "vbmeta_a");
    assert_eq!(change.flags_before_a, "107b000000000000");
    assert_eq!(change.flags_after_b, "107f000000000000");
}

#[test]
fn an_already_active_slot_b_partition_is_rejected() {
    let gpt = SyntheticGpt::standard().with_flags("vbmeta_b", 0x107f << 48);
    let err = rejection(activate(&gpt), "vbmeta_b is already active");
    assert!(matches!(err, Front2Error::Unsupported(_)));
    let boot = SyntheticGpt::standard().with_flags("boot_b", 0x0077 << 48);
    rejection(activate(&boot), "boot_b is already active");
}

#[test]
fn a_repeated_partition_name_is_rejected() {
    let gpt = SyntheticGpt::standard().with_entry("misc", 0);
    let err = rejection(activate(&gpt), "more than once");
    assert!(matches!(err, Front2Error::Image(_)));
}

#[test]
fn a_different_signature_is_rejected() {
    let gpt = SyntheticGpt::standard();
    let mut main = gpt.main();
    main[MAIN_HEADER] = b'X';
    seal(GptCopyKind::Main, &mut main);
    let err = rejection(
        SlotActivation::prepare(LUN, &main, &gpt.backup()),
        "EFI PART signature",
    );
    assert!(matches!(err, Front2Error::Image(_)));
    let mut backup = gpt.backup();
    backup[BACKUP_HEADER + 7] = b'X';
    seal(GptCopyKind::Backup, &mut backup);
    rejection(
        SlotActivation::prepare(LUN, &gpt.main(), &backup),
        "EFI PART signature",
    );
}

#[test]
fn a_dump_of_the_wrong_size_is_rejected() {
    let gpt = SyntheticGpt::standard();
    let main = gpt.main();
    rejection(
        SlotActivation::prepare(LUN, &main[..SECTOR * 5], &gpt.backup()),
        "a dump of 24576 bytes is required",
    );
    rejection(
        SlotActivation::prepare(LUN, &gpt.backup(), &gpt.main()),
        "a dump of 24576 bytes is required",
    );
}

#[test]
fn a_header_whose_positions_do_not_fit_the_dump_is_rejected() {
    let gpt = SyntheticGpt::standard();
    let mut backup = gpt.backup();
    backup[BACKUP_HEADER + 72..BACKUP_HEADER + 80].copy_from_slice(&(LAST_LBA - 5).to_le_bytes());
    seal(GptCopyKind::Backup, &mut backup);
    rejection(
        SlotActivation::prepare(LUN, &gpt.main(), &backup),
        "the dump layout does not allow this",
    );
    let mut main = gpt.main();
    main[MAIN_HEADER + 24..MAIN_HEADER + 32].copy_from_slice(&2u64.to_le_bytes());
    seal(GptCopyKind::Main, &mut main);
    rejection(
        SlotActivation::prepare(LUN, &main, &gpt.backup()),
        "the dump layout does not allow this",
    );
}

#[test]
fn a_gpt_without_slot_partitions_is_rejected() {
    let gpt = SyntheticGpt::empty()
        .with_entry("misc", 0)
        .with_entry("super", 0);
    let err = rejection(activate(&gpt), "no partition whose name ends with _a");
    assert!(matches!(err, Front2Error::Unsupported(_)));
}

#[test]
fn the_current_gpt_check_accepts_the_original_and_the_candidate() {
    let gpt = SyntheticGpt::standard();
    let activation = activate(&gpt).expect("activation");
    for (kind, original) in [
        (GptCopyKind::Main, gpt.main()),
        (GptCopyKind::Backup, gpt.backup()),
    ] {
        let candidate = &activation.output(kind).expect("output").data;
        require_current_copy_matches(kind, &original, candidate).expect("original device");
        require_current_copy_matches(kind, candidate, candidate).expect("written device");
    }
    let other_slot_free_change = SyntheticGpt::standard().with_flags("misc", 1 << 60);
    let candidate = &activation.output(GptCopyKind::Main).expect("main").data;
    require_current_copy_matches(GptCopyKind::Main, &other_slot_free_change.main(), candidate)
        .expect("the flags field of every entry is a permitted position");
}

#[test]
fn the_current_gpt_check_rejects_an_invalid_current_copy() {
    let gpt = SyntheticGpt::standard();
    let activation = activate(&gpt).expect("activation");
    let candidate = &activation.output(GptCopyKind::Main).expect("main").data;
    let mut damaged_header = gpt.main();
    damaged_header[MAIN_HEADER + 60] ^= 0x01;
    rejection(
        require_current_copy_matches(GptCopyKind::Main, &damaged_header, candidate),
        "header CRC32",
    );
    let mut damaged_table = gpt.main();
    damaged_table[MAIN_TABLE + 56] ^= 0x01;
    rejection(
        require_current_copy_matches(GptCopyKind::Main, &damaged_table, candidate),
        "table CRC32",
    );
}

#[test]
fn the_current_gpt_check_rejects_differences_outside_the_permitted_positions() {
    let gpt = SyntheticGpt::standard();
    let activation = activate(&gpt).expect("activation");
    let candidate = activation
        .output(GptCopyKind::Main)
        .expect("main")
        .data
        .clone();
    for offset in [
        MAIN_TABLE + 56,
        MAIN_TABLE + 32,
        MAIN_TABLE + 16,
        MAIN_TABLE + ENTRY_BYTES * 6 + 40,
        MAIN_HEADER + 56,
        MAIN_HEADER + 80,
        0,
        MAIN_TABLE + ENTRY_BYTES * 8,
    ] {
        let mut changed = candidate.clone();
        changed[offset] ^= 0x01;
        let err = rejection(
            require_current_copy_matches(GptCopyKind::Main, &gpt.main(), &changed),
            &format!("byte {offset} differs outside the permitted positions"),
        );
        assert!(matches!(err, Front2Error::Verification(_)));
    }
    rejection(
        require_current_copy_matches(GptCopyKind::Main, &gpt.main(), &candidate[..SECTOR * 5]),
        "the candidate is 20480 bytes",
    );
}

#[test]
fn the_current_gpt_check_compares_the_whole_table_areas() {
    let gpt = SyntheticGpt::standard();
    let other = SyntheticGpt::standard().with_flags("misc", 1);
    require_matching_table_areas(&gpt.main(), &gpt.backup()).expect("same tables");
    let err = rejection(
        require_matching_table_areas(&gpt.main(), &other.backup()),
        "do not agree in tables",
    );
    assert!(matches!(err, Front2Error::Verification(_)));
    let mut main = gpt.main();
    main[MAIN_TABLE + SECTOR * 3] = 1;
    rejection(
        require_matching_table_areas(&main, &gpt.backup()),
        "table areas",
    );
    rejection(
        require_matching_table_areas(&gpt.main()[..SECTOR * 5], &gpt.backup()),
        "a dump of 24576 bytes is required",
    );
}

fn backup_with(gpt: &SyntheticGpt, change: impl FnOnce(&mut [u8])) -> Vec<u8> {
    let mut backup = gpt.backup();
    change(&mut backup[BACKUP_HEADER..BACKUP_HEADER + SECTOR]);
    seal(GptCopyKind::Backup, &mut backup);
    backup
}

fn main_with(gpt: &SyntheticGpt, change: impl FnOnce(&mut [u8])) -> Vec<u8> {
    let mut main = gpt.main();
    change(&mut main);
    seal(GptCopyKind::Main, &mut main);
    main
}

#[test]
fn letter_1126_item_1_a_backup_with_another_disk_guid_is_rejected() {
    let gpt = SyntheticGpt::standard();
    let backup = backup_with(&gpt, |header| header[56] ^= 1);
    GptCopy::parse(GptCopyKind::Backup, &backup).expect("the copy alone is valid");
    let err = rejection(
        SlotActivation::prepare(LUN, &gpt.main(), &backup),
        "do not agree in disk GUID",
    );
    assert!(matches!(err, Front2Error::Verification(_)));
    rejection(
        require_matching_table_areas(&gpt.main(), &backup),
        "do not agree in disk GUID",
    );
}

#[test]
fn letter_1126_item_2_a_backup_at_a_position_the_main_header_does_not_name_is_rejected() {
    let gpt = SyntheticGpt::standard();
    let misplaced = LAST_LBA + 10000;
    let backup = backup_with(&gpt, |header| {
        header[24..32].copy_from_slice(&misplaced.to_le_bytes());
        header[72..80].copy_from_slice(&(misplaced - 4).to_le_bytes());
    });
    GptCopy::parse(GptCopyKind::Backup, &backup).expect("the copy alone is valid");
    rejection(
        SlotActivation::prepare(LUN, &gpt.main(), &backup),
        "do not agree in backup header position",
    );

    let backup = backup_with(&gpt, |header| {
        header[32..40].copy_from_slice(&2u64.to_le_bytes());
    });
    rejection(
        SlotActivation::prepare(LUN, &gpt.main(), &backup),
        "do not agree in main header position",
    );

    let backup = backup_with(&gpt, |header| {
        header[48..56].copy_from_slice(&(LAST_LBA - 6).to_le_bytes());
    });
    rejection(
        SlotActivation::prepare(LUN, &gpt.main(), &backup),
        "do not agree in usable LBA range",
    );
}

#[test]
fn letter_1126_item_3_partitions_that_cover_the_same_sectors_are_rejected() {
    let gpt = SyntheticGpt::standard();
    let main = main_with(&gpt, |main| {
        let first_range = main[MAIN_TABLE + 32..MAIN_TABLE + 48].to_vec();
        main[MAIN_TABLE + ENTRY_BYTES + 32..MAIN_TABLE + ENTRY_BYTES + 48]
            .copy_from_slice(&first_range);
    });
    let err = rejection(
        PartitionMap::from_main_copies(&[(LUN, &main)]),
        "entries \"boot_a\" and \"boot_b\" cover the same LBA",
    );
    assert!(matches!(err, Front2Error::Image(_)));

    let main = main_with(&gpt, |main| {
        let last = FIRST_PARTITION_LBA + PARTITION_SECTORS;
        main[MAIN_TABLE + 40..MAIN_TABLE + 48].copy_from_slice(&last.to_le_bytes());
    });
    rejection(
        PartitionMap::from_main_copies(&[(LUN, &main)]),
        "cover the same LBA",
    );
}

#[test]
fn a_partition_outside_the_usable_range_is_rejected() {
    let gpt = SyntheticGpt::standard();
    let main = main_with(&gpt, |main| {
        let entry = MAIN_TABLE + ENTRY_BYTES * 6;
        main[entry + 32..entry + 40].copy_from_slice(&(LAST_LBA - 5).to_le_bytes());
        main[entry + 40..entry + 48].copy_from_slice(&(LAST_LBA - 4).to_le_bytes());
    });
    rejection(
        PartitionMap::from_main_copies(&[(LUN, &main)]),
        "the usable range is 6 to 4090",
    );

    let main = main_with(&gpt, |main| {
        main[MAIN_TABLE + 32..MAIN_TABLE + 40].copy_from_slice(&5u64.to_le_bytes());
    });
    rejection(
        PartitionMap::from_main_copies(&[(LUN, &main)]),
        "the usable range is 6 to 4090",
    );
}

#[test]
fn a_usable_range_that_covers_a_gpt_copy_is_rejected() {
    let gpt = SyntheticGpt::standard();
    let main = main_with(&gpt, |main| {
        main[MAIN_HEADER + 40..MAIN_HEADER + 48].copy_from_slice(&5u64.to_le_bytes());
    });
    rejection(
        GptCopy::parse(GptCopyKind::Main, &main),
        "gives the usable LBA range 5 to 4090",
    );

    let backup = backup_with(&gpt, |header| {
        header[48..56].copy_from_slice(&(LAST_LBA - 4).to_le_bytes());
    });
    rejection(
        GptCopy::parse(GptCopyKind::Backup, &backup),
        "gives the usable LBA range 6 to 4091",
    );
}

#[test]
fn letter_1136_a_main_copy_whose_usable_range_covers_the_backup_table_is_rejected() {
    let gpt = SyntheticGpt::standard();
    for last_usable in [LAST_LBA - 4, LAST_LBA - 1] {
        let main = main_with(&gpt, |main| {
            main[MAIN_HEADER + 48..MAIN_HEADER + 56].copy_from_slice(&last_usable.to_le_bytes());
        });
        rejection(
            GptCopy::parse(GptCopyKind::Main, &main),
            &format!("gives the usable LBA range 6 to {last_usable}"),
        );
    }

    let main = main_with(&gpt, |main| {
        main[MAIN_HEADER + 48..MAIN_HEADER + 56].copy_from_slice(&(LAST_LBA - 1).to_le_bytes());
        main[MAIN_TABLE + 32..MAIN_TABLE + 40].copy_from_slice(&(LAST_LBA - 4).to_le_bytes());
        main[MAIN_TABLE + 40..MAIN_TABLE + 48].copy_from_slice(&(LAST_LBA - 1).to_le_bytes());
    });
    let err = rejection(
        PartitionMap::from_main_copies(&[(LUN, &main)]),
        "gives the usable LBA range 6 to 4094",
    );
    assert!(matches!(err, Front2Error::Image(_)));

    let main = main_with(&gpt, |main| {
        let entry = MAIN_TABLE + ENTRY_BYTES * 6;
        main[entry + 32..entry + 40].copy_from_slice(&(LAST_LBA - 8).to_le_bytes());
        main[entry + 40..entry + 48].copy_from_slice(&(LAST_LBA - 5).to_le_bytes());
    });
    let map = PartitionMap::from_main_copies(&[(LUN, &main)]).expect("the last usable sectors");
    assert_eq!(
        map.locate("misc").expect("misc"),
        PartitionLocation {
            lun: LUN,
            start_sector: LAST_LBA - 8,
            sectors: 4
        }
    );
}

#[test]
fn the_partition_map_rejects_a_lun_that_is_given_twice() {
    let gpt = SyntheticGpt::empty().with_entry("misc", 0);
    let other = SyntheticGpt::empty().with_entry("persist", 0);
    rejection(
        PartitionMap::from_main_copies(&[(LUN, &gpt.main()), (LUN, &other.main())]),
        "LUN 4 is in the list of GPT copies more than once",
    );
}

#[test]
fn the_misc_sector_requests_a_data_wipe_and_keeps_the_second_half() {
    let original: Vec<u8> = (0..SECTOR).map(|i| (i % 251) as u8 + 1).collect();
    let sector = misc_wipe_sector(&original).expect("misc sector");
    assert_eq!(sector.len(), SECTOR);
    assert_eq!(&sector[..13], b"boot-recovery");
    assert!(sector[13..64].iter().all(|byte| *byte == 0));
    assert_eq!(&sector[64..85], b"recovery\n--wipe_data\n");
    assert!(sector[85..2048].iter().all(|byte| *byte == 0));
    assert_eq!(&sector[2048..], &original[2048..]);
    assert!(original[2048..].iter().all(|byte| *byte != 0));
}

#[test]
fn a_misc_sector_of_the_wrong_size_is_rejected() {
    rejection(
        misc_wipe_sector(&[0u8; SECTOR - 1]),
        "4096 bytes are required",
    );
    rejection(
        misc_wipe_sector(&[0u8; SECTOR * 2]),
        "4096 bytes are required",
    );
    rejection(misc_wipe_sector(&[]), "4096 bytes are required");
}

#[test]
fn the_partition_map_locates_names_across_luns() {
    let first = SyntheticGpt::empty()
        .with_entry("super", 0)
        .with_entry("metadata", 0)
        .main();
    let second = SyntheticGpt::standard().main();
    let map = PartitionMap::from_main_copies(&[(0, &first), (4, &second)]).expect("map");
    assert_eq!(
        map.locate("super").expect("super"),
        PartitionLocation {
            lun: 0,
            start_sector: FIRST_PARTITION_LBA,
            sectors: PARTITION_SECTORS
        }
    );
    assert_eq!(
        map.locate("vbmeta_b").expect("vbmeta_b"),
        PartitionLocation {
            lun: 4,
            start_sector: FIRST_PARTITION_LBA + 3 * PARTITION_SECTORS,
            sectors: PARTITION_SECTORS
        }
    );
    assert_eq!(map.partitions().len(), 9);
    let err = rejection(map.locate("userdata"), "userdata");
    assert!(matches!(err, Front2Error::Plan(_)));
    let value = serde_json::to_value(&map).expect("serialize");
    assert_eq!(
        value["metadata"],
        json!({"lun": 0, "start_sector": 80, "sectors": 16})
    );
}

#[test]
fn the_partition_map_rejects_repeated_names_and_invalid_copies() {
    let standard = SyntheticGpt::standard();
    let err = rejection(
        PartitionMap::from_main_copies(&[(0, &standard.main()), (4, &standard.main())]),
        "is on LUN 0 and on LUN 4",
    );
    assert!(matches!(err, Front2Error::Image(_)));
    let repeated = SyntheticGpt::standard().with_entry("boot_a", 0).main();
    rejection(
        PartitionMap::from_main_copies(&[(4, &repeated)]),
        "more than once",
    );
    rejection(
        PartitionMap::from_main_copies(&[(4, &standard.backup())]),
        "LUN 4: GPT main copy is 20480 bytes",
    );
    let mut damaged = standard.main();
    damaged[MAIN_TABLE] ^= 0x01;
    rejection(
        PartitionMap::from_main_copies(&[(4, &damaged)]),
        "table CRC32",
    );
}

fn real_directories() -> Option<(PathBuf, PathBuf)> {
    let backup = std::env::var_os("FRONT2_REAL_BACKUP_DIR").map(PathBuf::from);
    let evidence = std::env::var_os("FRONT2_REAL_EVIDENCE_DIR").map(PathBuf::from);
    match (backup, evidence) {
        (Some(backup), Some(evidence)) => Some((backup, evidence)),
        _ => {
            eprintln!(
                "skipped: FRONT2_REAL_BACKUP_DIR and FRONT2_REAL_EVIDENCE_DIR are not both set"
            );
            None
        }
    }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("evidence JSON")).expect("JSON")
}

fn read_first_sector(path: &Path) -> Vec<u8> {
    let mut sector = vec![0u8; SECTOR];
    fs::File::open(path)
        .expect("open for reading")
        .read_exact(&mut sector)
        .expect("first sector");
    sector
}

fn real_gpt(backup: &Path, kind: GptCopyKind, lun: u8) -> Vec<u8> {
    let file = format!("gpt_{}{lun}.bin", kind.label());
    fs::read(backup.join("gpt").join(file)).expect("real GPT copy")
}

fn rows<'value>(value: &'value Value, key: &str) -> &'value Vec<Value> {
    value[key].as_array().expect("an array in the evidence")
}

fn assert_real_outputs(activation: &SlotActivation, proposal_directory: &Path, proposal: &Value) {
    for output in &activation.outputs {
        let label = output.copy.label();
        let lun = output.lun;
        let file = format!("gpt_{label}{lun}-slot-b.bin");
        let expected = fs::read(proposal_directory.join(&file)).expect("expected GPT image");
        assert!(output.data == expected, "{file} differs");
        let row = rows(proposal, "gpt_outputs")
            .iter()
            .find(|row| row["lun"] == lun && row["copy"] == label)
            .expect("proposal row");
        assert_eq!(row["sha256"], output.sha256, "{file}");
        assert_eq!(row["start_sector"], output.start_sector, "{file}");
        assert_eq!(row["sectors"], output.sectors, "{file}");
        assert_eq!(row["bytes"], output.bytes, "{file}");
        assert_eq!(row["restore_sha256"], output.restore_sha256, "{file}");
        println!(
            "{file}: bytes identical, sha256 {} start_sector {} sectors {} equal proposal.json",
            output.sha256, output.start_sector, output.sectors
        );
    }
}

fn assert_real_changes(activation: &SlotActivation, proposal: &Value) {
    let expected: Vec<Value> = rows(proposal, "changes")
        .iter()
        .filter(|change| change["lun"] == activation.lun)
        .cloned()
        .collect();
    let actual = serde_json::to_value(&activation.changes).expect("changes");
    assert!(!expected.is_empty());
    assert_eq!(actual, Value::Array(expected));
    println!(
        "LUN {}: {} changes equal proposal.json",
        activation.lun,
        activation.changes.len()
    );
}

fn assert_real_current_checks(backup: &Path, evidence: &Path, activation: &SlotActivation) {
    let lun = activation.lun;
    let retry = evidence.join("installation").join("rollback-retry");
    for output in &activation.outputs {
        let original = real_gpt(backup, output.copy, lun);
        require_current_copy_matches(output.copy, &original, &output.data)
            .expect("the original GPT differs from the candidate only at permitted positions");
        let preflight = retry.join(format!(
            "retry-preflight-gpt_{}{lun}.bin",
            output.copy.label()
        ));
        if !preflight.exists() {
            println!("{}: not present, check not run", preflight.display());
            continue;
        }
        let current = fs::read(&preflight).expect("retry preflight readback");
        require_current_copy_matches(output.copy, &current, &output.data)
            .expect("the retry preflight readback passes the current GPT check");
        println!(
            "{}: passes the current GPT check, identical to the candidate: {}",
            preflight.display(),
            current == output.data
        );
    }
    require_matching_table_areas(
        &real_gpt(backup, GptCopyKind::Main, lun),
        &real_gpt(backup, GptCopyKind::Backup, lun),
    )
    .expect("original table areas");
}

#[test]
fn real_backup_gives_the_gpt_images_that_worked_on_the_device_when_available() {
    let Some((backup, evidence)) = real_directories() else {
        return;
    };
    let proposal_directory = evidence.join("prepared-gsi").join("activation-proposal");
    let proposal = read_json(&proposal_directory.join("proposal.json"));
    for lun in [0u8, 4u8] {
        let activation = SlotActivation::prepare(
            lun,
            &real_gpt(&backup, GptCopyKind::Main, lun),
            &real_gpt(&backup, GptCopyKind::Backup, lun),
        )
        .expect("real activation");
        assert_real_outputs(&activation, &proposal_directory, &proposal);
        assert_real_changes(&activation, &proposal);
        assert_real_current_checks(&backup, &evidence, &activation);
    }
}

#[test]
fn real_misc_gives_the_wipe_sector_that_worked_on_the_device_when_available() {
    let Some((backup, evidence)) = real_directories() else {
        return;
    };
    let proposal_directory = evidence.join("prepared-gsi").join("activation-proposal");
    let proposal = read_json(&proposal_directory.join("proposal.json"));
    let original = read_first_sector(&backup.join("partitions").join("lun0").join("misc.bin"));
    let sector = misc_wipe_sector(&original).expect("misc sector");
    let expected =
        fs::read(proposal_directory.join("misc-first-sector-wipe.bin")).expect("expected sector");
    assert!(sector == expected, "misc-first-sector-wipe.bin differs");
    let row = rows(&proposal, "initialization_outputs")
        .iter()
        .find(|row| row["partition"] == "misc")
        .expect("misc row");
    assert_eq!(row["sha256"], sha256_bytes(&sector));
    assert_eq!(row["bytes"], sector.len());
    println!(
        "misc-first-sector-wipe.bin: bytes identical, sha256 {} equals proposal.json",
        sha256_bytes(&sector)
    );
}

fn is_partition_row(row: &Value) -> bool {
    row["partition"]
        .as_str()
        .is_some_and(|name| !name.starts_with("gpt_"))
}

fn assert_located(map: &PartitionMap, row: &Value, with_sectors: bool) -> String {
    let name = row["partition"].as_str().expect("partition name");
    let location = map.locate(name).expect("location");
    assert_eq!(row["lun"], location.lun, "{name}");
    assert_eq!(row["start_sector"], location.start_sector, "{name}");
    if with_sectors {
        assert_eq!(row["sectors"], location.sectors, "{name}");
    }
    println!(
        "{name}: lun {} start_sector {} sectors {} (sectors compared: {with_sectors})",
        location.lun, location.start_sector, location.sectors
    );
    name.to_string()
}

#[test]
fn real_gpt_gives_the_partition_positions_of_the_install_plan_when_available() {
    let Some((backup, evidence)) = real_directories() else {
        return;
    };
    let copies: Vec<(u8, Vec<u8>)> = (0u8..6)
        .map(|lun| (lun, real_gpt(&backup, GptCopyKind::Main, lun)))
        .collect();
    let borrowed: Vec<(u8, &[u8])> = copies
        .iter()
        .map(|(lun, bytes)| (*lun, bytes.as_slice()))
        .collect();
    let map = PartitionMap::from_main_copies(&borrowed).expect("real partition map");
    let plan = read_json(
        &evidence
            .join("prepared-gsi")
            .join("install-plan-proposal.json"),
    );
    let mut steps = Vec::new();
    for row in rows(&plan, "steps")
        .iter()
        .filter(|row| is_partition_row(row))
    {
        let whole_partition = row["partition"] != "misc";
        steps.push(assert_located(&map, row, whole_partition));
    }
    let mut restores = Vec::new();
    for row in rows(&plan, "restore_files")
        .iter()
        .filter(|row| is_partition_row(row))
    {
        restores.push(assert_located(&map, row, true));
    }
    for name in ["vbmeta_b", "super", "misc"] {
        assert!(steps.iter().any(|known| known == name), "{name} step");
        assert!(restores.iter().any(|known| known == name), "{name} restore");
    }
    println!(
        "partition map: {} names, {} plan steps and {} restore rows equal install-plan-proposal.json",
        map.partitions().len(),
        steps.len(),
        restores.len()
    );
}
