use crate::edl_link::EdlLink;
use front2::full_backup::FullBackupPlan;
use front2::install::{
    DataWipeApproval, InstallExecutor, InstallPlan, InstallProgress, RestartApproval,
    SectorStorage, WriteApproval,
};
use front2::{Front2Error, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const NOT_CHECKED: &str = "the device was not checked against an install plan";

pub fn back_up<Storage: SectorStorage>(
    link: &mut EdlLink<Storage>,
    folder: &Path,
    transferred: &AtomicU64,
    expected: &AtomicU64,
) -> Result<(PathBuf, Vec<String>)> {
    if let Some(parent) = folder.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let plan = FullBackupPlan::read(&mut link.storage)?;
    expected.store(plan.read_bytes(), Ordering::Relaxed);
    transferred.store(0, Ordering::Relaxed);
    let result = plan.write(&mut link.storage, folder)?;
    Ok((result.directory, result.single_read_partitions))
}

pub fn check<Storage: SectorStorage>(
    link: &mut EdlLink<Storage>,
    plan_directory: &Path,
) -> Result<InstallProgress> {
    link.install = None;
    let plan = InstallPlan::load(plan_directory)?;
    let mut executor = InstallExecutor::new(&plan, &mut link.storage);
    let outcome = executor.run_preflight();
    let progress = executor.progress().clone();
    link.install = Some((plan, progress.clone()));
    outcome.map(|()| progress)
}

pub fn write<Storage: SectorStorage>(
    link: &mut EdlLink<Storage>,
    write: &WriteApproval,
    data_wipe: &DataWipeApproval,
) -> Result<InstallProgress> {
    continue_install(link, |executor| executor.run_write_steps(write, data_wipe))
}

pub fn wipe<Storage: SectorStorage>(
    link: &mut EdlLink<Storage>,
    write: &WriteApproval,
    data_wipe: &DataWipeApproval,
) -> Result<InstallProgress> {
    continue_install(link, |executor| {
        executor.run_data_wipe_step(write, data_wipe)
    })
}

pub fn restart<Storage: SectorStorage>(
    link: &mut EdlLink<Storage>,
    restart: &RestartApproval,
) -> Result<InstallProgress> {
    continue_install(link, |executor| executor.request_restart(restart))
}

fn continue_install<Storage, Work>(
    link: &mut EdlLink<Storage>,
    work: Work,
) -> Result<InstallProgress>
where
    Storage: SectorStorage,
    Work: FnOnce(&mut InstallExecutor<'_>) -> Result<()>,
{
    let Some((plan, progress)) = link.install.take() else {
        return Err(Front2Error::Plan(NOT_CHECKED.into()));
    };
    link.full_restore = None;
    let mut executor = InstallExecutor::with_progress(&plan, &mut link.storage, progress)?;
    let outcome = work(&mut executor);
    let progress = executor.progress().clone();
    link.install = Some((plan, progress.clone()));
    outcome.map(|()| progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install_support::{InstallFixture, MockCall, MockStorage};
    use std::cell::RefCell;
    use std::rc::Rc;

    type Calls = Rc<RefCell<Vec<MockCall>>>;

    fn planned() -> (InstallFixture, PathBuf, String) {
        let fixture = InstallFixture::standard();
        let plan = fixture.stored_plan();
        let plan_directory = fixture.plan_directory();
        (fixture, plan_directory, plan.plan_sha256().to_string())
    }

    fn link_of(fixture: InstallFixture) -> (tempfile::TempDir, EdlLink<MockStorage>, Calls) {
        let InstallFixture {
            directory,
            storage,
            calls,
            ..
        } = fixture;
        (directory, EdlLink::new(storage), calls)
    }

    fn approvals(plan_sha256: &str) -> (WriteApproval, DataWipeApproval, RestartApproval) {
        let plan_sha256 = plan_sha256.to_string();
        (
            WriteApproval {
                plan_sha256: plan_sha256.clone(),
                granted_unix: 1,
            },
            DataWipeApproval {
                plan_sha256: plan_sha256.clone(),
                granted_unix: 2,
            },
            RestartApproval {
                plan_sha256,
                granted_unix: 3,
            },
        )
    }

    fn changes(calls: &Calls) -> usize {
        calls
            .borrow()
            .iter()
            .filter(|call| call.changes_the_device())
            .count()
    }

    fn resets(calls: &Calls) -> usize {
        calls.borrow().iter().filter(|call| call.is_reset()).count()
    }

    #[test]
    fn a_full_backup_goes_into_a_new_folder_and_tells_the_bytes_that_it_reads() {
        use crate::counted_storage::CountedStorage;
        use crate::full_backup_support::BackupFixture;
        use std::sync::Arc;

        let fixture = BackupFixture::new();
        let folder = fixture.output();
        let transferred = Arc::new(AtomicU64::new(9));
        let expected = AtomicU64::new(0);
        let storage = CountedStorage::new(fixture.storage, transferred.clone());
        let mut link = EdlLink::new(storage);

        let (written, single_read) =
            back_up(&mut link, &folder, &transferred, &expected).expect("backup");
        assert_eq!(written, folder);
        assert!(expected.load(Ordering::Relaxed) > 0);
        assert_eq!(
            transferred.load(Ordering::Relaxed),
            expected.load(Ordering::Relaxed)
        );
        assert!(single_read.iter().any(|name| name == "super"));
        assert_eq!(crate::full_backup_store::refusal(&folder), None);
        assert!(crate::full_backup_store::refusal(fixture.directory.path()).is_some());
        let again = back_up(&mut link, &folder, &transferred, &expected);
        assert!(again.is_err(), "the folder is not new");
    }

    #[test]
    fn the_stages_run_one_at_a_time_over_one_link() {
        let (fixture, plan_directory, plan_sha256) = planned();
        let (write_approval, wipe_approval, restart_approval) = approvals(&plan_sha256);
        let (_directory, mut link, calls) = link_of(fixture);

        let checked = check(&mut link, &plan_directory).expect("check");
        assert!(checked.preflight.is_some_and(|record| record.passed));
        assert_eq!(changes(&calls), 0);

        let written = write(&mut link, &write_approval, &wipe_approval).expect("write");
        assert!(written.boot_lun_passed());
        let reads_and_writes = calls.borrow().len();

        let wiped = wipe(&mut link, &write_approval, &wipe_approval).expect("wipe");
        assert_eq!(wiped.verified_steps(), written.verified_steps() + 1);
        assert!(calls.borrow().len() > reads_and_writes);
        assert_eq!(resets(&calls), 0);

        let restarted = restart(&mut link, &restart_approval).expect("restart");
        assert!(restarted.restart_sent);
        assert_eq!(resets(&calls), 1);
    }

    #[test]
    fn a_stage_without_a_check_leaves_the_device_alone() {
        let (fixture, _, plan_sha256) = planned();
        let (write_approval, wipe_approval, restart_approval) = approvals(&plan_sha256);
        let (_directory, mut link, calls) = link_of(fixture);

        for outcome in [
            write(&mut link, &write_approval, &wipe_approval),
            wipe(&mut link, &write_approval, &wipe_approval),
            restart(&mut link, &restart_approval),
        ] {
            let error = outcome.expect_err("no check ran");
            assert!(error.to_string().contains(NOT_CHECKED), "{error}");
        }
        assert!(calls.borrow().is_empty());
    }

    #[test]
    fn a_failed_check_stops_the_stages_that_change_the_device() {
        let (mut fixture, plan_directory, plan_sha256) = planned();
        let super_location = fixture.location("super");
        let other = vec![0xA5u8; 4096];
        fixture
            .storage
            .load(super_location.lun, super_location.start_sector, &other);
        let (write_approval, wipe_approval, restart_approval) = approvals(&plan_sha256);
        let (_directory, mut link, calls) = link_of(fixture);

        let error = check(&mut link, &plan_directory).expect_err("the device differs");
        assert!(error.to_string().contains("preflight"), "{error}");
        assert!(write(&mut link, &write_approval, &wipe_approval).is_err());
        assert!(restart(&mut link, &restart_approval).is_err());
        assert_eq!(changes(&calls), 0);

        let again = check(&mut link, &plan_directory).expect_err("the device still differs");
        assert!(again.to_string().contains("preflight"), "{again}");
    }

    #[test]
    fn letter_1376_a_check_stopped_by_another_gpt_copy_is_told_in_words_of_the_user() {
        use crate::install_stage::InstallStage;

        let (mut fixture, plan_directory, _) = planned();
        let mut table = fixture.storage.content(1, 0, 1);
        table[0] ^= 0xff;
        fixture.storage.load(1, 0, &table);
        let (_directory, mut link, _) = link_of(fixture);
        let error = check(&mut link, &plan_directory).expect_err("another GPT copy");
        let error = error.to_string();
        assert!(error.contains("gpt_main1"), "{error}");
        assert!(InstallStage::Check.stopped_by_another_device_state(&error));
        assert!(!InstallStage::Write.stopped_by_another_device_state(&error));

        let (mut fixture, plan_directory, _) = planned();
        let super_location = fixture.location("super");
        let other = vec![0xA5u8; 4096];
        fixture
            .storage
            .load(super_location.lun, super_location.start_sector, &other);
        let (_directory, mut link, _) = link_of(fixture);
        let error = check(&mut link, &plan_directory).expect_err("the device differs");
        let error = error.to_string();
        assert!(
            !InstallStage::Check.stopped_by_another_device_state(&error),
            "{error}"
        );
    }

    #[test]
    fn an_approval_of_another_plan_changes_nothing() {
        let (fixture, plan_directory, _) = planned();
        let (other_write, other_wipe, other_restart) = approvals(&"f".repeat(64));
        let (_directory, mut link, calls) = link_of(fixture);

        check(&mut link, &plan_directory).expect("check");
        assert!(write(&mut link, &other_write, &other_wipe).is_err());
        assert!(restart(&mut link, &other_restart).is_err());
        assert_eq!(changes(&calls), 0);
    }
}
