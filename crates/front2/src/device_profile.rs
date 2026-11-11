use crate::device_state::DeviceState;
use crate::error::{Front2Error, Result};

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
        if self.supports_gsi {
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
