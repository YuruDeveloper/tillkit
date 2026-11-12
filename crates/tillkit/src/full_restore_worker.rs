use crate::edl_link::EdlLink;
use crate::full_restore_review::FullRestoreReview;
use front2::full_backup::FullBackupRestore;
use front2::install::SectorStorage;
use front2::{Front2Error, Result};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub const NOT_CHECKED: &str =
    "the device was not compared with this full backup on this connection";

pub fn check<Storage: SectorStorage>(
    link: &mut EdlLink<Storage>,
    backup: &Path,
    with_user_data: bool,
    transferred: &AtomicU64,
    expected: &AtomicU64,
) -> Result<FullRestoreReview> {
    link.full_restore = None;
    let restore = FullBackupRestore::load(backup, with_user_data)?;
    transferred.store(0, Ordering::Relaxed);
    expected.store(restore.bytes(), Ordering::Relaxed);
    let check = restore.compare(&mut link.storage)?;
    let review = FullRestoreReview::of(backup, with_user_data, restore.boot_lun(), &check);
    link.full_restore = Some((restore, check, review.clone()));
    Ok(review)
}

pub fn restore<Storage: SectorStorage>(
    link: &mut EdlLink<Storage>,
    review: &FullRestoreReview,
    transferred: &AtomicU64,
    expected: &AtomicU64,
) -> Result<()> {
    let checked = link.full_restore.take();
    let Some((restore, check, _)) = checked.filter(|(_, _, shown)| shown == review) else {
        return Err(Front2Error::Plan(NOT_CHECKED.into()));
    };
    link.install = None;
    transferred.store(0, Ordering::Relaxed);
    expected.store(check.different_bytes().saturating_mul(2), Ordering::Relaxed);
    restore.write(&mut link.storage, &check)
}

pub fn restart<Storage: SectorStorage>(link: &mut EdlLink<Storage>) -> Result<()> {
    link.install = None;
    link.full_restore = None;
    link.storage.reset()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::full_backup_support::BackupFixture;
    use crate::gpt_support::{LAST_LBA, SyntheticGpt};
    use crate::install_support::{InstallFixture, MockStorage};
    use crate::install_worker;
    use front2::full_backup::FullBackupPlan;
    use front2::install::{DataWipeApproval, InstallProgress, StoredInstallPlan, WriteApproval};
    use std::path::PathBuf;
    use tempfile::TempDir;

    const ACTIVE: u64 = 1 << 50;

    fn backed_up() -> (TempDir, PathBuf, EdlLink<MockStorage>) {
        let mut fixture = BackupFixture::new();
        for (lun, name, flags) in [(1, "xbl_a", ACTIVE), (2, "xbl_b", 0)] {
            let table = SyntheticGpt::empty().with_entry(name, flags);
            fixture.storage.inner.load(lun, 0, &table.main());
            fixture
                .storage
                .inner
                .load(lun, LAST_LBA - 4, &table.backup());
        }
        let output = fixture.output();
        let plan = FullBackupPlan::read(&mut fixture.storage).expect("backup plan");
        plan.write(&mut fixture.storage, &output)
            .expect("backup files");
        fixture.storage.inner.calls().borrow_mut().clear();
        let BackupFixture {
            directory, storage, ..
        } = fixture;
        (directory, output, EdlLink::new(storage.inner))
    }

    fn change_on_device(link: &mut EdlLink<MockStorage>, backup: &Path, name: &str) {
        let restore = FullBackupRestore::load(backup, true).expect("restore");
        let area = restore
            .areas()
            .iter()
            .find(|area| area.partition.name == name)
            .expect("the backup holds the area");
        let (lun, start_sector) = (area.partition.lun, area.start_sector);
        let mut data = link.storage.content(lun, start_sector, 1);
        data[0] ^= 0xff;
        link.storage.load(lun, start_sector, &data);
    }

    fn install_progress() -> (StoredInstallPlan, InstallProgress, String) {
        let fixture = InstallFixture::standard();
        let stored = fixture.stored_plan();
        let sha256 = stored.plan_sha256().to_string();
        let progress = InstallProgress::new(&sha256);
        (stored, progress, sha256)
    }

    fn device_changes(link: &EdlLink<MockStorage>) -> usize {
        let calls = link.storage.calls();
        calls
            .borrow()
            .iter()
            .filter(|call| call.changes_the_device())
            .count()
    }

    #[test]
    fn letter_1371_a_restore_drops_the_install_progress_of_the_connection() {
        let (_directory, backup, mut link) = backed_up();
        change_on_device(&mut link, &backup, "super");
        let (stored, progress, _) = install_progress();
        link.install = Some((stored, progress));
        let (transferred, expected) = (AtomicU64::new(0), AtomicU64::new(0));

        let review = check(&mut link, &backup, true, &transferred, &expected).expect("check");
        assert_eq!(review.different, ["super"]);
        assert_eq!(review.boot_lun, 1);
        assert!(review.can_be_written());
        assert!(link.install.is_some(), "the check only reads");
        assert_eq!(device_changes(&link), 0);

        restore(&mut link, &review, &transferred, &expected).expect("restore");
        assert!(link.install.is_none() && link.full_restore.is_none());
        let both_ways = review.different_bytes * 2;
        assert_eq!(expected.load(Ordering::Relaxed), both_ways);
        assert_eq!(device_changes(&link), 1);

        let again = restore(&mut link, &review, &transferred, &expected);
        let error = again.expect_err("the check was used");
        assert!(error.to_string().contains(NOT_CHECKED), "{error}");
    }

    #[test]
    fn a_review_of_another_check_writes_nothing() {
        let (_directory, backup, mut link) = backed_up();
        change_on_device(&mut link, &backup, "super");
        let (transferred, expected) = (AtomicU64::new(0), AtomicU64::new(0));
        let review = check(&mut link, &backup, true, &transferred, &expected).expect("check");
        let mut other = review.clone();
        other.different.clear();

        let error = restore(&mut link, &other, &transferred, &expected).expect_err("other check");
        assert!(error.to_string().contains(NOT_CHECKED), "{error}");
        assert_eq!(device_changes(&link), 0);
        assert!(link.full_restore.is_none(), "a new check is needed");
    }

    #[test]
    fn a_review_of_another_user_data_choice_writes_nothing() {
        let (_directory, backup, mut link) = backed_up();
        change_on_device(&mut link, &backup, "super");
        let (transferred, expected) = (AtomicU64::new(0), AtomicU64::new(0));
        let review = check(&mut link, &backup, false, &transferred, &expected).expect("check");
        let other = FullRestoreReview {
            with_user_data: true,
            ..review.clone()
        };

        let error = restore(&mut link, &other, &transferred, &expected).expect_err("other choice");
        assert!(error.to_string().contains(NOT_CHECKED), "{error}");
        assert_eq!(device_changes(&link), 0);
    }

    #[test]
    fn letter_1371_an_install_write_drops_the_restore_check_of_the_connection() {
        let (_directory, backup, mut link) = backed_up();
        let (transferred, expected) = (AtomicU64::new(0), AtomicU64::new(0));
        check(&mut link, &backup, false, &transferred, &expected).expect("check");
        assert!(link.full_restore.is_some());
        let (stored, progress, sha256) = install_progress();
        link.install = Some((stored, progress));
        let write = WriteApproval {
            plan_sha256: sha256.clone(),
            granted_unix: 1,
        };
        let data_wipe = DataWipeApproval {
            plan_sha256: sha256,
            granted_unix: 1,
        };
        let _ = install_worker::write(&mut link, &write, &data_wipe);
        assert!(link.full_restore.is_none());
    }

    #[test]
    fn the_restart_resets_the_device_and_drops_both_checks() {
        let (_directory, backup, mut link) = backed_up();
        let (transferred, expected) = (AtomicU64::new(0), AtomicU64::new(0));
        check(&mut link, &backup, true, &transferred, &expected).expect("check");
        let (stored, progress, _) = install_progress();
        link.install = Some((stored, progress));
        restart(&mut link).expect("restart");
        assert!(link.install.is_none() && link.full_restore.is_none());
        let calls = link.storage.calls();
        assert_eq!(
            calls.borrow().iter().filter(|call| call.is_reset()).count(),
            1
        );
    }
}
