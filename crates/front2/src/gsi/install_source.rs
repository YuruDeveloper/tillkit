use crate::error::{Front2Error, Result};
use crate::gpt::BootSlot;

/// Choose the slot that the install reads the stock images and the vbmeta from.
///
/// For a target slot B install the source is the boot slot. The switch install
/// from a slot A boot reads slot A, and the first device in-place install from a
/// slot B boot reads slot B. For the kk device the boot slot B uses a vendor key
/// that the tool cannot reproduce, so the in-place install to the test key slot A
/// reads slot A. The install from a slot A boot to the target slot A is not
/// supported.
pub fn install_source_slot(boot_slot: BootSlot, target_slot: BootSlot) -> Result<BootSlot> {
    match (boot_slot, target_slot) {
        (_, BootSlot::B) => Ok(boot_slot),
        (BootSlot::B, BootSlot::A) => Ok(BootSlot::A),
        (BootSlot::A, BootSlot::A) => Err(Front2Error::Unsupported(
            "the install from a slot A boot to the target slot A is not supported".into(),
        )),
    }
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
    fn a_slot_b_boot_to_the_target_slot_a_reads_slot_a() {
        assert_eq!(
            install_source_slot(BootSlot::B, BootSlot::A).expect("a slot B boot to slot A"),
            BootSlot::A
        );
    }

    #[test]
    fn a_slot_a_boot_to_the_target_slot_a_is_not_supported() {
        let error = install_source_slot(BootSlot::A, BootSlot::A)
            .expect_err("a slot A boot to slot A is not supported");
        assert!(matches!(error, Front2Error::Unsupported(_)), "{error}");
    }
}
