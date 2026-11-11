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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_state::PROPERTY_KEYS;

    #[test]
    fn doc_1058_every_identity_property_of_a_profile_is_read_from_the_device() {
        let missing: Vec<(&str, &str)> = ALL
            .iter()
            .flat_map(|profile| profile.identity.iter().map(|(key, _)| (profile.id, *key)))
            .filter(|(_, key)| !PROPERTY_KEYS.contains(key))
            .collect();
        assert!(missing.is_empty(), "{missing:?}");
    }
}
