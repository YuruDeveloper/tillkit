use front2::profiles::{self, toss_front2};
use front2::{DeviceProfile, DeviceState, SupportReport};

#[derive(Debug, Clone)]
pub struct Diagnosis {
    pub state: DeviceState,
    pub report: SupportReport,
    pub profile: &'static DeviceProfile,
}

impl Diagnosis {
    pub fn evaluate(state: DeviceState) -> Self {
        let profile = Self::profile_for(&state);
        let report = SupportReport::evaluate(profile, &state);
        Self {
            state,
            report,
            profile,
        }
    }

    pub fn profile_for(state: &DeviceState) -> &'static DeviceProfile {
        match profiles::matching(state).as_slice() {
            [only] => only,
            _ => &toss_front2::PROFILE,
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use front2::RootAccess;
    use std::collections::BTreeMap;

    pub const OTHER_BUILD: &str =
        "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/468:user/dev-keys";
    pub const OTHER_DEVICE_BUILD: &str =
        "OTHER/other_device/other_device:13/TKQ1.230110.001/468:user/dev-keys";

    pub fn front2_with_fingerprint(
        fingerprint: &str,
        root_access: Option<RootAccess>,
    ) -> Diagnosis {
        let mut properties: BTreeMap<String, String> =
            SupportReport::expected_properties(&toss_front2::PROFILE)
                .into_iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect();
        properties.insert(
            toss_front2::VENDOR_BUILD_FINGERPRINT_PROPERTY.to_string(),
            fingerprint.to_string(),
        );
        Diagnosis::evaluate(DeviceState {
            properties,
            shell_uid: Some(2000),
            root_access,
            magisk_version: None,
            kernel_release: None,
        })
    }

    fn state(properties: &[(&str, &str)]) -> DeviceState {
        DeviceState {
            properties: properties
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect::<BTreeMap<_, _>>(),
            shell_uid: None,
            root_access: None,
            magisk_version: None,
            kernel_release: None,
        }
    }

    #[test]
    fn a_front2_device_gets_the_front2_profile() {
        let diagnosis = Diagnosis::evaluate(state(toss_front2::PROFILE.identity));
        assert_eq!(diagnosis.profile.id, toss_front2::ID);
        assert!(diagnosis.report.unmet().iter().all(|check| {
            !toss_front2::PROFILE
                .identity
                .iter()
                .any(|(property, _)| check.id == *property)
        }));
    }

    #[test]
    fn an_unknown_device_is_judged_with_the_front2_profile() {
        let diagnosis = Diagnosis::evaluate(state(&[("ro.product.vendor.device", "other")]));
        assert_eq!(diagnosis.profile.id, toss_front2::ID);
        assert!(!diagnosis.report.writes_allowed());
        assert_eq!(
            diagnosis
                .report
                .first_blocker()
                .map(|check| check.id.as_str()),
            Some("ro.product.vendor.manufacturer")
        );
    }
}
