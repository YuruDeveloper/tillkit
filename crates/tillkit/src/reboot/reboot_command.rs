use super::device_connection::DeviceConnection;
use super::reboot_target::RebootTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebootCommand {
    Adb(&'static str),
    AdbToEdl,
    FastbootReboot,
    FastbootRebootBootloader,
    FastbootThroughAndroidToEdl,
    FirehoseReset,
    FirehoseResetToEdl,
}

impl RebootCommand {
    pub fn plan(connection: DeviceConnection, target: RebootTarget) -> Option<Self> {
        use DeviceConnection::{Android, Bootloader, Edl, FastbootUserspace, Recovery, Sideload};
        match connection {
            Android | Recovery => Some(Self::from_adb(target)),
            Sideload if target != RebootTarget::Edl => Some(Self::from_adb(target)),
            Bootloader | FastbootUserspace => Self::from_fastboot(target),
            Edl => Self::from_edl(target),
            _ => None,
        }
    }

    fn from_adb(target: RebootTarget) -> Self {
        match target {
            RebootTarget::System => Self::Adb("system"),
            RebootTarget::Recovery => Self::Adb("recovery"),
            RebootTarget::Bootloader => Self::Adb("bootloader"),
            RebootTarget::Edl => Self::AdbToEdl,
        }
    }

    fn from_fastboot(target: RebootTarget) -> Option<Self> {
        match target {
            RebootTarget::System => Some(Self::FastbootReboot),
            RebootTarget::Bootloader => Some(Self::FastbootRebootBootloader),
            RebootTarget::Edl => Some(Self::FastbootThroughAndroidToEdl),
            RebootTarget::Recovery => None,
        }
    }

    fn from_edl(target: RebootTarget) -> Option<Self> {
        match target {
            RebootTarget::System => Some(Self::FirehoseReset),
            RebootTarget::Edl => Some(Self::FirehoseResetToEdl),
            RebootTarget::Recovery | RebootTarget::Bootloader => None,
        }
    }

    pub fn needs_loader(self) -> bool {
        matches!(self, Self::FirehoseReset | Self::FirehoseResetToEdl)
    }
}

#[cfg(test)]
mod tests {
    use super::RebootCommand;
    use crate::reboot::{DeviceConnection, RebootTarget};

    #[test]
    fn adb_targets_use_the_ltbox_reboot_arguments() {
        let cases = [
            (RebootTarget::System, RebootCommand::Adb("system")),
            (RebootTarget::Recovery, RebootCommand::Adb("recovery")),
            (RebootTarget::Bootloader, RebootCommand::Adb("bootloader")),
            (RebootTarget::Edl, RebootCommand::AdbToEdl),
        ];
        for (target, expected) in cases {
            assert_eq!(
                RebootCommand::plan(DeviceConnection::Android, target),
                Some(expected)
            );
        }
    }

    #[test]
    fn sideload_cannot_reach_edl_and_fastboot_cannot_reach_recovery() {
        assert_eq!(
            RebootCommand::plan(DeviceConnection::Sideload, RebootTarget::Edl),
            None
        );
        assert_eq!(
            RebootCommand::plan(DeviceConnection::Bootloader, RebootTarget::Recovery),
            None
        );
        assert_eq!(
            RebootCommand::plan(DeviceConnection::Edl, RebootTarget::Bootloader),
            None
        );
    }

    #[test]
    fn fastboot_reaches_edl_through_android() {
        assert_eq!(
            RebootCommand::plan(DeviceConnection::Bootloader, RebootTarget::Edl),
            Some(RebootCommand::FastbootThroughAndroidToEdl)
        );
        assert_eq!(
            RebootCommand::plan(DeviceConnection::FastbootUserspace, RebootTarget::System),
            Some(RebootCommand::FastbootReboot)
        );
    }

    #[test]
    fn edl_leaves_through_a_firehose_reset() {
        assert_eq!(
            RebootCommand::plan(DeviceConnection::Edl, RebootTarget::System),
            Some(RebootCommand::FirehoseReset)
        );
        assert_eq!(
            RebootCommand::plan(DeviceConnection::Edl, RebootTarget::Edl),
            Some(RebootCommand::FirehoseResetToEdl)
        );
    }
}
