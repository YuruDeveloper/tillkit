#![allow(dead_code, unused_imports)]

mod synthetic_gpt;

pub use synthetic_gpt::{
    ENTRY_BYTES, ENTRY_COUNT, FIRST_PARTITION_LBA, LAST_LBA, PARTITION_SECTORS, SECTOR,
    SyntheticGpt, seal,
};
