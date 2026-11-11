use ltbox_device::adb::AdbManager;
use ltbox_device::edl;
use ltbox_device::fastboot::FastbootDevice;
use ltbox_device::selection::{UsbSelectionError, ensure_single_usb_target};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceConnection {
    Absent,
    MultipleDevices,
    Android,
    Recovery,
    Sideload,
    Unauthorized,
    AdbServerBlocking,
    Bootloader,
    FastbootUserspace,
    Edl,
}

impl DeviceConnection {
    pub fn detect() -> Self {
        if let Err(UsbSelectionError::MultipleDevices) = ensure_single_usb_target() {
            return Self::MultipleDevices;
        }
        if edl::check_device() {
            return Self::Edl;
        }
        if let Ok(mut device) = FastbootDevice::open() {
            return Self::from_fastboot(device.is_userspace());
        }
        let mut manager = AdbManager::new();
        Self::from_adb_state(manager.check_device_state().ok().flatten())
    }

    pub fn from_fastboot(userspace: bool) -> Self {
        if userspace {
            Self::FastbootUserspace
        } else {
            Self::Bootloader
        }
    }

    pub fn from_adb_state(state: Option<&str>) -> Self {
        match state {
            Some("device") => Self::Android,
            Some("recovery") => Self::Recovery,
            Some("sideload") => Self::Sideload,
            Some("unauthorized" | "authorizing") => Self::Unauthorized,
            Some("adb_server_blocking") => Self::AdbServerBlocking,
            _ => Self::Absent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DeviceConnection;

    #[test]
    fn adb_states_map_to_connections() {
        let cases = [
            (Some("device"), DeviceConnection::Android),
            (Some("recovery"), DeviceConnection::Recovery),
            (Some("sideload"), DeviceConnection::Sideload),
            (Some("unauthorized"), DeviceConnection::Unauthorized),
            (Some("authorizing"), DeviceConnection::Unauthorized),
            (
                Some("adb_server_blocking"),
                DeviceConnection::AdbServerBlocking,
            ),
            (Some("offline"), DeviceConnection::Absent),
            (None, DeviceConnection::Absent),
        ];
        for (state, expected) in cases {
            assert_eq!(DeviceConnection::from_adb_state(state), expected);
        }
    }

    #[test]
    fn fastboot_userspace_is_kept_apart_from_the_bootloader() {
        assert_eq!(
            DeviceConnection::from_fastboot(false),
            DeviceConnection::Bootloader
        );
        assert_eq!(
            DeviceConnection::from_fastboot(true),
            DeviceConnection::FastbootUserspace
        );
    }
}
