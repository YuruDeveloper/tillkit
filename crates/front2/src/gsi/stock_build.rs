use crate::error::{Front2Error, Result};
use crate::profiles::toss_front2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StockBuild {
    Tested,
    Untested(String),
}

impl StockBuild {
    pub fn from_fingerprints(values: &[&[u8]]) -> std::result::Result<Self, String> {
        let refusal = || {
            let shown: Vec<String> = values
                .iter()
                .map(|value| String::from_utf8_lossy(value).into_owned())
                .collect();
            format!(
                "property {} has the values {shown:?}; exactly one value of the form {}<build number>{} is necessary",
                toss_front2::BOOT_FINGERPRINT_PROPERTY,
                toss_front2::BUILD_FINGERPRINT_PREFIX,
                toss_front2::BUILD_FINGERPRINT_SUFFIX
            )
        };
        let [value] = values else {
            return Err(refusal());
        };
        if *value == toss_front2::VENDOR_BUILD_FINGERPRINT.as_bytes() {
            return Ok(Self::Tested);
        }
        let fingerprint = std::str::from_utf8(value).map_err(|_| refusal())?;
        let build_number = fingerprint
            .strip_prefix(toss_front2::BUILD_FINGERPRINT_PREFIX)
            .and_then(|rest| rest.strip_suffix(toss_front2::BUILD_FINGERPRINT_SUFFIX))
            .unwrap_or_default();
        let is_a_number =
            !build_number.is_empty() && build_number.bytes().all(|byte| byte.is_ascii_digit());
        if !is_a_number {
            return Err(refusal());
        }
        Ok(Self::Untested(fingerprint.to_string()))
    }

    pub fn untested_fingerprint(&self) -> Option<&str> {
        match self {
            Self::Tested => None,
            Self::Untested(fingerprint) => Some(fingerprint),
        }
    }

    pub fn unaccepted_fingerprint(&self, accepted: Option<&str>) -> Option<&str> {
        self.untested_fingerprint()
            .filter(|fingerprint| accepted != Some(*fingerprint))
    }

    pub fn require_accepted(&self, accepted: Option<&str>) -> Result<()> {
        let Some(fingerprint) = self.unaccepted_fingerprint(accepted) else {
            return Ok(());
        };
        Err(Front2Error::Unsupported(format!(
            "the stock build {fingerprint} is not the tested build {} and the user did not accept it",
            toss_front2::VENDOR_BUILD_FINGERPRINT
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OTHER_BUILD: &str =
        "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/468:user/dev-keys";

    fn build(values: &[&str]) -> std::result::Result<StockBuild, String> {
        let values: Vec<&[u8]> = values.iter().map(|value| value.as_bytes()).collect();
        StockBuild::from_fingerprints(&values)
    }

    #[test]
    fn the_fingerprint_parts_make_the_tested_fingerprint() {
        let made = format!(
            "{}475{}",
            toss_front2::BUILD_FINGERPRINT_PREFIX,
            toss_front2::BUILD_FINGERPRINT_SUFFIX
        );
        assert_eq!(made, toss_front2::VENDOR_BUILD_FINGERPRINT);
    }

    #[test]
    fn only_another_build_number_is_an_untested_build() {
        let tested = build(&[toss_front2::VENDOR_BUILD_FINGERPRINT]);
        assert_eq!(tested, Ok(StockBuild::Tested));
        let other = build(&[OTHER_BUILD]);
        assert_eq!(other, Ok(StockBuild::Untested(OTHER_BUILD.to_string())));

        for refused in [
            "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/:user/dev-keys",
            "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/46x:user/dev-keys",
            "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/468:user/release-keys",
            "TOSSPLACE/toss_front2/toss_front2:14/TKQ1.230110.001/468:user/dev-keys",
            "OTHER/other_device/other_device:13/TKQ1.230110.001/475:user/dev-keys",
            "",
        ] {
            let message = build(&[refused]).expect_err(refused);
            assert!(message.contains(&format!("[{refused:?}]")), "{message}");
            assert!(message.contains("<build number>"), "{message}");
        }
    }

    #[test]
    fn the_refusal_tells_the_values_that_the_backup_has() {
        let none = build(&[]).expect_err("no property");
        assert!(none.contains("has the values []"), "{none}");
        let two = build(&[toss_front2::VENDOR_BUILD_FINGERPRINT, OTHER_BUILD])
            .expect_err("two properties");
        assert!(two.contains(OTHER_BUILD), "{two}");
        assert!(two.contains(toss_front2::VENDOR_BUILD_FINGERPRINT), "{two}");
        let not_text = StockBuild::from_fingerprints(&[&[0xff, 0xfe]]).expect_err("not text");
        assert!(not_text.contains("has the values"), "{not_text}");
    }

    #[test]
    fn an_untested_build_needs_the_acceptance_of_its_own_fingerprint() {
        StockBuild::Tested
            .require_accepted(None)
            .expect("the tested build needs no acceptance");
        assert_eq!(StockBuild::Tested.untested_fingerprint(), None);
        let other = StockBuild::Untested(OTHER_BUILD.to_string());
        assert_eq!(other.untested_fingerprint(), Some(OTHER_BUILD));
        assert_eq!(other.unaccepted_fingerprint(None), Some(OTHER_BUILD));
        assert_eq!(other.unaccepted_fingerprint(Some(OTHER_BUILD)), None);
        other
            .require_accepted(Some(OTHER_BUILD))
            .expect("the accepted build");
        for accepted in [None, Some(toss_front2::VENDOR_BUILD_FINGERPRINT), Some("")] {
            let refused = other
                .require_accepted(accepted)
                .expect_err("the build is not accepted");
            assert!(matches!(refused, Front2Error::Unsupported(_)), "{refused}");
            assert!(refused.to_string().contains(OTHER_BUILD), "{refused}");
        }
    }
}
