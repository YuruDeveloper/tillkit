use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplyState {
    NotStarted,
    Writing(String),
    VerifiedPendingReboot,
    ApplyFailedRestoring,
    RollbackVerified,
    RollbackFailedDoNotReboot,
    Unrecognized(String),
}

impl ApplyState {
    pub fn parse(text: &str) -> Self {
        match text.trim() {
            "" => Self::NotStarted,
            "VERIFIED_PENDING_REBOOT" => Self::VerifiedPendingReboot,
            "APPLY_FAILED_RESTORING" => Self::ApplyFailedRestoring,
            "ROLLBACK_VERIFIED" => Self::RollbackVerified,
            "ROLLBACK_FAILED_DO_NOT_REBOOT" => Self::RollbackFailedDoNotReboot,
            other if other.starts_with("WRITING_") => Self::Writing(other.to_string()),
            other => Self::Unrecognized(other.to_string()),
        }
    }

    pub fn is_final(&self) -> bool {
        matches!(
            self,
            Self::VerifiedPendingReboot | Self::RollbackVerified | Self::RollbackFailedDoNotReboot
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_file_text_maps_to_states() {
        assert_eq!(ApplyState::parse(""), ApplyState::NotStarted);
        assert_eq!(
            ApplyState::parse("WRITING_INIT_BOOT\n"),
            ApplyState::Writing("WRITING_INIT_BOOT".into())
        );
        assert!(ApplyState::parse("VERIFIED_PENDING_REBOOT").is_final());
        assert!(ApplyState::parse("ROLLBACK_VERIFIED").is_final());
        assert!(ApplyState::parse("ROLLBACK_FAILED_DO_NOT_REBOOT").is_final());
        assert!(!ApplyState::parse("APPLY_FAILED_RESTORING").is_final());
        assert!(!ApplyState::parse("garbage").is_final());
    }
}
