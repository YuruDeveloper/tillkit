use crate::error::{Front2Error, Result};
use crate::kernel::boot_image_header::{BootImageHeader, HEADER_PAGE_BYTES, KERNEL_SIZE_OFFSET};
use crate::kernel::kernel_release::KernelRelease;

const CURRENT: &str = "current boot image";
const NEW: &str = "new boot image";

pub fn candidate_payload_bytes(current: &[u8], candidate: &[u8], maximum: u64) -> Result<u64> {
    let failure = |reason: String| Front2Error::Image(format!("{NEW} {reason}"));
    let current_header = labeled(CURRENT, kernel_only_header(current))?;
    let header = labeled(NEW, kernel_only_header(candidate))?;
    if candidate.len() > current.len() {
        return Err(failure(format!(
            "is {} bytes, the partition is {} bytes",
            candidate.len(),
            current.len()
        )));
    }
    let size_field = KERNEL_SIZE_OFFSET..KERNEL_SIZE_OFFSET + 4;
    let header_differs = (0..HEADER_PAGE_BYTES)
        .any(|index| !size_field.contains(&index) && current[index] != candidate[index]);
    if header_differs {
        return Err(failure(
            "changes header fields other than the kernel size".into(),
        ));
    }
    let payload = header.payload_bytes();
    if payload > maximum || payload > candidate.len() as u64 {
        return Err(failure(format!(
            "has a payload of {payload} bytes, the file has {} bytes and the AVB footer limit is {maximum} bytes",
            candidate.len()
        )));
    }
    let current_kernel = labeled(CURRENT, current_header.kernel(current))?;
    let kernel = labeled(NEW, header.kernel(candidate))?;
    let current_release = labeled(CURRENT, KernelRelease::from_kernel(current_kernel))?;
    let release = labeled(NEW, KernelRelease::from_kernel(kernel))?;
    if release.kmi != current_release.kmi {
        return Err(failure(format!(
            "has KMI {} from release {}, the current kernel has KMI {} from release {}",
            release.kmi, release.release, current_release.kmi, current_release.release
        )));
    }
    if kernel == current_kernel {
        return Err(failure(
            "carries the same kernel as the current boot image".into(),
        ));
    }
    Ok(payload)
}

fn kernel_only_header(image: &[u8]) -> Result<BootImageHeader> {
    let header = BootImageHeader::parse(image)?;
    header.require_kernel_only()?;
    Ok(header)
}

fn labeled<T>(label: &str, result: Result<T>) -> Result<T> {
    result.map_err(|error| match error {
        Front2Error::Image(reason) => Front2Error::Image(format!("{label}: {reason}")),
        other => other,
    })
}
