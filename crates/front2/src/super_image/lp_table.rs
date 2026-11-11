use super::lp_format::{put, u32_at};
use crate::error::{Front2Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LpTable {
    pub offset: usize,
    pub count: usize,
    pub entry_bytes: usize,
}

impl LpTable {
    pub fn read(
        header: &[u8],
        position: usize,
        entry_bytes: usize,
        tables_bytes: usize,
    ) -> Result<Self> {
        let offset = u32_at(header, position)?;
        let count = u32_at(header, position + 4)?;
        let stored_entry_bytes = u32_at(header, position + 8)?;
        let end = u64::from(offset) + u64::from(count) * u64::from(stored_entry_bytes);
        if stored_entry_bytes as usize != entry_bytes || end > tables_bytes as u64 {
            return Err(Front2Error::Image(format!(
                "super metadata: table at header offset {position} has {count} entries of {stored_entry_bytes} bytes at {offset}, expected entries of {entry_bytes} bytes inside {tables_bytes} bytes"
            )));
        }
        Ok(Self {
            offset: offset as usize,
            count: count as usize,
            entry_bytes,
        })
    }

    pub fn end(&self) -> usize {
        self.offset + self.count * self.entry_bytes
    }

    pub fn records<'a>(&self, tables: &'a [u8]) -> Result<Vec<&'a [u8]>> {
        let data = tables.get(self.offset..self.end()).ok_or_else(|| {
            Front2Error::Image(format!(
                "super metadata: {self:?} is outside tables of {} bytes",
                tables.len()
            ))
        })?;
        Ok(data.chunks_exact(self.entry_bytes).collect())
    }

    pub fn write(&self, header: &mut [u8], position: usize) -> Result<()> {
        put_field(header, position, self.offset)?;
        put_field(header, position + 4, self.count)?;
        put_field(header, position + 8, self.entry_bytes)
    }
}

fn put_field(header: &mut [u8], position: usize, value: usize) -> Result<()> {
    let value = u32::try_from(value).map_err(|_| {
        Front2Error::Image(format!(
            "super metadata: table field {value} does not fit 32 bits"
        ))
    })?;
    put(header, position, &value.to_le_bytes())
}
