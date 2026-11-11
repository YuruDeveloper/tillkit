use crate::digest::hex;
use crate::error::{Front2Error, Result};
use crate::gpt::SECTOR_BYTES;
use crate::install::layout::CHUNK_SECTORS;
use crate::install::step_source::StepSource;
use crate::install::stored_install_plan::StoredInstallPlan;
use crate::install::write_step::WriteStep;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct PayloadReader {
    file: Option<File>,
    remaining: u64,
}

impl PayloadReader {
    pub fn open(plan: &StoredInstallPlan, step: &WriteStep) -> Result<Self> {
        let file = source_path(plan, &step.source)
            .map(|path| open_sized(&path, step.bytes))
            .transpose()?;
        Ok(Self {
            file,
            remaining: step.bytes,
        })
    }

    pub fn remaining(&self) -> u64 {
        self.remaining
    }

    pub fn next_chunk(&mut self, maximum_bytes: u64) -> Result<Vec<u8>> {
        let length = self.remaining.min(maximum_bytes);
        let mut chunk = vec![0u8; length as usize];
        if let Some(file) = self.file.as_mut() {
            file.read_exact(&mut chunk)?;
        }
        self.remaining -= length;
        Ok(chunk)
    }

    pub fn sha256(mut self) -> Result<String> {
        let mut hasher = Sha256::new();
        while self.remaining > 0 {
            hasher.update(self.next_chunk(CHUNK_SECTORS * SECTOR_BYTES as u64)?);
        }
        Ok(hex(&hasher.finalize()))
    }
}

fn source_path(plan: &StoredInstallPlan, source: &StepSource) -> Option<PathBuf> {
    match source {
        StepSource::CandidateFile { file } => Some(plan.plan().candidate_directory.join(file)),
        StepSource::PlanFile { file } => Some(plan.directory().join(file)),
        StepSource::ZeroFill => None,
    }
}

fn open_sized(path: &Path, bytes: u64) -> Result<File> {
    let file = File::open(path)?;
    let actual = file.metadata()?.len();
    if actual != bytes {
        return Err(Front2Error::Verification(format!(
            "{} is {actual} bytes, the plan records {bytes} bytes",
            path.display()
        )));
    }
    Ok(file)
}
