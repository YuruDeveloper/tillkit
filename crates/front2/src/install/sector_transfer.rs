use crate::digest::hex;
use crate::error::{Front2Error, Result};
use crate::install::layout::{CHUNK_SECTORS, sectors_to_bytes};
use crate::install::payload_reader::PayloadReader;
use crate::install::sector_storage::SectorStorage;
use crate::install::stored_install_plan::StoredInstallPlan;
use crate::install::write_step::WriteStep;
use sha2::{Digest, Sha256};

pub fn read_sectors_sha256(
    storage: &mut dyn SectorStorage,
    lun: u8,
    start_sector: u64,
    sectors: u64,
) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    while done < sectors {
        let count = (sectors - done).min(CHUNK_SECTORS);
        let position = chunk_position(lun, start_sector, done)?;
        let data = storage.read_sectors(lun, position, count)?;
        require_chunk_bytes("read", data.len(), lun, position, count)?;
        hasher.update(&data);
        done += count;
    }
    Ok(hex(&hasher.finalize()))
}

pub fn write_payload(
    storage: &mut dyn SectorStorage,
    step: &WriteStep,
    reader: &mut PayloadReader,
) -> Result<()> {
    let mut done = 0u64;
    while done < step.sectors {
        let count = (step.sectors - done).min(CHUNK_SECTORS);
        let position = chunk_position(step.lun, step.start_sector, done)?;
        let chunk = reader.next_chunk(sectors_to_bytes(count).unwrap_or(u64::MAX))?;
        require_chunk_bytes("source", chunk.len(), step.lun, position, count)?;
        storage.write_sectors(step.lun, position, &chunk)?;
        done += count;
    }
    if reader.remaining() != 0 {
        return Err(Front2Error::Plan(format!(
            "the source of {} holds {} bytes more than the step covers",
            step.name,
            reader.remaining()
        )));
    }
    Ok(())
}

pub fn write_and_read_back(
    plan: &StoredInstallPlan,
    storage: &mut dyn SectorStorage,
    step: &WriteStep,
) -> Result<String> {
    let mut reader = PayloadReader::open(plan, step)?;
    write_payload(storage, step, &mut reader)?;
    let readback = read_sectors_sha256(storage, step.lun, step.start_sector, step.sectors)?;
    if readback != step.sha256 {
        return Err(Front2Error::Verification(format!(
            "{}: the readback of {} sectors at sector {} of LUN {} has SHA-256 {readback}, the plan records {}",
            step.name, step.sectors, step.start_sector, step.lun, step.sha256
        )));
    }
    Ok(readback)
}

fn chunk_position(lun: u8, start_sector: u64, done: u64) -> Result<u64> {
    start_sector.checked_add(done).ok_or_else(|| {
        Front2Error::Plan(format!(
            "sector {start_sector} plus {done} on LUN {lun} is out of range"
        ))
    })
}

fn require_chunk_bytes(
    role: &str,
    actual: usize,
    lun: u8,
    start_sector: u64,
    sectors: u64,
) -> Result<()> {
    if sectors_to_bytes(sectors) != Some(actual as u64) {
        return Err(Front2Error::Verification(format!(
            "the {role} of {sectors} sectors at sector {start_sector} of LUN {lun} gave {actual} bytes"
        )));
    }
    Ok(())
}
