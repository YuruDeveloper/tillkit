use super::kernel_device::KernelDevice;
use super::synthetic_kernel::{CURRENT_RELEASE, SyntheticKernel};
use crate::support::FakeDevice;
use avbtool_rs::builder::{ChainPartitionSpec, PropertySpec, VbmetaImageArgs, make_vbmeta_image};
use avbtool_rs::footer::{HashFooterArgs, add_hash_footer};
use avbtool_rs::info::DescriptorInfo;
use sha2::{Digest, Sha256};
use std::path::Path;

pub const BOOT_PARTITION_BYTES: usize = 1024 * 1024;
pub const VBMETA_BYTES: usize = 65536;
pub const TOP_ROLLBACK_INDEX: u64 = 1_675_555_200;
pub const CURRENT_KERNEL_BYTES: usize = 150_000;
pub const DTBO_DESCRIPTOR_BYTES: usize = 10_997;

const REFERENCES: [(&str, &str, usize, usize, u8); 4] = [
    ("vendor_boot", "/dev/block/sde60", 131_072, 40_960, 0x11),
    (
        "dtbo",
        "/dev/block/sde54",
        65_536,
        DTBO_DESCRIPTOR_BYTES,
        0x22,
    ),
    ("init_boot", "/dev/block/sde58", 65_536, 20_480, 0x33),
    ("recovery", "/dev/block/sde56", 131_072, 61_440, 0x44),
];

pub fn current_kernel() -> SyntheticKernel {
    SyntheticKernel::with_release(CURRENT_RELEASE, CURRENT_KERNEL_BYTES, 0x5a)
}

pub fn reference_content(bytes: usize, seed: u8) -> Vec<u8> {
    (0..bytes).map(|index| (index % 251) as u8 ^ seed).collect()
}

pub fn reference_descriptor(
    base: &str,
    content: &[u8],
    covered: usize,
    seed: u8,
) -> DescriptorInfo {
    let salt = vec![seed; 32];
    let mut hasher = Sha256::new();
    hasher.update(&salt);
    hasher.update(&content[..covered]);
    DescriptorInfo::Hash {
        image_size: covered as u64,
        hash_algorithm: "sha256".into(),
        partition_name: base.into(),
        salt,
        digest: hasher.finalize().to_vec(),
        flags: 0,
    }
}

pub fn reference_descriptors() -> Vec<DescriptorInfo> {
    REFERENCES
        .iter()
        .map(|(base, _, bytes, covered, seed)| {
            reference_descriptor(base, &reference_content(*bytes, *seed), *covered, *seed)
        })
        .collect()
}

pub fn write_boot_partition(boot: &Path, payload: &[u8], partition_bytes: u64) {
    std::fs::write(boot, payload).expect("write boot payload");
    let property = |key: &str, value: &str| PropertySpec {
        key: format!("com.android.build.boot.{key}"),
        value: value.as_bytes().to_vec(),
    };
    add_hash_footer(
        boot,
        &HashFooterArgs {
            partition_size: Some(partition_bytes),
            dynamic_partition_size: false,
            partition_name: "boot".into(),
            hash_algorithm: "sha256".into(),
            salt: Some(vec![0x7d; 32]),
            chain_partitions: Vec::new(),
            algorithm_name: "NONE".into(),
            key_spec: None,
            public_key_metadata: None,
            rollback_index: 0,
            flags: 0,
            rollback_index_location: 0,
            properties: vec![
                property("os_version", "13"),
                property("security_patch", "2023-02-05"),
                property("security_patch", "2023-02-05"),
            ],
            kernel_cmdlines: Vec::new(),
            include_descriptors_from_images: Vec::new(),
            release_string: None,
            append_to_release_string: None,
            output_vbmeta_image: None,
            do_not_append_vbmeta_image: false,
            use_persistent_digest: false,
            do_not_use_ab: false,
        },
    )
    .expect("add boot hash footer");
}

pub fn write_top_vbmeta(vbmeta: &Path, boot: &Path, references: Vec<DescriptorInfo>) {
    make_vbmeta_image(
        vbmeta,
        &VbmetaImageArgs {
            algorithm_name: "SHA256_RSA4096".into(),
            key_spec: Some("testkey_rsa4096".into()),
            public_key_metadata: None,
            rollback_index: TOP_ROLLBACK_INDEX,
            flags: 0,
            rollback_index_location: 0,
            properties: vec![PropertySpec {
                key: "com.android.build.vendor.os_version".into(),
                value: b"13".to_vec(),
            }],
            kernel_cmdlines: Vec::new(),
            extra_descriptors: references,
            include_descriptors_from_images: vec![boot.to_path_buf()],
            chain_partitions: vec![ChainPartitionSpec {
                partition_name: "vbmeta_system".into(),
                rollback_index_location: 2,
                public_key: avbtool_rs::crypto::extract_public_key("testkey_rsa2048")
                    .expect("bundled key"),
                flags: 0,
            }],
            release_string: None,
            append_to_release_string: None,
            padding_size: 0,
        },
    )
    .expect("make top vbmeta");
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(vbmeta)
        .expect("open vbmeta");
    assert!(file.metadata().expect("vbmeta metadata").len() <= VBMETA_BYTES as u64);
    file.set_len(VBMETA_BYTES as u64).expect("pad vbmeta");
}

pub fn front2_device(directory: &Path) -> KernelDevice {
    let boot = directory.join("boot.img");
    let vbmeta = directory.join("vbmeta.img");
    write_boot_partition(
        &boot,
        &current_kernel().boot_image(),
        BOOT_PARTITION_BYTES as u64,
    );
    write_top_vbmeta(&vbmeta, &boot, reference_descriptors());
    let content = |base: &str| {
        let (_, _, bytes, _, seed) = REFERENCES
            .iter()
            .find(|reference| reference.0 == base)
            .expect("known reference");
        reference_content(*bytes, *seed)
    };
    let inner = FakeDevice::front2(
        content("init_boot"),
        std::fs::read(&vbmeta).expect("vbmeta"),
    );
    let mut device = KernelDevice {
        inner,
        running_release: CURRENT_RELEASE.to_string(),
    };
    device.add_partition(
        "boot_b",
        "/dev/block/sde52",
        std::fs::read(&boot).expect("boot"),
    );
    for (base, node, _, _, _) in REFERENCES {
        device.add_partition(&format!("{base}_b"), node, content(base));
    }
    device
}
