const POLYNOMIAL: u32 = 0xEDB8_8320;

const TABLE: [u32; 256] = build_table();

const fn table_value(index: u32) -> u32 {
    let mut value = index;
    let mut round = 0;
    while round < 8 {
        value = if value & 1 == 1 {
            (value >> 1) ^ POLYNOMIAL
        } else {
            value >> 1
        };
        round += 1;
    }
    value
}

const fn build_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        table[i] = table_value(i as u32);
        i += 1;
    }
    table
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut value = u32::MAX;
    for byte in data {
        let index = ((value ^ u32::from(*byte)) & 0xFF) as usize;
        value = (value >> 8) ^ TABLE[index];
    }
    !value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_of_known_inputs() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b"a"), 0xE8B7_BE43);
        assert_eq!(
            crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414F_A339
        );
        assert_eq!(crc32(&[0u8; 32]), 0x190A_55AD);
    }
}
