#![allow(dead_code, unused_imports)]

mod kernel_device;
mod kernel_fixture;
mod synthetic_kernel;

pub use kernel_device::KernelDevice;
pub use kernel_fixture::{
    BOOT_PARTITION_BYTES, CURRENT_KERNEL_BYTES, DTBO_DESCRIPTOR_BYTES, TOP_ROLLBACK_INDEX,
    current_kernel, front2_device, reference_descriptors, write_boot_partition, write_top_vbmeta,
};
pub use synthetic_kernel::{
    CURRENT_RELEASE, FORMAT_STRING, KMI, NEW_RELEASE, OS_VERSION, SyntheticKernel, banner,
    boot_image,
};
