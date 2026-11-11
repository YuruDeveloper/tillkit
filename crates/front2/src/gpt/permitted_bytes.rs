use crate::error::{Front2Error, Result};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermittedBytes {
    mask: Vec<bool>,
}

impl PermittedBytes {
    pub fn none(length: usize) -> Self {
        Self {
            mask: vec![false; length],
        }
    }

    pub fn len(&self) -> usize {
        self.mask.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mask.is_empty()
    }

    pub fn permit(&mut self, range: Range<usize>) -> Result<()> {
        let length = self.mask.len();
        let target = self.mask.get_mut(range.clone()).ok_or_else(|| {
            Front2Error::Image(format!(
                "permitted range {}..{} is outside {length} bytes",
                range.start, range.end
            ))
        })?;
        target.fill(true);
        Ok(())
    }

    pub fn permit_shifted(&mut self, other: &PermittedBytes, shift: usize) -> Result<()> {
        let length = self.mask.len();
        let target = shift
            .checked_add(other.mask.len())
            .and_then(|end| self.mask.get_mut(shift..end))
            .ok_or_else(|| {
                Front2Error::Image(format!(
                    "{} permitted bytes at {shift} are outside {length} bytes",
                    other.mask.len()
                ))
            })?;
        for (permitted, source) in target.iter_mut().zip(&other.mask) {
            *permitted = *permitted || *source;
        }
        Ok(())
    }

    pub fn require_only_permitted_differences(
        &self,
        before: &[u8],
        after: &[u8],
        subject: &str,
    ) -> Result<()> {
        if before.len() != self.mask.len() || after.len() != self.mask.len() {
            return Err(Front2Error::Verification(format!(
                "{subject}: {} and {} bytes are compared, {} bytes were expected",
                before.len(),
                after.len(),
                self.mask.len()
            )));
        }
        let difference = before
            .iter()
            .zip(after)
            .zip(&self.mask)
            .position(|((left, right), permitted)| left != right && !permitted);
        match difference {
            Some(offset) => Err(Front2Error::Verification(format!(
                "{subject}: byte {offset} differs outside the permitted positions"
            ))),
            None => Ok(()),
        }
    }
}
