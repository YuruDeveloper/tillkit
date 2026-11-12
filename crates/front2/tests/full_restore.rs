#[expect(
    dead_code,
    reason = "the restore tests use a part of this fixture of the full backup"
)]
mod full_backup_support;
mod gpt_support;
mod install_support;

use front2::Front2Error;
use front2::full_backup::{FullBackupPlan, FullBackupRestore, RestoreArea};
use full_backup_support::BackupFixture;
use gpt_support::{LAST_LBA, SyntheticGpt};
use install_support::{MockCall, MockStorage};
use std::fs;
use std::path::PathBuf;

const ACTIVE: u64 = 1 << 50;

fn load_boot_tables(storage: &mut MockStorage, flags_a: u64, flags_b: u64) {
    for (lun, name, flags) in [(1, "xbl_a", flags_a), (2, "xbl_b", flags_b)] {
        let table = SyntheticGpt::empty().with_entry(name, flags);
        storage.load(lun, 0, &table.main());
        storage.load(lun, LAST_LBA - 4, &table.backup());
    }
}

fn backed_up(flags_a: u64, flags_b: u64) -> (BackupFixture, PathBuf) {
    let mut fixture = BackupFixture::new();
    load_boot_tables(&mut fixture.storage.inner, flags_a, flags_b);
    let output = fixture.output();
    let plan = FullBackupPlan::read(&mut fixture.storage).expect("backup plan");
    plan.write(&mut fixture.storage, &output)
        .expect("backup files");
    fixture.storage.inner.calls().borrow_mut().clear();
    (fixture, output)
}

fn area(restore: &FullBackupRestore, name: &str) -> RestoreArea {
    restore
        .areas()
        .iter()
        .find(|area| area.partition.name == name)
        .expect("the backup holds the area")
        .clone()
}

fn change_on_device(storage: &mut MockStorage, area: &RestoreArea) {
    let mut data = storage.content(area.partition.lun, area.start_sector, 1);
    data[0] ^= 0xff;
    storage.load(area.partition.lun, area.start_sector, &data);
}

fn names(areas: &[RestoreArea]) -> Vec<&str> {
    areas
        .iter()
        .map(|area| area.partition.name.as_str())
        .collect()
}

fn device_calls(storage: &MockStorage) -> Vec<MockCall> {
    storage
        .calls()
        .borrow()
        .iter()
        .filter(|call| call.changes_the_device())
        .copied()
        .collect()
}

#[test]
fn a_device_that_is_the_same_as_the_backup_needs_no_write() {
    let (mut fixture, backup) = backed_up(ACTIVE, 0);
    let restore = FullBackupRestore::load(&backup, true).expect("restore");
    assert_eq!(restore.boot_lun(), 1);
    assert!(names(restore.areas()).contains(&"ALIGN_TO_128K_1"));
    assert!(names(restore.areas()).contains(&"userdata"));

    let storage = &mut fixture.storage.inner;
    let check = restore.compare(storage).expect("check");
    assert!(check.is_same(), "{:?}", names(&check.different));
    assert_eq!(check.areas, restore.areas().len());
    restore.write(storage, &check).expect("nothing to write");
    assert!(device_calls(storage).is_empty());
}

#[test]
fn the_restore_writes_only_the_different_areas_and_selects_the_boot_lun() {
    let (mut fixture, backup) = backed_up(ACTIVE, 0);
    let restore = FullBackupRestore::load(&backup, true).expect("restore");
    let storage = &mut fixture.storage.inner;
    load_boot_tables(storage, 0, ACTIVE);
    storage.start_with_boot_lun(2);
    change_on_device(storage, &area(&restore, "super"));
    change_on_device(storage, &area(&restore, "ALIGN_TO_128K_1"));

    let check = restore.compare(storage).expect("check");
    assert_eq!(
        names(&check.different),
        [
            "ALIGN_TO_128K_1",
            "super",
            "gpt_backup1",
            "gpt_backup2",
            "gpt_main1",
            "gpt_main2"
        ]
    );
    assert!(!check.boot_lun_selected);
    assert!(device_calls(storage).is_empty());

    restore.write(storage, &check).expect("restore");
    let calls = device_calls(storage);
    let (commands, writes): (Vec<MockCall>, Vec<MockCall>) =
        calls.iter().partition(|call| call.is_boot_lun_command());
    assert_eq!(commands, [MockCall::SetBootLun { lun: 1 }]);
    assert_eq!(writes.len(), check.different.len());
    for (call, area) in writes.iter().zip(&check.different) {
        assert!(
            call.writes_into(area.partition.lun, area.start_sector, area.sectors),
            "{call:?} {}",
            area.partition.name
        );
    }
    assert_eq!(calls.last(), commands.last());
    assert!(restore.compare(storage).expect("second check").is_same());
}

#[test]
fn letter_1365_a_changed_backup_file_stops_the_restore_before_the_first_write() {
    let (mut fixture, backup) = backed_up(ACTIVE, 0);
    let restore = FullBackupRestore::load(&backup, true).expect("restore");
    let storage = &mut fixture.storage.inner;
    let later = area(&restore, "super");
    change_on_device(storage, &area(&restore, "ALIGN_TO_128K_1"));
    change_on_device(storage, &later);
    let check = restore.compare(storage).expect("check");
    assert_eq!(names(&check.different), ["ALIGN_TO_128K_1", "super"]);

    let mut bytes = fs::read(&later.partition.path).expect("read the dump");
    bytes[0] ^= 1;
    fs::write(&later.partition.path, bytes).expect("change the dump");
    let error = restore
        .write(storage, &check)
        .expect_err("a changed backup file");
    assert!(matches!(error, Front2Error::Verification(_)), "{error}");
    assert!(device_calls(storage).is_empty());
}

#[test]
fn letter_1371_a_different_first_boot_image_stops_the_restore_before_the_first_write() {
    let (mut fixture, backup) = backed_up(ACTIVE, 0);
    let restore = FullBackupRestore::load(&backup, true).expect("restore");
    let storage = &mut fixture.storage.inner;
    change_on_device(storage, &area(&restore, "super"));
    change_on_device(storage, &area(&restore, "xbl_a"));
    let check = restore.compare(storage).expect("check");
    assert_eq!(names(&check.different), ["super", "xbl_a"]);
    assert_eq!(check.first_boot_images(), ["xbl_a"]);

    let error = restore
        .write(storage, &check)
        .expect_err("a first boot image differs");
    assert!(matches!(error, Front2Error::Unsupported(_)), "{error}");
    assert!(error.to_string().contains("xbl_a"), "{error}");
    assert!(device_calls(storage).is_empty());
}

#[test]
fn a_backup_that_does_not_show_one_boot_lun_is_rejected() {
    for (flags_a, flags_b) in [(0, 0), (ACTIVE, ACTIVE)] {
        let (_fixture, backup) = backed_up(flags_a, flags_b);
        let error = FullBackupRestore::load(&backup, true).expect_err("no single boot LUN");
        assert!(matches!(error, Front2Error::Unsupported(_)), "{error}");
    }
}

#[test]
fn a_backup_of_a_device_that_starts_from_lun_2_selects_lun_2() {
    let (mut fixture, backup) = backed_up(0, ACTIVE);
    let restore = FullBackupRestore::load(&backup, true).expect("restore");
    assert_eq!(restore.boot_lun(), 2);
    let storage = &mut fixture.storage.inner;
    let check = restore.compare(storage).expect("check");
    assert!(check.different.is_empty());
    assert!(!check.boot_lun_selected);
    restore.write(storage, &check).expect("restore");
    assert_eq!(device_calls(storage), [MockCall::SetBootLun { lun: 2 }]);
}

#[test]
fn the_restore_without_user_data_does_not_compare_userdata() {
    let (mut fixture, backup) = backed_up(ACTIVE, 0);
    let complete = FullBackupRestore::load(&backup, true).expect("restore");
    let partial = FullBackupRestore::load(&backup, false).expect("restore");
    assert_eq!(partial.areas().len() + 1, complete.areas().len());
    assert!(!names(partial.areas()).contains(&"userdata"));
    let user_data = area(&complete, "userdata");
    assert_eq!(
        complete.bytes() - partial.bytes(),
        user_data.partition.bytes
    );

    let storage = &mut fixture.storage.inner;
    change_on_device(storage, &user_data);
    assert!(partial.compare(storage).expect("check").is_same());
    let check = complete.compare(storage).expect("check");
    assert_eq!(names(&check.different), ["userdata"]);
    assert_eq!(check.different_bytes(), user_data.partition.bytes);
}

#[test]
fn a_device_with_another_lun_size_is_rejected_before_a_read() {
    let (mut fixture, backup) = backed_up(ACTIVE, 0);
    let restore = FullBackupRestore::load(&backup, true).expect("restore");
    fixture.storage.total_sectors *= 2;
    fixture.storage.reads.clear();
    let error = restore
        .compare(&mut fixture.storage)
        .expect_err("another LUN size");
    assert!(matches!(error, Front2Error::Unsupported(_)), "{error}");
    assert!(fixture.storage.reads.is_empty());
}

#[test]
fn a_write_that_the_device_does_not_keep_stops_the_restore() {
    let (mut fixture, backup) = backed_up(ACTIVE, 0);
    let restore = FullBackupRestore::load(&backup, true).expect("restore");
    let storage = &mut fixture.storage.inner;
    let changed = area(&restore, "super");
    change_on_device(storage, &changed);
    let check = restore.compare(storage).expect("check");
    storage.ignore_writes_that_cover(changed.partition.lun, changed.start_sector);
    let error = restore
        .write(storage, &check)
        .expect_err("the readback differs");
    assert!(matches!(error, Front2Error::Verification(_)), "{error}");
    assert!(error.to_string().contains("super"), "{error}");
}

#[test]
fn a_boot_lun_command_without_effect_stops_the_restore() {
    let (mut fixture, backup) = backed_up(ACTIVE, 0);
    let restore = FullBackupRestore::load(&backup, true).expect("restore");
    let storage = &mut fixture.storage.inner;
    storage.start_with_boot_lun(2);
    storage.ignore_the_boot_lun_command();
    let check = restore.compare(storage).expect("check");
    assert!(check.different.is_empty());
    assert!(!check.is_same());
    let error = restore
        .write(storage, &check)
        .expect_err("the boot LUN did not change");
    assert!(matches!(error, Front2Error::Verification(_)), "{error}");
    assert!(error.to_string().contains("boot WLUN"), "{error}");
}
