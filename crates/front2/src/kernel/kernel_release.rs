use crate::error::{Front2Error, Result};
use serde::{Deserialize, Serialize};

const ARM64_MAGIC_OFFSET: usize = 0x38;
const ARM64_MAGIC: &[u8; 4] = b"ARMd";
const BANNER_PREFIX: &[u8] = b"Linux version ";
const RELEASE_LIMIT: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelRelease {
    pub release: String,
    pub kmi: String,
}

impl KernelRelease {
    pub fn from_kernel(kernel: &[u8]) -> Result<Self> {
        let magic = kernel.get(ARM64_MAGIC_OFFSET..ARM64_MAGIC_OFFSET + ARM64_MAGIC.len());
        if magic != Some(ARM64_MAGIC.as_slice()) {
            return Err(Front2Error::Image(
                "kernel has no ARM64 Image magic; a compressed kernel is not accepted".into(),
            ));
        }
        let mut releases = banner_release_tokens(kernel)
            .into_iter()
            .map(|token| Self::parse(&String::from_utf8_lossy(token)))
            .collect::<Result<Vec<Self>>>()?;
        releases.dedup();
        if releases.len() > 1 {
            return Err(Front2Error::Image(format!(
                "kernel carries banners with different releases: {:?} and {:?}",
                releases[0].release, releases[1].release
            )));
        }
        releases.pop().ok_or_else(|| {
            Front2Error::Image("kernel carries no Linux banner with a release".into())
        })
    }

    pub fn parse(release: &str) -> Result<Self> {
        let kmi = kmi_of(release).ok_or_else(|| {
            Front2Error::Image(format!(
                "kernel release {release:?} is not in the form <major>.<minor>.<sublevel>-android<release>-<generation>"
            ))
        })?;
        Ok(Self {
            release: release.to_string(),
            kmi,
        })
    }
}

fn banner_release_tokens(kernel: &[u8]) -> Vec<&[u8]> {
    let mut tokens = Vec::new();
    let mut offset = 0;
    while let Some(found) = find(&kernel[offset..], BANNER_PREFIX) {
        let start = offset + found + BANNER_PREFIX.len();
        let rest = &kernel[start..];
        let length = rest
            .iter()
            .position(|byte| !byte.is_ascii_graphic())
            .unwrap_or(rest.len());
        if rest.first().is_some_and(u8::is_ascii_digit) {
            tokens.push(&rest[..length]);
        }
        offset = start;
    }
    tokens
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window[0] == needle[0] && window == needle)
}

fn kmi_of(release: &str) -> Option<String> {
    if release.len() > RELEASE_LIMIT {
        return None;
    }
    let (version, tail) = release.split_once('-')?;
    let (android, tail) = tail.split_once('-')?;
    let generation = tail.split('-').next()?;
    let android_release = android.strip_prefix("android")?;
    let numbers: Vec<&str> = version.split('.').collect();
    let all_numbers = numbers.len() == 3
        && numbers
            .iter()
            .chain([&android_release, &generation])
            .all(|text| is_number(text));
    all_numbers.then(|| format!("{}.{}-{android}-{generation}", numbers[0], numbers[1]))
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}
