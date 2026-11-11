use crate::device_state::DeviceState;
use crate::error::{Front2Error, Result};
use crate::profile;
use crate::support_check::{BOOT_COMPLETED, ROOT_ACCESS, SLOT_SUFFIX, SupportCheck};
use crate::support_override::SupportOverride;
use crate::verdict::Verdict;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportReport {
    pub checks: Vec<SupportCheck>,
}

impl SupportReport {
    pub fn expected_properties() -> [(&'static str, &'static str); 10] {
        [
            (
                "ro.product.vendor.manufacturer",
                profile::VENDOR_MANUFACTURER,
            ),
            ("ro.product.vendor.device", profile::VENDOR_DEVICE),
            (
                "ro.vendor.build.fingerprint",
                profile::VENDOR_BUILD_FINGERPRINT,
            ),
            ("ro.soc.model", profile::SOC_MODEL),
            ("ro.board.platform", profile::BOARD_PLATFORM),
            ("ro.build.ab_update", "true"),
            (SLOT_SUFFIX, profile::SUPPORTED_SLOT_SUFFIX),
            (
                "ro.boot.vbmeta.device_state",
                profile::REQUIRED_DEVICE_STATE,
            ),
            (
                "ro.boot.verifiedbootstate",
                profile::REQUIRED_VERIFIED_BOOT_STATE,
            ),
            (BOOT_COMPLETED, "1"),
        ]
    }

    pub fn evaluate(state: &DeviceState) -> Self {
        let mut checks: Vec<SupportCheck> = Self::expected_properties()
            .into_iter()
            .map(|(id, expected)| SupportCheck::exact(id, expected, state.property(id)))
            .collect();
        checks.push(SupportCheck::present(
            ROOT_ACCESS,
            "AdbRoot or Su",
            state.root_access.map(|access| format!("{access:?}")),
        ));
        Self { checks }
    }

    pub fn first_blocker(&self) -> Option<&SupportCheck> {
        self.checks
            .iter()
            .find(|check| check.verdict != Verdict::Pass)
    }

    pub fn writes_allowed(&self) -> bool {
        self.first_blocker().is_none()
    }

    pub fn unmet(&self) -> Vec<&SupportCheck> {
        self.checks
            .iter()
            .filter(|check| check.verdict != Verdict::Pass)
            .collect()
    }

    pub fn require_writes_allowed(&self) -> Result<()> {
        self.require_allowed_with(&SupportOverride::none())
    }

    pub fn require_allowed_with(&self, support_override: &SupportOverride) -> Result<()> {
        let unmet = self.unmet();
        if let Some(check) = unmet.iter().find(|check| !support_override.accepts(check)) {
            return Err(Front2Error::Unsupported(format!(
                "{} is {:?}, expected {}",
                check.id, check.observed, check.expected
            )));
        }
        support_override.require_acceptable()?;
        let absent = support_override
            .accepted
            .iter()
            .find(|check| !unmet.contains(check));
        match absent {
            None => Ok(()),
            Some(check) => Err(Front2Error::Unsupported(format!(
                "the approval accepts {} as {:?}, but this device does not have that difference; read the device again",
                check.id, check.observed
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_state::PROPERTY_KEYS;
    use crate::root_access::RootAccess;
    use std::collections::BTreeMap;

    pub fn supported_state() -> DeviceState {
        let mut properties: BTreeMap<String, String> = PROPERTY_KEYS
            .iter()
            .map(|key| (key.to_string(), String::new()))
            .collect();
        for (key, value) in [
            ("ro.product.vendor.manufacturer", "TOSSPLACE"),
            ("ro.product.vendor.device", "toss_front2"),
            (
                "ro.vendor.build.fingerprint",
                "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/475:user/dev-keys",
            ),
            ("ro.soc.model", "QCM4325"),
            ("ro.board.platform", "bengal"),
            ("ro.build.ab_update", "true"),
            ("ro.boot.slot_suffix", "_b"),
            ("ro.boot.vbmeta.device_state", "locked"),
            ("ro.boot.verifiedbootstate", "green"),
            ("sys.boot_completed", "1"),
        ] {
            properties.insert(key.to_string(), value.to_string());
        }
        DeviceState {
            properties,
            shell_uid: Some(2000),
            root_access: Some(RootAccess::Su),
            magisk_version: Some("31.1:MAGISK:R".to_string()),
            kernel_release: None,
        }
    }

    #[test]
    fn measured_front2_state_allows_writes() {
        let report = SupportReport::evaluate(&supported_state());
        assert!(report.writes_allowed(), "{:?}", report.first_blocker());
    }

    #[test]
    fn each_deviation_blocks_writes() {
        for (key, value) in [
            ("ro.product.vendor.manufacturer", "Lenovo"),
            ("ro.product.vendor.device", "toss_front1"),
            (
                "ro.vendor.build.fingerprint",
                "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/476:user/dev-keys",
            ),
            ("ro.soc.model", "SM8550"),
            ("ro.board.platform", "kalama"),
            ("ro.boot.slot_suffix", "_a"),
            ("ro.boot.vbmeta.device_state", "unlocked"),
            ("ro.boot.verifiedbootstate", "orange"),
            ("sys.boot_completed", "0"),
            ("ro.soc.model", ""),
        ] {
            let mut state = supported_state();
            state.properties.insert(key.to_string(), value.to_string());
            let report = SupportReport::evaluate(&state);
            assert_eq!(
                report.first_blocker().map(|check| check.id.as_str()),
                Some(key)
            );
            assert!(report.require_writes_allowed().is_err());
        }
    }

    #[test]
    fn missing_root_blocks_writes() {
        let mut state = supported_state();
        state.root_access = None;
        let report = SupportReport::evaluate(&state);
        assert_eq!(
            report.first_blocker().map(|check| check.id.as_str()),
            Some("root_access")
        );
    }
}
