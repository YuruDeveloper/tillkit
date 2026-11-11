use crate::error::{Front2Error, Result};
use crate::gpt::layout::SECTOR_BYTES;

pub const MISC_CLEARED_BYTES: usize = 2048;
pub const MISC_COMMAND: &[u8] = b"boot-recovery";
pub const MISC_RECOVERY_OFFSET: usize = 64;
pub const MISC_RECOVERY_MESSAGE: &[u8] = b"recovery\n--wipe_data\n";

pub fn misc_wipe_sector(original_first_sector: &[u8]) -> Result<Vec<u8>> {
    if original_first_sector.len() != SECTOR_BYTES {
        return Err(Front2Error::Image(format!(
            "the first misc sector is {} bytes, {SECTOR_BYTES} bytes are required",
            original_first_sector.len()
        )));
    }
    let mut sector = original_first_sector.to_vec();
    sector[..MISC_CLEARED_BYTES].fill(0);
    sector[..MISC_COMMAND.len()].copy_from_slice(MISC_COMMAND);
    sector[MISC_RECOVERY_OFFSET..MISC_RECOVERY_OFFSET + MISC_RECOVERY_MESSAGE.len()]
        .copy_from_slice(MISC_RECOVERY_MESSAGE);
    if sector[MISC_CLEARED_BYTES..] != original_first_sector[MISC_CLEARED_BYTES..] {
        return Err(Front2Error::Verification(format!(
            "the misc sector changed after offset {MISC_CLEARED_BYTES}"
        )));
    }
    Ok(sector)
}
