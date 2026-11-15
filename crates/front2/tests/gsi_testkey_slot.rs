use avbtool_rs::builder::{VbmetaImageArgs, make_vbmeta_image};
use front2::digest::sha256_file;
use front2::gpt::BootSlot;
use front2::gsi::{VbmetaSource, find_testkey_slot};
use std::path::Path;

const TESTKEY_TOP: (&str, &str) = ("testkey_rsa4096", "SHA256_RSA4096");
const TESTKEY_CHAINED: (&str, &str) = ("testkey_rsa2048", "SHA256_RSA2048");
const VENDOR_KEY: (&str, &str) = ("testkey_rsa8192", "SHA256_RSA8192");
const TESTKEY_TOP_SHA512: (&str, &str) = ("testkey_rsa4096", "SHA512_RSA4096");

fn write_vbmeta(path: &Path, key: (&str, &str)) {
    let args = VbmetaImageArgs {
        algorithm_name: key.1.to_string(),
        key_spec: Some(key.0.to_string()),
        public_key_metadata: None,
        rollback_index: 0,
        flags: 0,
        rollback_index_location: 0,
        properties: Vec::new(),
        kernel_cmdlines: Vec::new(),
        extra_descriptors: Vec::new(),
        include_descriptors_from_images: Vec::new(),
        chain_partitions: Vec::new(),
        release_string: Some("avbtool 1.2.0".to_string()),
        append_to_release_string: None,
        padding_size: 0,
    };
    make_vbmeta_image(path, &args).expect("fixture vbmeta");
}

struct Backup {
    directory: tempfile::TempDir,
}

impl Backup {
    fn new(
        top_a: (&str, &str),
        chained_a: (&str, &str),
        top_b: (&str, &str),
        chained_b: (&str, &str),
    ) -> Self {
        Self::build(top_a, chained_a, top_b, chained_b, None)
    }

    fn with_damaged_signature(
        top_a: (&str, &str),
        chained_a: (&str, &str),
        top_b: (&str, &str),
        chained_b: (&str, &str),
        damaged: &str,
    ) -> Self {
        Self::build(top_a, chained_a, top_b, chained_b, Some(damaged))
    }

    fn build(
        top_a: (&str, &str),
        chained_a: (&str, &str),
        top_b: (&str, &str),
        chained_b: (&str, &str),
        damaged: Option<&str>,
    ) -> Self {
        let directory = tempfile::tempdir().expect("tempdir");
        let lun = directory.path().join("partitions").join("lun4");
        std::fs::create_dir_all(&lun).expect("lun directory");
        let parts = [
            ("vbmeta_a", top_a),
            ("vbmeta_b", top_b),
            ("vbmeta_system_a", chained_a),
            ("vbmeta_system_b", chained_b),
        ];
        let entries: Vec<serde_json::Value> = parts
            .iter()
            .map(|(name, key)| manifest_entry(&lun, name, *key, damaged))
            .collect();
        let text = serde_json::to_string_pretty(&entries).expect("manifest text");
        std::fs::write(directory.path().join("sha256-manifest.json"), text).expect("manifest");
        Self { directory }
    }

    fn path(&self) -> &Path {
        self.directory.path()
    }
}

fn manifest_entry(
    lun: &Path,
    name: &str,
    key: (&str, &str),
    damaged: Option<&str>,
) -> serde_json::Value {
    let path = lun.join(format!("{name}.bin"));
    write_vbmeta(&path, key);
    if damaged == Some(name) {
        flip_signature_byte(&path);
    }
    let (bytes, sha256) = sha256_file(&path).expect("digest");
    serde_json::json!({
        "path": format!("partitions\\lun4\\{name}.bin"),
        "bytes": bytes,
        "sha256": sha256.to_ascii_uppercase(),
    })
}

fn flip_signature_byte(path: &Path) {
    let source = VbmetaSource::read(path).expect("source");
    let offset = 256 + source.info.header.signature_offset as usize;
    let mut bytes = std::fs::read(path).expect("read source");
    bytes[offset] ^= 1;
    std::fs::write(path, bytes).expect("write source");
}

#[test]
fn finds_slot_a_when_only_slot_a_is_signed_with_the_test_key() {
    let backup = Backup::new(TESTKEY_TOP, TESTKEY_CHAINED, VENDOR_KEY, VENDOR_KEY);
    assert_eq!(
        find_testkey_slot(backup.path()).expect("the test key slot"),
        BootSlot::A
    );
}

#[test]
fn finds_slot_b_when_only_slot_b_is_signed_with_the_test_key() {
    let backup = Backup::new(VENDOR_KEY, VENDOR_KEY, TESTKEY_TOP, TESTKEY_CHAINED);
    assert_eq!(
        find_testkey_slot(backup.path()).expect("the test key slot"),
        BootSlot::B
    );
}

#[test]
fn refuses_when_neither_slot_is_signed_with_the_test_key() {
    let backup = Backup::new(VENDOR_KEY, VENDOR_KEY, VENDOR_KEY, VENDOR_KEY);
    let message = find_testkey_slot(backup.path())
        .expect_err("no test key slot")
        .to_string();
    assert!(message.contains("neither slot"), "{message}");
}

#[test]
fn refuses_when_both_slots_are_signed_with_the_test_key() {
    let backup = Backup::new(TESTKEY_TOP, TESTKEY_CHAINED, TESTKEY_TOP, TESTKEY_CHAINED);
    let message = find_testkey_slot(backup.path())
        .expect_err("two test key slots")
        .to_string();
    assert!(message.contains("both slots"), "{message}");
}

#[test]
fn a_slot_needs_both_the_top_and_the_chained_vbmeta_signed_with_the_test_key() {
    let backup = Backup::new(TESTKEY_TOP, VENDOR_KEY, VENDOR_KEY, VENDOR_KEY);
    let message = find_testkey_slot(backup.path())
        .expect_err("the chained vbmeta is not the test key")
        .to_string();
    assert!(message.contains("neither slot"), "{message}");
}

// Clara doc #1106: a damaged signature on the test key slot must propagate,
// not become a "neither slot" result that hides the damage.
#[test]
fn clara_1106_a_damaged_signature_on_the_test_key_slot_propagates() {
    let backup = Backup::with_damaged_signature(
        TESTKEY_TOP,
        TESTKEY_CHAINED,
        VENDOR_KEY,
        VENDOR_KEY,
        "vbmeta_a",
    );
    let message = find_testkey_slot(backup.path())
        .expect_err("the damaged signature fails the investigation")
        .to_string();
    assert!(
        message.contains("RSA verification rejected the signature"),
        "{message}"
    );
    assert!(!message.contains("neither slot"), "{message}");
}

// Clara doc #1106: a slot that uses the test key with an unsupported
// algorithm is refused, and the message names the algorithm condition.
#[test]
fn clara_1106_an_unsupported_algorithm_names_the_algorithm_in_the_refusal() {
    let backup = Backup::new(TESTKEY_TOP_SHA512, TESTKEY_CHAINED, VENDOR_KEY, VENDOR_KEY);
    let message = find_testkey_slot(backup.path())
        .expect_err("the unsupported algorithm is not the install target")
        .to_string();
    assert!(message.contains("algorithm"), "{message}");
}
