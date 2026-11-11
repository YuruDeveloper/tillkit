use super::fault::Fault;
use front2::device_state::PROPERTY_KEYS;
use front2::digest::sha256_bytes;
use front2::{DeviceShell, Front2Error, Result, ShellOutput, WritePlan};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub struct FakeDevice {
    pub properties: BTreeMap<String, String>,
    pub su_allowed: bool,
    pub partitions: BTreeMap<String, Vec<u8>>,
    pub nodes: BTreeMap<String, String>,
    pub files: BTreeMap<String, Vec<u8>>,
    pub directories: BTreeSet<String>,
    pub plan: Option<WritePlan>,
    pub fault: Fault,
    pub rebooted: bool,
    pub commands: Vec<String>,
}

impl FakeDevice {
    pub fn front2(init_boot: Vec<u8>, vbmeta: Vec<u8>) -> Self {
        let mut properties: BTreeMap<String, String> = PROPERTY_KEYS
            .iter()
            .map(|key| (key.to_string(), String::new()))
            .collect();
        for (key, value) in [
            ("ro.product.vendor.manufacturer", "TOSSPLACE"),
            ("ro.product.vendor.device", "toss_front2"),
            (
                "ro.vendor.build.fingerprint",
                "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/475:user/dev-keys",
            ),
            ("ro.soc.manufacturer", "QTI"),
            ("ro.soc.model", "QCM4325"),
            ("ro.board.platform", "bengal"),
            ("ro.build.ab_update", "true"),
            ("ro.boot.slot_suffix", "_b"),
            ("ro.boot.vbmeta.device_state", "locked"),
            ("ro.boot.verifiedbootstate", "green"),
            ("sys.boot_completed", "1"),
        ] {
            properties.insert(key.to_string(), value.to_string());
        }
        let init_boot_path = "/dev/block/by-name/init_boot_b".to_string();
        let vbmeta_path = "/dev/block/by-name/vbmeta_b".to_string();
        Self {
            properties,
            su_allowed: true,
            nodes: BTreeMap::from([
                (init_boot_path.clone(), "/dev/block/sde58".to_string()),
                (vbmeta_path.clone(), "/dev/block/sde62".to_string()),
            ]),
            partitions: BTreeMap::from([(init_boot_path, init_boot), (vbmeta_path, vbmeta)]),
            files: BTreeMap::new(),
            directories: BTreeSet::new(),
            plan: None,
            fault: Fault::None,
            rebooted: false,
            commands: Vec::new(),
        }
    }

    pub fn partition(&self, label: &str) -> &[u8] {
        &self.partitions[&format!("/dev/block/by-name/{label}")]
    }

    pub fn partition_mut(&mut self, label: &str) -> &mut Vec<u8> {
        self.partitions
            .get_mut(&format!("/dev/block/by-name/{label}"))
            .expect("known partition")
    }

    fn content(&self, path: &str) -> Option<&Vec<u8>> {
        self.partitions.get(path).or_else(|| self.files.get(path))
    }

    fn dispatch(&mut self, command: &str, is_root: bool) -> ShellOutput {
        let words: Vec<&str> = command.split_whitespace().collect();
        match words.as_slice() {
            ["id", "-u"] if is_root => ok("0"),
            ["id", "-u"] => ok("2000"),
            ["for", "key", "in", ..] => ok(&self.property_listing()),
            ["magisk", "-v", ..] => ok("31.1:MAGISK:R"),
            ["uname", "-r", ..] => ok("5.15.0-fake"),
            ["readlink", "-f", path] => optional(self.nodes.get(*path).cloned()),
            ["blockdev", "--getsize64", path] if is_root => optional(self.size_text(path)),
            ["sha256sum", path] => optional(self.sha256_line(path)),
            ["mkdir", path] => self.make_directory(path),
            ["rm", "-rf", path] => self.remove_tree(path),
            ["dd", source, target, _, _, "&&", "chmod", ..] if is_root => self.copy(source, target),
            ["sh", _, "--check"] if is_root => self.script_check(),
            ["nohup", "sh", _, "--apply", ..] if is_root => self.script_apply(),
            ["cat", path, ..] => ok(&self.file_text(path)),
            _ => failed(),
        }
    }

    fn size_text(&self, path: &str) -> Option<String> {
        self.partitions.get(path).map(|data| data.len().to_string())
    }

    fn sha256_line(&self, path: &str) -> Option<String> {
        self.content(path)
            .map(|data| format!("{}  {path}", sha256_bytes(data)))
    }

    fn file_text(&self, path: &str) -> String {
        self.files
            .get(path)
            .map(|data| String::from_utf8_lossy(data).into_owned())
            .unwrap_or_default()
    }

    fn property_listing(&self) -> String {
        self.properties
            .iter()
            .map(|(key, value)| format!("{key}={value}\n"))
            .collect()
    }

    fn make_directory(&mut self, path: &str) -> ShellOutput {
        if self.directories.insert(path.to_string()) {
            ok("")
        } else {
            failed()
        }
    }

    fn remove_tree(&mut self, path: &str) -> ShellOutput {
        self.directories.remove(path);
        let prefix = format!("{path}/");
        self.files.retain(|name, _| !name.starts_with(&prefix));
        ok("")
    }

    fn copy(&mut self, source_argument: &str, target_argument: &str) -> ShellOutput {
        let source = source_argument.trim_start_matches("if=");
        let target = target_argument.trim_start_matches("of=");
        let Some(data) = self.content(source).cloned() else {
            return failed();
        };
        self.files.insert(target.to_string(), data);
        ok("")
    }

    fn staged(&self, plan: &WritePlan, name: &str) -> Option<&Vec<u8>> {
        self.files.get(&format!("{}/{name}", plan.remote_stage))
    }

    fn preconditions_hold(&self, plan: &WritePlan) -> bool {
        let state_file = format!("{}/apply-state.txt", plan.remote_stage);
        !self.files.contains_key(&state_file)
            && plan.writes.iter().all(|write| {
                let staged_new = self
                    .staged(plan, &write.staged_new)
                    .map(|data| sha256_bytes(data));
                let staged_old = self
                    .staged(plan, &write.staged_old)
                    .map(|data| sha256_bytes(data));
                let current = sha256_bytes(&self.partitions[&write.target.by_name]);
                staged_new.as_deref() == Some(write.new_image.sha256.as_str())
                    && staged_old.as_deref() == Some(write.old_image.sha256.as_str())
                    && current == write.old_image.sha256
            })
    }

    fn script_check(&mut self) -> ShellOutput {
        let plan = self.plan.clone().expect("the test registered the plan");
        if self.preconditions_hold(&plan) {
            ok("ALL_PRECONDITIONS_PASSED")
        } else {
            failed()
        }
    }

    fn script_apply(&mut self) -> ShellOutput {
        let plan = self.plan.clone().expect("the test registered the plan");
        if !self.preconditions_hold(&plan) {
            return ok("");
        }
        let new: Vec<Vec<u8>> = plan
            .writes
            .iter()
            .map(|write| {
                self.staged(&plan, &write.staged_new)
                    .expect("staged")
                    .clone()
            })
            .collect();
        let (writes_first, writes_second, state) = match self.fault {
            Fault::NeverFinishes => (true, false, "WRITING_VBMETA"),
            Fault::SecondWriteFails => (false, false, "ROLLBACK_VERIFIED"),
            Fault::RollbackFails => (true, false, "ROLLBACK_FAILED_DO_NOT_REBOOT"),
            Fault::ReportsVerifiedWithStaleVbmeta => (true, false, "VERIFIED_PENDING_REBOOT"),
            _ => (true, true, "VERIFIED_PENDING_REBOOT"),
        };
        if writes_first {
            self.partitions
                .insert(plan.writes[0].target.by_name.clone(), new[0].clone());
        }
        if writes_second {
            self.partitions
                .insert(plan.writes[1].target.by_name.clone(), new[1].clone());
        }
        self.files.insert(
            format!("{}/apply-state.txt", plan.remote_stage),
            format!("{state}\n").into_bytes(),
        );
        self.files.insert(
            format!("{}/apply.log", plan.remote_stage),
            b"fake device log\n".to_vec(),
        );
        ok("")
    }
}

fn ok(text: &str) -> ShellOutput {
    ShellOutput::new(text, 0)
}

fn failed() -> ShellOutput {
    ShellOutput::new("", 1)
}

fn optional(text: Option<String>) -> ShellOutput {
    text.map_or_else(failed, |text| ok(&text))
}

fn unwrap_su(command: &str) -> Option<String> {
    let quoted = command.strip_prefix("su -c ")?;
    Some(quoted.replace("'\\''", "\u{1}"))
}

impl DeviceShell for FakeDevice {
    fn run(&mut self, command: &str) -> Result<ShellOutput> {
        self.commands.push(command.to_string());
        let (inner, is_root) = match unwrap_su(command) {
            Some(_) if !self.su_allowed => return Ok(ShellOutput::new("", 13)),
            Some(inner) => (inner, true),
            None => (command.to_string(), false),
        };
        let plain = inner.replace(['\'', '\u{1}'], "");
        Ok(self.dispatch(&plain, is_root))
    }

    fn push(&mut self, local: &Path, remote: &str) -> Result<()> {
        let mut data = std::fs::read(local)?;
        if self.fault == Fault::CorruptPush && remote.ends_with(".new.img") {
            data[0] ^= 1;
        }
        self.files.insert(remote.to_string(), data);
        Ok(())
    }

    fn pull(&mut self, remote: &str, local: &Path) -> Result<()> {
        let mut data = self
            .files
            .get(remote)
            .cloned()
            .ok_or_else(|| Front2Error::Shell(format!("no remote file {remote}")))?;
        if self.fault == Fault::CorruptPull {
            data[0] ^= 1;
        }
        std::fs::write(local, data)?;
        Ok(())
    }

    fn reboot(&mut self) -> Result<()> {
        self.rebooted = true;
        Ok(())
    }
}
