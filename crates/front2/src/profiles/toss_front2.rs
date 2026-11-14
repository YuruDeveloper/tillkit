use crate::device_profile::DeviceProfile;

pub const ID: &str = "toss-front2";
pub const VENDOR_BUILD_FINGERPRINT: &str =
    "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/475:user/dev-keys";
pub const BUILD_FINGERPRINT_PREFIX: &str = "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/";
pub const BUILD_FINGERPRINT_SUFFIX: &str = ":user/dev-keys";
pub const SUPPORTED_SLOT_SUFFIX: &str = "_b";
pub const TOP_VBMETA_ALGORITHM: &str = "SHA256_RSA4096";
pub const TOP_VBMETA_KEY_SPEC: &str = "testkey_rsa4096";
pub const CHAINED_VBMETA_BASE: &str = "vbmeta_system";
pub const CHAINED_VBMETA_ALGORITHM: &str = "SHA256_RSA2048";
pub const CHAINED_VBMETA_KEY_SPEC: &str = "testkey_rsa2048";
pub const CHAINED_ROLLBACK_INDEX_LOCATION: u32 = 2;
pub const GSI_VBMETA_RELEASE_STRING: &str = "avbtool 1.3.0";
pub const GSI_SYSTEM_PARTITION: &str = "system";
pub const RETAINED_PRODUCT_PARTITION: &str = "product";
pub const RETAINED_PRODUCT_PROPERTY_PREFIX: &str = "com.android.build.product.";
pub const PHYSICAL_BOOT_PARTITIONS: [&str; 5] =
    ["boot", "dtbo", "init_boot", "recovery", "vendor_boot"];
pub const BOOT_FINGERPRINT_PROPERTY: &str = "com.android.build.boot.fingerprint";

pub static PROFILE: DeviceProfile = DeviceProfile {
    id: ID,
    name: "Toss Front 2",
    identity: &[
        ("ro.product.vendor.manufacturer", "TOSSPLACE"),
        ("ro.product.vendor.device", "toss_front2"),
        ("ro.vendor.build.fingerprint", VENDOR_BUILD_FINGERPRINT),
        ("ro.soc.model", "QCM4325"),
        ("ro.board.platform", "bengal"),
    ],
    slot_suffix: SUPPORTED_SLOT_SUFFIX,
    device_state: "locked",
    verified_boot_state: "green",
    storage_block_bytes: 4096,
    top_vbmeta_algorithm: TOP_VBMETA_ALGORITHM,
    top_vbmeta_key_spec: TOP_VBMETA_KEY_SPEC,
    remote_stage_parent: "/data/local/tmp",
    supports_gsi: true,
};
