use crate::error::{Front2Error, Result};
use crate::gpt::gpt_copy::GptCopy;
use crate::gpt::gpt_copy_kind::GptCopyKind;
use crate::gpt::gpt_entry::GptEntry;
use crate::gpt::partition_location::PartitionLocation;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PartitionMap {
    partitions: BTreeMap<String, PartitionLocation>,
}

impl PartitionMap {
    pub fn from_main_copies(copies: &[(u8, &[u8])]) -> Result<Self> {
        let mut map = Self {
            partitions: BTreeMap::new(),
        };
        for (i, (lun, bytes)) in copies.iter().enumerate() {
            require_new_lun(*lun, &copies[..i])?;
            let copy =
                GptCopy::parse(GptCopyKind::Main, bytes).map_err(|err| with_lun(*lun, err))?;
            map.insert_copy(*lun, &copy)?;
        }
        Ok(map)
    }

    pub fn locate(&self, name: &str) -> Result<PartitionLocation> {
        self.partitions.get(name).copied().ok_or_else(|| {
            Front2Error::Plan(format!(
                "no GPT that was read holds a partition named {name:?}"
            ))
        })
    }

    pub fn partitions(&self) -> &BTreeMap<String, PartitionLocation> {
        &self.partitions
    }

    fn insert_copy(&mut self, lun: u8, copy: &GptCopy) -> Result<()> {
        for entry in copy.entries() {
            let location = location_of(lun, entry)?;
            let known = self.partitions.insert(entry.name.clone(), location);
            require_new_name(&entry.name, known, lun)?;
        }
        Ok(())
    }
}

fn with_lun(lun: u8, err: Front2Error) -> Front2Error {
    match err {
        Front2Error::Image(text) => Front2Error::Image(format!("LUN {lun}: {text}")),
        other => other,
    }
}

fn location_of(lun: u8, entry: &GptEntry) -> Result<PartitionLocation> {
    Ok(PartitionLocation {
        lun,
        start_sector: entry.first_lba,
        sectors: entry.sectors()?,
    })
}

fn require_new_lun(lun: u8, earlier: &[(u8, &[u8])]) -> Result<()> {
    if earlier.iter().any(|(known, _)| *known == lun) {
        return Err(Front2Error::Image(format!(
            "LUN {lun} is in the list of GPT copies more than once"
        )));
    }
    Ok(())
}

fn require_new_name(name: &str, known: Option<PartitionLocation>, lun: u8) -> Result<()> {
    match known {
        Some(known) => Err(Front2Error::Image(format!(
            "the partition name {name:?} is on LUN {} and on LUN {lun}",
            known.lun
        ))),
        None => Ok(()),
    }
}
