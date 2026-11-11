use crate::apply_state::ApplyState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyOutcome {
    pub plan_id: String,
    pub plan_sha256: String,
    pub state: ApplyState,
    pub device_log: String,
    pub readback_sha256: Vec<String>,
    pub readback_matches_new_pair: bool,
    pub reboot_requested: bool,
}

impl ApplyOutcome {
    pub fn ready_for_reboot(&self) -> bool {
        self.state == ApplyState::VerifiedPendingReboot
            && self.readback_matches_new_pair
            && !self.reboot_requested
    }
}
