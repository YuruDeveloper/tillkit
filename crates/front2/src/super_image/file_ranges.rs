use super::byte_range::ByteRange;
use crate::error::{Front2Error, Result};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

pub const CHUNK_BYTES: usize = 4 * 1024 * 1024;

pub fn read_at<Source: Read + Seek>(
    source: &mut Source,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>> {
    let mut data = vec![0u8; length];
    source.seek(SeekFrom::Start(offset))?;
    source.read_exact(&mut data).map_err(|error| {
        Front2Error::Image(format!("read {length} bytes at offset {offset}: {error}"))
    })?;
    Ok(data)
}

pub fn copy_all(source: &mut File, target: &mut File, hasher: &mut Sha256) -> Result<u64> {
    let mut buffer = vec![0u8; CHUNK_BYTES];
    let mut total = 0u64;
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            return Ok(total);
        }
        target.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
        total += count as u64;
    }
}

pub fn copy_exact(
    source: &mut File,
    target: &mut File,
    length: u64,
    hasher: &mut Sha256,
) -> Result<()> {
    let mut buffer = vec![0u8; chunk_length(length)];
    let mut remaining = length;
    while remaining > 0 {
        let wanted = chunk_length(remaining);
        let count = source.read(&mut buffer[..wanted])?;
        if count == 0 {
            return Err(Front2Error::Image(format!(
                "the source ends {remaining} bytes before the end of a range of {length} bytes"
            )));
        }
        target.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
        remaining -= count as u64;
    }
    Ok(())
}

pub fn hash_range(
    source: &mut File,
    twin: Option<&mut File>,
    range: ByteRange,
    hasher: &mut Sha256,
) -> Result<u64> {
    walk(source, twin, range, &mut |data| hasher.update(data))
}

pub fn compare_range(original: &mut File, candidate: &mut File, range: ByteRange) -> Result<u64> {
    walk(candidate, Some(original), range, &mut |_| {})
}

pub fn compare_outside(
    original: &mut File,
    candidate: &mut File,
    allowed: &[ByteRange],
    total: u64,
) -> Result<u64> {
    let sentinel = ByteRange {
        start: total,
        end: total,
    };
    let mut start = 0u64;
    let mut outside = 0u64;
    for range in allowed.iter().chain(std::iter::once(&sentinel)) {
        if range.start < start {
            return Err(Front2Error::Verification(format!(
                "allowed range {range:?} starts before offset {start}"
            )));
        }
        let gap = ByteRange {
            start,
            end: range.start,
        };
        outside += compare_range(original, candidate, gap)?;
        start = range.end;
    }
    Ok(outside)
}

fn chunk_length(remaining: u64) -> usize {
    remaining.min(CHUNK_BYTES as u64) as usize
}

fn walk(
    source: &mut File,
    mut twin: Option<&mut File>,
    range: ByteRange,
    visit: &mut dyn FnMut(&[u8]),
) -> Result<u64> {
    let mut data = vec![0u8; chunk_length(range.bytes())];
    let mut twin_data = vec![0u8; data.len()];
    let mut offset = range.start;
    source.seek(SeekFrom::Start(offset))?;
    if let Some(file) = twin.as_deref_mut() {
        file.seek(SeekFrom::Start(offset))?;
    }
    while offset < range.end {
        let count = chunk_length(range.end - offset);
        read_chunk(source, &mut data[..count], offset)?;
        if let Some(file) = twin.as_deref_mut() {
            read_chunk(file, &mut twin_data[..count], offset)?;
            require_same(&data[..count], &twin_data[..count], offset)?;
        }
        visit(&data[..count]);
        offset += count as u64;
    }
    Ok(range.bytes())
}

fn read_chunk(source: &mut File, data: &mut [u8], offset: u64) -> Result<()> {
    source.read_exact(data).map_err(|error| {
        Front2Error::Verification(format!(
            "read {} bytes at offset {offset}: {error}",
            data.len()
        ))
    })
}

fn require_same(data: &[u8], twin_data: &[u8], offset: u64) -> Result<()> {
    let Some(index) = data
        .iter()
        .zip(twin_data)
        .position(|(left, right)| left != right)
    else {
        return Ok(());
    };
    Err(Front2Error::Verification(format!(
        "the candidate differs from the original super at offset {}",
        offset + index as u64
    )))
}
