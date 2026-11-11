//! SHA-256 file hashing and GitHub asset digest verification.

use sha2::{Digest, Sha256};
use std::path::Path;

use crate::error::{LtboxError, Result};

/// Lowercase hex SHA-256 of a file, streamed in 64 KiB chunks.
pub fn sha256_file_hex(path: &Path) -> std::io::Result<String> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Require `path` to match a GitHub `sha256:<hex>` asset digest.
///
/// For downloads that run with privilege: an absent, malformed or
/// non-SHA-256 digest fails exactly like a mismatch.
pub fn verify_github_sha256_digest(path: &Path, reported: Option<&str>) -> Result<()> {
    let reported = reported.unwrap_or_default();
    let expected = reported
        .strip_prefix("sha256:")
        .filter(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| {
            LtboxError::Download(format!(
                "{}: no usable SHA-256 digest reported (got {reported:?})",
                path.display()
            ))
        })?;
    let actual = sha256_file_hex(path)?;
    if !expected.eq_ignore_ascii_case(&actual) {
        return Err(LtboxError::Download(format!(
            "{}: SHA-256 mismatch (expected {expected}, got {actual})",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn github_digest_accepts_only_a_matching_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abc.bin");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(sha256_file_hex(&path).unwrap(), ABC_SHA256);
        let upper = format!("sha256:{}", ABC_SHA256.to_ascii_uppercase());
        verify_github_sha256_digest(&path, Some(&upper)).unwrap();

        let wrong = format!("sha256:{}", "0".repeat(64));
        let sha512 = format!("sha512:{ABC_SHA256}");
        for reported in [
            None,
            Some(""),
            Some(ABC_SHA256),
            Some("sha256:"),
            Some(sha512.as_str()),
            Some(wrong.as_str()),
        ] {
            assert!(
                verify_github_sha256_digest(&path, reported).is_err(),
                "{reported:?}"
            );
        }
    }
}
