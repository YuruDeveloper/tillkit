use crate::device_state::DeviceState;
use crate::error::{Front2Error, Result};
use crate::profiles::toss_front2;

#[derive(Debug, PartialEq, Eq)]
pub struct DeviceProfile {
    pub id: &'static str,
    pub name: &'static str,
    pub identity: &'static [(&'static str, &'static str)],
    pub slot_suffix: &'static str,
    pub device_state: &'static str,
    pub verified_boot_state: &'static str,
    pub storage_block_bytes: u64,
    pub top_vbmeta_algorithm: &'static str,
    pub top_vbmeta_key_spec: &'static str,
    pub remote_stage_parent: &'static str,
    pub supports_gsi: bool,
}

impl DeviceProfile {
    pub fn matches(&self, state: &DeviceState) -> bool {
        self.identity
            .iter()
            .all(|(property, expected)| state.property(property) == *expected)
    }

    pub fn require_gsi(&self) -> Result<()> {
        if self.supports_gsi && self.id == toss_front2::ID {
            return Ok(());
        }
        Err(Front2Error::Unsupported(format!(
            "the profile {} does not support the GSI install",
            self.id
        )))
    }

    pub fn require_id(&self, id: &str) -> Result<()> {
        if self.id == id {
            return Ok(());
        }
        Err(Front2Error::Plan(format!(
            "the plan is for the profile {id:?}, not {:?}",
            self.id
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static OTHER_DEVICE_WITH_GSI: DeviceProfile = DeviceProfile {
        id: "other-device",
        name: "Other device",
        identity: &[("ro.product.vendor.device", "other")],
        slot_suffix: "_b",
        device_state: "locked",
        verified_boot_state: "green",
        storage_block_bytes: 4096,
        top_vbmeta_algorithm: "SHA256_RSA4096",
        top_vbmeta_key_spec: "testkey_rsa4096",
        remote_stage_parent: "/data/local/tmp",
        supports_gsi: true,
    };

    #[test]
    fn doc_1058_the_front2_profile_allows_the_gsi_install() {
        assert!(toss_front2::PROFILE.require_gsi().is_ok());
    }

    #[test]
    fn doc_1058_another_profile_with_the_gsi_flag_is_refused() {
        let refused = OTHER_DEVICE_WITH_GSI.require_gsi();
        assert!(
            matches!(refused, Err(Front2Error::Unsupported(_))),
            "{refused:?}"
        );
    }
}
