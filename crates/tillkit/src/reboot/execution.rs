use super::device_connection::DeviceConnection;
use super::reboot_command::RebootCommand;
use super::reboot_target::RebootTarget;
use front2::{Front2Error, Result};
use ltbox_device::adb::AdbManager;
use ltbox_device::edl::{self, EdlSession};
use ltbox_device::fastboot::FastbootDevice;
use std::fmt::Display;
use std::path::Path;

pub fn reboot_device(
    connection: DeviceConnection,
    target: RebootTarget,
    loader: Option<&Path>,
) -> Result<()> {
    let command = RebootCommand::plan(connection, target).ok_or_else(|| {
        Front2Error::Unsupported(format!("{target:?} cannot be reached from {connection:?}"))
    })?;
    let current = DeviceConnection::detect();
    if current != connection {
        return Err(Front2Error::Unsupported(format!(
            "the device is now {current:?}, the reboot was chosen for {connection:?}"
        )));
    }
    run(command, loader)
}

fn run(command: RebootCommand, loader: Option<&Path>) -> Result<()> {
    match command {
        RebootCommand::Adb(argument) => adb_reboot(argument),
        RebootCommand::AdbToEdl => adb_reboot_to_edl(&mut AdbManager::new()),
        RebootCommand::FastbootReboot => open_fastboot()?.reboot().map_err(failure),
        RebootCommand::FastbootRebootBootloader => {
            open_fastboot()?.reboot_bootloader().map_err(failure)
        }
        RebootCommand::FastbootThroughAndroidToEdl => fastboot_to_edl(),
        RebootCommand::FirehoseReset => firehose_reset(loader, false),
        RebootCommand::FirehoseResetToEdl => firehose_reset(loader, true),
    }
}

fn adb_reboot(argument: &str) -> Result<()> {
    let mut manager = AdbManager::new();
    manager.check_device_state().map_err(failure)?;
    manager.reboot(argument).map_err(failure)
}

fn adb_reboot_to_edl(manager: &mut AdbManager) -> Result<()> {
    let state = manager.check_device_state().map_err(failure)?;
    if !matches!(state, Some("device" | "recovery")) {
        return Err(Front2Error::Shell(format!(
            "ADB is {}, a shell is needed to reboot to EDL",
            state.unwrap_or("absent")
        )));
    }
    manager.reboot("edl").map_err(failure)?;
    edl::wait_for_device().map(|_| ()).map_err(failure)
}

fn fastboot_to_edl() -> Result<()> {
    open_fastboot()?.reboot().map_err(failure)?;
    let mut manager = AdbManager::new();
    let reached_android = manager
        .wait_for_device_or(edl::check_device)
        .map_err(failure)?;
    if reached_android {
        return adb_reboot_to_edl(&mut manager);
    }
    edl::wait_for_device().map(|_| ()).map_err(failure)
}

fn firehose_reset(loader: Option<&Path>, to_edl: bool) -> Result<()> {
    let loader = loader
        .ok_or_else(|| Front2Error::Plan("a Firehose loader is needed to leave EDL".to_string()))?;
    let mut log = Vec::new();
    let mut session = EdlSession::open(loader, &mut log).map_err(failure)?;
    if to_edl {
        return session.reset_to_edl(&mut log).map_err(failure);
    }
    session.reset_tolerant(&mut log);
    Ok(())
}

fn open_fastboot() -> Result<FastbootDevice> {
    FastbootDevice::open().map_err(failure)
}

fn failure(error: impl Display) -> Front2Error {
    Front2Error::Shell(error.to_string())
}
