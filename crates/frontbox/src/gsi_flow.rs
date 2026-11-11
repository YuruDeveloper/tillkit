use crate::gsi_action::GsiAction;
use crate::gsi_review::GsiReview;
use crate::install_review::InstallReview;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct GsiFlow {
    pub backup_directory: Option<PathBuf>,
    pub gsi_image: Option<PathBuf>,
    pub review: Option<GsiReview>,
    pub failure: Option<String>,
    pub install: Option<InstallReview>,
    pub install_failure: Option<String>,
}

impl GsiFlow {
    pub fn inputs(&self) -> Option<(PathBuf, PathBuf)> {
        if self.review.is_some() {
            return None;
        }
        self.backup_directory.clone().zip(self.gsi_image.clone())
    }

    pub fn install_inputs(&self) -> Option<(GsiReview, PathBuf)> {
        if self.install.is_some() {
            return None;
        }
        self.review.clone().zip(self.backup_directory.clone())
    }

    pub fn next_action(&self) -> Option<GsiAction> {
        if self.install.is_some() {
            return None;
        }
        let candidate = self.review.is_some();
        Some(match (&self.backup_directory, &self.gsi_image) {
            (None, _) => GsiAction::PickBackup,
            (Some(_), _) if candidate => GsiAction::PlanInstall,
            (Some(_), None) => GsiAction::PickImage,
            (Some(_), Some(_)) => GsiAction::Build,
        })
    }

    pub fn allows(&self, action: GsiAction) -> bool {
        match action {
            GsiAction::PickBackup => self.install.is_none(),
            GsiAction::PickImage => self.review.is_none(),
            GsiAction::Build => self.inputs().is_some(),
            GsiAction::PlanInstall => self.install_inputs().is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_needs_both_inputs_and_no_candidate() {
        let mut flow = GsiFlow::default();
        assert!(flow.inputs().is_none());
        assert_eq!(flow.next_action(), Some(GsiAction::PickBackup));
        assert!(flow.allows(GsiAction::PickBackup) && flow.allows(GsiAction::PickImage));
        assert!(!flow.allows(GsiAction::Build) && !flow.allows(GsiAction::PlanInstall));
        flow.backup_directory = Some(PathBuf::from("backup"));
        assert!(flow.inputs().is_none());
        assert_eq!(flow.next_action(), Some(GsiAction::PickImage));
        flow.gsi_image = Some(PathBuf::from("gsi.img"));
        assert_eq!(flow.next_action(), Some(GsiAction::Build));
        assert!(flow.allows(GsiAction::Build));
        assert_eq!(
            flow.inputs(),
            Some((PathBuf::from("backup"), PathBuf::from("gsi.img")))
        );
        assert!(flow.install_inputs().is_none());
    }
}
