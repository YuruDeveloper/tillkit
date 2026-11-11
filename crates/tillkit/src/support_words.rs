use front2::{SupportCheck, SupportOverride, SupportReport};

pub const ROOT_CHECK: &str = "root_access";
const NOT_READ: &str = "읽지 못함";
const NAMES: [(&str, &str); 11] = [
    ("ro.product.vendor.manufacturer", "제조사"),
    ("ro.product.vendor.device", "기기 종류"),
    ("ro.vendor.build.fingerprint", "펌웨어 판"),
    ("ro.soc.model", "칩"),
    ("ro.board.platform", "칩 플랫폼"),
    ("ro.build.ab_update", "A/B 슬롯 구조"),
    ("ro.boot.slot_suffix", "현재 슬롯"),
    ("ro.boot.vbmeta.device_state", "부트로더 잠금"),
    ("ro.boot.verifiedbootstate", "부팅 검증 상태"),
    ("sys.boot_completed", "부팅 완료"),
    (ROOT_CHECK, "루트 권한"),
];

pub fn name(check: &SupportCheck) -> &str {
    NAMES
        .iter()
        .find(|(id, _)| *id == check.id)
        .map_or(check.id.as_str(), |(_, name)| *name)
}

pub fn observed(check: &SupportCheck) -> &str {
    if check.observed.is_empty() {
        NOT_READ
    } else {
        &check.observed
    }
}

pub fn unmet(report: &SupportReport) -> Vec<&SupportCheck> {
    report.unmet()
}

pub fn root_is_missing(report: &SupportReport) -> bool {
    unmet(report).iter().any(|check| check.id == ROOT_CHECK)
}

pub fn can_be_forced(report: &SupportReport) -> bool {
    SupportOverride::accept_unmet(report).is_ok_and(|accepted| !accepted.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(checks: &[(&str, &str, &str)]) -> SupportReport {
        let checks = checks
            .iter()
            .map(|(id, expected, observed)| SupportCheck::exact(id, expected, observed))
            .collect();
        SupportReport { checks }
    }

    #[test]
    fn every_check_of_the_engine_has_a_plain_name() {
        let known = report(&[("ro.boot.slot_suffix", "_b", "_a")]);
        assert_eq!(name(&known.checks[0]), "현재 슬롯");
        let unknown = report(&[("ro.new.property", "1", "2")]);
        assert_eq!(name(&unknown.checks[0]), "ro.new.property");
    }

    #[test]
    fn an_empty_value_is_shown_as_not_read() {
        let empty = report(&[("ro.soc.model", "QCM4325", "")]);
        assert_eq!(observed(&empty.checks[0]), NOT_READ);
        assert_eq!(unmet(&empty).len(), 1);
    }

    #[test]
    fn a_device_without_root_cannot_be_forced() {
        let supported = report(&[("ro.soc.model", "QCM4325", "QCM4325")]);
        assert!(!can_be_forced(&supported));
        let other_slot = report(&[("ro.boot.slot_suffix", "_b", "_a")]);
        assert!(can_be_forced(&other_slot));
        let mut rootless = other_slot.clone();
        rootless
            .checks
            .push(SupportCheck::present(ROOT_CHECK, "AdbRoot or Su", None));
        assert!(root_is_missing(&rootless));
        assert!(!can_be_forced(&rootless));
    }

    #[test]
    fn a_device_without_a_known_slot_cannot_be_forced() {
        for slot in ["", "_c"] {
            let unknown_slot = report(&[("ro.boot.slot_suffix", "_b", slot)]);
            assert!(!can_be_forced(&unknown_slot), "{slot:?}");
        }
    }
}
