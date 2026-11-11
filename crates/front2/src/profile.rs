pub const SOC_MODEL: &str = "QCM4325";
pub const BOARD_PLATFORM: &str = "bengal";
pub const VENDOR_MANUFACTURER: &str = "TOSSPLACE";
pub const VENDOR_DEVICE: &str = "toss_front2";
pub const VENDOR_BUILD_FINGERPRINT: &str =
    "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/475:user/dev-keys";
pub const SUPPORTED_SLOT_SUFFIX: &str = "_b";
pub const REQUIRED_DEVICE_STATE: &str = "locked";
pub const REQUIRED_VERIFIED_BOOT_STATE: &str = "green";
pub const BLOCK_SIZE: u64 = 4096;
pub const TOP_VBMETA_ALGORITHM: &str = "SHA256_RSA4096";
pub const TOP_VBMETA_KEY_SPEC: &str = "testkey_rsa4096";
pub const INIT_BOOT_BASE: &str = "init_boot";
pub const VBMETA_BASE: &str = "vbmeta";
pub const CHAINED_VBMETA_BASE: &str = "vbmeta_system";
pub const CHAINED_VBMETA_ALGORITHM: &str = "SHA256_RSA2048";
pub const CHAINED_VBMETA_KEY_SPEC: &str = "testkey_rsa2048";
pub const CHAINED_ROLLBACK_INDEX_LOCATION: u32 = 2;
pub const GSI_VBMETA_RELEASE_STRING: &str = "avbtool 1.3.0";
pub const GSI_SYSTEM_PARTITION: &str = "system";
pub const RETAINED_PRODUCT_PARTITION: &str = "product";
pub const RETAINED_PRODUCT_PROPERTY_PREFIX: &str = "com.android.build.product.";
pub const REMOTE_STAGE_PARENT: &str = "/data/local/tmp";

pub fn by_name_path(label: &str) -> String {
    format!("/dev/block/by-name/{label}")
}

pub fn slot_label(base: &str, slot_suffix: &str) -> String {
    format!("{base}{slot_suffix}")
}
