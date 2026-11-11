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

    pub fn can_be_accepted(&self) -> bool {
        match self.id.as_str() {
            ROOT_ACCESS | BOOT_COMPLETED => false,
            SLOT_SUFFIX => KNOWN_SLOT_SUFFIXES.contains(&self.observed.as_str()),
            _ => true,
        }
    }
}
