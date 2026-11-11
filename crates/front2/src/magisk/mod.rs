mod init_boot_patch;
mod init_boot_state;
mod magisk_candidate;
mod magisk_package;
mod package_record;
mod patch_inputs;
mod preinit_device;

pub use init_boot_patch::patch_init_boot;
pub use init_boot_state::InitBootState;
pub use ltbox_patch::boot::{dispatch_magiskboot_helper, register_magiskboot_host};
pub use magisk_candidate::build_magisk_candidate;
pub use magisk_package::{LOCAL_VERSION, MAGISK_REPOSITORY, MagiskPackage, select_release_apk};
pub use package_record::RECORD_FILE;
pub use patch_inputs::PatchInputs;
pub use preinit_device::read_preinit_device;
