use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DigestCheck {
    pub name: String,
    pub sha256: String,
    pub expected_sha256: String,
}

impl DigestCheck {
    pub fn new(name: &str, sha256: &str, expected_sha256: &str) -> Self {
        Self {
            name: name.to_string(),
            sha256: sha256.to_string(),
            expected_sha256: expected_sha256.to_string(),
        }
    }

    pub fn matches(&self) -> bool {
        self.sha256 == self.expected_sha256
    }
}
