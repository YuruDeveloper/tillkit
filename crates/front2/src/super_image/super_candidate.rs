use super::byte_range::ByteRange;
use super::file_ranges::{compare_outside, copy_all, copy_exact, read_at};
use super::rebuilt_metadata::RebuiltMetadata;
use super::super_candidate_report::SuperCandidateReport;
use super::super_candidate_request::SuperCandidateRequest;
use super::super_layout_plan::SuperLayoutPlan;
use crate::digest::{hex, sha256_file};
use crate::error::{Front2Error, Result};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};

pub fn build_super_candidate(
    request: &SuperCandidateRequest<'_>,
    plan: &SuperLayoutPlan,
) -> Result<SuperCandidateReport> {
    let metadata = RebuiltMetadata::from_super_file(request.original_super, plan)?;
    let allowed = allowed_ranges(plan, &metadata)?;
    let (gsi_bytes, gsi_sha256) = sha256_file(request.gsi_image)?;
    if gsi_bytes != plan.gsi_bytes || gsi_sha256 != request.gsi_sha256 {
        return Err(Front2Error::Image(format!(
            "the GSI is {gsi_bytes} bytes with SHA-256 {gsi_sha256}, expected {} bytes with SHA-256 {}",
            plan.gsi_bytes, request.gsi_sha256
        )));
    }
    let mut output = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(request.output)?;
    let written = write_candidate(request, plan, &metadata, &mut output);
    drop(output);
    let report = written.and_then(|()| check_candidate(request, plan, metadata, allowed));
    if report.is_err() {
        let _ = fs::remove_file(request.output);
    }
    report
}

fn failure(reason: String) -> Front2Error {
    Front2Error::Verification(format!("super candidate: {reason}"))
}

fn allowed_ranges(plan: &SuperLayoutPlan, metadata: &RebuiltMetadata) -> Result<Vec<ByteRange>> {
    let mut allowed = vec![
        ByteRange::from_length(metadata.primary_offset, metadata.candidate_bytes)?,
        ByteRange::from_length(metadata.backup_offset, metadata.candidate_bytes)?,
    ];
    for extent in &plan.allocation {
        allowed.push(extent.byte_range()?);
    }
    ByteRange::sort_disjoint(
        &mut allowed,
        plan.super_bytes,
        "the ranges the candidate may change",
    )?;
    Ok(allowed)
}

fn write_candidate(
    request: &SuperCandidateRequest<'_>,
    plan: &SuperLayoutPlan,
    metadata: &RebuiltMetadata,
    output: &mut File,
) -> Result<()> {
    copy_original(request, plan, output)?;
    for offset in [metadata.primary_offset, metadata.backup_offset] {
        output.seek(SeekFrom::Start(offset))?;
        output.write_all(&metadata.blob)?;
    }
    write_gsi(request, plan, output)?;
    output.flush()?;
    output.sync_all()?;
    Ok(())
}

fn copy_original(
    request: &SuperCandidateRequest<'_>,
    plan: &SuperLayoutPlan,
    output: &mut File,
) -> Result<()> {
    let mut original = File::open(request.original_super)?;
    let mut hasher = Sha256::new();
    let copied = copy_all(&mut original, output, &mut hasher)?;
    let sha256 = hex(&hasher.finalize());
    if copied != plan.super_bytes || sha256 != request.original_super_sha256 {
        return Err(Front2Error::Image(format!(
            "the original super is {copied} bytes with SHA-256 {sha256}, expected {} bytes with SHA-256 {}",
            plan.super_bytes, request.original_super_sha256
        )));
    }
    Ok(())
}

fn write_gsi(
    request: &SuperCandidateRequest<'_>,
    plan: &SuperLayoutPlan,
    output: &mut File,
) -> Result<()> {
    let mut gsi = File::open(request.gsi_image)?;
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    for extent in &plan.allocation {
        let range = extent.byte_range()?;
        output.seek(SeekFrom::Start(range.start))?;
        copy_exact(&mut gsi, output, range.bytes(), &mut hasher)?;
        total += range.bytes();
    }
    let sha256 = hex(&hasher.finalize());
    let at_end = gsi.read(&mut [0u8; 1])? == 0;
    if !at_end || total != plan.gsi_bytes || sha256 != request.gsi_sha256 {
        return Err(Front2Error::Image(format!(
            "the planned extents took {total} bytes with SHA-256 {sha256} from the GSI, expected all {} bytes with SHA-256 {}",
            plan.gsi_bytes, request.gsi_sha256
        )));
    }
    Ok(())
}

fn check_candidate(
    request: &SuperCandidateRequest<'_>,
    plan: &SuperLayoutPlan,
    metadata: RebuiltMetadata,
    allowed: Vec<ByteRange>,
) -> Result<SuperCandidateReport> {
    let mut original = File::open(request.original_super)?;
    let mut candidate = File::open(request.output)?;
    let outside = compare_outside(&mut original, &mut candidate, &allowed, plan.super_bytes)?;
    for offset in [metadata.primary_offset, metadata.backup_offset] {
        require_metadata_at(&mut candidate, offset, &metadata.blob)?;
    }
    let (candidate_bytes, candidate_sha256) = sha256_file(request.output)?;
    let (original_bytes, original_sha256) = sha256_file(request.original_super)?;
    let sizes_match = candidate_bytes == plan.super_bytes && original_bytes == plan.super_bytes;
    if !sizes_match || original_sha256 != request.original_super_sha256 {
        return Err(failure(format!(
            "the candidate is {candidate_bytes} bytes and the original super is now {original_bytes} bytes with SHA-256 {original_sha256}, expected {} bytes and SHA-256 {}",
            plan.super_bytes, request.original_super_sha256
        )));
    }
    Ok(SuperCandidateReport {
        original_super_bytes: original_bytes,
        original_super_sha256: original_sha256,
        original_super_unchanged: true,
        gsi_bytes: plan.gsi_bytes,
        gsi_sha256: request.gsi_sha256.to_string(),
        metadata,
        allowed_super_ranges: allowed,
        outside_ranges_equal_bytes: outside,
        candidate_bytes,
        candidate_sha256,
    })
}

fn require_metadata_at(candidate: &mut File, offset: u64, blob: &[u8]) -> Result<()> {
    if read_at(candidate, offset, blob.len())? != blob {
        return Err(failure(format!(
            "the metadata at offset {offset} is not the rebuilt metadata"
        )));
    }
    Ok(())
}
