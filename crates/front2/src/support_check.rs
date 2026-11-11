use crate::verdict::Verdict;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportCheck {
    pub id: String,
    pub expected: String,
    pub observed: String,
    pub verdict: Verdict,
}

impl SupportCheck {
    pub fn exact(id: &str, expected: &str, observed: &str) -> Self {
        let verdict = if observed.is_empty() {
            Verdict::Unknown
        } else if observed == expected {
            Verdict::Pass
        } else {
            Verdict::Fail
        };
        Self {
            id: id.to_string(),
            expected: expected.to_string(),
            observed: observed.to_string(),
            verdict,
        }
    }

    pub fn present(id: &str, expected: &str, observed: Option<String>) -> Self {
        let verdict = if observed.is_some() {
            Verdict::Pass
        } else {
            Verdict::Fail
        };
        Self {
            id: id.to_string(),
            expected: expected.to_string(),
            observed: observed.unwrap_or_default(),
            verdict,
        }
    }
}
