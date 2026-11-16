use crate::error::Result;
use crate::gpt::BootSlot;

/// Choose the slot that the install reads the stock images and the vbmeta from.
///
/// The source is the boot slot for each target slot. The boot slot holds the
/// build that the device starts. On a device where the two slots use the same
/// extents in the super partition, the logical partitions hold that build too.
/// An install that changes the boot slot copies the physical images of the boot
/// slot to the target slot.
pub fn install_source_slot(boot_slot: BootSlot, _target_slot: BootSlot) -> Result<BootSlot> {
    Ok(boot_slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_slot_b_install_reads_the_boot_slot() {
        assert_eq!(
            install_source_slot(BootSlot::A, BootSlot::B).expect("a slot A boot to slot B"),
            BootSlot::A
        );
        assert_eq!(
            install_source_slot(BootSlot::B, BootSlot::B).expect("a slot B boot to slot B"),
            BootSlot::B
        );
    }

    #[test]
    fn a_slot_b_boot_to_the_target_slot_a_reads_slot_b() {
        assert_eq!(
            install_source_slot(BootSlot::B, BootSlot::A).expect("a slot B boot to slot A"),
            BootSlot::B
        );
    }

    #[test]
    fn a_slot_a_boot_to_the_target_slot_a_reads_slot_a() {
        assert_eq!(
            install_source_slot(BootSlot::A, BootSlot::A).expect("a slot A boot to slot A"),
            BootSlot::A
        );
    }
}
