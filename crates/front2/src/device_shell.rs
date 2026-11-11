use crate::error::{Front2Error, Result};
use crate::shell_output::ShellOutput;
use std::path::Path;

pub trait DeviceShell {
    fn run(&mut self, command: &str) -> Result<ShellOutput>;

    fn push(&mut self, local: &Path, remote: &str) -> Result<()>;

    fn pull(&mut self, remote: &str, local: &Path) -> Result<()>;

    fn reboot(&mut self) -> Result<()>;

    fn run_checked(&mut self, command: &str) -> Result<String> {
        let output = self.run(command)?;
        if !output.succeeded() {
            return Err(Front2Error::Shell(format!(
                "command exited with status {}: {command}: {}",
                output.status,
                output.text()
            )));
        }
        Ok(output.text().to_string())
    }
}
