use super::package_record::PackageRecord;
use crate::digest::sha256_file;
use crate::error::{Front2Error, Result};
use ltbox_core::downloader::download_to_file;
use ltbox_core::github::GitHubClient;
use ltbox_patch::magisk::extract_apk_payload;
use std::fs;
use std::path::{Path, PathBuf};

pub const MAGISK_REPOSITORY: &str = "topjohnwu/Magisk";
pub const LOCAL_VERSION: &str = "local";
const DOWNLOAD_FILE: &str = "download.apk";
const PAYLOAD_CHECK_DIRECTORY: &str = "payload-check";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MagiskPackage {
    pub path: PathBuf,
    pub version: String,
    pub sha256: String,
}

impl MagiskPackage {
    pub fn fetch_latest(cache_dir: &Path, log: &mut Vec<String>) -> Result<Self> {
        let (version, assets) = GitHubClient::new(MAGISK_REPOSITORY)?.latest_release_assets()?;
        let (name, url) = select_release_apk(&assets).ok_or_else(|| {
            Front2Error::Plan(format!(
                "the {MAGISK_REPOSITORY} release {version} has no release APK"
            ))
        })?;
        if let Some(cached) = Self::cached(cache_dir)?
            && cached.version == version
        {
            return Ok(cached);
        }
        fs::create_dir_all(cache_dir)?;
        let download = cache_dir.join(DOWNLOAD_FILE);
        log.push(format!("Magisk {version}: {name}"));
        download_to_file(&url, &download, log)?;
        Self::admit(cache_dir, &download, &version)
    }

    pub fn from_local_file(source: &Path, cache_dir: &Path) -> Result<Self> {
        fs::create_dir_all(cache_dir)?;
        let staged = cache_dir.join(DOWNLOAD_FILE);
        fs::copy(source, &staged)?;
        Self::admit(cache_dir, &staged, LOCAL_VERSION)
    }

    pub fn cached(cache_dir: &Path) -> Result<Option<Self>> {
        let Some(record) = PackageRecord::load(cache_dir)? else {
            return Ok(None);
        };
        let path = cache_dir.join(&record.file_name);
        if !path.is_file() || sha256_file(&path)?.1 != record.sha256 {
            return Ok(None);
        }
        Ok(Some(Self {
            path,
            version: record.version,
            sha256: record.sha256,
        }))
    }

    pub fn admit(cache_dir: &Path, downloaded: &Path, version: &str) -> Result<Self> {
        let check = cache_dir.join(PAYLOAD_CHECK_DIRECTORY);
        let extracted = extract_apk_payload(downloaded, &check);
        let _ = fs::remove_dir_all(&check);
        if let Err(error) = extracted {
            let _ = fs::remove_file(downloaded);
            return Err(Front2Error::Image(format!(
                "{} is not a usable Magisk package: {error}",
                downloaded.display()
            )));
        }
        let previous = PackageRecord::load(cache_dir).ok().flatten();
        let file_name = format!("Magisk-{}.apk", file_safe(version));
        let path = cache_dir.join(&file_name);
        fs::rename(downloaded, &path)?;
        let sha256 = sha256_file(&path)?.1;
        let record = PackageRecord {
            file_name,
            version: version.to_string(),
            sha256: sha256.clone(),
        };
        record.save(cache_dir)?;
        if let Some(previous) = previous.filter(|old| old.file_name != record.file_name) {
            let _ = fs::remove_file(cache_dir.join(previous.file_name));
        }
        Ok(Self {
            path,
            version: version.to_string(),
            sha256,
        })
    }

    pub fn require_unchanged(&self) -> Result<()> {
        let actual = sha256_file(&self.path)?.1;
        if actual != self.sha256 {
            return Err(Front2Error::Verification(format!(
                "Magisk package {} has SHA-256 {actual}, recorded {}",
                self.path.display(),
                self.sha256
            )));
        }
        Ok(())
    }
}

pub fn select_release_apk(assets: &[(String, String)]) -> Option<(String, String)> {
    assets
        .iter()
        .find(|(name, _)| {
            let lower = name.to_lowercase();
            lower.ends_with(".apk") && !lower.contains("debug")
        })
        .cloned()
}

fn file_safe(version: &str) -> String {
    version
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' => character,
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{file_safe, select_release_apk};

    fn assets(names: &[&str]) -> Vec<(String, String)> {
        names
            .iter()
            .map(|name| (name.to_string(), format!("https://example.invalid/{name}")))
            .collect()
    }

    #[test]
    fn release_apk_skips_debug_builds_and_other_files() {
        let picked = select_release_apk(&assets(&[
            "Magisk-v29.0-debug.apk",
            "notes.txt",
            "Magisk-v29.0.apk",
        ]));
        assert_eq!(
            picked.map(|(name, _)| name).as_deref(),
            Some("Magisk-v29.0.apk")
        );
        assert!(select_release_apk(&assets(&["app-debug.apk", "Magisk.zip"])).is_none());
    }

    #[test]
    fn versions_become_plain_file_names() {
        assert_eq!(file_safe("v29.0"), "v29.0");
        assert_eq!(file_safe("../v29 0"), ".._v29_0");
    }
}
