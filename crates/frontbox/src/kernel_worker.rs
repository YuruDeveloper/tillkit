use crate::adb_shell::AdbShell;
use crate::clock::now_unix;
use crate::data_paths::DataPaths;
use crate::plan_review::PlanReview;
use crate::plan_source::PlanSource;
use front2::kernel::{
    KernelPlan, KernelSet, KernelStore, build_kernel_candidate, find_stored_current_kernel_set,
    read_current_kernel_set,
};
use front2::{DeviceState, PairRequest, Result, SupportReport};
use std::path::Path;

pub fn read_current_set(paths: &DataPaths, note: &str) -> Result<KernelSet> {
    let mut shell = AdbShell::connect()?;
    let state = DeviceState::read(&mut shell)?;
    let created_unix = now_unix();
    let id = format!("readback-{created_unix}");
    let request = PairRequest {
        id: &id,
        created_unix,
        note,
    };
    let store = KernelStore::new(paths.kernel_sets());
    read_current_kernel_set(&mut shell, &state, &store, request)
}

pub fn build_candidate(
    paths: &DataPaths,
    original: &str,
    new_boot: &Path,
    note: &str,
) -> Result<KernelSet> {
    let created_unix = now_unix();
    let id = format!("candidate-{created_unix}");
    let request = PairRequest {
        id: &id,
        created_unix,
        note,
    };
    let store = KernelStore::new(paths.kernel_sets());
    build_kernel_candidate(&store, original, new_boot, request)
}

pub fn plan_write(paths: &DataPaths, new_set: &str) -> Result<PlanReview> {
    let mut shell = AdbShell::connect()?;
    let state = DeviceState::read(&mut shell)?;
    SupportReport::evaluate(&state).require_writes_allowed()?;
    let store = KernelStore::new(paths.kernel_sets());
    let old_set = find_stored_current_kernel_set(&mut shell, &state, &store)?;
    let created_unix = now_unix();
    let id = format!("kernel-plan-{created_unix}");
    let kernel_plan = KernelPlan::prepare(
        &mut shell,
        &state,
        &store,
        new_set,
        &old_set.id,
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
