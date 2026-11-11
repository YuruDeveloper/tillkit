use crate::device_shell::DeviceShell;
use crate::error::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootAccess {
    AdbRoot,
    Su,
}

impl RootAccess {
    pub fn detect(shell: &mut dyn DeviceShell) -> Result<Option<Self>> {
        if shell.run("id -u")?.text() == "0" {
            return Ok(Some(Self::AdbRoot));
        }
        let elevated = shell.run(&Self::Su.command("id -u"))?;
        if elevated.succeeded() && elevated.text() == "0" {
            return Ok(Some(Self::Su));
        }
        Ok(None)
    }

    pub fn command(self, command: &str) -> String {
        match self {
            Self::AdbRoot => command.to_string(),
            Self::Su => format!("su -c {}", quote(command)),
        }
    }
}

pub fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn su_command_quotes_embedded_single_quotes() {
        assert_eq!(
            RootAccess::Su.command("echo 'a b'"),
            "su -c 'echo '\\''a b'\\'''"
        );
        assert_eq!(RootAccess::AdbRoot.command("id -u"), "id -u");
    }
}
