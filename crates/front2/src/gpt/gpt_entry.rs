use crate::error::{Front2Error, Result};
use crate::gpt::layout::{
    ENTRY_FIRST_LBA_OFFSET, ENTRY_FLAGS_OFFSET, ENTRY_LAST_LBA_OFFSET, ENTRY_NAME_END,
    ENTRY_NAME_OFFSET, ENTRY_TYPE_GUID_BYTES, read_u64, slice,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GptEntry {
    pub name: String,
    pub type_guid: [u8; ENTRY_TYPE_GUID_BYTES],
    pub flags: u64,
    pub first_lba: u64,
    pub last_lba: u64,
    pub table_offset: usize,
}

impl GptEntry {
    pub fn parse(entry: &[u8], table_offset: usize) -> Result<Option<Self>> {
        let type_guid: [u8; ENTRY_TYPE_GUID_BYTES] = slice(entry, 0, ENTRY_TYPE_GUID_BYTES)?
            .try_into()
            .map_err(|_| Front2Error::Image("GPT entry type GUID is not 16 bytes".into()))?;
        if type_guid == [0u8; ENTRY_TYPE_GUID_BYTES] {
            return Ok(None);
        }
        let name = decode_name(
            slice(entry, ENTRY_NAME_OFFSET, ENTRY_NAME_END - ENTRY_NAME_OFFSET)?,
            table_offset,
        )?;
        Ok(Some(Self {
            name,
            type_guid,
            flags: read_u64(entry, ENTRY_FLAGS_OFFSET)?,
            first_lba: read_u64(entry, ENTRY_FIRST_LBA_OFFSET)?,
            last_lba: read_u64(entry, ENTRY_LAST_LBA_OFFSET)?,
            table_offset,
        }))
    }

    pub fn sectors(&self) -> Result<u64> {
        self.last_lba
            .checked_sub(self.first_lba)
            .and_then(|span| span.checked_add(1))
            .ok_or_else(|| {
                Front2Error::Image(format!(
                    "GPT entry {:?} starts at LBA {} and ends at LBA {}",
                    self.name, self.first_lba, self.last_lba
                ))
            })
    }
}

fn decode_name(field: &[u8], table_offset: usize) -> Result<String> {
    let units: Vec<u16> = field
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let text = String::from_utf16(&units).map_err(|_| {
        Front2Error::Image(format!(
            "GPT entry at table offset {table_offset} has a name that is not UTF-16"
        ))
    })?;
    match text.find('\0') {
        Some(end) => Ok(text[..end].to_string()),
        None => Ok(text),
    }
}
