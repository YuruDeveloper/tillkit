use crate::install::plan_payload::PlanPayload;
use crate::install::restore_entry::RestoreEntry;
use crate::install::write_step::WriteStep;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedTarget {
    pub step: WriteStep,
    pub restore: RestoreEntry,
    pub payload: Option<PlanPayload>,
}
