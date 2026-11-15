use crate::verdict::Verdict;
use serde::{Deserialize, Serialize};

pub const ROOT_ACCESS: &str = "root_access";
pub const SLOT_SUFFIX: &str = "ro.boot.slot_suffix";
pub const BOOT_COMPLETED: &str = "sys.boot_completed";
const KNOWN_SLOT_SUFFIXES: [&str; 2] = ["_a", "_b"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportCheck {
    pub id: String,
    pub expected: String,
    pub observed: String,
    pub verdict: Verdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptable: Option<bool>,
}

impl SupportCheck {
    pub fn exact(id: &str, expected: &str, observed: &str) -> Self {
        Self::new(id, expected, observed, exact_verdict(expected, observed))
    }

    pub fn present(id: &str, expected: &str, observed: Option<String>) -> Self {
        let verdict = if observed.is_some() {
            Verdict::Pass
        } else {
            Verdict::Fail
        };
        Self::new(id, expected, &observed.unwrap_or_default(), verdict)
    }

    pub fn build_number(
        id: &str,
        expected: &str,
        observed: &str,
        prefix: &str,
        suffix: &str,
    ) -> Self {
        Self {
            acceptable: Some(is_build_number_of(observed, prefix, suffix)),
            ..Self::exact(id, expected, observed)
        }
    }

    pub fn can_be_accepted(&self) -> bool {
        self.acceptable
            .unwrap_or_else(|| acceptable_by_id(&self.id, &self.observed))
    }

    fn new(id: &str, expected: &str, observed: &str, verdict: Verdict) -> Self {
        Self {
            id: id.to_string(),
            expected: expected.to_string(),
            observed: observed.to_string(),
            verdict,
            acceptable: Some(acceptable_by_id(id, observed)),
        }
    }
}

fn exact_verdict(expected: &str, observed: &str) -> Verdict {
    if observed.is_empty() {
        Verdict::Unknown
    } else if observed == expected {
        Verdict::Pass
    } else {
        Verdict::Fail
    }
}

fn is_build_number_of(observed: &str, prefix: &str, suffix: &str) -> bool {
    let build_number = observed
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(suffix))
        .unwrap_or_default();
    !build_number.is_empty() && build_number.bytes().all(|byte| byte.is_ascii_digit())
}

fn acceptable_by_id(id: &str, observed: &str) -> bool {
    match id {
        ROOT_ACCESS | BOOT_COMPLETED => false,
        SLOT_SUFFIX => KNOWN_SLOT_SUFFIXES.contains(&observed),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FINGERPRINT: &str = "ro.vendor.build.fingerprint";
    const PREFIX: &str = "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/";
    const SUFFIX: &str = ":user/dev-keys";

    fn fingerprint_check(build_number: &str, observed: &str) -> SupportCheck {
        let expected = format!("{PREFIX}{build_number}{SUFFIX}");
        SupportCheck::build_number(FINGERPRINT, &expected, observed, PREFIX, SUFFIX)
    }

    #[test]
    fn the_constructors_keep_the_acceptance_rules_of_each_check() {
        let cases = [
            (SupportCheck::exact(SLOT_SUFFIX, "_b", "_a"), true),
            (SupportCheck::exact(SLOT_SUFFIX, "_b", "_c"), false),
            (SupportCheck::exact(BOOT_COMPLETED, "1", "0"), false),
            (
                SupportCheck::exact("ro.soc.model", "QCM4325", "SM8550"),
                true,
            ),
            (
                SupportCheck::present(ROOT_ACCESS, "AdbRoot or Su", None),
                false,
            ),
        ];
        for (check, acceptable) in cases {
            assert_eq!(check.can_be_accepted(), acceptable, "{check:?}");
        }
    }

    #[test]
    fn only_the_build_number_of_a_fingerprint_can_differ() {
        let same = fingerprint_check("475", &format!("{PREFIX}475{SUFFIX}"));
        assert_eq!(same.verdict, Verdict::Pass);
        let other_build = fingerprint_check("475", &format!("{PREFIX}468{SUFFIX}"));
        assert_eq!(other_build.verdict, Verdict::Fail);
        assert!(other_build.can_be_accepted());
        for observed in [
            format!("OTHER/other/other:13/TKQ1.230110.001/468{SUFFIX}"),
            format!("{PREFIX}468:user/release-keys"),
            format!("{PREFIX}{SUFFIX}"),
            format!("{PREFIX}468/x{SUFFIX}"),
        ] {
            let check = fingerprint_check("475", &observed);
            assert_eq!(check.verdict, Verdict::Fail, "{observed}");
            assert!(!check.can_be_accepted(), "{observed}");
        }
        let unread = fingerprint_check("475", "");
        assert_eq!(unread.verdict, Verdict::Unknown);
        assert!(!unread.can_be_accepted());
    }

    #[test]
    fn a_stored_check_without_the_acceptance_uses_the_rule_of_its_id() {
        let stored = |id: &str, observed: &str| -> SupportCheck {
            let text = format!(
                r#"{{"id":"{id}","expected":"x","observed":"{observed}","verdict":"Fail"}}"#
            );
            serde_json::from_str(&text).expect("a check without the acceptance field")
        };
        assert_eq!(stored(FINGERPRINT, "other").acceptable, None);
        assert!(stored(FINGERPRINT, "other").can_be_accepted());
        assert!(stored(SLOT_SUFFIX, "_a").can_be_accepted());
        assert!(!stored(SLOT_SUFFIX, "_c").can_be_accepted());
        assert!(!stored(ROOT_ACCESS, "").can_be_accepted());
    }

    #[test]
    fn a_saved_check_loads_with_the_same_value() {
        let check = fingerprint_check("475", &format!("{PREFIX}468{SUFFIX}"));
        let text = serde_json::to_string(&check).expect("serialize");
        let loaded: SupportCheck = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(loaded, check);
        let other = fingerprint_check("475", "OTHER");
        let text = serde_json::to_string(&other).expect("serialize");
        let loaded: SupportCheck = serde_json::from_str(&text).expect("deserialize");
        assert!(!loaded.can_be_accepted());
    }
}
