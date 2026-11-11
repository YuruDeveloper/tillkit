use crate::error::{Front2Error, Result};

const BOOT_MAGIC: &[u8; 8] = b"ANDROID!";
const KERNEL_SIZE_OFFSET: usize = 8;
const RAMDISK_SIZE_OFFSET: usize = 12;
const HEADER_SIZE_OFFSET: usize = 20;
const HEADER_VERSION_OFFSET: usize = 40;
const SIGNATURE_SIZE_OFFSET: usize = 1580;
const BOOT_V4_HEADER_SIZE: u32 = 1584;
const HEADER_PAGE_BYTES: usize = 4096;
const IMAGE_ALIGNMENT: u64 = HEADER_PAGE_BYTES as u64;

pub fn payload_bytes(original: &[u8], patched: &[u8], maximum: u64) -> Result<u64> {
    let failure = |reason: String| Front2Error::Image(format!("patched init_boot: {reason}"));
    if patched.len() != original.len() {
        return Err(failure(format!(
            "is {} bytes, the partition image is {} bytes",
            patched.len(),
            original.len()
        )));
    }
    if patched.len() < HEADER_PAGE_BYTES || &patched[..8] != BOOT_MAGIC {
        return Err(failure("has no Android boot image header".into()));
    }
    let field = |offset: usize| {
        u32::from_le_bytes(
            patched[offset..offset + 4]
                .try_into()
                .expect("four header bytes"),
        )
    };
    if field(HEADER_VERSION_OFFSET) != 4 || field(HEADER_SIZE_OFFSET) != BOOT_V4_HEADER_SIZE {
        return Err(failure("is not a version 4 boot image".into()));
    }
    if field(KERNEL_SIZE_OFFSET) != 0 {
        return Err(failure("carries a kernel, init_boot must not".into()));
    }
    if field(SIGNATURE_SIZE_OFFSET) != 0 {
        return Err(failure("carries a boot signature block".into()));
    }
    let ramdisk_field = RAMDISK_SIZE_OFFSET..RAMDISK_SIZE_OFFSET + 4;
    let header_differs = (0..HEADER_PAGE_BYTES)
        .any(|index| !ramdisk_field.contains(&index) && original[index] != patched[index]);
    if header_differs {
        return Err(failure(
            "changes header fields other than the ramdisk size".into(),
        ));
    }
    let ramdisk_bytes = u64::from(field(RAMDISK_SIZE_OFFSET));
    let payload =
        HEADER_PAGE_BYTES as u64 + ramdisk_bytes.div_ceil(IMAGE_ALIGNMENT) * IMAGE_ALIGNMENT;
    if ramdisk_bytes == 0 || payload > maximum || payload > patched.len() as u64 {
        return Err(failure(format!(
            "payload of {payload} bytes does not fit the limit of {maximum} bytes"
        )));
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(ramdisk_bytes: u32) -> Vec<u8> {
        let mut data = vec![0u8; 8 * 1024 * 1024];
        data[..8].copy_from_slice(BOOT_MAGIC);
        data[RAMDISK_SIZE_OFFSET..RAMDISK_SIZE_OFFSET + 4]
            .copy_from_slice(&ramdisk_bytes.to_le_bytes());
        data[HEADER_SIZE_OFFSET..HEADER_SIZE_OFFSET + 4]
            .copy_from_slice(&BOOT_V4_HEADER_SIZE.to_le_bytes());
        data[HEADER_VERSION_OFFSET..HEADER_VERSION_OFFSET + 4].copy_from_slice(&4u32.to_le_bytes());
        data
    }

    const MAXIMUM: u64 = 8_318_976;

    #[test]
    fn payload_is_header_page_plus_padded_ramdisk() {
        let original = image(1_000_000);
        assert_eq!(
            payload_bytes(&original, &image(1_234_567), MAXIMUM).expect("payload"),
            4096 + 302 * 4096
        );
        assert_eq!(
            payload_bytes(&original, &image(4096), MAXIMUM).expect("payload"),
            8192
        );
    }

    #[test]
    fn other_header_changes_and_bad_shapes_are_rejected() {
        let original = image(1_000_000);
        let mut kernel = image(1_000_000);
        kernel[KERNEL_SIZE_OFFSET] = 1;
        let mut version = image(1_000_000);
        version[HEADER_VERSION_OFFSET] = 3;
        let mut signature = image(1_000_000);
        signature[SIGNATURE_SIZE_OFFSET] = 1;
        let mut header_field = image(1_000_000);
        header_field[64] = 1;
        let mut magic = image(1_000_000);
        magic[0] = b'X';
        for patched in [
            kernel,
            version,
            signature,
            header_field,
            magic,
            image(0),
            image(8_400_000),
            vec![0u8; 16],
        ] {
            assert!(payload_bytes(&original, &patched, MAXIMUM).is_err());
        }
    }
}
