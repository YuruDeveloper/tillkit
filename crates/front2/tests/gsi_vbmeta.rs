use avbtool_rs::builder::{ChainPartitionSpec, PropertySpec, VbmetaImageArgs, make_vbmeta_image};
use avbtool_rs::crypto::extract_public_key;
use avbtool_rs::info::DescriptorInfo;
use front2::digest::sha256_file;
use front2::gsi::{
    RawDescriptor, RollbackEvidence, VbmetaSource, VbmetaTemplate, build_system_vbmeta,
    build_top_vbmeta,
};
use front2::top_signature::verify_top_signature;
use std::path::{Path, PathBuf};

const PARTITION_BYTES: u64 = 65536;
const STOCK_ROLLBACK: u64 = 1_700_000_000;
const SYSTEM_ROLLBACK: u64 = 1_710_000_000;
const OTHER_KEY: &str = "testkey_rsa8192";
const OTHER_ALGORITHM: &str = "SHA256_RSA8192";

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
            ("com.android.build.system.fingerprint", "stock/475"),
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
        let stock_path = self.path("stock-top.bin");
        let original_path = self.path("original-top.bin");
        stock.write(&stock_path);
        original.write(&original_path);
        let output = self.path("vbmeta.img");
        let template = VbmetaTemplate {
            stock_slot_a: &stock_path,
            original_slot_b: &original_path,
        };
        build_top_vbmeta(&template, evidence, &output).map(|_| output)
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
    let error = build_top_vbmeta(&template, &[], &output).expect_err("damaged signature");
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
    let built = build_top_vbmeta(&template, &[], &output).expect("top");
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
