use front2::gpt::{GptCopyKind, crc32};

pub const SECTOR: usize = 4096;
pub const ENTRY_BYTES: usize = 128;
pub const ENTRY_COUNT: usize = 8;
pub const LAST_LBA: u64 = 4095;
pub const FIRST_PARTITION_LBA: u64 = 64;
pub const PARTITION_SECTORS: u64 = 16;

const HEADER_BYTES: usize = 92;

#[derive(Debug, Clone)]
pub struct SyntheticGpt {
    entries: Vec<(String, [u8; ENTRY_BYTES])>,
}

impl SyntheticGpt {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn standard() -> Self {
        Self::empty()
            .with_entry("boot_a", 0x0077 << 48)
            .with_entry("boot_b", 0x00c3 << 48)
            .with_entry("vbmeta_a", 0x107f << 48)
            .with_entry("vbmeta_b", 0x107b << 48)
            .with_entry("dtbo_a", 0x007f << 48)
            .with_entry("dtbo_b", 0x007b << 48)
            .with_entry("misc", 0)
    }

    pub fn with_entry(mut self, name: &str, flags: u64) -> Self {
        let position = self.entries.len() as u8;
        let first_lba = FIRST_PARTITION_LBA + u64::from(position) * PARTITION_SECTORS;
        let last_lba = first_lba + PARTITION_SECTORS - 1;
        let mut entry = [0u8; ENTRY_BYTES];
        entry[..16].fill(0xA0 + position);
        entry[16..32].fill(0x10 + position);
        entry[32..40].copy_from_slice(&first_lba.to_le_bytes());
        entry[40..48].copy_from_slice(&last_lba.to_le_bytes());
        entry[48..56].copy_from_slice(&flags.to_le_bytes());
        for (i, unit) in name.encode_utf16().enumerate() {
            entry[56 + i * 2..58 + i * 2].copy_from_slice(&unit.to_le_bytes());
        }
        self.entries.push((name.to_string(), entry));
        self
    }

    pub fn with_flags(mut self, name: &str, flags: u64) -> Self {
        let (_, entry) = self
            .entries
            .iter_mut()
            .find(|(known, _)| known == name)
            .expect("the synthetic GPT holds the entry");
        entry[48..56].copy_from_slice(&flags.to_le_bytes());
        self
    }

    pub fn without(mut self, name: &str) -> Self {
        self.entries.retain(|(known, _)| known != name);
        self
    }

    pub fn table(&self) -> Vec<u8> {
        assert!(self.entries.len() <= ENTRY_COUNT);
        let mut table = vec![0u8; ENTRY_COUNT * ENTRY_BYTES];
        for (i, (_, entry)) in self.entries.iter().enumerate() {
            table[i * ENTRY_BYTES..(i + 1) * ENTRY_BYTES].copy_from_slice(entry);
        }
        table
    }

    pub fn main(&self) -> Vec<u8> {
        self.copy(GptCopyKind::Main, 1, LAST_LBA, 2)
    }

    pub fn backup(&self) -> Vec<u8> {
        self.copy(GptCopyKind::Backup, LAST_LBA, 1, LAST_LBA - 4)
    }

    fn copy(
        &self,
        kind: GptCopyKind,
        current_lba: u64,
        other_lba: u64,
        entries_lba: u64,
    ) -> Vec<u8> {
        let table = self.table();
        let mut bytes = vec![0u8; kind.dump_bytes()];
        let table_offset = kind.table_offset();
        bytes[table_offset..table_offset + table.len()].copy_from_slice(&table);
        let header = &mut bytes[kind.header_offset()..kind.header_offset() + SECTOR];
        header[..8].copy_from_slice(b"EFI PART");
        header[8..12].copy_from_slice(&[0, 0, 1, 0]);
        header[12..16].copy_from_slice(&(HEADER_BYTES as u32).to_le_bytes());
        header[24..32].copy_from_slice(&current_lba.to_le_bytes());
        header[32..40].copy_from_slice(&other_lba.to_le_bytes());
        header[40..48].copy_from_slice(&6u64.to_le_bytes());
        header[48..56].copy_from_slice(&(LAST_LBA - 5).to_le_bytes());
        header[56..72].fill(0xD1);
        header[72..80].copy_from_slice(&entries_lba.to_le_bytes());
        header[80..84].copy_from_slice(&(ENTRY_COUNT as u32).to_le_bytes());
        header[84..88].copy_from_slice(&(ENTRY_BYTES as u32).to_le_bytes());
        seal(kind, &mut bytes);
        bytes
    }
}

pub fn seal(kind: GptCopyKind, bytes: &mut [u8]) {
    let header = kind.header_offset();
    let table = kind.table_offset();
    let table_checksum = crc32(&bytes[table..table + ENTRY_COUNT * ENTRY_BYTES]);
    bytes[header + 88..header + 92].copy_from_slice(&table_checksum.to_le_bytes());
    bytes[header + 16..header + 20].fill(0);
    let header_checksum = crc32(&bytes[header..header + HEADER_BYTES]);
    bytes[header + 16..header + 20].copy_from_slice(&header_checksum.to_le_bytes());
}
