use crate::device_shell::DeviceShell;
use crate::error::{Front2Error, Result};
use crate::root_access::RootAccess;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const PROPERTY_KEYS: [&str; 17] = [
    "ro.soc.manufacturer",
    "ro.soc.model",
    "ro.board.platform",
    "ro.boot.slot_suffix",
    "ro.boot.vbmeta.device_state",
    "ro.boot.verifiedbootstate",
    "ro.build.ab_update",
    "ro.build.version.release",
    "ro.build.version.sdk",
    "ro.build.display.id",
    "ro.build.type",
    "sys.boot_completed",
    "ro.product.vendor.manufacturer",
    "ro.product.vendor.model",
    "ro.product.vendor.device",
    "ro.product.vendor.name",
    "ro.vendor.build.fingerprint",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceState {
    pub properties: BTreeMap<String, String>,
    pub shell_uid: Option<u32>,
    pub root_access: Option<RootAccess>,
    pub magisk_version: Option<String>,
    pub kernel_release: Option<String>,
}

impl DeviceState {
    pub fn read(shell: &mut dyn DeviceShell) -> Result<Self> {
        let command = format!(
            "for key in {}; do echo \"$key=$(getprop \"$key\")\"; done",
            PROPERTY_KEYS.join(" ")
        );
        let listing = shell.run_checked(&command)?;
        let properties = parse_properties(&listing)?;
        let shell_uid = shell.run("id -u")?.text().parse().ok();
        let root_access = RootAccess::detect(shell)?;
        let magisk_version = optional_text(shell, "magisk -v 2>/dev/null")?;
        let kernel_release = optional_text(shell, "uname -r 2>/dev/null")?;
        Ok(Self {
            properties,
            shell_uid,
            root_access,
            magisk_version,
            kernel_release,
        })
    }

    pub fn property(&self, key: &str) -> &str {
        self.properties.get(key).map_or("", String::as_str)
    }

    pub fn slot_suffix(&self) -> &str {
        self.property("ro.boot.slot_suffix")
    }
}

fn optional_text(shell: &mut dyn DeviceShell, command: &str) -> Result<Option<String>> {
    let output = shell.run(command)?;
    let text = output.text();
    Ok((output.succeeded() && !text.is_empty()).then(|| text.to_string()))
}

fn parse_properties(listing: &str) -> Result<BTreeMap<String, String>> {
    let mut properties = BTreeMap::new();
    for line in listing.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if PROPERTY_KEYS.contains(&key.trim()) {
            properties.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    if properties.len() != PROPERTY_KEYS.len() {
        return Err(Front2Error::Shell(format!(
            "property listing returned {} of {} keys",
            properties.len(),
            PROPERTY_KEYS.len()
        )));
    }
    Ok(properties)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_with_every_key_parses_and_keeps_empty_values() {
        let listing: String = PROPERTY_KEYS
            .iter()
            .map(|key| match *key {
                "ro.boot.slot_suffix" => format!("{key}=_b\n"),
                _ => format!("{key}=\n"),
            })
            .collect();
        let properties = parse_properties(&listing).expect("complete listing");
        assert_eq!(properties["ro.boot.slot_suffix"], "_b");
        assert_eq!(properties["ro.soc.model"], "");
    }

    #[test]
    fn listing_with_a_missing_key_is_rejected() {
        assert!(parse_properties("ro.soc.model=QCM4325\n").is_err());
    }
}
