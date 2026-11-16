use avbtool_rs::builder::{ChainPartitionSpec, PropertySpec, VbmetaImageArgs, make_vbmeta_image};
use avbtool_rs::crypto::extract_public_key;
use avbtool_rs::info::DescriptorInfo;
use front2::digest::sha256_file;
use front2::gpt::BootSlot;
use front2::gsi::{
    GsiCandidate, GsiCandidateRequest, RawDescriptor, RollbackEvidence, StockBuild, SystemOrigin,
    VbmetaSource, VbmetaTemplate, backup_stock_build, backup_system_origin, build_system_vbmeta,
    build_top_vbmeta, require_stock_front2_system, require_stock_front2_top,
};
use front2::profiles::toss_front2::PROFILE;
use front2::top_signature::verify_top_signature;
use std::path::{Path, PathBuf};

mod super_support;
use super_support::SyntheticSuper;

const PARTITION_BYTES: u64 = 65536;
const STOCK_ROLLBACK: u64 = 1_700_000_000;
const SYSTEM_ROLLBACK: u64 = 1_710_000_000;
const OTHER_KEY: &str = "testkey_rsa8192";
const OTHER_ALGORITHM: &str = "SHA256_RSA8192";
const STOCK_SYSTEM: &str = "qti/qssi/qssi:13/TKQ1.230110.001/475:user/dev-keys";
const GSI_SYSTEM: &str =
    "generic/lineage_gsi_arm64_gN/lineage_gsi_arm64:13/TQ3A.230901.001/1:userdebug/test-keys";

struct Source<'a> {
    key_spec: &'a str,
    algorithm: &'a str,
    rollback_index: u64,
    flags: u32,
    properties: Vec<(&'a str, &'a str)>,
    descriptors: Vec<DescriptorInfo>,
    chains: Vec<(&'a str, u32, &'a str)>,
}

impl Source<'_> {
    fn write(&self, path: &Path) {
        let args = VbmetaImageArgs {
            algorithm_name: self.algorithm.to_string(),
            key_spec: Some(self.key_spec.to_string()),
            public_key_metadata: None,
            rollback_index: self.rollback_index,
            flags: self.flags,
            rollback_index_location: 0,
            properties: self.properties.iter().map(property).collect(),
            kernel_cmdlines: Vec::new(),
            extra_descriptors: self.descriptors.clone(),
            include_descriptors_from_images: Vec::new(),
            chain_partitions: self.chains.iter().map(chain).collect(),
            release_string: Some("avbtool 1.2.0".to_string()),
            append_to_release_string: None,
            padding_size: 0,
        };
        make_vbmeta_image(path, &args).expect("source vbmeta");
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open source")
            .set_len(PARTITION_BYTES)
            .expect("pad source");
    }
}

fn property(pair: &(&str, &str)) -> PropertySpec {
    PropertySpec {
        key: pair.0.to_string(),
        value: pair.1.as_bytes().to_vec(),
    }
}

fn chain(entry: &(&str, u32, &str)) -> ChainPartitionSpec {
    ChainPartitionSpec {
        partition_name: entry.0.to_string(),
        rollback_index_location: entry.1,
        public_key: extract_public_key(entry.2).expect("chain key"),
        flags: 0,
    }
}

fn expected_in_top(stock: &RawDescriptor) -> RawDescriptor {
    if !matches!(stock.info, DescriptorInfo::ChainPartition { .. }) {
        return stock.clone();
    }
    let test_key = extract_public_key("testkey_rsa2048").expect("key");
    RawDescriptor::chain_partition("vbmeta_system", 2, &test_key, 0)
}

fn hash(partition: &str, digest_byte: u8) -> DescriptorInfo {
    DescriptorInfo::Hash {
        image_size: 4096,
        hash_algorithm: "sha256".to_string(),
        partition_name: partition.to_string(),
        salt: vec![1; 32],
        digest: vec![digest_byte; 32],
        flags: 0,
    }
}

fn hashtree(partition: &str, digest_byte: u8) -> DescriptorInfo {
    DescriptorInfo::Hashtree {
        dm_verity_version: 1,
        image_size: 8192,
        tree_offset: 8192,
        tree_size: 4096,
        data_block_size: 4096,
        hash_block_size: 4096,
        fec_num_roots: 0,
        fec_offset: 0,
        fec_size: 0,
        hash_algorithm: "sha256".to_string(),
        partition_name: partition.to_string(),
        salt: vec![2; 32],
        root_digest: vec![digest_byte; 32],
        flags: 0,
    }
}

fn stock_top() -> Source<'static> {
    Source {
        key_spec: OTHER_KEY,
        algorithm: OTHER_ALGORITHM,
        rollback_index: STOCK_ROLLBACK,
        flags: 0,
        properties: vec![("com.android.build.boot.fingerprint", "stock/475")],
        descriptors: vec![hash("boot", 3), hash("init_boot", 4), hashtree("vendor", 5)],
        chains: vec![("vbmeta_system", 2, OTHER_KEY)],
    }
}

fn original_top() -> Source<'static> {
    Source {
        key_spec: "testkey_rsa4096",
        algorithm: "SHA256_RSA4096",
        rollback_index: 0,
        flags: 0,
        properties: vec![("com.android.build.boot.fingerprint", "stock/468")],
        descriptors: vec![hash("boot", 6), hash("init_boot", 7), hashtree("vendor", 8)],
        chains: vec![("vbmeta_system", 2, "testkey_rsa2048")],
    }
}

fn stock_system() -> Source<'static> {
    Source {
        key_spec: OTHER_KEY,
        algorithm: OTHER_ALGORITHM,
        rollback_index: SYSTEM_ROLLBACK,
        flags: 0,
        properties: vec![
            ("com.android.build.system.fingerprint", STOCK_SYSTEM),
            ("com.android.build.product.fingerprint", "stock/475"),
            ("com.android.build.system_ext.fingerprint", "stock/475"),
        ],
        descriptors: vec![
            hashtree("product", 9),
            hashtree("system", 10),
            hashtree("system_ext", 11),
        ],
        chains: Vec::new(),
    }
}

fn original_system() -> Source<'static> {
    Source {
        key_spec: "testkey_rsa2048",
        algorithm: "SHA256_RSA2048",
        rollback_index: SYSTEM_ROLLBACK,
        flags: 0,
        properties: vec![("com.android.build.product.fingerprint", "stock/468")],
        descriptors: vec![hashtree("product", 12), hashtree("system", 13)],
        chains: Vec::new(),
    }
}

fn gsi() -> Source<'static> {
    Source {
        key_spec: "testkey_rsa4096",
        algorithm: "SHA256_RSA4096",
        rollback_index: 0,
        flags: 0,
        properties: vec![("com.android.build.system.fingerprint", "gsi/1")],
        descriptors: vec![hashtree("system", 14)],
        chains: Vec::new(),
    }
}

struct Bench {
    directory: tempfile::TempDir,
}

impl Bench {
    fn new() -> Self {
        Self {
            directory: tempfile::tempdir().expect("tempdir"),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }

    fn top(&self, stock: &Source<'_>, original: &Source<'_>) -> front2::Result<PathBuf> {
        self.top_with_evidence(stock, original, &[])
    }

    fn top_with_evidence(
        &self,
        stock: &Source<'_>,
        original: &Source<'_>,
        evidence: &[RollbackEvidence],
    ) -> front2::Result<PathBuf> {
        self.top_full(stock, original, evidence, None)
    }

    fn top_with_floor(
        &self,
        stock: &Source<'_>,
        original: &Source<'_>,
        floor: Option<u64>,
    ) -> front2::Result<PathBuf> {
        self.top_full(stock, original, &[], floor)
    }

    fn top_full(
        &self,
        stock: &Source<'_>,
        original: &Source<'_>,
        evidence: &[RollbackEvidence],
        floor: Option<u64>,
    ) -> front2::Result<PathBuf> {
        let stock_path = self.path("stock-top.bin");
        let original_path = self.path("original-top.bin");
        stock.write(&stock_path);
        original.write(&original_path);
        let output = self.path("vbmeta.img");
        let template = VbmetaTemplate {
            stock_slot_a: &stock_path,
            original_slot_b: &original_path,
        };
        build_top_vbmeta(&template, evidence, floor, &output).map(|_| output)
    }

    fn system(
        &self,
        stock: &Source<'_>,
        original: &Source<'_>,
        gsi: &Source<'_>,
    ) -> front2::Result<PathBuf> {
        let stock_path = self.path("stock-system.bin");
        let original_path = self.path("original-system.bin");
        let gsi_path = self.path("gsi.bin");
        stock.write(&stock_path);
        original.write(&original_path);
        gsi.write(&gsi_path);
        let output = self.path("vbmeta_system.img");
        let template = VbmetaTemplate {
            stock_slot_a: &stock_path,
            original_slot_b: &original_path,
        };
        build_system_vbmeta(&template, &gsi_path, &output).map(|_| output)
    }
}

fn refusal(result: front2::Result<PathBuf>) -> String {
    result.expect_err("the build must be refused").to_string()
}

#[test]
fn top_keeps_the_stock_descriptors_and_replaces_only_the_chain_key() {
    let bench = Bench::new();
    let output = bench.top(&stock_top(), &original_top()).expect("top");
    let built = VbmetaSource::read(&output).expect("built");
    let stock = VbmetaSource::read(&bench.path("stock-top.bin")).expect("stock");

    assert_eq!(built.file_bytes, PARTITION_BYTES);
    assert_eq!(built.info.header.rollback_index, STOCK_ROLLBACK);
    assert_eq!(built.info.header.rollback_index_location, 0);
    assert_eq!(built.info.header.flags, 0);
    assert_eq!(built.info.algorithm_name, "SHA256_RSA4096");
    verify_top_signature(&output, "testkey_rsa4096").expect("signature");

    assert_eq!(built.descriptors.len(), stock.descriptors.len());
    let expected: Vec<RawDescriptor> = stock.descriptors.iter().map(expected_in_top).collect();
    assert_eq!(built.descriptors, expected);
    assert_ne!(built.descriptors, stock.descriptors);
}

#[test]
fn top_refuses_an_original_slot_b_with_another_key() {
    let bench = Bench::new();
    let mut original = original_top();
    original.key_spec = OTHER_KEY;
    original.algorithm = OTHER_ALGORITHM;
    let message = refusal(bench.top(&stock_top(), &original));
    assert!(
        message.contains("is not signed with testkey_rsa4096"),
        "{message}"
    );
    assert!(message.contains("no known key"), "{message}");
    assert!(!bench.path("vbmeta.img").exists());
}

#[test]
fn top_names_a_known_key_that_is_not_the_expected_one() {
    let bench = Bench::new();
    let mut original = original_top();
    original.key_spec = "testkey_rsa2048";
    original.algorithm = "SHA256_RSA2048";
    let message = refusal(bench.top(&stock_top(), &original));
    assert!(
        message.contains("the known key testkey_rsa2048"),
        "{message}"
    );
    assert!(!bench.path("vbmeta.img").exists());
}

#[test]
fn top_refuses_rollback_evidence_above_the_stock_value() {
    let bench = Bench::new();
    let evidence = [RollbackEvidence {
        source: "devinfo candidate".to_string(),
        value: STOCK_ROLLBACK + 1,
    }];
    let message = refusal(bench.top_with_evidence(&stock_top(), &original_top(), &evidence));
    assert!(
        message.contains("devinfo candidate gives rollback index"),
        "{message}"
    );

    let bench = Bench::new();
    let mut original = original_top();
    original.rollback_index = STOCK_ROLLBACK + 1;
    let message = refusal(bench.top(&stock_top(), &original));
    assert!(message.contains("must not be lower"), "{message}");

    let bench = Bench::new();
    let evidence = [RollbackEvidence {
        source: "equal".to_string(),
        value: STOCK_ROLLBACK,
    }];
    bench
        .top_with_evidence(&stock_top(), &original_top(), &evidence)
        .expect("equal evidence is accepted");
}

const BOOT_SLOT_FLOOR: u64 = 1_675_555_200;

#[test]
fn top_signs_at_the_rollback_floor_above_the_source_value() {
    // The source top vbmeta declares rollback index 0, below the floor of the
    // boot slot. An in-place slot A install signs the target top vbmeta at exactly
    // the floor so the boot switch passes the rollback check without raising it.
    let bench = Bench::new();
    let output = bench
        .top_with_floor(&original_top(), &original_top(), Some(BOOT_SLOT_FLOOR))
        .expect("the floor is signed");
    let built = VbmetaSource::read(&output).expect("built");
    assert_eq!(built.info.header.rollback_index, BOOT_SLOT_FLOOR);
    assert_eq!(built.info.header.rollback_index_location, 0);
    verify_top_signature(&output, "testkey_rsa4096").expect("signature");
}

#[test]
fn top_refuses_a_rollback_floor_below_the_source_value() {
    let bench = Bench::new();
    let floor = Some(STOCK_ROLLBACK - 1);
    let message = refusal(bench.top_with_floor(&stock_top(), &original_top(), floor));
    assert!(
        message.contains("below the source top vbmeta rollback index"),
        "{message}"
    );
    assert!(!bench.path("vbmeta.img").exists());
}

#[test]
fn top_refuses_rollback_evidence_above_the_floor() {
    let bench = Bench::new();
    let evidence = [RollbackEvidence {
        source: "devinfo candidate".to_string(),
        value: BOOT_SLOT_FLOOR + 1,
    }];
    let result = bench.top_full(
        &original_top(),
        &original_top(),
        &evidence,
        Some(BOOT_SLOT_FLOOR),
    );
    let message = refusal(result);
    assert!(message.contains("do not agree"), "{message}");
    assert!(!bench.path("vbmeta.img").exists());
}

#[test]
fn top_refuses_flags_and_unexpected_chains() {
    let bench = Bench::new();
    let mut stock = stock_top();
    stock.flags = 2;
    assert!(refusal(bench.top(&stock, &original_top())).contains("only 0 is supported"));

    let bench = Bench::new();
    let mut stock = stock_top();
    stock.chains = vec![("vbmeta_system", 3, OTHER_KEY)];
    assert!(refusal(bench.top(&stock, &original_top())).contains("at rollback index location 3"));

    let bench = Bench::new();
    let mut stock = stock_top();
    stock.chains = vec![("vbmeta_vendor", 2, OTHER_KEY)];
    assert!(refusal(bench.top(&stock, &original_top())).contains("chains vbmeta_vendor"));

    let bench = Bench::new();
    let mut stock = stock_top();
    stock.chains = Vec::new();
    assert!(refusal(bench.top(&stock, &original_top())).contains("has 0 chain descriptors"));
}

#[test]
fn top_does_not_overwrite_an_existing_output() {
    let bench = Bench::new();
    std::fs::write(bench.path("vbmeta.img"), b"keep").expect("existing output");
    let result = bench.top(&stock_top(), &original_top());
    assert!(result.is_err());
    assert_eq!(
        std::fs::read(bench.path("vbmeta.img")).expect("read"),
        b"keep"
    );
}

#[test]
fn system_takes_product_from_stock_and_all_descriptors_from_the_gsi() {
    let bench = Bench::new();
    let output = bench
        .system(&stock_system(), &original_system(), &gsi())
        .expect("system");
    let built = VbmetaSource::read(&output).expect("built");
    let stock = VbmetaSource::read(&bench.path("stock-system.bin")).expect("stock");
    let gsi_source = VbmetaSource::read(&bench.path("gsi.bin")).expect("gsi");

    assert_eq!(built.file_bytes, PARTITION_BYTES);
    assert_eq!(built.info.header.rollback_index, SYSTEM_ROLLBACK);
    assert_eq!(built.info.algorithm_name, "SHA256_RSA2048");
    verify_top_signature(&output, "testkey_rsa2048").expect("signature");

    let mut expected: Vec<RawDescriptor> = stock
        .descriptors
        .iter()
        .filter(|descriptor| match &descriptor.info {
            DescriptorInfo::Hashtree { partition_name, .. } => partition_name == "product",
            DescriptorInfo::Property { key, .. } => key == "com.android.build.product.fingerprint",
            _ => false,
        })
        .cloned()
        .collect();
    assert_eq!(expected.len(), 2);
    expected.extend(gsi_source.descriptors.iter().cloned());
    assert_eq!(built.descriptors, expected);
}

#[test]
fn system_refuses_rollback_values_that_differ_between_the_slots() {
    let bench = Bench::new();
    let mut original = original_system();
    original.rollback_index = SYSTEM_ROLLBACK - 1;
    let message = refusal(bench.system(&stock_system(), &original, &gsi()));
    assert!(
        message.contains("no tested rule selects between them"),
        "{message}"
    );
}

#[test]
fn system_refuses_an_original_slot_b_with_another_key() {
    let bench = Bench::new();
    let mut original = original_system();
    original.key_spec = "testkey_rsa4096";
    original.algorithm = "SHA256_RSA4096";
    let message = refusal(bench.system(&stock_system(), &original, &gsi()));
    assert!(
        message.contains("is not signed with testkey_rsa2048"),
        "{message}"
    );
    assert!(
        message.contains("the known key testkey_rsa4096"),
        "{message}"
    );
}

#[test]
fn system_refuses_a_gsi_with_other_descriptors() {
    let bench = Bench::new();
    let mut image = gsi();
    image.descriptors = vec![hashtree("system", 14), hash("boot", 15)];
    let message = refusal(bench.system(&stock_system(), &original_system(), &image));
    assert!(
        message.contains("not a hashtree or a property"),
        "{message}"
    );

    let bench = Bench::new();
    let mut image = gsi();
    image.descriptors = vec![hashtree("vendor", 14)];
    let message = refusal(bench.system(&stock_system(), &original_system(), &image));
    assert!(
        message.contains("must give one hashtree for system"),
        "{message}"
    );

    let bench = Bench::new();
    let mut image = gsi();
    image.descriptors = vec![hashtree("system", 14), hashtree("system", 16)];
    let message = refusal(bench.system(&stock_system(), &original_system(), &image));
    assert!(
        message.contains("must give one hashtree for system"),
        "{message}"
    );
}

#[test]
fn system_refuses_a_stock_image_without_one_product_hashtree() {
    let bench = Bench::new();
    let mut stock = stock_system();
    stock.descriptors = vec![hashtree("system", 10)];
    let message = refusal(bench.system(&stock, &original_system(), &gsi()));
    assert!(
        message.contains("must give one hashtree for product"),
        "{message}"
    );
}

fn flip_signature_byte(path: &Path) {
    let source = VbmetaSource::read(path).expect("source");
    let offset = 256 + source.info.header.signature_offset as usize;
    let mut bytes = std::fs::read(path).expect("read source");
    bytes[offset] ^= 1;
    std::fs::write(path, bytes).expect("write source");
}

#[test]
fn letter_1134_top_refuses_an_original_slot_b_with_a_damaged_signature() {
    let bench = Bench::new();
    let output = bench
        .top(&stock_top(), &original_top())
        .expect("the control builds");
    std::fs::remove_file(&output).expect("remove the control");

    let stock = bench.path("stock-top.bin");
    let original = bench.path("original-top.bin");
    flip_signature_byte(&original);
    let template = VbmetaTemplate {
        stock_slot_a: &stock,
        original_slot_b: &original,
    };
    let error = build_top_vbmeta(&template, &[], None, &output).expect_err("damaged signature");
    assert!(
        matches!(error, front2::Front2Error::Verification(_)),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("RSA verification rejected the signature"),
        "{error}"
    );
    assert!(!output.exists());
}

#[test]
fn letter_1134_system_refuses_an_original_slot_b_with_a_damaged_signature() {
    let bench = Bench::new();
    let output = bench
        .system(&stock_system(), &original_system(), &gsi())
        .expect("the control builds");
    std::fs::remove_file(&output).expect("remove the control");

    let stock = bench.path("stock-system.bin");
    let original = bench.path("original-system.bin");
    flip_signature_byte(&original);
    let template = VbmetaTemplate {
        stock_slot_a: &stock,
        original_slot_b: &original,
    };
    let error = build_system_vbmeta(&template, &bench.path("gsi.bin"), &output)
        .expect_err("damaged signature");
    assert!(
        matches!(error, front2::Front2Error::Verification(_)),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("RSA verification rejected the signature"),
        "{error}"
    );
    assert!(!output.exists());
}

#[test]
fn letter_1515_a_damaged_signature_is_not_told_as_signed_with_a_known_key() {
    let bench = Bench::new();
    let path = bench.path("signed-system.bin");
    original_system().write(&path);
    flip_signature_byte(&path);
    let source = VbmetaSource::read(&path).expect("source");

    let against_its_own_key = source
        .require_signed_with("testkey_rsa2048", "SHA256_RSA2048")
        .expect_err("the damaged signature fails");
    assert!(
        against_its_own_key
            .to_string()
            .contains("RSA verification rejected the signature"),
        "{against_its_own_key}"
    );

    let against_another_key = source
        .require_signed_with("testkey_rsa4096", "SHA256_RSA4096")
        .expect_err("the key does not match")
        .to_string();
    assert!(
        against_another_key.contains("its public key matches the known key testkey_rsa2048"),
        "{against_another_key}"
    );
    assert!(
        !against_another_key.contains("signed with the known key"),
        "{against_another_key}"
    );
}

const TESTED_BUILD: &str = "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/475:user/dev-keys";
const OTHER_BUILD: &str = "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/468:user/dev-keys";
const OTHER_DEVICE: &str = "OTHER/other_device/other_device:13/TKQ1.230110.001/475:user/dev-keys";

fn front2_stock_top(fingerprints: &[&'static str]) -> Source<'static> {
    Source {
        key_spec: "testkey_rsa4096",
        algorithm: "SHA256_RSA4096",
        rollback_index: STOCK_ROLLBACK,
        flags: 0,
        properties: fingerprints
            .iter()
            .map(|value| ("com.android.build.boot.fingerprint", *value))
            .collect(),
        descriptors: vec![
            hash("boot", 3),
            hash("dtbo", 4),
            hash("init_boot", 5),
            hash("recovery", 6),
            hash("vendor_boot", 7),
        ],
        chains: vec![("vbmeta_system", 2, OTHER_KEY)],
    }
}

fn stock_build(fingerprints: &[&'static str]) -> front2::Result<StockBuild> {
    let bench = Bench::new();
    let path = bench.path("stock-top.bin");
    front2_stock_top(fingerprints).write(&path);
    require_stock_front2_top(&VbmetaSource::read(&path)?)
}

#[test]
fn the_stock_top_gives_the_tested_build_or_an_untested_build_number() {
    let tested = stock_build(&[TESTED_BUILD]).expect("the tested build");
    assert_eq!(tested, StockBuild::Tested);
    let other = stock_build(&[OTHER_BUILD]).expect("another build number");
    assert_eq!(other, StockBuild::Untested(OTHER_BUILD.to_string()));
}

#[test]
fn the_stock_top_refusal_tells_the_fingerprints_of_the_backup() {
    let cases: [&[&'static str]; 3] = [&[], &[OTHER_DEVICE], &[TESTED_BUILD, OTHER_BUILD]];
    for fingerprints in cases {
        let refused = stock_build(fingerprints).expect_err("the stock top is not used");
        assert!(
            matches!(refused, front2::Front2Error::Unsupported(_)),
            "{refused}"
        );
        let message = refused.to_string();
        assert!(message.contains("stock-top.bin"), "{message}");
        assert!(
            message.contains(&format!("has the values {fingerprints:?}")),
            "{message}"
        );
    }
}

fn stock_system_with(fingerprint: &str) -> Source<'_> {
    Source {
        key_spec: "testkey_rsa2048",
        algorithm: "SHA256_RSA2048",
        rollback_index: SYSTEM_ROLLBACK,
        flags: 0,
        properties: vec![("com.android.build.system.fingerprint", fingerprint)],
        descriptors: vec![hashtree("system", 10)],
        chains: Vec::new(),
    }
}

fn system_without_fingerprint() -> Source<'static> {
    Source {
        key_spec: "testkey_rsa2048",
        algorithm: "SHA256_RSA2048",
        rollback_index: SYSTEM_ROLLBACK,
        flags: 0,
        properties: vec![("com.android.build.product.fingerprint", "stock/475")],
        descriptors: vec![hashtree("system", 10)],
        chains: Vec::new(),
    }
}

fn read_system(source: &Source<'_>) -> front2::Result<()> {
    let bench = Bench::new();
    let path = bench.path("vbmeta_system.bin");
    source.write(&path);
    require_stock_front2_system(&VbmetaSource::read(&path)?)
}

#[test]
fn the_stock_system_passes_a_qti_fingerprint() {
    read_system(&stock_system()).expect("the stock system passes");
    read_system(&stock_system_with(
        "qti/qssi/qssi:13/TKQ1.230110.001/468:user/dev-keys",
    ))
    .expect("another stock build passes");
}

#[test]
fn a_gsi_or_a_broken_system_fingerprint_is_refused() {
    for refused in [
        GSI_SYSTEM,
        "qti/qssi/qssi:13/TKQ1.230110.001/:user/dev-keys",
        "qti/qssi/qssi:13/TKQ1.230110.001/46x:user/dev-keys",
        "qti/qssi/qssi:13/TKQ1.230110.001/475:user/release-keys",
    ] {
        let error = read_system(&stock_system_with(refused)).expect_err(refused);
        assert!(
            matches!(error, front2::Front2Error::Unsupported(_)),
            "{refused}: {error}"
        );
        assert!(
            error.to_string().contains("not the form"),
            "{refused}: {error}"
        );
    }
    let missing = read_system(&system_without_fingerprint()).expect_err("no system fingerprint");
    assert!(
        missing
            .to_string()
            .contains("no com.android.build.system.fingerprint"),
        "{missing}"
    );
}

fn write_vbmeta_backup(backup: &Path, stock_top: &Source<'_>) {
    let directory = backup.join("partitions").join("lun4");
    std::fs::create_dir_all(&directory).expect("create the LUN directory");
    let mut entries = Vec::new();
    for (name, source) in [
        ("vbmeta_a", stock_top),
        ("vbmeta_b", &original_top()),
        ("vbmeta_system_a", &stock_system()),
        ("vbmeta_system_b", &original_system()),
    ] {
        let path = directory.join(format!("{name}.bin"));
        source.write(&path);
        let (bytes, sha256) = sha256_file(&path).expect("digest of the dump");
        entries.push(serde_json::json!({
            "path": format!("partitions\\lun4\\{name}.bin"),
            "bytes": bytes,
            "sha256": sha256.to_ascii_uppercase(),
        }));
    }
    let text = serde_json::to_string_pretty(&entries).expect("manifest text");
    std::fs::write(backup.join("sha256-manifest.json"), text).expect("write the manifest");
}

#[test]
fn a_candidate_from_an_untested_build_needs_the_acceptance_of_its_fingerprint() {
    let bench = Bench::new();
    let backup = bench.path("backup");
    write_vbmeta_backup(&backup, &front2_stock_top(&[OTHER_BUILD]));
    let gsi = bench.path("gsi.img");
    std::fs::write(&gsi, b"gsi").expect("write the GSI");
    let output = bench.path("candidate");
    let build = |accepted| {
        GsiCandidate::build(&GsiCandidateRequest {
            profile: &PROFILE,
            backup_directory: &backup,
            gsi_image: &gsi,
            output_directory: &output,
            rollback_evidence: &[],
            accepted_untested_build: accepted,
            stock_slot: BootSlot::A,
            target_slot: BootSlot::B,
        })
        .expect_err("the backup has no super dump")
        .to_string()
    };

    let asked = backup_stock_build(&backup, BootSlot::A).expect("the build of the backup");
    assert_eq!(asked, StockBuild::Untested(OTHER_BUILD.to_string()));
    for accepted in [None, Some(TESTED_BUILD)] {
        let refused = build(accepted);
        assert!(refused.contains("the user did not accept it"), "{refused}");
        assert!(refused.contains(OTHER_BUILD), "{refused}");
        assert!(!output.exists());
    }
    let later = build(Some(OTHER_BUILD));
    assert!(!later.contains("did not accept"), "{later}");
    assert!(later.contains("partition super"), "{later}");
    assert!(!output.exists());
}

#[test]
fn a_candidate_from_the_tested_build_needs_no_acceptance() {
    let bench = Bench::new();
    let backup = bench.path("backup");
    write_vbmeta_backup(&backup, &front2_stock_top(&[TESTED_BUILD]));
    let gsi = bench.path("gsi.img");
    std::fs::write(&gsi, b"gsi").expect("write the GSI");
    let output = bench.path("candidate");
    let later = GsiCandidate::build(&GsiCandidateRequest {
        profile: &PROFILE,
        backup_directory: &backup,
        gsi_image: &gsi,
        output_directory: &output,
        rollback_evidence: &[],
        accepted_untested_build: None,
        stock_slot: BootSlot::A,
        target_slot: BootSlot::B,
    })
    .expect_err("the backup has no super dump")
    .to_string();

    let asked = backup_stock_build(&backup, BootSlot::A).expect("the build of the backup");
    assert_eq!(asked, StockBuild::Tested);
    assert!(later.contains("partition super"), "{later}");
}

fn write_front2_tops(backup: &Path, slot_a: &Source<'_>, slot_b: &Source<'_>) {
    let directory = backup.join("partitions").join("lun4");
    std::fs::create_dir_all(&directory).expect("create the LUN directory");
    let mut entries = Vec::new();
    for (name, source) in [("vbmeta_a", slot_a), ("vbmeta_b", slot_b)] {
        let path = directory.join(format!("{name}.bin"));
        source.write(&path);
        let (bytes, sha256) = sha256_file(&path).expect("digest of the dump");
        entries.push(serde_json::json!({
            "path": format!("partitions\\lun4\\{name}.bin"),
            "bytes": bytes,
            "sha256": sha256.to_ascii_uppercase(),
        }));
    }
    let text = serde_json::to_string_pretty(&entries).expect("manifest text");
    std::fs::write(backup.join("sha256-manifest.json"), text).expect("write the manifest");
}

#[test]
fn backup_stock_build_reads_the_top_vbmeta_of_the_given_slot() {
    let bench = Bench::new();
    let backup = bench.path("backup");
    write_front2_tops(
        &backup,
        &front2_stock_top(&[TESTED_BUILD]),
        &front2_stock_top(&[OTHER_BUILD]),
    );
    assert_eq!(
        backup_stock_build(&backup, BootSlot::A).expect("the build of slot A"),
        StockBuild::Tested
    );
    assert_eq!(
        backup_stock_build(&backup, BootSlot::B).expect("the build of slot B"),
        StockBuild::Untested(OTHER_BUILD.to_string())
    );
}

#[test]
fn real_images_give_the_bytes_that_started_the_device_when_available() {
    let Some(evidence) = std::env::var_os("FRONT2_REAL_EVIDENCE_DIR").map(PathBuf::from) else {
        eprintln!("skipped: FRONT2_REAL_EVIDENCE_DIR is not set");
        return;
    };
    let directory = tempfile::tempdir().expect("tempdir");
    let check = evidence.join("avb-check");

    let stock_top = check.join("vbmeta_a.bin");
    let original_top = check.join("vbmeta_b.bin");
    let template = VbmetaTemplate {
        stock_slot_a: &stock_top,
        original_slot_b: &original_top,
    };
    let output = directory.path().join("vbmeta.img");
    let built = build_top_vbmeta(&template, &[], None, &output).expect("top");
    let reference = evidence
        .join("root-preparation")
        .join("current-vbmeta_b.img");
    let (reference_bytes, reference_sha256) = sha256_file(&reference).expect("reference");
    println!("top vbmeta: built {built:?}");
    println!("top vbmeta: reference {reference_bytes} bytes {reference_sha256}");
    assert_eq!(built.image.bytes, reference_bytes);
    assert_eq!(built.image.sha256, reference_sha256);

    let Some(gsi_image) = std::env::var_os("FRONT2_REAL_GSI_IMAGE").map(PathBuf::from) else {
        eprintln!("skipped the vbmeta_system part: FRONT2_REAL_GSI_IMAGE is not set");
        return;
    };
    let stock_system = check.join("vbmeta_system_a.bin");
    let original_system = check.join("vbmeta_system_b.bin");
    let template = VbmetaTemplate {
        stock_slot_a: &stock_system,
        original_slot_b: &original_system,
    };
    let output = directory.path().join("vbmeta_system.img");
    let built = build_system_vbmeta(&template, &gsi_image, &output).expect("system");
    let record: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            evidence
                .join("prepared-gsi")
                .join("candidate-verification.json"),
        )
        .expect("candidate verification"),
    )
    .expect("json");
    let reference_sha256 = record["outputs"]
        .as_array()
        .expect("outputs")
        .iter()
        .find(|output| output["partition"] == "vbmeta_system_b")
        .and_then(|output| output["sha256"].as_str())
        .expect("vbmeta_system_b record");
    println!("vbmeta_system: built {built:?}");
    println!("vbmeta_system: reference {reference_sha256}");
    assert_eq!(built.image.sha256, reference_sha256);
}

const GSI_BYTES: u64 = 4608 * 512;

// The descriptors below carry empty digests. libavb treats an empty digest as a
// persistent digest and does not compare it against the image, so verify_view
// passes without a full consistent hash tree. The digest arithmetic is checked
// by the build_top_vbmeta and build_system_vbmeta tests above, so this test can
// keep the images small and check the test key slot A candidate build instead.
fn hash_persistent(partition: &str) -> DescriptorInfo {
    DescriptorInfo::Hash {
        image_size: 4096,
        hash_algorithm: "sha256".to_string(),
        partition_name: partition.to_string(),
        salt: vec![1; 32],
        digest: Vec::new(),
        flags: 0,
    }
}

fn hashtree_persistent(partition: &str) -> DescriptorInfo {
    DescriptorInfo::Hashtree {
        dm_verity_version: 1,
        image_size: 4096,
        tree_offset: 0,
        tree_size: 0,
        data_block_size: 4096,
        hash_block_size: 4096,
        fec_num_roots: 0,
        fec_offset: 0,
        fec_size: 0,
        hash_algorithm: "sha256".to_string(),
        partition_name: partition.to_string(),
        salt: vec![2; 32],
        root_digest: Vec::new(),
        flags: 0,
    }
}

fn testkey_slot_a_top_vbmeta(fingerprints: &[&'static str]) -> Source<'static> {
    Source {
        key_spec: "testkey_rsa4096",
        algorithm: "SHA256_RSA4096",
        rollback_index: 0,
        flags: 0,
        properties: fingerprints
            .iter()
            .map(|value| ("com.android.build.boot.fingerprint", *value))
            .collect(),
        descriptors: vec![
            hash_persistent("boot"),
            hash_persistent("dtbo"),
            hash_persistent("init_boot"),
            hash_persistent("recovery"),
            hash_persistent("vendor_boot"),
        ],
        chains: vec![("vbmeta_system", 2, OTHER_KEY)],
    }
}

fn stock_slot_b_top_vbmeta() -> Source<'static> {
    let mut source = stock_top();
    source.rollback_index = BOOT_SLOT_FLOOR;
    source
}

fn testkey_slot_a_system_vbmeta() -> Source<'static> {
    Source {
        key_spec: "testkey_rsa2048",
        algorithm: "SHA256_RSA2048",
        rollback_index: SYSTEM_ROLLBACK,
        flags: 0,
        properties: vec![
            ("com.android.build.system.fingerprint", STOCK_SYSTEM),
            ("com.android.build.product.fingerprint", "stock/468"),
        ],
        descriptors: vec![hashtree_persistent("product")],
        chains: Vec::new(),
    }
}

fn gsi_source() -> Source<'static> {
    Source {
        key_spec: "testkey_rsa4096",
        algorithm: "SHA256_RSA4096",
        rollback_index: 0,
        flags: 0,
        properties: vec![("com.android.build.system.fingerprint", "gsi/1")],
        descriptors: vec![hashtree_persistent("system")],
        chains: Vec::new(),
    }
}

fn record_entry(entries: &mut Vec<serde_json::Value>, relative: &str, path: &Path) {
    let (bytes, sha256) = sha256_file(path).expect("digest of the dump");
    entries.push(serde_json::json!({
        "path": relative,
        "bytes": bytes,
        "sha256": sha256.to_ascii_uppercase(),
    }));
}

fn write_testkey_slot_a_backup(backup: &Path) {
    let lun0 = backup.join("partitions").join("lun0");
    let lun4 = backup.join("partitions").join("lun4");
    std::fs::create_dir_all(&lun0).expect("create lun0");
    std::fs::create_dir_all(&lun4).expect("create lun4");
    let mut entries = Vec::new();

    let super_path = lun0.join("super.bin");
    std::fs::write(&super_path, SyntheticSuper::standard().image()).expect("write the super");
    record_entry(&mut entries, "partitions\\lun0\\super.bin", &super_path);

    for name in [
        "boot_a",
        "dtbo_a",
        "init_boot_a",
        "recovery_a",
        "vendor_boot_a",
    ] {
        let path = lun4.join(format!("{name}.bin"));
        std::fs::write(&path, vec![0x5A; 4096]).expect("write the physical image");
        record_entry(
            &mut entries,
            &format!("partitions\\lun4\\{name}.bin"),
            &path,
        );
    }

    for (name, source) in [
        ("vbmeta_a", testkey_slot_a_top_vbmeta(&[TESTED_BUILD])),
        ("vbmeta_b", stock_slot_b_top_vbmeta()),
        ("vbmeta_system_a", testkey_slot_a_system_vbmeta()),
    ] {
        let path = lun4.join(format!("{name}.bin"));
        source.write(&path);
        record_entry(
            &mut entries,
            &format!("partitions\\lun4\\{name}.bin"),
            &path,
        );
    }

    let text = serde_json::to_string_pretty(&entries).expect("manifest text");
    std::fs::write(backup.join("sha256-manifest.json"), text).expect("write the manifest");
}

fn write_gsi_image(path: &Path) {
    gsi_source().write(path);
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open the GSI")
        .set_len(GSI_BYTES)
        .expect("pad the GSI");
}

#[test]
fn an_install_to_the_test_key_slot_a_builds_a_candidate_in_place() {
    let bench = Bench::new();
    let backup = bench.path("backup");
    write_testkey_slot_a_backup(&backup);
    let gsi = bench.path("gsi.img");
    write_gsi_image(&gsi);
    let output = bench.path("candidate");

    let candidate = GsiCandidate::build(&GsiCandidateRequest {
        profile: &PROFILE,
        backup_directory: &backup,
        gsi_image: &gsi,
        output_directory: &output,
        rollback_evidence: &[],
        accepted_untested_build: None,
        stock_slot: BootSlot::B,
        target_slot: BootSlot::A,
    })
    .expect("the test key slot A candidate builds");

    assert_eq!(candidate.stock_slot, BootSlot::B);
    assert_eq!(candidate.target_slot, BootSlot::A);
    assert_eq!(candidate.source_slot, BootSlot::A);
    assert_eq!(candidate.top_vbmeta.rollback_index, BOOT_SLOT_FLOOR);

    let view: Vec<&str> = candidate
        .logical_view
        .iter()
        .map(|record| record.file.as_str())
        .collect();
    assert_eq!(
        view,
        [
            "odm.img",
            "product.img",
            "system.img",
            "system_dlkm.img",
            "vendor.img",
            "vendor_dlkm.img"
        ]
    );

    let physical: Vec<(&str, Option<&str>)> = candidate
        .physical_images
        .iter()
        .map(|image| (image.partition.as_str(), image.backup_source.as_deref()))
        .collect();
    assert_eq!(
        physical,
        [
            ("boot_a", Some("boot_a")),
            ("dtbo_a", Some("dtbo_a")),
            ("init_boot_a", Some("init_boot_a")),
            ("recovery_a", Some("recovery_a")),
            ("vendor_boot_a", Some("vendor_boot_a")),
        ]
    );

    assert_eq!(candidate.images().len(), 8);
    assert!(!candidate.avb_verification.is_empty());

    let (loaded, _) = GsiCandidate::load(&output).expect("load the saved candidate");
    assert_eq!(loaded.source_slot, BootSlot::A);
    assert_eq!(loaded.target_slot, BootSlot::A);
    assert_eq!(loaded.images(), candidate.images());
    loaded
        .verify_files(&output)
        .expect("the files match their records");
}

fn write_installed_slot_a_backup(backup: &Path, installed: &Path, candidate: &GsiCandidate) {
    let lun0 = backup.join("partitions").join("lun0");
    let lun4 = backup.join("partitions").join("lun4");
    std::fs::create_dir_all(&lun0).expect("create lun0");
    std::fs::create_dir_all(&lun4).expect("create lun4");
    let view = installed.join("view");
    let mut entries = Vec::new();
    let mut copy = |source: PathBuf, folder: &Path, relative: String| {
        let name = relative.rsplit('\\').next().expect("file name").to_string();
        let path = folder.join(name);
        std::fs::copy(&source, &path).expect("copy the installed image");
        record_entry(&mut entries, &relative, &path);
    };

    copy(
        installed.join("super.img"),
        &lun0,
        "partitions\\lun0\\super.bin".to_string(),
    );
    for image in &candidate.physical_images {
        copy(
            view.join(&image.image.file),
            &lun4,
            format!("partitions\\lun4\\{}.bin", image.partition),
        );
    }
    copy(
        view.join(&candidate.top_vbmeta.image.file),
        &lun4,
        "partitions\\lun4\\vbmeta_a.bin".to_string(),
    );
    copy(
        view.join(&candidate.chained_vbmeta.image.file),
        &lun4,
        "partitions\\lun4\\vbmeta_system_a.bin".to_string(),
    );
    for (name, source) in [
        ("vbmeta_b", stock_slot_b_top_vbmeta()),
        ("vbmeta_system_b", testkey_slot_a_system_vbmeta()),
    ] {
        let path = lun4.join(format!("{name}.bin"));
        source.write(&path);
        record_entry(
            &mut entries,
            &format!("partitions\\lun4\\{name}.bin"),
            &path,
        );
    }

    let text = serde_json::to_string_pretty(&entries).expect("manifest text");
    std::fs::write(backup.join("sha256-manifest.json"), text).expect("write the manifest");
}

#[test]
fn an_in_place_install_builds_a_candidate_on_a_slot_that_holds_a_gsi() {
    let bench = Bench::new();
    let stock_backup = bench.path("stock-backup");
    write_testkey_slot_a_backup(&stock_backup);
    let gsi = bench.path("gsi.img");
    write_gsi_image(&gsi);
    let first_output = bench.path("first-candidate");
    let request =
        |backup_directory, output_directory, stock_slot, target_slot| GsiCandidateRequest {
            profile: &PROFILE,
            backup_directory,
            gsi_image: &gsi,
            output_directory,
            rollback_evidence: &[],
            accepted_untested_build: None,
            stock_slot,
            target_slot,
        };
    let first = GsiCandidate::build(&request(
        &stock_backup,
        &first_output,
        BootSlot::B,
        BootSlot::A,
    ))
    .expect("the first candidate builds");
    assert_eq!(
        backup_system_origin(&stock_backup, BootSlot::A).expect("the stock system"),
        SystemOrigin::Stock
    );

    let installed_backup = bench.path("installed-backup");
    write_installed_slot_a_backup(&installed_backup, &first_output, &first);
    assert_eq!(
        backup_system_origin(&installed_backup, BootSlot::A).expect("the installed system"),
        SystemOrigin::Other("gsi/1".to_string())
    );

    let second_output = bench.path("second-candidate");
    let second = GsiCandidate::build(&request(
        &installed_backup,
        &second_output,
        BootSlot::A,
        BootSlot::A,
    ))
    .expect("the in-place candidate builds on the slot that holds a GSI");
    assert_eq!(second.stock_slot, BootSlot::A);
    assert_eq!(second.source_slot, BootSlot::A);
    assert_eq!(second.target_slot, BootSlot::A);
    assert_eq!(second.top_vbmeta.rollback_index, BOOT_SLOT_FLOOR);
    assert_eq!(second.logical_view, first.logical_view);
    assert!(!second.avb_verification.is_empty());

    let switch_output = bench.path("switch-candidate");
    let refused = GsiCandidate::build(&request(
        &installed_backup,
        &switch_output,
        BootSlot::A,
        BootSlot::B,
    ))
    .expect_err("a switch install does not read a slot that holds a GSI");
    assert!(
        matches!(refused, front2::Front2Error::Unsupported(_)),
        "{refused}"
    );
    assert!(refused.to_string().contains("not the form"), "{refused}");
    assert!(!switch_output.exists());
}
