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
    let tests: [(&str, fn()); 5] = [
        (
            "stock_init_boot_becomes_a_signed_magisk_candidate",
            stock_init_boot_becomes_a_signed_magisk_candidate,
        ),
        (
            "a_package_with_a_stub_adds_stub_xz",
            a_package_with_a_stub_adds_stub_xz,
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
    real_front2_image_matches_the_magisk_app();
}

const REAL_INPUTS: [&str; 3] = [
    "FRONT2_MAGISK_APK",
    "FRONT2_STOCK_INIT_BOOT",
    "FRONT2_MAGISK_APP_INIT_BOOT",
];

fn real_front2_image_matches_the_magisk_app() {
    let inputs: Vec<std::path::PathBuf> = REAL_INPUTS
        .iter()
        .filter_map(|name| std::env::var_os(name).map(std::path::PathBuf::from))
        .collect();
    if inputs.len() != REAL_INPUTS.len() {
        println!("test real_front2_image_matches_the_magisk_app ... skipped, set {REAL_INPUTS:?}");
        return;
    }
    let root = tempfile::tempdir().expect("tempdir");
    let reference = unpacked_entries(&inputs[2], &root.path().join("reference"));
    let preinit = config_value(&reference, "PREINITDEVICE");
    let package = MagiskPackage::from_local_file(&inputs[0], &root.path().join("cache"))
        .expect("admit the real package");
    let mut log = Vec::new();
    let patched = patch_init_boot(
        &inputs[1],
        &package,
        &preinit,
        &root.path().join("work"),
        &mut log,
    )
    .expect("patch the real image");
    let ours = unpacked_entries(&patched, &root.path().join("ours"));
    assert_eq!(
        ours.keys().collect::<Vec<_>>(),
        reference.keys().collect::<Vec<_>>()
    );
    let compare = root.path().join("compare");
    std::fs::create_dir(&compare).expect("compare directory");
    for (name, entry) in &reference {
        let our_entry = &ours[name];
        assert_eq!(our_entry.0, entry.0, "{name} mode");
        assert_eq!(
            content(&compare, name, &our_entry.1, "ours"),
            content(&compare, name, &entry.1, "reference"),
            "{name}"
        );
    }
    let our_header = header_page(&patched);
    let reference_header = header_page(&inputs[2]);
    assert_eq!(our_header, reference_header);
    real_candidate_verifies(&inputs[1], &package, &preinit, root.path());
    println!("test real_front2_image_matches_the_magisk_app ... ok");
}

fn real_candidate_verifies(stock: &Path, package: &MagiskPackage, preinit: &str, root: &Path) {
    let Some(vbmeta) = std::env::var_os("FRONT2_STOCK_VBMETA") else {
        println!("real candidate skipped, set FRONT2_STOCK_VBMETA");
        return;
    };
    let store = PairStore::new(root.join("pairs"));
    let directory = store.begin(READBACK).expect("begin the real readback pair");
    std::fs::copy(stock, directory.join("init_boot.img")).expect("copy init_boot");
    std::fs::copy(vbmeta, directory.join("vbmeta.img")).expect("copy vbmeta");
    store
        .commit(READBACK, PairKind::DeviceReadback, 2000, "_b", None, "real")
        .expect("commit the real readback pair");
    let mut log = Vec::new();
    build_magisk_candidate(
        &store,
        READBACK,
        package,
        preinit,
        &root.join("candidate-work"),
        request(CANDIDATE),
        &mut log,
    )
    .expect("build the real candidate");
    store
        .load_verified(CANDIDATE)
        .expect("the real candidate verifies");
}

fn content(directory: &Path, name: &str, bytes: &[u8], side: &str) -> Vec<u8> {
    if !name.ends_with(".xz") {
        return bytes.to_vec();
    }
    let compressed = format!("{side}.xz");
    let plain = format!("{side}.raw");
    std::fs::write(directory.join(&compressed), bytes).expect("write the compressed entry");
    ltbox_patch::boot::decompress(directory, &compressed, &plain).expect("decompress the entry");
    std::fs::read(directory.join(plain)).expect("read the decompressed entry")
}

fn header_page(image: &Path) -> Vec<u8> {
    let mut page = std::fs::read(image).expect("read the image")[..PAGE_BYTES].to_vec();
    page[12..16].fill(0);
    page
}

type CpioEntries = std::collections::BTreeMap<String, (u32, Vec<u8>)>;

fn unpacked_entries(image: &Path, directory: &Path) -> CpioEntries {
    std::fs::create_dir_all(directory).expect("unpack directory");
    ltbox_patch::boot::unpack(image, directory).expect("unpack");
    parse_newc(&std::fs::read(directory.join("ramdisk.cpio")).expect("ramdisk"))
}

fn hex_field(header: &[u8], index: usize) -> usize {
    let text = std::str::from_utf8(&header[6 + index * 8..14 + index * 8]).expect("hex field");
    usize::from_str_radix(text, 16).expect("hex number")
}

fn parse_newc(bytes: &[u8]) -> CpioEntries {
    let mut entries = CpioEntries::new();
    let mut offset = 0usize;
    while offset + 110 <= bytes.len() && &bytes[offset..offset + 6] == b"070701" {
        let header = &bytes[offset..offset + 110];
        let size = hex_field(header, 6);
        let name_end = offset + 110 + hex_field(header, 11);
        let name = String::from_utf8_lossy(&bytes[offset + 110..name_end - 1]).into_owned();
        let data_start = name_end.div_ceil(4) * 4;
        offset = (data_start + size).div_ceil(4) * 4;
        let mode = hex_field(header, 1) as u32;
        entries.insert(name, (mode, bytes[data_start..data_start + size].to_vec()));
    }
    entries.remove("TRAILER!!!");
    entries
}

fn config_value(entries: &CpioEntries, key: &str) -> String {
    let config = entries.get(".backup/.magisk").expect("Magisk config");
    String::from_utf8_lossy(&config.1)
        .lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix('=').map(str::to_owned))
        .unwrap_or_default()
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

fn synthetic_magisk_apk(with_stub: bool) -> Vec<u8> {
    let mut entries: Vec<(&str, &[u8])> = vec![
        ("lib/arm64-v8a/libmagisk.so", b"synthetic magisk daemon"),
        ("lib/arm64-v8a/libmagiskinit.so", b"synthetic magiskinit"),
        ("lib/arm64-v8a/libinit-ld.so", b"synthetic init-ld"),
    ];
    if with_stub {
        entries.push(("assets/stub.apk", b"synthetic stub"));
    }
    stored_zip(&entries)
}

fn admitted_package(root: &Path, with_stub: bool) -> MagiskPackage {
    let source = root.join("Magisk-synthetic.apk");
    std::fs::write(&source, synthetic_magisk_apk(with_stub)).expect("write the synthetic package");
    MagiskPackage::from_local_file(&source, &root.join("cache")).expect("admit the package")
}

fn a_package_with_a_stub_adds_stub_xz() {
    let root = tempfile::tempdir().expect("tempdir");
    let store = store_with_readback(root.path());
    let stock = store
        .directory(READBACK)
        .expect("readback directory")
        .join("init_boot.img");
    for with_stub in [true, false] {
        let package = admitted_package(root.path(), with_stub);
        let work = root.path().join(format!("work-{with_stub}"));
        let patched = patch_init_boot(&stock, &package, "", &work, &mut Vec::new())
            .expect("patch the stock image");
        let entries =
            unpacked_entries(&patched, &root.path().join(format!("unpacked-{with_stub}")));
        assert_eq!(
            entries.contains_key("overlay.d/sbin/stub.xz"),
            with_stub,
            "stub.xz with a stub in the package: {with_stub}"
        );
        assert!(entries.contains_key("overlay.d/sbin/magisk.xz"));
    }
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
    let package = admitted_package(root.path(), false);
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
    let package = admitted_package(root.path(), false);
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
    let package = admitted_package(root.path(), false);
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
