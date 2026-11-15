use crate::gpt::BootSlot;

pub const RETAINED_BASES: [&str; 5] = ["odm", "product", "system_dlkm", "vendor", "vendor_dlkm"];
const SYSTEM_BASE: &str = "system";
const SYSTEM_EXT_BASE: &str = "system_ext";
const DYNAMIC_GROUP_BASE: &str = "qti_dynamic_partitions";

#[derive(Debug, Clone)]
pub struct SuperSlot {
    target: BootSlot,
    target_index: u32,
    opposite_index: u32,
    system: String,
    system_ext: String,
    retained: [String; 5],
    dynamic_group: String,
}

impl SuperSlot {
    pub fn new(target: BootSlot) -> Self {
        let (target_index, opposite_index) = match target {
            BootSlot::A => (0, 1),
            BootSlot::B => (1, 0),
        };
        let suffix = target.suffix();
        Self {
            target,
            target_index,
            opposite_index,
            system: format!("{SYSTEM_BASE}{suffix}"),
            system_ext: format!("{SYSTEM_EXT_BASE}{suffix}"),
            retained: RETAINED_BASES.map(|base| format!("{base}{suffix}")),
            dynamic_group: format!("{DYNAMIC_GROUP_BASE}{suffix}"),
        }
    }

    pub fn target(&self) -> BootSlot {
        self.target
    }

    pub fn target_index(&self) -> u32 {
        self.target_index
    }

    pub fn opposite_index(&self) -> u32 {
        self.opposite_index
    }

    pub fn suffix(&self) -> &'static str {
        self.target.suffix()
    }

    pub fn system(&self) -> &str {
        &self.system
    }

    pub fn system_ext(&self) -> &str {
        &self.system_ext
    }

    pub fn retained(&self) -> &[String; 5] {
        &self.retained
    }

    pub fn dynamic_group(&self) -> &str {
        &self.dynamic_group
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::super_image::{
        DYNAMIC_GROUP, ORIGINAL_SLOT, RETAINED_PARTITIONS, SYSTEM_EXT_PARTITION, SYSTEM_PARTITION,
        TARGET_SLOT,
    };

    #[test]
    fn the_slot_b_layout_matches_the_fixed_slot_b_constants() {
        let slot = SuperSlot::new(BootSlot::B);
        assert_eq!(slot.target_index(), TARGET_SLOT);
        assert_eq!(slot.opposite_index(), ORIGINAL_SLOT);
        assert_eq!(slot.suffix(), "_b");
        assert_eq!(slot.system(), SYSTEM_PARTITION);
        assert_eq!(slot.system_ext(), SYSTEM_EXT_PARTITION);
        assert_eq!(slot.retained(), &RETAINED_PARTITIONS.map(String::from));
        assert_eq!(slot.dynamic_group(), DYNAMIC_GROUP);
    }

    #[test]
    fn the_slot_a_layout_reads_the_opposite_index_and_the_slot_a_names() {
        let slot = SuperSlot::new(BootSlot::A);
        assert_eq!(slot.target_index(), ORIGINAL_SLOT);
        assert_eq!(slot.opposite_index(), TARGET_SLOT);
        assert_eq!(slot.suffix(), "_a");
        assert_eq!(slot.system(), "system_a");
        assert_eq!(slot.system_ext(), "system_ext_a");
        assert_eq!(
            slot.retained(),
            &[
                "odm_a",
                "product_a",
                "system_dlkm_a",
                "vendor_a",
                "vendor_dlkm_a"
            ]
            .map(String::from)
        );
        assert_eq!(slot.dynamic_group(), "qti_dynamic_partitions_a");
    }
}
