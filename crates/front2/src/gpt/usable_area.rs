use crate::error::{Front2Error, Result};
use crate::gpt::gpt_copy_kind::GptCopyKind;
use crate::gpt::gpt_entry::GptEntry;
use crate::gpt::layout::TABLE_AREA_SECTORS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsableArea {
    pub first_lba: u64,
    pub last_lba: u64,
}

impl UsableArea {
    pub fn require_outside_gpt(
        &self,
        kind: GptCopyKind,
        entries_lba: u64,
        alternate_lba: u64,
    ) -> Result<()> {
        let main_table_end = entries_lba.checked_add(TABLE_AREA_SECTORS as u64);
        let after_main = main_table_end <= Some(self.first_lba) && self.last_lba < alternate_lba;
        let before_backup = self.last_lba < entries_lba && alternate_lba < self.first_lba;
        let separate = match kind {
            GptCopyKind::Main => after_main,
            GptCopyKind::Backup => before_backup,
        };
        if self.first_lba > self.last_lba || !separate {
            return Err(Front2Error::Image(format!(
                "GPT {} header gives the usable LBA range {} to {}, its table is at LBA {entries_lba} and the other header at LBA {alternate_lba}",
                kind.label(),
                self.first_lba,
                self.last_lba
            )));
        }
        Ok(())
    }

    pub fn require_separate_entries(&self, kind: GptCopyKind, entries: &[GptEntry]) -> Result<()> {
        let mut sorted: Vec<&GptEntry> = entries.iter().collect();
        sorted.sort_by_key(|entry| entry.first_lba);
        for entry in &sorted {
            self.require_inside(kind, entry)?;
        }
        let shared = sorted
            .windows(2)
            .find(|pair| pair[0].last_lba >= pair[1].first_lba);
        let Some(pair) = shared else {
            return Ok(());
        };
        Err(Front2Error::Image(format!(
            "GPT {} entries {:?} and {:?} cover the same LBA {}",
            kind.label(),
            pair[0].name,
            pair[1].name,
            pair[1].first_lba
        )))
    }

    fn require_inside(&self, kind: GptCopyKind, entry: &GptEntry) -> Result<()> {
        entry.sectors()?;
        if entry.first_lba < self.first_lba || entry.last_lba > self.last_lba {
            return Err(Front2Error::Image(format!(
                "GPT {} entry {:?} covers LBA {} to {}, the usable range is {} to {}",
                kind.label(),
                entry.name,
                entry.first_lba,
                entry.last_lba,
                self.first_lba,
                self.last_lba
            )));
        }
        Ok(())
    }
}
