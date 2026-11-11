use crate::support::FakeDevice;
use front2::digest::sha256_bytes;
use front2::{DeviceShell, Result, ShellOutput};
use std::path::Path;

pub struct KernelDevice {
    pub inner: FakeDevice,
    pub running_release: String,
}

impl KernelDevice {
    pub fn partition(&self, label: &str) -> &[u8] {
        self.inner.partition(label)
    }

    pub fn partition_mut(&mut self, label: &str) -> &mut Vec<u8> {
        self.inner.partition_mut(label)
    }

    pub fn add_partition(&mut self, label: &str, node: &str, data: Vec<u8>) {
        let by_name = format!("/dev/block/by-name/{label}");
        self.inner.nodes.insert(by_name.clone(), node.to_string());
        self.inner.partitions.insert(by_name, data);
    }

    fn salted_digest(&self, command: &str) -> Option<ShellOutput> {
        let inner = command.strip_prefix("su -c ").unwrap_or(command);
        let plain = inner.replace("'\\''", "").replace('\'', "");
        let words: Vec<&str> = plain.split_whitespace().collect();
        let [
            "{",
            "printf",
            salt,
            "head",
            "-c",
            count,
            path,
            "}",
            "|",
            "sha256sum",
        ] = words.as_slice()
        else {
            return None;
        };
        let mut data = octal_bytes(salt.trim_end_matches(';'))?;
        let count: usize = count.parse().ok()?;
        let partition = self.inner.partitions.get(path.trim_end_matches(';'))?;
        data.extend_from_slice(&partition[..count.min(partition.len())]);
        Some(ShellOutput::new(format!("{}  -", sha256_bytes(&data)), 0))
    }
}

fn octal_bytes(escaped: &str) -> Option<Vec<u8>> {
    escaped
        .split('\\')
        .skip(1)
        .map(|digits| {
            u8::from_str_radix(digits, 8)
                .ok()
                .filter(|_| digits.len() == 3)
        })
        .collect()
}

impl DeviceShell for KernelDevice {
    fn run(&mut self, command: &str) -> Result<ShellOutput> {
        if command.starts_with("uname -r") {
            return Ok(ShellOutput::new(self.running_release.clone(), 0));
        }
        let Some(output) = self.salted_digest(command) else {
            return self.inner.run(command);
        };
        self.inner.commands.push(command.to_string());
        Ok(output)
    }

    fn push(&mut self, local: &Path, remote: &str) -> Result<()> {
        self.inner.push(local, remote)
    }

    fn pull(&mut self, remote: &str, local: &Path) -> Result<()> {
        self.inner.pull(remote, local)
    }

    fn reboot(&mut self) -> Result<()> {
        self.inner.reboot()
    }
}
