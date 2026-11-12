use crate::install::digest_check::DigestCheck;
use crate::install::resume_point::ResumePoint;
use crate::install::step_check::StepCheck;
use crate::install::storage_info::StorageInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreflightRecord {
    pub plan_sha256: String,
    pub passed: bool,
    pub sources: Vec<DigestCheck>,
    pub restore_files: Vec<DigestCheck>,
    pub storage: Vec<StorageInfo>,
    pub gpt_copies: Vec<DigestCheck>,
    pub steps: Vec<StepCheck>,
    pub resume: Option<ResumePoint>,
    pub failure: Option<String>,
}

impl PreflightRecord {
    pub fn started(plan_sha256: &str) -> Self {
        Self {
            plan_sha256: plan_sha256.to_string(),
            passed: false,
            sources: Vec::new(),
            restore_files: Vec::new(),
            storage: Vec::new(),
            gpt_copies: Vec::new(),
            steps: Vec::new(),
            resume: None,
            failure: None,
        }
    }
}
