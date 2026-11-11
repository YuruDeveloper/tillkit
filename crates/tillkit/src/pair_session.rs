use crate::adb_shell::AdbShell;
use crate::clock::now_unix;
use crate::data_paths::DataPaths;
use crate::plan_review::PlanReview;
use crate::plan_source::PlanSource;
use crate::worker::current_backup;
use front2::{
    DeviceState, PairManifest, PairRequest, PairStore, PlanRequest, Result, SupportOverride,
    SupportReport, WritePlan,
};

pub struct PairSession {
    pub shell: AdbShell,
    pub state: DeviceState,
    pub store: PairStore,
    pub backup: PairManifest,
    pub created_unix: u64,
    support_override: SupportOverride,
}

impl PairSession {
    pub fn open(paths: &DataPaths, support_override: SupportOverride) -> Result<Self> {
        let mut shell = AdbShell::connect()?;
        let state = DeviceState::read(&mut shell)?;
        SupportReport::evaluate(&state).require_allowed_with(&support_override)?;
        let store = PairStore::new(paths.pairs());
        let backup = current_backup(&mut shell, &state, &store)?;
        Ok(Self {
            shell,
            state,
            store,
            backup,
            created_unix: now_unix(),
            support_override,
        })
    }

    pub fn request<'a>(&self, id: &'a str) -> PairRequest<'a> {
        PairRequest {
            id,
            created_unix: self.created_unix,
            note: "",
        }
    }

    pub fn plan(mut self, paths: &DataPaths, candidate: &PairManifest) -> Result<PlanReview> {
        let id = format!("plan-{}", self.created_unix);
        let request = PlanRequest {
            id: &id,
            created_unix: self.created_unix,
            support_override: self.support_override,
        };
        let plan = WritePlan::prepare(
            &mut self.shell,
            &self.state,
            &self.store,
            &candidate.id,
            &self.backup.id,
            request,
        )?;
        let directory = paths.plans().join(&id);
        let sha256 = plan.save(&directory)?;
        Ok(PlanReview {
            plan,
            sha256,
            directory,
            source: PlanSource::Pair,
        })
    }
}
