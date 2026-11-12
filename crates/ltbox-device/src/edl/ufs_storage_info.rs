use super::{EdlError, Result};

const TOTAL_BLOCKS_FIELD: &str = "UFS LU Total Blocks:";
const BLOCK_SIZE_FIELD: &str = "UFS Logical Block Size:";
const SUPPORTED_BLOCK_SIZE_EXPONENT: u64 = 0xc;
const SUPPORTED_BLOCK_BYTES: u64 = 4096;
const HEXADECIMAL_PREFIX: &str = "0x";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UfsStorageInfo {
    pub total_blocks: u64,
    pub block_bytes: u64,
}

impl UfsStorageInfo {
    pub fn parse(lines: &[String]) -> Result<Self> {
        let total_blocks = field_value(lines, TOTAL_BLOCKS_FIELD)?;
        if total_blocks == 0 {
            return Err(EdlError::StorageInfo(format!("{TOTAL_BLOCKS_FIELD} is 0")));
        }
        let block_size_exponent = field_value(lines, BLOCK_SIZE_FIELD)?;
        if block_size_exponent != SUPPORTED_BLOCK_SIZE_EXPONENT {
            return Err(EdlError::StorageInfo(format!(
                "{BLOCK_SIZE_FIELD} is {block_size_exponent:#x}, only {SUPPORTED_BLOCK_SIZE_EXPONENT:#x} ({SUPPORTED_BLOCK_BYTES} bytes) is supported"
            )));
        }
        Ok(Self {
            total_blocks,
            block_bytes: SUPPORTED_BLOCK_BYTES,
        })
    }
}

fn field_value(lines: &[String], field: &str) -> Result<u64> {
    let mut found: Option<u64> = None;
    for line in lines {
        let Some((_, text)) = line.split_once(field) else {
            continue;
        };
        let value = hexadecimal_value(field, text)?;
        if found.is_some_and(|previous| previous != value) {
            return Err(EdlError::StorageInfo(format!(
                "the response gives {field} more than once with different values"
            )));
        }
        found = Some(value);
    }
    found.ok_or_else(|| EdlError::StorageInfo(format!("the response has no {field} line")))
}

fn hexadecimal_value(field: &str, text: &str) -> Result<u64> {
    let trimmed = text.trim();
    let trimmed = trimmed.strip_prefix('=').map_or(trimmed, str::trim);
    let digits = trimmed.strip_prefix(HEXADECIMAL_PREFIX).unwrap_or_default();
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(EdlError::StorageInfo(format!(
            "{field} has the value {trimmed:?}, a hexadecimal number with the prefix {HEXADECIMAL_PREFIX} is necessary"
        )));
    }
    u64::from_str_radix(digits, 16)
        .map_err(|_| EdlError::StorageInfo(format!("{field} value {trimmed:?} is too large")))
}
