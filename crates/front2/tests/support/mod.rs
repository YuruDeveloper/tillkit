#![allow(dead_code, unused_imports)]

mod fake_device;
mod fault;
mod fixture;

pub use fake_device::FakeDevice;
pub use fault::Fault;
pub use fixture::{
    BOOT_DIGEST_BYTE, PARTITION_BYTES, TOP_ROLLBACK_INDEX, patched_init_boot, write_readback_pair,
    write_readback_pair_from, write_readback_pair_with_boot_digest,
};
