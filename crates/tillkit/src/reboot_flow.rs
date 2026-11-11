use crate::reboot::{DeviceConnection, RebootTarget};
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct RebootFlow {
    pub connection: Option<DeviceConnection>,
    pub loader: Option<PathBuf>,
    pub asked: Option<RebootTarget>,
    pub sent: Option<RebootTarget>,
    pub failure: Option<String>,
}

impl RebootFlow {
    pub fn has_no_target(&self) -> bool {
        self.connection
            .is_some_and(|connection| RebootTarget::available_targets(connection).is_empty())
    }

    pub fn reaches(&self, target: RebootTarget) -> bool {
        self.connection
            .is_some_and(|connection| target.available_from(connection))
    }

    pub fn lacks_loader(&self, target: RebootTarget) -> bool {
        let needed = self
            .connection
            .is_some_and(|connection| target.needs_loader(connection));
        needed && self.loader.is_none()
    }

    pub fn can_ask(&self, target: RebootTarget) -> bool {
        self.reaches(target) && !self.lacks_loader(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected(connection: DeviceConnection) -> RebootFlow {
        RebootFlow {
            connection: Some(connection),
            ..RebootFlow::default()
        }
    }

    #[test]
    fn nothing_can_be_asked_before_the_connection_is_known() {
        let flow = RebootFlow::default();
        assert!(
            RebootTarget::ALL
                .iter()
                .all(|target| !flow.can_ask(*target))
        );
        assert!(!flow.has_no_target());
        assert!(connected(DeviceConnection::Unauthorized).has_no_target());
        assert!(!connected(DeviceConnection::Android).has_no_target());
    }

    #[test]
    fn only_reachable_targets_can_be_asked() {
        let fastboot = connected(DeviceConnection::Bootloader);
        assert!(fastboot.can_ask(RebootTarget::System));
        assert!(!fastboot.can_ask(RebootTarget::Recovery));
    }

    #[test]
    fn leaving_edl_needs_the_loader_file() {
        let mut edl = connected(DeviceConnection::Edl);
        assert!(edl.lacks_loader(RebootTarget::System));
        assert!(!edl.can_ask(RebootTarget::System));
        edl.loader = Some(PathBuf::from("loader.elf"));
        assert!(edl.can_ask(RebootTarget::System) && edl.can_ask(RebootTarget::Edl));
        assert!(!edl.can_ask(RebootTarget::Bootloader));
        assert!(!connected(DeviceConnection::Android).lacks_loader(RebootTarget::Edl));
    }
}
