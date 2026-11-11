#![allow(dead_code, unused_imports)]

mod synthetic_partition;
mod synthetic_slot;
mod synthetic_super;

pub use synthetic_partition::SyntheticPartition;
pub use synthetic_slot::SyntheticSlot;
pub use synthetic_super::{
    ALIGNMENT_BYTES, FIRST_LOGICAL_SECTOR, GROUP_MAXIMUM_BYTES, MAXIMUM_SIZE, SUPER_BYTES,
    SyntheticSuper, gsi_image,
};
