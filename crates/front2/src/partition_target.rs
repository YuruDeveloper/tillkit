use crate::device_shell::DeviceShell;
use crate::error::{Front2Error, Result};
use crate::profile;
use crate::root_access::{RootAccess, quote};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionTarget {
    pub label: String,
    pub by_name: String,
    pub node: String,
    pub bytes: u64,
}

impl PartitionTarget {
    pub fn resolve(
        shell: &mut dyn DeviceShell,
        root: RootAccess,
        label: &str,
        block_bytes: u64,
    ) -> Result<Self> {
        if !is_safe_label(label) {
            return Err(Front2Error::Plan(format!("partition label {label:?}")));
        }
        let by_name = profile::by_name_path(label);
        let quoted = quote(&by_name);
        let node = shell.run_checked(&root.command(&format!("readlink -f {quoted}")))?;
        if !is_block_node(&node) {
            return Err(Front2Error::Verification(format!(
                "{by_name} resolves to {node:?}, not a block device node"
            )));
        }
        let size_text =
            shell.run_checked(&root.command(&format!("blockdev --getsize64 {quoted}")))?;
        let bytes: u64 = size_text.parse().map_err(|_| {
            Front2Error::Verification(format!("{by_name} size {size_text:?} is not a number"))
        })?;
        if bytes == 0 || !bytes.is_multiple_of(block_bytes) {
            return Err(Front2Error::Verification(format!(
                "{by_name} size {bytes} is not a positive multiple of {block_bytes}"
            )));
        }
        Ok(Self {
            label: label.to_string(),
            by_name,
            node,
            bytes,
        })
    }

    pub fn sha256(&self, shell: &mut dyn DeviceShell, root: RootAccess) -> Result<String> {
        device_sha256(shell, root, &self.by_name)
    }
}

pub fn device_sha256(shell: &mut dyn DeviceShell, root: RootAccess, path: &str) -> Result<String> {
    let line = shell.run_checked(&root.command(&format!("sha256sum {}", quote(path))))?;
    let digest = line.split_whitespace().next().unwrap_or_default();
    if !crate::digest::is_sha256_hex(digest) {
        return Err(Front2Error::Verification(format!(
            "sha256sum {path} returned {line:?}"
        )));
    }
    Ok(digest.to_string())
}

pub fn is_safe_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 36
        && label
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn is_block_node(node: &str) -> bool {
    node.strip_prefix("/dev/block/").is_some_and(|name| {
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_nodes_are_restricted_to_plain_names() {
        assert!(is_safe_label("init_boot_b"));
        assert!(!is_safe_label("init_boot_b; reboot"));
        assert!(!is_safe_label("../misc"));
        assert!(!is_safe_label(""));
        assert!(is_block_node("/dev/block/sde58"));
        assert!(is_block_node("/dev/block/dm-3"));
        assert!(!is_block_node("/dev/block/by-name/init_boot_b"));
        assert!(!is_block_node("/data/local/tmp/fake"));
    }
}
