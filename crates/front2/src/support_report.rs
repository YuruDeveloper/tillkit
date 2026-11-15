use crate::device_profile::DeviceProfile;
use crate::device_state::DeviceState;
use crate::error::{Front2Error, Result};
use crate::support_check::{BOOT_COMPLETED, ROOT_ACCESS, SLOT_SUFFIX, SupportCheck};
use crate::support_override::SupportOverride;
use crate::verdict::Verdict;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportReport {
    pub checks: Vec<SupportCheck>,
}

impl SupportReport {
    pub fn expected_properties(profile: &DeviceProfile) -> Vec<(&'static str, &'static str)> {
        let mut properties = profile.identity.to_vec();
        properties.extend([
            ("ro.build.ab_update", "true"),
            (SLOT_SUFFIX, profile.slot_suffix),
            ("ro.boot.vbmeta.device_state", profile.device_state),
            ("ro.boot.verifiedbootstate", profile.verified_boot_state),
            (BOOT_COMPLETED, "1"),
        ]);
        properties
    }

    pub fn evaluate(profile: &DeviceProfile, state: &DeviceState) -> Self {
        let mut checks: Vec<SupportCheck> = Self::expected_properties(profile)
            .into_iter()
            .map(|(id, expected)| property_check(profile, id, expected, state.property(id)))
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

fn property_check(
    profile: &DeviceProfile,
    id: &str,
    expected: &str,
    observed: &str,
) -> SupportCheck {
    if id != profile.build_fingerprint_key {
        return SupportCheck::exact(id, expected, observed);
    }
    SupportCheck::build_number(
        id,
        expected,
        observed,
        profile.build_fingerprint_prefix,
        profile.build_fingerprint_suffix,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_state::PROPERTY_KEYS;
    use crate::profiles::toss_front2::{self, PROFILE};
    use crate::root_access::RootAccess;
    use std::collections::BTreeMap;

    const OTHER_BUILD: &str =
        "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/468:user/dev-keys";

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
        let report = SupportReport::evaluate(&PROFILE, &supported_state());
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
            let report = SupportReport::evaluate(&PROFILE, &state);
            assert_eq!(
                report.first_blocker().map(|check| check.id.as_str()),
                Some(key)
            );
            assert!(report.require_writes_allowed().is_err());
        }
    }

    fn report_with_fingerprint(fingerprint: &str) -> SupportReport {
        let mut state = supported_state();
        state.properties.insert(
            toss_front2::VENDOR_BUILD_FINGERPRINT_PROPERTY.to_string(),
            fingerprint.to_string(),
        );
        SupportReport::evaluate(&PROFILE, &state)
    }

    fn fingerprint_check(report: &SupportReport) -> &SupportCheck {
        report
            .checks
            .iter()
            .find(|check| check.id == toss_front2::VENDOR_BUILD_FINGERPRINT_PROPERTY)
            .expect("the report checks the vendor fingerprint")
    }

    #[test]
    fn the_tested_fingerprint_passes() {
        let report = report_with_fingerprint(toss_front2::VENDOR_BUILD_FINGERPRINT);
        assert_eq!(fingerprint_check(&report).verdict, Verdict::Pass);
    }

    #[test]
    fn a_fingerprint_with_another_build_number_can_be_accepted() {
        let report = report_with_fingerprint(OTHER_BUILD);
        let check = fingerprint_check(&report);
        assert_eq!(check.verdict, Verdict::Fail);
        assert!(check.can_be_accepted());
        assert!(report.require_writes_allowed().is_err());
        let accepted =
            SupportOverride::accept_unmet(&report).expect("the build number is accepted");
        assert_eq!(accepted.accepted, std::slice::from_ref(check));
        report
            .require_allowed_with(&accepted)
            .expect("the accepted build number allows writes");
    }

    #[test]
    fn a_fingerprint_of_another_device_cannot_be_accepted() {
        for fingerprint in [
            "OTHER/other_device/other_device:13/TKQ1.230110.001/475:user/dev-keys",
            "TOSSPLACE/toss_front2/toss_front2:14/TKQ1.230110.001/468:user/dev-keys",
            "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/468:user/release-keys",
            "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/468a:user/dev-keys",
            "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/:user/dev-keys",
        ] {
            let report = report_with_fingerprint(fingerprint);
            let check = fingerprint_check(&report);
            assert_eq!(check.verdict, Verdict::Fail, "{fingerprint}");
            assert!(!check.can_be_accepted(), "{fingerprint}");
            assert!(
                SupportOverride::accept_unmet(&report).is_err(),
                "{fingerprint}"
            );
        }
    }

    #[test]
    fn an_unread_fingerprint_cannot_be_accepted() {
        let report = report_with_fingerprint("");
        let check = fingerprint_check(&report);
        assert_eq!(check.verdict, Verdict::Unknown);
        assert!(!check.can_be_accepted());
    }

    #[test]
    fn missing_root_blocks_writes() {
        let mut state = supported_state();
        state.root_access = None;
        let report = SupportReport::evaluate(&PROFILE, &state);
        assert_eq!(
            report.first_blocker().map(|check| check.id.as_str()),
            Some("root_access")
        );
    }
}
