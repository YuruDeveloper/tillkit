use crate::device_shell::DeviceShell;
use crate::error::Result;
use ltbox_patch::magisk::{derive_encrypt_type, resolve_preinit_device};

pub fn read_preinit_device(shell: &mut dyn DeviceShell) -> Result<String> {
    let crypto_state = shell.run_checked("getprop ro.crypto.state")?;
    let crypto_type = shell.run_checked("getprop ro.crypto.type")?;
    let metadata_enabled = shell.run_checked("getprop ro.crypto.metadata.enabled")?;
    let mountinfo = shell.run_checked("cat /proc/self/mountinfo")?;
    let encrypt_type = derive_encrypt_type(&crypto_state, &crypto_type, &metadata_enabled);
    Ok(resolve_preinit_device(&mountinfo, encrypt_type).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::read_preinit_device;
    use crate::device_shell::DeviceShell;
    use crate::error::Result;
    use crate::shell_output::ShellOutput;
    use std::collections::HashMap;
    use std::path::Path;

    struct ScriptedShell(HashMap<&'static str, &'static str>);

    impl DeviceShell for ScriptedShell {
        fn run(&mut self, command: &str) -> Result<ShellOutput> {
            let text = self.0.get(command).copied().unwrap_or("");
            Ok(ShellOutput::new(text.to_string(), 0))
        }

        fn push(&mut self, _local: &Path, _remote: &str) -> Result<()> {
            unreachable!("no push")
        }

        fn pull(&mut self, _remote: &str, _local: &Path) -> Result<()> {
            unreachable!("no pull")
        }

        fn reboot(&mut self) -> Result<()> {
            unreachable!("no reboot")
        }
    }

    const MOUNTINFO: &str = "\
1 1 259:5 / /metadata rw,seclabel shared:1 - ext4 /dev/block/by-name/metadata rw
2 1 259:9 / /data rw,seclabel shared:2 - f2fs /dev/block/by-name/userdata rw";

    fn shell(crypto_state: &'static str, metadata: &'static str) -> ScriptedShell {
        ScriptedShell(HashMap::from([
            ("getprop ro.crypto.state", crypto_state),
            ("getprop ro.crypto.type", "file"),
            ("getprop ro.crypto.metadata.enabled", metadata),
            ("cat /proc/self/mountinfo", MOUNTINFO),
        ]))
    }

    #[test]
    fn metadata_partition_wins_over_userdata() {
        let mut device = shell("encrypted", "true");
        assert_eq!(
            read_preinit_device(&mut device).expect("preinit"),
            "metadata"
        );
    }

    #[test]
    fn nothing_eligible_gives_an_empty_name() {
        let mut device = ScriptedShell(HashMap::from([("getprop ro.crypto.state", "encrypted")]));
        assert_eq!(read_preinit_device(&mut device).expect("preinit"), "");
    }
}
