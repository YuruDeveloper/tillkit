use crate::adb_shell::AdbShell;
use crate::clock::now_unix;
use crate::data_paths::DataPaths;
use crate::diagnosis::Diagnosis;
use crate::message::Message;
use crate::plan_review::PlanReview;
use front2::candidate::build_signed_candidate;
use front2::current_pair::find_stored_current_pair;
use front2::pair_backup;
use front2::{
    ApplyOutcome, Authorization, DeviceState, Front2Error, PairManifest, PairRequest, PairStore,
    Result, SupportReport, WritePlan, pair_apply,
};
use iced::Task;
use std::path::Path;
use std::time::Duration;

const STATE_POLL_INTERVAL: Duration = Duration::from_secs(1);

pub fn run_blocking<T, F>(
    work: F,
    done: fn(std::result::Result<T, String>) -> Message,
) -> Task<Message>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    Task::perform(
        async move {
            match tokio::task::spawn_blocking(work).await {
                Ok(result) => result.map_err(|error| error.to_string()),
                Err(error) => Err(format!("the worker stopped: {error}")),
            }
        },
        done,
    )
}

pub fn read_device() -> Result<Diagnosis> {
    let mut shell = AdbShell::connect()?;
    let state = DeviceState::read(&mut shell)?;
    let report = SupportReport::evaluate(&state);
    Ok(Diagnosis { state, report })
}

pub fn read_current_pair(paths: &DataPaths, note: &str) -> Result<PairManifest> {
    let mut shell = AdbShell::connect()?;
    let state = DeviceState::read(&mut shell)?;
    let created_unix = now_unix();
    let id = format!("readback-{created_unix}");
    let request = PairRequest {
        id: &id,
        created_unix,
        note,
    };
    pair_backup::read_current_pair(&mut shell, &state, &PairStore::new(paths.pairs()), request)
}

pub fn build_candidate(
    paths: &DataPaths,
    original: &str,
    patched_init_boot: &Path,
    note: &str,
) -> Result<PairManifest> {
    let created_unix = now_unix();
    let id = format!("candidate-{created_unix}");
    let request = PairRequest {
        id: &id,
        created_unix,
        note,
    };
    build_signed_candidate(
        &PairStore::new(paths.pairs()),
        original,
        patched_init_boot,
        request,
    )
}

pub fn plan_write(paths: &DataPaths, new_pair: &str) -> Result<PlanReview> {
    let mut shell = AdbShell::connect()?;
    let state = DeviceState::read(&mut shell)?;
    SupportReport::evaluate(&state).require_writes_allowed()?;
    let store = PairStore::new(paths.pairs());
    let old_pair = find_stored_current_pair(&mut shell, &state, &store)?;
    let created_unix = now_unix();
    let id = format!("plan-{created_unix}");
    let plan = WritePlan::prepare(
        &mut shell,
        &state,
        &store,
        new_pair,
        &old_pair.id,
        &id,
        created_unix,
    )?;
    let directory = paths.plans().join(&id);
    let sha256 = plan.save(&directory)?;
    Ok(PlanReview {
        plan,
        sha256,
        directory,
    })
}

fn load_reviewed(review: &PlanReview) -> Result<WritePlan> {
    let (plan, sha256) = WritePlan::load(&review.directory)?;
    if sha256 != review.sha256 || plan != review.plan {
        return Err(Front2Error::Plan(
            "the plan file changed after the review".into(),
        ));
    }
    Ok(plan)
}

pub fn stage_plan(paths: &DataPaths, review: &PlanReview) -> Result<()> {
    let plan = load_reviewed(review)?;
    let mut shell = AdbShell::connect()?;
    let store = PairStore::new(paths.pairs());
    pair_apply::stage(&mut shell, &store, &plan, &review.directory)?;
    pair_apply::check(&mut shell, &plan)
}

pub fn write_pair(review: &PlanReview, authorization: &Authorization) -> Result<ApplyOutcome> {
    let plan = load_reviewed(review)?;
    let mut shell = AdbShell::connect()?;
    pair_apply::apply(
        &mut shell,
        &plan,
        &review.sha256,
        authorization,
        &mut || {
            std::thread::sleep(STATE_POLL_INTERVAL);
        },
    )
}

pub fn reboot(
    review: &PlanReview,
    authorization: &Authorization,
    mut outcome: ApplyOutcome,
) -> Result<ApplyOutcome> {
    let plan = load_reviewed(review)?;
    let mut shell = AdbShell::connect()?;
    pair_apply::reboot(
        &mut shell,
        &plan,
        &review.sha256,
        authorization,
        &mut outcome,
    )?;
    Ok(outcome)
}
