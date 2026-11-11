use crate::digest::sha256_bytes;
use crate::error::{Front2Error, Result};
use crate::gpt::gpt_copy::GptCopy;
use crate::gpt::gpt_copy_kind::GptCopyKind;
use crate::gpt::gpt_entry::GptEntry;
use crate::gpt::gpt_output::GptOutput;
use crate::gpt::layout::{ENTRY_FLAGS_BYTES, ENTRY_FLAGS_OFFSET, ENTRY_TYPE_GUID_BYTES, write_u64};
use crate::gpt::permitted_bytes::PermittedBytes;
use crate::gpt::slot_change::SlotChange;
use serde::Serialize;

pub const SLOT_A_SUFFIX: &str = "_a";
pub const SLOT_B_SUFFIX: &str = "_b";
pub const BOOT_A_NAME: &str = "boot_a";
pub const ACTIVE_FLAG: u64 = 1 << 50;
pub const PRIORITY_MASK: u64 = 3 << 48;
pub const INACTIVE_BOOT_PRIORITY: u64 = 2 << 48;
pub const RETRY_COUNT_MASK: u64 = 7 << 51;
pub const SUCCESSFUL_FLAG: u64 = 1 << 54;
pub const UNBOOTABLE_FLAG: u64 = 1 << 55;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SlotActivation {
    pub lun: u8,
    pub outputs: Vec<GptOutput>,
    pub changes: Vec<SlotChange>,
}

impl SlotActivation {
    pub fn prepare(lun: u8, main: &[u8], backup: &[u8]) -> Result<Self> {
        let main_copy = GptCopy::parse(GptCopyKind::Main, main)?;
        let backup_copy = GptCopy::parse(GptCopyKind::Backup, backup)?;
        main_copy.require_pair_with(&backup_copy, &format!("LUN {lun}"))?;
        let mut table = main_copy.table().to_vec();
        let mut permitted = PermittedBytes::none(table.len());
        let changes = switch_slots(lun, &main_copy, &mut table, &mut permitted)?;
        permitted.require_only_permitted_differences(
            main_copy.table(),
            &table,
            &format!("LUN {lun} GPT table"),
        )?;
        let outputs = vec![
            build_output(lun, &main_copy, &table, &permitted)?,
            build_output(lun, &backup_copy, &table, &permitted)?,
        ];
        Ok(Self {
            lun,
            outputs,
            changes,
        })
    }

    pub fn output(&self, kind: GptCopyKind) -> Result<&GptOutput> {
        self.outputs
            .iter()
            .find(|output| output.copy == kind)
            .ok_or_else(|| {
                Front2Error::Plan(format!(
                    "LUN {}: the activation holds no {} GPT output",
                    self.lun,
                    kind.label()
                ))
            })
    }
}

fn switch_slots(
    lun: u8,
    copy: &GptCopy,
    table: &mut [u8],
    permitted: &mut PermittedBytes,
) -> Result<Vec<SlotChange>> {
    let mut changes = Vec::new();
    for entry_a in copy.entries() {
        let Some(entry_b) = partner_of(lun, copy, entry_a)? else {
            continue;
        };
        changes.push(switch_pair(lun, entry_a, entry_b, table)?);
        permit_entry(permitted, entry_a)?;
        permit_entry(permitted, entry_b)?;
    }
    if changes.is_empty() {
        return Err(Front2Error::Unsupported(format!(
            "LUN {lun}: the GPT holds no partition whose name ends with {SLOT_A_SUFFIX}"
        )));
    }
    Ok(changes)
}

fn partner_of<'copy>(
    lun: u8,
    copy: &'copy GptCopy,
    entry_a: &GptEntry,
) -> Result<Option<&'copy GptEntry>> {
    let Some(base) = entry_a.name.strip_suffix(SLOT_A_SUFFIX) else {
        return Ok(None);
    };
    let name_b = format!("{base}{SLOT_B_SUFFIX}");
    match copy.entry(&name_b) {
        Some(entry_b) => Ok(Some(entry_b)),
        None => Err(Front2Error::Unsupported(format!(
            "LUN {lun}: {} has no partner named {name_b}",
            entry_a.name
        ))),
    }
}

fn switch_pair(
    lun: u8,
    entry_a: &GptEntry,
    entry_b: &GptEntry,
    table: &mut [u8],
) -> Result<SlotChange> {
    if entry_a.flags & ACTIVE_FLAG == 0 {
        return Err(Front2Error::Unsupported(format!(
            "LUN {lun}: {} is not active, flags {:016x}",
            entry_a.name, entry_a.flags
        )));
    }
    if entry_b.flags & ACTIVE_FLAG != 0 {
        return Err(Front2Error::Unsupported(format!(
            "LUN {lun}: {} is already active, flags {:016x}",
            entry_b.name, entry_b.flags
        )));
    }
    let mut flags_a = entry_a.flags & !ACTIVE_FLAG;
    let mut flags_b = entry_b.flags | ACTIVE_FLAG;
    if entry_a.name == BOOT_A_NAME {
        flags_a = (flags_a & !PRIORITY_MASK) | INACTIVE_BOOT_PRIORITY;
        flags_b =
            (flags_b | PRIORITY_MASK | RETRY_COUNT_MASK) & !SUCCESSFUL_FLAG & !UNBOOTABLE_FLAG;
    }
    write_entry(table, entry_a, &entry_b.type_guid, flags_a)?;
    write_entry(table, entry_b, &entry_a.type_guid, flags_b)?;
    Ok(SlotChange {
        lun,
        a: entry_a.name.clone(),
        b: entry_b.name.clone(),
        flags_before_a: SlotChange::flags_text(entry_a.flags),
        flags_before_b: SlotChange::flags_text(entry_b.flags),
        flags_after_a: SlotChange::flags_text(flags_a),
        flags_after_b: SlotChange::flags_text(flags_b),
        type_guids_swapped: true,
    })
}

fn write_entry(
    table: &mut [u8],
    entry: &GptEntry,
    type_guid: &[u8; ENTRY_TYPE_GUID_BYTES],
    flags: u64,
) -> Result<()> {
    let length = table.len();
    let target = entry
        .table_offset
        .checked_add(ENTRY_TYPE_GUID_BYTES)
        .and_then(|end| table.get_mut(entry.table_offset..end))
        .ok_or_else(|| {
            Front2Error::Image(format!(
                "GPT entry {:?} at {} is outside the table of {length} bytes",
                entry.name, entry.table_offset
            ))
        })?;
    target.copy_from_slice(type_guid);
    write_u64(table, entry.table_offset + ENTRY_FLAGS_OFFSET, flags)
}

fn permit_entry(permitted: &mut PermittedBytes, entry: &GptEntry) -> Result<()> {
    let flags = entry.table_offset + ENTRY_FLAGS_OFFSET;
    permitted.permit(entry.table_offset..entry.table_offset + ENTRY_TYPE_GUID_BYTES)?;
    permitted.permit(flags..flags + ENTRY_FLAGS_BYTES)
}

fn build_output(
    lun: u8,
    original: &GptCopy,
    table: &[u8],
    permitted_table: &PermittedBytes,
) -> Result<GptOutput> {
    let kind = original.kind();
    let updated = original.with_table(table)?;
    let mut permitted = PermittedBytes::none(original.bytes().len());
    permitted.permit_shifted(permitted_table, kind.table_offset())?;
    for range in kind.checksum_ranges() {
        permitted.permit(range)?;
    }
    permitted.require_only_permitted_differences(
        original.bytes(),
        updated.bytes(),
        &format!("LUN {lun} GPT {} copy", kind.label()),
    )?;
    Ok(GptOutput {
        lun,
        copy: kind,
        start_sector: updated.start_sector(),
        sectors: updated.sectors(),
        bytes: updated.bytes().len() as u64,
        sha256: sha256_bytes(updated.bytes()),
        restore_sha256: sha256_bytes(original.bytes()),
        data: updated.into_bytes(),
    })
}
