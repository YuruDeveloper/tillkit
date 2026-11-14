use front2::{DeviceShell, Front2Error, Result, ShellOutput};
use ltbox_device::adb::{AdbError, AdbManager};
use std::path::Path;

pub struct AdbShell {
    manager: AdbManager,
}

impl AdbShell {
    pub fn connect() -> Result<Self> {
        let mut manager = AdbManager::new();
        let state = manager.check_device_state().map_err(shell_error)?;
        if state != Some("device") {
            return Err(state_error(state));
        }
        Ok(Self { manager })
    }
}

fn state_error(state: Option<&str>) -> Front2Error {
    Front2Error::Shell(format!(
        "the ADB device state is {}, expected device",
        state.unwrap_or("absent")
    ))
}

pub fn is_absent(error: &str) -> bool {
    error == state_error(None).to_string()
}

fn shell_error(error: AdbError) -> Front2Error {
    Front2Error::Shell(error.to_string())
}

impl DeviceShell for AdbShell {
    fn run(&mut self, command: &str) -> Result<ShellOutput> {
        let (stdout, status) = self
            .manager
            .shell_with_status(command)
            .map_err(shell_error)?;
        Ok(ShellOutput::new(stdout, status))
    }

    fn push(&mut self, local: &Path, remote: &str) -> Result<()> {
        self.manager.push_file(local, remote).map_err(shell_error)
    }

    fn pull(&mut self, remote: &str, local: &Path) -> Result<()> {
        self.manager.pull_file(remote, local).map_err(shell_error)
    }

    fn reboot(&mut self) -> Result<()> {
        self.manager.reboot("system").map_err(shell_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_device_is_told_from_other_device_states() {
        assert!(is_absent(&state_error(None).to_string()));
        assert!(!is_absent(&state_error(Some("unauthorized")).to_string()));
        assert!(!is_absent(&state_error(Some("recovery")).to_string()));
        let other = Front2Error::Shell("command exited with status 1".into()).to_string();
        assert!(!is_absent(&other));
        assert!(!is_absent(""));
    }
}
