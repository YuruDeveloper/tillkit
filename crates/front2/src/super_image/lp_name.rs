use super::lp_format::{NAME_BYTES, bytes_at};
use crate::error::{Front2Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LpName {
    bytes: [u8; NAME_BYTES],
    text: String,
}

impl LpName {
    pub fn decode(record: &[u8], offset: usize) -> Result<Self> {
        let bytes: [u8; NAME_BYTES] = bytes_at(record, offset)?;
        let length = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(NAME_BYTES);
        let (text, padding) = bytes.split_at(length);
        if !text.is_ascii() || padding.iter().any(|byte| *byte != 0) {
            return Err(Front2Error::Image(format!(
                "super metadata: name field {bytes:?} is not ASCII text padded with zero bytes"
            )));
        }
        Ok(Self {
            bytes,
            text: String::from_utf8_lossy(text).into_owned(),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn bytes(&self) -> &[u8; NAME_BYTES] {
        &self.bytes
    }
}
