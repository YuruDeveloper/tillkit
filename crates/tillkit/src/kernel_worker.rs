use crate::adb_shell::AdbShell;
use crate::clock::now_unix;
use crate::data_paths::DataPaths;
use crate::plan_review::PlanReview;
use crate::plan_source::PlanSource;
use crate::worker::NO_BACKUP;
use front2::kernel::{
    KernelPlan, KernelSet, KernelStore, build_kernel_candidate, find_stored_current_kernel_set,
};
use front2::{DeviceState, Front2Error, PairKind, PairRequest, Result, SupportReport};
use std::path::Path;

pub fn is_backup_of(set: &KernelSet, current: &KernelSet) -> bool {
    set.kind == PairKind::DeviceReadback
        && set.slot_suffix == current.slot_suffix
        && set.boot.sha256 == current.boot.sha256
        && set.vbmeta.sha256 == current.vbmeta.sha256
}

fn current_backup(
    shell: &mut AdbShell,
    state: &DeviceState,
    store: &KernelStore,
) -> Result<KernelSet> {
    let current = find_stored_current_kernel_set(shell, state, store)?;
    store
        .list()?
        .into_iter()
        .find(|set| is_backup_of(set, &current))
        .ok_or_else(|| Front2Error::Store(NO_BACKUP.into()))
}

pub fn prepare_write(paths: &DataPaths, new_boot: &Path) -> Result<PlanReview> {
    let mut shell = AdbShell::connect()?;
    let state = DeviceState::read(&mut shell)?;
    SupportReport::evaluate(&state).require_writes_allowed()?;
    let store = KernelStore::new(paths.kernel_sets());
    let backup = current_backup(&mut shell, &state, &store)?;
    let created_unix = now_unix();
    let candidate_id = format!("candidate-{created_unix}");
    let request = PairRequest {
        id: &candidate_id,
        created_unix,
        note: "",
    };
    let candidate = build_kernel_candidate(&store, &backup.id, new_boot, request)?;
    let id = format!("kernel-plan-{created_unix}");
    let kernel_plan = KernelPlan::prepare(
        &mut shell,
        &state,
        &store,
        &candidate.id,
        &backup.id,
        &id,
        created_unix,
    )?;
    let directory = paths.plans().join(&id);
    let sha256 = kernel_plan.save(&directory)?;
    Ok(PlanReview {
        plan: kernel_plan.plan,
        sha256,
        directory,
        source: PlanSource::Kernel(Box::new(kernel_plan.facts)),
    })
}
