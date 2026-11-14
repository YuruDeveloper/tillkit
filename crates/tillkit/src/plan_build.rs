use front2::Result;
use front2::gsi::StockBuild;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanBuild {
    Tested,
    Untested(String),
    NotRead(String),
}

impl PlanBuild {
    pub fn from_read(build: Result<StockBuild>) -> Self {
        match build {
            Ok(StockBuild::Tested) => Self::Tested,
            Ok(StockBuild::Untested(fingerprint)) => Self::Untested(fingerprint),
            Err(error) => Self::NotRead(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use front2::Front2Error;

    #[test]
    fn letter_1462_a_build_that_is_not_read_is_not_shown_as_the_tested_build() {
        let other = "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/468:user/dev-keys";
        let untested = StockBuild::Untested(other.to_string());
        let changed = Front2Error::Verification("vbmeta_a.bin has a different SHA-256".into());

        assert_eq!(
            PlanBuild::from_read(Ok(StockBuild::Tested)),
            PlanBuild::Tested
        );
        assert_eq!(
            PlanBuild::from_read(Ok(untested)),
            PlanBuild::Untested(other.to_string())
        );
        let PlanBuild::NotRead(reason) = PlanBuild::from_read(Err(changed)) else {
            panic!("a failure of the read is not a build");
        };
        assert!(reason.contains("different SHA-256"), "{reason}");
    }
}
