mod support;

use front2::magisk::{
    MagiskPackage, build_magisk_candidate, dispatch_magiskboot_helper, patch_init_boot,
    register_magiskboot_host,
};
use front2::{PairKind, PairRequest, PairStore};
use std::path::Path;
use support::{BOOT_DIGEST_BYTE, write_readback_pair_from};

const READBACK: &str = "2000-readback";
const CANDIDATE: &str = "2001-magisk";
const PAGE_BYTES: usize = 4096;

fn main() {
    if let Some(code) = dispatch_magiskboot_helper() {
        std::process::exit(code);
    }
    register_magiskboot_host(std::env::current_exe().expect("test executable"))
        .expect("register the magiskboot host");
    let tests: [(&str, fn()); 4] = [
        (
            "stock_init_boot_becomes_a_signed_magisk_candidate",
            stock_init_boot_becomes_a_signed_magisk_candidate,
        ),
        (
            "an_already_patched_init_boot_is_refused",
            an_already_patched_init_boot_is_refused,
        ),
        (
            "a_package_without_the_magisk_payload_is_refused",
            a_package_without_the_magisk_payload_is_refused,
        ),
        (
            "a_changed_cached_package_is_not_used",
            a_changed_cached_package_is_not_used,
        ),
    ];
    for (name, test) in tests {
        test();
        println!("test {name} ... ok");
    }
}

fn request(id: &str) -> PairRequest<'_> {
    PairRequest {
        id,
        created_unix: 2000,
        note: "magisk test",
    }
}

fn pad(bytes: &mut Vec<u8>, alignment: usize) {
    bytes.resize(bytes.len().div_ceil(alignment) * alignment, 0);
}

fn cpio_entry(archive: &mut Vec<u8>, inode: u32, name: &str, mode: u32, data: &[u8]) {
    let name_bytes = name.len() as u32 + 1;
    let fields = [
        inode,
        mode,
        0,
        0,
        1,
        0,
        data.len() as u32,
        0,
        0,
        0,
        0,
        name_bytes,
        0,
    ];
    archive.extend_from_slice(b"070701");
    for field in fields {
        archive.extend_from_slice(format!("{field:08x}").as_bytes());
    }
    archive.extend_from_slice(name.as_bytes());
    archive.push(0);
    pad(archive, 4);
    archive.extend_from_slice(data);
    pad(archive, 4);
}

fn stock_ramdisk() -> Vec<u8> {
    let mut archive = Vec::new();
    cpio_entry(&mut archive, 1, "init", 0o100_750, b"stock init binary\n");
    cpio_entry(&mut archive, 2, "system", 0o040_755, b"");
    cpio_entry(&mut archive, 0, "TRAILER!!!", 0, b"");
    archive
}

fn stock_init_boot_payload() -> Vec<u8> {
    let ramdisk = stock_ramdisk();
    let mut image = vec![0u8; PAGE_BYTES];
    image[..8].copy_from_slice(b"ANDROID!");
    image[12..16].copy_from_slice(&(ramdisk.len() as u32).to_le_bytes());
    image[20..24].copy_from_slice(&1584u32.to_le_bytes());
    image[40..44].copy_from_slice(&4u32.to_le_bytes());
    image.extend_from_slice(&ramdisk);
    pad(&mut image, PAGE_BYTES);
    image
}

fn crc_step(crc: u32) -> u32 {
    (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg())
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in data {
        crc ^= u32::from(*byte);
        crc = (0..8).fold(crc, |value, _| crc_step(value));
    }
    !crc
}

fn zip_header(signature: u32, crc: u32, size: u32, name: &str) -> Vec<u8> {
    let mut header = signature.to_le_bytes().to_vec();
    if signature == 0x0201_4b50 {
        header.extend_from_slice(&20u16.to_le_bytes());
    }
    for half in [20u16, 0, 0, 0, 0x21] {
        header.extend_from_slice(&half.to_le_bytes());
    }
    for word in [crc, size, size] {
        header.extend_from_slice(&word.to_le_bytes());
    }
    header.extend_from_slice(&(name.len() as u16).to_le_bytes());
    header.extend_from_slice(&0u16.to_le_bytes());
    header
}

fn stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut archive = Vec::new();
    let mut directory = Vec::new();
    for (name, data) in entries {
        let offset = archive.len() as u32;
        let crc = crc32(data);
        archive.extend(zip_header(0x0403_4b50, crc, data.len() as u32, name));
        archive.extend_from_slice(name.as_bytes());
        archive.extend_from_slice(data);
        directory.extend(zip_header(0x0201_4b50, crc, data.len() as u32, name));
        directory.extend_from_slice(&[0u8; 6]);
        directory.extend_from_slice(&0u32.to_le_bytes());
        directory.extend_from_slice(&offset.to_le_bytes());
        directory.extend_from_slice(name.as_bytes());
    }
    let directory_offset = archive.len() as u32;
    archive.extend_from_slice(&directory);
    archive.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    archive.extend_from_slice(&[0u8; 4]);
    for count in [entries.len() as u16, entries.len() as u16] {
        archive.extend_from_slice(&count.to_le_bytes());
    }
    archive.extend_from_slice(&(directory.len() as u32).to_le_bytes());
    archive.extend_from_slice(&directory_offset.to_le_bytes());
    archive.extend_from_slice(&0u16.to_le_bytes());
    archive
}

fn synthetic_magisk_apk() -> Vec<u8> {
    stored_zip(&[
        ("lib/arm64-v8a/libmagisk.so", b"synthetic magisk daemon"),
        ("lib/arm64-v8a/libmagiskinit.so", b"synthetic magiskinit"),
        ("lib/arm64-v8a/libinit-ld.so", b"synthetic init-ld"),
        ("assets/stub.apk", b"synthetic stub"),
    ])
}

fn admitted_package(root: &Path) -> MagiskPackage {
    let source = root.join("Magisk-synthetic.apk");
    std::fs::write(&source, synthetic_magisk_apk()).expect("write the synthetic package");
    MagiskPackage::from_local_file(&source, &root.join("cache")).expect("admit the package")
}

fn store_with_readback(root: &Path) -> PairStore {
    let store = PairStore::new(root.join("pairs"));
    let directory = store.begin(READBACK).expect("begin the readback pair");
    write_readback_pair_from(&directory, &stock_init_boot_payload(), BOOT_DIGEST_BYTE);
    store
        .commit(
            READBACK,
            PairKind::DeviceReadback,
            2000,
            "_b",
            None,
            "stock",
        )
        .expect("commit the readback pair");
    store
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn stock_init_boot_becomes_a_signed_magisk_candidate() {
    let root = tempfile::tempdir().expect("tempdir");
    let store = store_with_readback(root.path());
    let package = admitted_package(root.path());
    let mut log = Vec::new();
    let candidate = build_magisk_candidate(
        &store,
        READBACK,
        &package,
        "metadata",
        &root.path().join("work"),
        request(CANDIDATE),
        &mut log,
    )
    .expect("build the Magisk candidate");
    assert_eq!(candidate.kind, PairKind::SignedCandidate);
    assert_eq!(candidate.parent.as_deref(), Some(READBACK));
    store
        .load_verified(CANDIDATE)
        .expect("the candidate verifies");

    let unpacked = root.path().join("unpacked");
    std::fs::create_dir(&unpacked).expect("unpack directory");
    let candidate_init_boot = store
        .directory(CANDIDATE)
        .expect("candidate directory")
        .join("init_boot.img");
    ltbox_patch::boot::unpack(&candidate_init_boot, &unpacked).expect("unpack the candidate");
    let ramdisk = std::fs::read(unpacked.join("ramdisk.cpio")).expect("candidate ramdisk");
    for needle in [
        &b"overlay.d/sbin/magisk.xz"[..],
        b".backup/.magisk",
        b"PREINITDEVICE=metadata",
        b"KEEPVERITY=true",
    ] {
        assert!(
            contains(&ramdisk, needle),
            "{}",
            String::from_utf8_lossy(needle)
        );
    }
}

fn an_already_patched_init_boot_is_refused() {
    let root = tempfile::tempdir().expect("tempdir");
    let store = store_with_readback(root.path());
    let package = admitted_package(root.path());
    let stock = store
        .directory(READBACK)
        .expect("readback directory")
        .join("init_boot.img");
    let mut log = Vec::new();
    let first = patch_init_boot(&stock, &package, "", &root.path().join("first"), &mut log)
        .expect("patch the stock image");
    let again = patch_init_boot(&first, &package, "", &root.path().join("second"), &mut log);
    assert!(again.is_err(), "a patched image must not be patched again");
}

fn a_package_without_the_magisk_payload_is_refused() {
    let root = tempfile::tempdir().expect("tempdir");
    let cache = root.path().join("cache");
    let source = root.path().join("other.apk");
    std::fs::write(&source, stored_zip(&[("classes.dex", b"dex")])).expect("write the package");
    assert!(MagiskPackage::from_local_file(&source, &cache).is_err());
    assert!(
        MagiskPackage::cached(&cache)
            .expect("read the cache")
            .is_none()
    );
    std::fs::write(&source, b"not a zip").expect("write the package");
    assert!(MagiskPackage::from_local_file(&source, &cache).is_err());
}

fn a_changed_cached_package_is_not_used() {
    let root = tempfile::tempdir().expect("tempdir");
    let package = admitted_package(root.path());
    let cache = root.path().join("cache");
    assert_eq!(
        MagiskPackage::cached(&cache).expect("read the cache"),
        Some(package.clone())
    );
    std::fs::write(&package.path, b"changed").expect("change the cached package");
    assert!(
        MagiskPackage::cached(&cache)
            .expect("read the cache")
            .is_none()
    );
    assert!(package.require_unchanged().is_err());
}
