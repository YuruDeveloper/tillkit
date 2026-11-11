use super::device_connection::DeviceConnection;
use super::reboot_command::RebootCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebootTarget {
    System,
    Recovery,
    Bootloader,
    Edl,
}

impl RebootTarget {
    pub const ALL: [RebootTarget; 4] = [
        RebootTarget::System,
        RebootTarget::Recovery,
        RebootTarget::Bootloader,
        RebootTarget::Edl,
    ];

    pub fn available_from(self, connection: DeviceConnection) -> bool {
        RebootCommand::plan(connection, self).is_some()
    }

    pub fn available_targets(connection: DeviceConnection) -> Vec<RebootTarget> {
        Self::ALL
            .into_iter()
            .filter(|target| target.available_from(connection))
            .collect()
    }

    pub fn is_current(self, connection: DeviceConnection) -> bool {
        matches!(
            (connection, self),
            (DeviceConnection::Android, RebootTarget::System)
                | (DeviceConnection::Recovery, RebootTarget::Recovery)
                | (DeviceConnection::Sideload, RebootTarget::Recovery)
                | (DeviceConnection::Bootloader, RebootTarget::Bootloader)
                | (DeviceConnection::Edl, RebootTarget::Edl)
        )
    }

    pub fn needs_loader(self, connection: DeviceConnection) -> bool {
        RebootCommand::plan(connection, self).is_some_and(RebootCommand::needs_loader)
    }
}

#[cfg(test)]
mod tests {
    use super::RebootTarget;
    use crate::reboot::DeviceConnection;

    #[test]
    fn unusable_connections_offer_no_target() {
        for connection in [
            DeviceConnection::Absent,
            DeviceConnection::MultipleDevices,
            DeviceConnection::Unauthorized,
            DeviceConnection::AdbServerBlocking,
        ] {
            assert!(RebootTarget::available_targets(connection).is_empty());
        }
    }

    #[test]
    fn each_connection_offers_only_reachable_targets() {
        let cases = [
            (DeviceConnection::Android, RebootTarget::ALL.to_vec()),
            (DeviceConnection::Recovery, RebootTarget::ALL.to_vec()),
            (
                DeviceConnection::Sideload,
                vec![
                    RebootTarget::System,
                    RebootTarget::Recovery,
                    RebootTarget::Bootloader,
                ],
            ),
            (
                DeviceConnection::Bootloader,
                vec![
                    RebootTarget::System,
                    RebootTarget::Bootloader,
                    RebootTarget::Edl,
                ],
            ),
            (
                DeviceConnection::FastbootUserspace,
                vec![
                    RebootTarget::System,
                    RebootTarget::Bootloader,
                    RebootTarget::Edl,
                ],
            ),
            (
                DeviceConnection::Edl,
                vec![RebootTarget::System, RebootTarget::Edl],
            ),
        ];
        for (connection, expected) in cases {
            assert_eq!(RebootTarget::available_targets(connection), expected);
        }
    }

    #[test]
    fn only_a_firehose_reset_needs_a_loader() {
        assert!(RebootTarget::System.needs_loader(DeviceConnection::Edl));
        assert!(RebootTarget::Edl.needs_loader(DeviceConnection::Edl));
        assert!(!RebootTarget::Edl.needs_loader(DeviceConnection::Android));
        assert!(!RebootTarget::Recovery.needs_loader(DeviceConnection::Edl));
    }

    #[test]
    fn current_mode_is_reported() {
        assert!(RebootTarget::System.is_current(DeviceConnection::Android));
        assert!(RebootTarget::Recovery.is_current(DeviceConnection::Sideload));
        assert!(RebootTarget::Edl.is_current(DeviceConnection::Edl));
        assert!(!RebootTarget::Bootloader.is_current(DeviceConnection::FastbootUserspace));
        assert!(!RebootTarget::System.is_current(DeviceConnection::Recovery));
    }
}
