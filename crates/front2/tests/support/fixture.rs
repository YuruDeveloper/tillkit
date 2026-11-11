use avbtool_rs::builder::{ChainPartitionSpec, PropertySpec, VbmetaImageArgs, make_vbmeta_image};
use avbtool_rs::footer::{HashFooterArgs, add_hash_footer};
use avbtool_rs::info::DescriptorInfo;
use std::path::Path;

pub const PARTITION_BYTES: usize = 1024 * 1024;
pub const VBMETA_BYTES: usize = 65536;
pub const TOP_ROLLBACK_INDEX: u64 = 1_675_555_200;
pub const BOOT_DIGEST_BYTE: u8 = 4;

fn boot_image(ramdisk_bytes: u32, fill: u8, total: usize) -> Vec<u8> {
    let payload = 4096 + (ramdisk_bytes as usize).div_ceil(4096) * 4096;
    let mut data = vec![0u8; total.max(payload)];
    data[..8].copy_from_slice(b"ANDROID!");
    data[12..16].copy_from_slice(&ramdisk_bytes.to_le_bytes());
    data[20..24].copy_from_slice(&1584u32.to_le_bytes());
    data[40..44].copy_from_slice(&4u32.to_le_bytes());
    data[4096..4096 + ramdisk_bytes as usize].fill(fill);
    data
}

pub fn patched_init_boot() -> Vec<u8> {
    boot_image(70_000, 0x5a, PARTITION_BYTES)
}

pub fn write_readback_pair(directory: &Path) {
    write_readback_pair_with_boot_digest(directory, BOOT_DIGEST_BYTE);
}

pub fn write_readback_pair_with_boot_digest(directory: &Path, boot_digest_byte: u8) {
    let init_boot = directory.join("init_boot.img");
    std::fs::write(&init_boot, boot_image(40_000, 0x33, 0)).expect("write init_boot payload");
    add_hash_footer(
        &init_boot,
        &HashFooterArgs {
            partition_size: Some(PARTITION_BYTES as u64),
            dynamic_partition_size: false,
            partition_name: "init_boot".into(),
            hash_algorithm: "sha256".into(),
            salt: Some(vec![7; 32]),
            chain_partitions: Vec::new(),
            algorithm_name: "NONE".into(),
            key_spec: None,
            public_key_metadata: None,
            rollback_index: 0,
            flags: 0,
            rollback_index_location: 0,
            properties: vec![PropertySpec {
                key: "com.android.build.init_boot.os_version".into(),
                value: b"13".to_vec(),
            }],
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
    .expect("add init_boot hash footer");

    let vbmeta = directory.join("vbmeta.img");
    make_vbmeta_image(
        &vbmeta,
        &VbmetaImageArgs {
            algorithm_name: "SHA256_RSA4096".into(),
            key_spec: Some("testkey_rsa4096".into()),
            public_key_metadata: None,
            rollback_index: TOP_ROLLBACK_INDEX,
            flags: 0,
            rollback_index_location: 0,
            properties: vec![PropertySpec {
                key: "com.android.build.boot.os_version".into(),
                value: b"13".to_vec(),
            }],
            kernel_cmdlines: Vec::new(),
            extra_descriptors: vec![DescriptorInfo::Hash {
                image_size: 8192,
                hash_algorithm: "sha256".into(),
                partition_name: "boot".into(),
                salt: vec![9; 32],
                digest: vec![boot_digest_byte; 32],
                flags: 0,
            }],
            include_descriptors_from_images: vec![init_boot.clone()],
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
        .open(&vbmeta)
        .expect("open vbmeta");
    assert!(file.metadata().expect("vbmeta metadata").len() <= VBMETA_BYTES as u64);
    file.set_len(VBMETA_BYTES as u64).expect("pad vbmeta");
}
