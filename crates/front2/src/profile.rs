pub const INIT_BOOT_BASE: &str = "init_boot";
pub const VBMETA_BASE: &str = "vbmeta";

pub fn by_name_path(label: &str) -> String {
    format!("/dev/block/by-name/{label}")
}

pub fn slot_label(base: &str, slot_suffix: &str) -> String {
    format!("{base}{slot_suffix}")
}
