//! Magisk root patching — extracts payload from the Magisk APK and patches the
//! selected root ramdisk via the magiskboot library (no shell-out).
//! APK layout: `lib/arm64-v8a/libmagisk{,64}.so` → `magisk`,
//! `libmagiskinit.so` → `magiskinit`, `libinit-ld.so` → `init-ld`,
//! `assets/stub.apk` → `stub.apk`.

use fs_err as fs;
use std::path::{Path, PathBuf};

use ltbox_core::i18n::tr;
use ltbox_core::{LtboxError, Result, tr_args};

use crate::boot;
use crate::root_pipeline::RootImageTarget;

const STUB_APK: &str = "stub.apk";

fn magisk_cpio_commands(has_stub: bool) -> Vec<&'static str> {
    let mut commands = vec![
        "add 0750 init magiskinit",
        "mkdir 0750 overlay.d",
        "mkdir 0750 overlay.d/sbin",
        "add 0644 overlay.d/sbin/magisk.xz magisk.xz",
    ];
    if has_stub {
        commands.push("add 0644 overlay.d/sbin/stub.xz stub.xz");
    }
    commands.extend([
        "add 0644 overlay.d/sbin/init-ld.xz init-ld.xz",
        "patch",
        "backup ramdisk.cpio.orig",
        "mkdir 000 .backup",
        "add 000 .backup/.magisk config",
    ]);
    commands
}

/// Map the three crypto getprops to the `encrypt_type` v2 uses to gate
/// `/data` eligibility. `"none"` = unencrypted, `"file"` = FBE,
/// `"block"` = full-disk, `"metadata"` = metadata crypto. Matches the
/// cascade in `_find_magisk_preinit_device_from_mountinfo` from
/// `bin/ltbox/patch/root.py`.
pub fn derive_encrypt_type(
    crypto_state: &str,
    crypto_type: &str,
    crypto_metadata_enabled: &str,
) -> &'static str {
    if crypto_state.trim() != "encrypted" {
        "none"
    } else if crypto_type.trim() == "block" {
        "block"
    } else if crypto_metadata_enabled.trim() == "true" {
        "metadata"
    } else {
        "file"
    }
}

/// Resolve Magisk's `PREINITDEVICE` partition name from `/proc/self/mountinfo`.
/// Returns the bare partition name (e.g. `"metadata"`), not a block path.
///
/// Priority: `/metadata` > `/persist` | `/mnt/vendor/persist` > `/klogdump` >
/// `/cache` > `/data`. Within a tier, ext4 beats f2fs. Read-only, non-`/`-root,
/// pseudo-fs, and device-mapper mounts are filtered out.
///
/// `encrypt_type` gates `/data` eligibility: `"none"` (plain) and `"file"`
/// (FBE) keep `/data` eligible; `"block"` / `"metadata"` devices cannot use
/// userdata for preinit because the partition is unavailable pre-unlock.
/// Mirrors `bin/ltbox/patch/root.py::_find_magisk_preinit_device_from_mountinfo`.
///
/// Also drops dynamic-major partitions (majors 240-254) unless the source
/// path carries `/by-name/` or contains `/vd` — prevents grabbing a
/// super-partition lvm mapping that disappears at next boot.
pub fn resolve_preinit_device(mountinfo: &str, encrypt_type: &str) -> Option<String> {
    const PRIO_METADATA: u32 = 5;
    const PRIO_PERSIST: u32 = 4;
    const PRIO_KLOGDUMP: u32 = 3;
    const PRIO_CACHE: u32 = 2;
    const PRIO_DATA: u32 = 1;

    const DYN_MAJOR_MIN: u32 = 240;
    const DYN_MAJOR_MAX: u32 = 254;

    let data_ok = encrypt_type == "none" || encrypt_type == "file";

    let priority_for_mount = |target: &str| -> Option<u32> {
        match target {
            "/metadata" => Some(PRIO_METADATA),
            "/persist" | "/mnt/vendor/persist" => Some(PRIO_PERSIST),
            "/klogdump" => Some(PRIO_KLOGDUMP),
            "/cache" => Some(PRIO_CACHE),
            "/data" if data_ok => Some(PRIO_DATA),
            _ => None,
        }
    };

    // (priority, fs_preference, partition_name); higher tuple wins under Ord (ext4=1, f2fs=0).
    let mut candidates: Vec<(u32, u32, String)> = Vec::new();

    for line in mountinfo.lines() {
        // mountinfo: `mount_id parent major:minor root mount_point options - fs_type source super_options`
        let (pre, post) = match line.split_once(" - ") {
            Some(p) => p,
            None => continue,
        };
        let pre_parts: Vec<&str> = pre.split_whitespace().collect();
        let post_parts: Vec<&str> = post.split_whitespace().collect();
        if pre_parts.len() < 6 || post_parts.len() < 3 {
            continue;
        }
        let device_major: u32 = pre_parts[2]
            .split_once(':')
            .and_then(|(maj, _)| maj.parse().ok())
            .unwrap_or(0);
        let root = pre_parts[3];
        let target = pre_parts[4];
        let mount_opts = pre_parts[5];
        let fs_type = post_parts[0];
        let source = post_parts[1];

        if root != "/" {
            continue;
        }
        if !source.starts_with('/') {
            continue;
        }
        if source.contains("/dm-") {
            continue;
        }
        if fs_type != "ext4" && fs_type != "f2fs" {
            continue;
        }
        if !mount_opts.split(',').any(|o| o == "rw") {
            continue;
        }
        // Accept only /dev/block/by-name/* or /dev/block/* sources.
        let parent_ok = {
            let trimmed = source.trim_end_matches(|c| c != '/').trim_end_matches('/');
            trimmed.ends_with("by-name") || trimmed.ends_with("block")
        };
        if !parent_ok {
            continue;
        }

        // Dynamic-major filter: super-partition lvm entries get majors in
        // 240..=254 and vanish on the next boot. Keep them only when the
        // source path signals a stable name (`/by-name/`) or a virtual-disk
        // mapping (`/vd`).
        if (DYN_MAJOR_MIN..=DYN_MAJOR_MAX).contains(&device_major)
            && !source.contains("/vd")
            && !source.contains("/by-name/")
        {
            continue;
        }

        let Some(prio) = priority_for_mount(target) else {
            continue;
        };
        let fs_pref = if fs_type == "ext4" { 1 } else { 0 };
        let name = source.rsplit('/').next().unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        candidates.push((prio, fs_pref, name));
    }

    candidates.sort_by(|a, b| b.cmp(a));
    candidates.into_iter().next().map(|(_, _, name)| name)
}

/// Extract Magisk payload from `apk_path` into `staging_dir`, overwriting
/// existing files.
/// Accepts both `libmagisk.so` (newer) and `libmagisk64.so` (older) for `magisk`.
pub fn extract_apk_payload(apk_path: &Path, staging_dir: &Path) -> Result<()> {
    fs::create_dir_all(staging_dir)?;

    let file = fs::File::open(apk_path)
        .map_err(|e| LtboxError::Patch(format!("open APK {}: {e}", apk_path.display())))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| LtboxError::Patch(format!("APK read: {e}")))?;

    let mapping: &[(&[&str], &str, bool)] = &[
        (
            &["lib/arm64-v8a/libmagisk.so", "lib/arm64-v8a/libmagisk64.so"],
            "magisk",
            true,
        ),
        (&["lib/arm64-v8a/libmagiskinit.so"], "magiskinit", true),
        (&["lib/arm64-v8a/libinit-ld.so"], "init-ld", true),
        (&["assets/stub.apk"], STUB_APK, false),
    ];

    for (candidates, dst_name, required) in mapping {
        let mut staged = false;
        for entry_name in candidates.iter() {
            let mut entry = match zip.by_name(entry_name) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let dst_path = staging_dir.join(dst_name);
            crate::zip_util::copy_capped(
                &mut entry,
                &dst_path,
                crate::zip_util::MAX_ENTRY_BYTES,
                entry_name,
            )?;
            staged = true;
            break;
        }
        if !staged && *required {
            return Err(LtboxError::Patch(format!(
                "APK missing entry for {dst_name} (checked {candidates:?})"
            )));
        }
        if !staged {
            remove_stale_payload(&staging_dir.join(dst_name))?;
        }
    }
    Ok(())
}

fn remove_stale_payload(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(LtboxError::Patch(format!(
            "remove the old payload {}: {error}",
            path.display()
        ))),
    }
}

/// Patch the selected root image with Magisk. `work_dir` must contain the image
/// plus the four payload files from [`extract_apk_payload`]. Writes
/// `work_dir/new-boot.img`; caller handles resign + flash.
///
/// `preinit_device` → Magisk `PREINITDEVICE` config. Empty string omits
/// the entry and lets Magisk resolve at boot (can fail on some devices).
pub fn patch_root_image(
    work_dir: &Path,
    target: RootImageTarget,
    preinit_device: &str,
    log: &mut Vec<String>,
) -> Result<PathBuf> {
    let img_name = target.filename();
    let img_path = work_dir.join(img_name);
    if !img_path.exists() {
        return Err(LtboxError::Patch(format!(
            "{img_name} not found in {}",
            work_dir.display()
        )));
    }

    ltbox_core::live!(
        log,
        "[Magisk] {}",
        tr_args!("log_root_unpack_image", image = img_name)
    );
    boot::unpack(&img_path, work_dir)?;

    let ramdisk_name = boot::root_ramdisk_name(work_dir)?;
    let ramdisk = work_dir.join(ramdisk_name);

    // magiskboot cpio test: 0=stock, 1=already-patched, 2=unsupported.
    ltbox_core::live!(log, "[Magisk] {}", tr("log_magisk_cpio_test"));
    let status = boot::cpio(work_dir, ramdisk_name, &["test"])?;
    match status {
        0 => {}
        1 => {
            return Err(LtboxError::Patch(format!(
                "{img_name} is already Magisk-patched — flash stock first"
            )));
        }
        other => {
            return Err(LtboxError::Patch(format!(
                "Unsupported boot image layout (cpio test = {other})"
            )));
        }
    }

    // SHA-1 of stock boot for Magisk config.
    let sha1 = boot::sha1(&img_path)?;
    ltbox_core::live!(
        log,
        "[Magisk] {}",
        tr_args!("log_magisk_stock_sha1", sha1 = sha1)
    );

    // Back up stock ramdisk so Magisk can restore on unroot.
    fs::copy(&ramdisk, work_dir.join("ramdisk.cpio.orig"))?;

    ltbox_core::live!(log, "[Magisk] {}", tr("log_magisk_compressing_payload"));
    let has_stub = work_dir.join(STUB_APK).is_file();
    boot::compress(work_dir, "xz", "magisk", "magisk.xz")?;
    if has_stub {
        boot::compress(work_dir, "xz", STUB_APK, "stub.xz")?;
    }
    boot::compress(work_dir, "xz", "init-ld", "init-ld.xz")?;

    // Config file baked into .backup/.magisk.
    let mut config = String::new();
    config.push_str("KEEPVERITY=true\n");
    config.push_str("KEEPFORCEENCRYPT=true\n");
    config.push_str("RECOVERYMODE=false\n");
    config.push_str("VENDORBOOT=false\n");
    if !preinit_device.is_empty() {
        config.push_str(&format!("PREINITDEVICE={preinit_device}\n"));
    }
    config.push_str(&format!("SHA1={sha1}\n"));
    fs::write(work_dir.join("config"), &config)?;

    // Patch ramdisk in one cpio pass. `KEEPVERITY` / `KEEPFORCEENCRYPT`
    // must be set or magiskboot's patcher strips dm-verity and
    // forceencrypt flags from fstab — bricks devices that expect AVB
    // + metadata encryption. v2 `bin/ltbox/patch/root.py` sets the
    // same env via `_get_tool_env()` before invoking magiskboot.
    ltbox_core::live!(log, "[Magisk] {}", tr("log_magisk_cpio_patch"));
    let cpio_cmds = magisk_cpio_commands(has_stub);
    boot::cpio_with_env(
        work_dir,
        ramdisk_name,
        &cpio_cmds,
        &[
            ("KEEPVERITY", "true"),
            ("KEEPFORCEENCRYPT", "true"),
            ("PATCHVBMETAFLAG", "false"),
        ],
    )?;

    // Clean up staging — don't leave plaintext payload next to the repacked image.
    for name in [
        "ramdisk.cpio.orig",
        "config",
        "magisk.xz",
        "stub.xz",
        "init-ld.xz",
    ] {
        let _ = fs::remove_file(work_dir.join(name));
    }

    ltbox_core::live!(
        log,
        "[Magisk] {}",
        tr_args!("log_root_repack_image", image = img_name)
    );
    boot::repack(img_name, work_dir)?;

    let new_boot = work_dir.join("new-boot.img");
    if !new_boot.exists() {
        return Err(LtboxError::Patch(
            "magiskboot repack produced no new-boot.img".into(),
        ));
    }
    ltbox_core::live!(log, "[Magisk] {}", tr("log_patch_complete"));
    Ok(new_boot)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal mountinfo line template; only parser-relevant fields matter.
    // `major` matters for the dynamic-major filter; default 259 is ufs/sda.
    fn line(target: &str, opts: &str, fs: &str, source: &str) -> String {
        format!("1 1 259:1 / {target} {opts} shared:1 - {fs} {source} rw",)
    }

    fn line_major(major: u32, target: &str, opts: &str, fs: &str, source: &str) -> String {
        format!("1 1 {major}:1 / {target} {opts} shared:1 - {fs} {source} rw",)
    }

    #[test]
    fn picks_metadata_over_persist() {
        let info = format!(
            "{}\n{}",
            line(
                "/persist",
                "rw,seclabel",
                "ext4",
                "/dev/block/by-name/persist"
            ),
            line(
                "/metadata",
                "rw,seclabel",
                "ext4",
                "/dev/block/by-name/metadata"
            ),
        );
        assert_eq!(
            resolve_preinit_device(&info, "file").as_deref(),
            Some("metadata")
        );
    }

    #[test]
    fn prefers_ext4_over_f2fs_within_same_tier() {
        let info = format!(
            "{}\n{}",
            line(
                "/persist",
                "rw,seclabel",
                "f2fs",
                "/dev/block/by-name/persist"
            ),
            line(
                "/mnt/vendor/persist",
                "rw,seclabel",
                "ext4",
                "/dev/block/by-name/vendor_persist"
            ),
        );
        assert_eq!(
            resolve_preinit_device(&info, "file").as_deref(),
            Some("vendor_persist")
        );
    }

    #[test]
    fn drops_readonly_mounts() {
        let info = line(
            "/metadata",
            "ro,seclabel",
            "ext4",
            "/dev/block/by-name/metadata",
        );
        assert_eq!(resolve_preinit_device(&info, "file"), None);
    }

    #[test]
    fn drops_device_mapper_sources() {
        let info = line("/metadata", "rw", "ext4", "/dev/block/dm-5");
        assert_eq!(resolve_preinit_device(&info, "file"), None);
    }

    #[test]
    fn drops_tmpfs_mounts() {
        let info = "1 1 0:1 / /metadata rw shared:1 - tmpfs tmpfs rw".to_string();
        assert_eq!(resolve_preinit_device(&info, "file"), None);
    }

    #[test]
    fn empty_returns_none() {
        assert_eq!(resolve_preinit_device("", "file"), None);
    }

    #[test]
    fn data_eligible_when_unencrypted_or_fbe() {
        let info = line(
            "/data",
            "rw,seclabel",
            "f2fs",
            "/dev/block/by-name/userdata",
        );
        assert_eq!(
            resolve_preinit_device(&info, "none").as_deref(),
            Some("userdata")
        );
        assert_eq!(
            resolve_preinit_device(&info, "file").as_deref(),
            Some("userdata")
        );
    }

    #[test]
    fn data_dropped_on_block_or_metadata_crypto() {
        let info = line(
            "/data",
            "rw,seclabel",
            "f2fs",
            "/dev/block/by-name/userdata",
        );
        assert_eq!(resolve_preinit_device(&info, "block"), None);
        assert_eq!(resolve_preinit_device(&info, "metadata"), None);
    }

    #[test]
    fn drops_dynamic_major_without_stable_hint() {
        // major 253 (dm-crypt / super lvm) + no `/vd` or `/by-name/` in path → drop.
        let info = line_major(253, "/metadata", "rw,seclabel", "ext4", "/dev/block/dm-0");
        assert_eq!(resolve_preinit_device(&info, "file"), None);
    }

    #[test]
    fn keeps_dynamic_major_with_by_name() {
        let info = line_major(
            253,
            "/metadata",
            "rw,seclabel",
            "ext4",
            "/dev/block/by-name/metadata",
        );
        assert_eq!(
            resolve_preinit_device(&info, "file").as_deref(),
            Some("metadata")
        );
    }

    #[test]
    fn stub_entry_is_added_only_when_the_package_has_a_stub() {
        let stub = "add 0644 overlay.d/sbin/stub.xz stub.xz";
        assert!(magisk_cpio_commands(true).contains(&stub));
        let without = magisk_cpio_commands(false);
        assert!(!without.contains(&stub));
        assert_eq!(without.first(), Some(&"add 0750 init magiskinit"));
        assert_eq!(without.last(), Some(&"add 000 .backup/.magisk config"));
    }

    fn package_without_a_stub(directory: &Path) -> PathBuf {
        use std::io::Write;
        let apk = directory.join("magisk.apk");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&apk).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for name in [
            "lib/arm64-v8a/libmagisk.so",
            "lib/arm64-v8a/libmagiskinit.so",
            "lib/arm64-v8a/libinit-ld.so",
        ] {
            writer.start_file(name, options).unwrap();
            writer.write_all(name.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
        apk
    }

    fn new_staging(directory: &Path) -> PathBuf {
        let staging = directory.join("staging");
        std::fs::create_dir(&staging).unwrap();
        staging
    }

    #[test]
    fn package_without_a_stub_extracts_and_clears_an_old_stub() {
        let directory = tempfile::tempdir().unwrap();
        let apk = package_without_a_stub(directory.path());
        let staging = new_staging(directory.path());
        std::fs::write(staging.join(STUB_APK), b"old stub").unwrap();
        extract_apk_payload(&apk, &staging).unwrap();
        assert!(staging.join("magiskinit").is_file());
        assert!(!staging.join(STUB_APK).exists());
    }

    #[test]
    fn doc_1053_package_without_a_stub_and_no_old_stub_extracts() {
        let directory = tempfile::tempdir().unwrap();
        let apk = package_without_a_stub(directory.path());
        let staging = new_staging(directory.path());
        extract_apk_payload(&apk, &staging).unwrap();
        assert!(staging.join("magiskinit").is_file());
        assert!(!staging.join(STUB_APK).exists());
    }

    #[test]
    fn doc_1053_an_old_stub_that_cannot_be_removed_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let apk = package_without_a_stub(directory.path());
        let staging = new_staging(directory.path());
        std::fs::create_dir(staging.join(STUB_APK)).unwrap();
        let refused = extract_apk_payload(&apk, &staging);
        assert!(matches!(refused, Err(LtboxError::Patch(_))), "{refused:?}");
        assert!(staging.join(STUB_APK).exists());
    }

    #[cfg(windows)]
    #[test]
    fn doc_1053_an_old_stub_held_open_without_delete_sharing_is_an_error() {
        use std::os::windows::fs::OpenOptionsExt;
        let directory = tempfile::tempdir().unwrap();
        let apk = package_without_a_stub(directory.path());
        let staging = new_staging(directory.path());
        let old_stub = staging.join(STUB_APK);
        std::fs::write(&old_stub, b"old stub").unwrap();
        let read_and_write_sharing = 3;
        let handle = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(read_and_write_sharing)
            .open(&old_stub)
            .unwrap();
        let refused = extract_apk_payload(&apk, &staging);
        drop(handle);
        assert!(matches!(refused, Err(LtboxError::Patch(_))), "{refused:?}");
        assert_eq!(std::fs::read(&old_stub).unwrap(), b"old stub");
    }

    #[test]
    fn encrypt_type_cascade() {
        assert_eq!(derive_encrypt_type("unencrypted", "", ""), "none");
        assert_eq!(derive_encrypt_type("encrypted", "block", ""), "block");
        assert_eq!(derive_encrypt_type("encrypted", "file", "true"), "metadata");
        assert_eq!(derive_encrypt_type("encrypted", "file", "false"), "file");
        assert_eq!(derive_encrypt_type("encrypted", "file", ""), "file");
    }
}
