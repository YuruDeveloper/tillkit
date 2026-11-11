use crate::error::{Front2Error, Result};
use crate::gpt::gpt_copy::GptCopy;
use crate::gpt::gpt_copy_kind::GptCopyKind;
use crate::gpt::layout::{
    ENTRY_FLAGS_BYTES, ENTRY_FLAGS_OFFSET, ENTRY_TYPE_GUID_BYTES, TABLE_AREA_BYTES, slice,
};
use crate::gpt::permitted_bytes::PermittedBytes;

pub fn require_current_copy_matches(
    kind: GptCopyKind,
    current: &[u8],
    candidate: &[u8],
) -> Result<()> {
    let copy = GptCopy::parse(kind, current)?;
    if current.len() != candidate.len() {
        return Err(Front2Error::Verification(format!(
            "the current GPT {} copy is {} bytes, the candidate is {} bytes",
            kind.label(),
            current.len(),
            candidate.len()
        )));
    }
    let mut permitted = PermittedBytes::none(current.len());
    for range in kind.checksum_ranges() {
        permitted.permit(range)?;
    }
    for i in 0..copy.entry_count() {
        let entry = kind.table_offset() + i * copy.entry_size();
        let flags = entry + ENTRY_FLAGS_OFFSET;
        permitted.permit(entry..entry + ENTRY_TYPE_GUID_BYTES)?;
        permitted.permit(flags..flags + ENTRY_FLAGS_BYTES)?;
    }
    permitted.require_only_permitted_differences(
        current,
        candidate,
        &format!("current GPT {} copy against the candidate", kind.label()),
    )
}

pub fn require_matching_table_areas(current_main: &[u8], current_backup: &[u8]) -> Result<()> {
    GptCopy::parse(GptCopyKind::Main, current_main)?.require_pair_with(
        &GptCopy::parse(GptCopyKind::Backup, current_backup)?,
        "current GPT",
    )?;
    let main_area = slice(
        current_main,
        GptCopyKind::Main.table_offset(),
        TABLE_AREA_BYTES,
    )?;
    let backup_area = slice(
        current_backup,
        GptCopyKind::Backup.table_offset(),
        TABLE_AREA_BYTES,
    )?;
    if main_area != backup_area {
        return Err(Front2Error::Verification(
            "the table areas of the current main and backup GPT copies differ".into(),
        ));
    }
    Ok(())
}
