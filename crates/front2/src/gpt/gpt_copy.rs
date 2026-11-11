use crate::error::{Front2Error, Result};
use crate::gpt::crc32::crc32;
use crate::gpt::gpt_copy_kind::GptCopyKind;
use crate::gpt::gpt_entry::GptEntry;
use crate::gpt::layout::{
    DISK_GUID_BYTES, ENTRY_MINIMUM_BYTES, HEADER_ALTERNATE_LBA_OFFSET, HEADER_CHECKSUM_OFFSET,
    HEADER_CURRENT_LBA_OFFSET, HEADER_DISK_GUID_OFFSET, HEADER_ENTRIES_LBA_OFFSET,
    HEADER_ENTRY_COUNT_OFFSET, HEADER_ENTRY_SIZE_OFFSET, HEADER_FIRST_USABLE_LBA_OFFSET,
    HEADER_LAST_USABLE_LBA_OFFSET, HEADER_MINIMUM_BYTES, HEADER_SIZE_OFFSET,
    HEADER_TABLE_CHECKSUM_OFFSET, SECTOR_BYTES, SIGNATURE, TABLE_AREA_BYTES, TABLE_AREA_SECTORS,
    read_u32, read_u64, slice, write_u32,
};
use crate::gpt::usable_area::UsableArea;

const MAIN_HEADER_LBA: u64 = 1;
const MAIN_ENTRIES_LBA: u64 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GptCopy {
    kind: GptCopyKind,
    bytes: Vec<u8>,
    header_size: usize,
    current_lba: u64,
    alternate_lba: u64,
    usable_area: UsableArea,
    disk_guid: Vec<u8>,
    start_sector: u64,
    entry_count: usize,
    entry_size: usize,
    entries: Vec<GptEntry>,
}

impl GptCopy {
    pub fn parse(kind: GptCopyKind, bytes: &[u8]) -> Result<Self> {
        if bytes.len() != kind.dump_bytes() {
            return Err(Front2Error::Image(format!(
                "GPT {} copy is {} bytes, a dump of {} bytes is required",
                kind.label(),
                bytes.len(),
                kind.dump_bytes()
            )));
        }
        let header = slice(bytes, kind.header_offset(), SECTOR_BYTES)?;
        if !header.starts_with(SIGNATURE) {
            return Err(Front2Error::Image(format!(
                "GPT {} copy has no EFI PART signature",
                kind.label()
            )));
        }
        let header_size = read_u32(header, HEADER_SIZE_OFFSET)? as usize;
        if !(HEADER_MINIMUM_BYTES..=SECTOR_BYTES).contains(&header_size) {
            return Err(Front2Error::Image(format!(
                "GPT {} header size is {header_size} bytes",
                kind.label()
            )));
        }
        let recorded_checksum = read_u32(header, HEADER_CHECKSUM_OFFSET)?;
        let actual_checksum = header_checksum(header, header_size)?;
        if recorded_checksum != actual_checksum {
            return Err(Front2Error::Image(format!(
                "GPT {} header CRC32 is {actual_checksum:08x}, the header records {recorded_checksum:08x}",
                kind.label()
            )));
        }
        let current_lba = read_u64(header, HEADER_CURRENT_LBA_OFFSET)?;
        let entries_lba = read_u64(header, HEADER_ENTRIES_LBA_OFFSET)?;
        let start_sector = dump_start_sector(kind, current_lba, entries_lba)?;
        let alternate_lba = read_u64(header, HEADER_ALTERNATE_LBA_OFFSET)?;
        let usable_area = UsableArea {
            first_lba: read_u64(header, HEADER_FIRST_USABLE_LBA_OFFSET)?,
            last_lba: read_u64(header, HEADER_LAST_USABLE_LBA_OFFSET)?,
        };
        usable_area.require_outside_gpt(kind, entries_lba, alternate_lba)?;
        let disk_guid = slice(header, HEADER_DISK_GUID_OFFSET, DISK_GUID_BYTES)?.to_vec();
        let entry_count = read_u32(header, HEADER_ENTRY_COUNT_OFFSET)? as usize;
        let entry_size = read_u32(header, HEADER_ENTRY_SIZE_OFFSET)? as usize;
        let table_bytes = table_length(kind, entry_count, entry_size)?;
        let table = slice(bytes, kind.table_offset(), table_bytes)?;
        let recorded_table_checksum = read_u32(header, HEADER_TABLE_CHECKSUM_OFFSET)?;
        let actual_table_checksum = crc32(table);
        if recorded_table_checksum != actual_table_checksum {
            return Err(Front2Error::Image(format!(
                "GPT {} table CRC32 is {actual_table_checksum:08x}, the header records {recorded_table_checksum:08x}",
                kind.label()
            )));
        }
        let entries = parse_entries(kind, table, entry_size)?;
        usable_area.require_separate_entries(kind, &entries)?;
        Ok(Self {
            kind,
            bytes: bytes.to_vec(),
            header_size,
            current_lba,
            alternate_lba,
            usable_area,
            disk_guid,
            start_sector,
            entry_count,
            entry_size,
            entries,
        })
    }

    pub fn kind(&self) -> GptCopyKind {
        self.kind
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn table(&self) -> &[u8] {
        let start = self.kind.table_offset();
        &self.bytes[start..start + self.entry_count * self.entry_size]
    }

    pub fn entries(&self) -> &[GptEntry] {
        &self.entries
    }

    pub fn entry(&self, name: &str) -> Option<&GptEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    pub fn entry_count(&self) -> usize {
        self.entry_count
    }

    pub fn entry_size(&self) -> usize {
        self.entry_size
    }

    pub fn header_size(&self) -> usize {
        self.header_size
    }

    pub fn current_lba(&self) -> u64 {
        self.current_lba
    }

    pub fn alternate_lba(&self) -> u64 {
        self.alternate_lba
    }

    pub fn usable_area(&self) -> UsableArea {
        self.usable_area
    }

    pub fn disk_guid(&self) -> &[u8] {
        &self.disk_guid
    }

    pub fn start_sector(&self) -> u64 {
        self.start_sector
    }

    pub fn require_pair_with(&self, backup: &Self, subject: &str) -> Result<()> {
        let differences: Vec<&str> = [
            (
                self.kind != GptCopyKind::Main || backup.kind != GptCopyKind::Backup,
                "copy kinds",
            ),
            (self.disk_guid != backup.disk_guid, "disk GUID"),
            (
                self.alternate_lba != backup.current_lba,
                "backup header position",
            ),
            (
                backup.alternate_lba != self.current_lba,
                "main header position",
            ),
            (self.usable_area != backup.usable_area, "usable LBA range"),
            (
                self.entry_count != backup.entry_count || self.entry_size != backup.entry_size,
                "table dimensions",
            ),
            (self.table() != backup.table(), "tables"),
        ]
        .into_iter()
        .filter_map(|(differs, name)| differs.then_some(name))
        .collect();
        if !differences.is_empty() {
            return Err(Front2Error::Verification(format!(
                "{subject}: the main and backup GPT copies do not agree in {}",
                differences.join(", ")
            )));
        }
        Ok(())
    }

    pub fn sectors(&self) -> u64 {
        (self.bytes.len() / SECTOR_BYTES) as u64
    }

    pub fn with_table(&self, table: &[u8]) -> Result<Self> {
        let current = self.table();
        if table.len() != current.len() {
            return Err(Front2Error::Image(format!(
                "the new GPT table is {} bytes, the {} copy holds {} bytes",
                table.len(),
                self.kind.label(),
                current.len()
            )));
        }
        let mut bytes = self.bytes.clone();
        let table_offset = self.kind.table_offset();
        bytes[table_offset..table_offset + table.len()].copy_from_slice(table);
        let header_offset = self.kind.header_offset();
        write_u32(
            &mut bytes,
            header_offset + HEADER_TABLE_CHECKSUM_OFFSET,
            crc32(table),
        )?;
        let checksum = header_checksum(
            slice(&bytes, header_offset, SECTOR_BYTES)?,
            self.header_size,
        )?;
        write_u32(&mut bytes, header_offset + HEADER_CHECKSUM_OFFSET, checksum)?;
        let updated = Self::parse(self.kind, &bytes)?;
        if updated.table() != table {
            return Err(Front2Error::Verification(format!(
                "the rebuilt GPT {} copy does not hold the new table",
                self.kind.label()
            )));
        }
        Ok(updated)
    }
}

fn header_checksum(header: &[u8], header_size: usize) -> Result<u32> {
    let mut covered = slice(header, 0, header_size)?.to_vec();
    write_u32(&mut covered, HEADER_CHECKSUM_OFFSET, 0)?;
    Ok(crc32(&covered))
}

fn dump_start_sector(kind: GptCopyKind, current_lba: u64, entries_lba: u64) -> Result<u64> {
    let expected = match kind {
        GptCopyKind::Main => Some((MAIN_HEADER_LBA, MAIN_ENTRIES_LBA, 0)),
        GptCopyKind::Backup => current_lba
            .checked_sub(TABLE_AREA_SECTORS as u64)
            .map(|table_lba| (current_lba, table_lba, table_lba)),
    };
    match expected {
        Some((header_lba, table_lba, start_sector))
            if current_lba == header_lba && entries_lba == table_lba =>
        {
            Ok(start_sector)
        }
        _ => Err(Front2Error::Image(format!(
            "GPT {} header is at LBA {current_lba} with its table at LBA {entries_lba}, the dump layout does not allow this",
            kind.label()
        ))),
    }
}

fn table_length(kind: GptCopyKind, entry_count: usize, entry_size: usize) -> Result<usize> {
    let length = entry_count
        .checked_mul(entry_size)
        .filter(|length| *length <= TABLE_AREA_BYTES);
    match length {
        Some(length) if entry_count > 0 && entry_size >= ENTRY_MINIMUM_BYTES => Ok(length),
        _ => Err(Front2Error::Image(format!(
            "GPT {} table has {entry_count} entries of {entry_size} bytes",
            kind.label()
        ))),
    }
}

fn parse_entries(kind: GptCopyKind, table: &[u8], entry_size: usize) -> Result<Vec<GptEntry>> {
    let mut entries: Vec<GptEntry> = Vec::new();
    for (i, raw) in table.chunks_exact(entry_size).enumerate() {
        let Some(entry) = GptEntry::parse(raw, i * entry_size)? else {
            continue;
        };
        if entries.iter().any(|known| known.name == entry.name) {
            return Err(Front2Error::Image(format!(
                "GPT {} table holds the name {:?} more than once",
                kind.label(),
                entry.name
            )));
        }
        entries.push(entry);
    }
    Ok(entries)
}
