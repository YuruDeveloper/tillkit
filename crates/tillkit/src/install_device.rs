use crate::counted_storage::CountedStorage;
use crate::edl_link::EdlLink;
use crate::edl_storage::EdlStorage;
use front2::install::SectorStorage;
use front2::{Front2Error, Result};
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, MutexGuard};

pub const NOT_CONNECTED: &str = "no device is connected in EDL mode";

pub type Slot<Storage> = Mutex<Option<EdlLink<Storage>>>;
pub type DeviceSlot = Arc<Slot<CountedStorage<EdlStorage>>>;

pub fn connect(slot: &DeviceSlot, loader: &Path, transferred: Arc<AtomicU64>) -> Result<()> {
    let mut held = lock(slot)?;
    *held = None;
    let storage = CountedStorage::new(EdlStorage::open(loader)?, transferred);
    *held = Some(EdlLink::new(storage));
    Ok(())
}

pub fn release<Storage: SectorStorage>(slot: &Slot<Storage>) -> Result<()> {
    *lock(slot)? = None;
    Ok(())
}

pub fn with_link<Storage, Value, Work>(slot: &Slot<Storage>, work: Work) -> Result<Value>
where
    Storage: SectorStorage,
    Work: FnOnce(&mut EdlLink<Storage>) -> Result<Value>,
{
    let mut held = lock(slot)?;
    let link = held
        .as_mut()
        .ok_or_else(|| Front2Error::Shell(NOT_CONNECTED.into()))?;
    work(link)
}

fn lock<Storage: SectorStorage>(
    slot: &Slot<Storage>,
) -> Result<MutexGuard<'_, Option<EdlLink<Storage>>>> {
    slot.lock()
        .map_err(|_| Front2Error::Shell("an earlier operation on the device stopped".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install_support::InstallFixture;
    use crate::install_worker;

    #[test]
    fn work_without_a_link_stops_and_work_with_a_link_runs() {
        let fixture = InstallFixture::standard();
        let plan_directory = fixture.plan_directory();
        fixture.stored_plan();
        let slot = Mutex::new(None);
        let check = |link: &mut EdlLink<_>| install_worker::check(link, &plan_directory);

        let error = with_link(&slot, check).expect_err("no link");
        assert!(error.to_string().contains(NOT_CONNECTED), "{error}");

        *slot.lock().expect("slot") = Some(EdlLink::new(fixture.storage));
        let progress = with_link(&slot, check).expect("check");
        assert!(progress.preflight.is_some_and(|record| record.passed));

        release(&slot).expect("release");
        assert!(with_link(&slot, check).is_err());
    }
}
