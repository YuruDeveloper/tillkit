pub mod toss_front2;

use crate::device_profile::DeviceProfile;
use crate::device_state::DeviceState;
use crate::error::{Front2Error, Result};

pub static ALL: [&DeviceProfile; 1] = [&toss_front2::PROFILE];

pub fn find(id: &str) -> Result<&'static DeviceProfile> {
    ALL.iter()
        .copied()
        .find(|profile| profile.id == id)
        .ok_or_else(|| Front2Error::Plan(format!("no device profile has the id {id:?}")))
}

pub fn matching(state: &DeviceState) -> Vec<&'static DeviceProfile> {
    ALL.iter()
        .copied()
        .filter(|profile| profile.matches(state))
        .collect()
}
