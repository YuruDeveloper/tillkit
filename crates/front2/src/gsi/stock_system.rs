use crate::error::{Front2Error, Result};
use crate::gsi::vbmeta_source::VbmetaSource;
use crate::profiles::toss_front2;
use avbtool_rs::info::DescriptorInfo;

pub fn require_stock_front2_system(stock: &VbmetaSource) -> Result<()> {
    let failure =
        |reason: String| Front2Error::Unsupported(format!("{}: {reason}", stock.path.display()));
    let advice = "pick a stock backup, not one that already has a GSI";
    let Some(value) = stock
        .descriptors
        .iter()
        .find_map(|descriptor| system_fingerprint(&descriptor.info))
    else {
        return Err(failure(format!(
            "it has no {} property. {advice}",
            toss_front2::SYSTEM_FINGERPRINT_PROPERTY
        )));
    };
    let shown = String::from_utf8_lossy(value);
    let build_number = std::str::from_utf8(value)
        .ok()
        .and_then(|text| text.strip_prefix(toss_front2::STOCK_SYSTEM_FINGERPRINT_PREFIX))
        .and_then(|rest| rest.strip_suffix(toss_front2::STOCK_SYSTEM_FINGERPRINT_SUFFIX))
        .unwrap_or_default();
    let is_a_number =
        !build_number.is_empty() && build_number.bytes().all(|byte| byte.is_ascii_digit());
    if !is_a_number {
        return Err(failure(format!(
            "its {} is {shown:?}, not the form {}<build number>{}. {advice}",
            toss_front2::SYSTEM_FINGERPRINT_PROPERTY,
            toss_front2::STOCK_SYSTEM_FINGERPRINT_PREFIX,
            toss_front2::STOCK_SYSTEM_FINGERPRINT_SUFFIX
        )));
    }
    Ok(())
}

fn system_fingerprint(descriptor: &DescriptorInfo) -> Option<&[u8]> {
    match descriptor {
        DescriptorInfo::Property { key, value }
            if key == toss_front2::SYSTEM_FINGERPRINT_PROPERTY =>
        {
            Some(value.as_slice())
        }
        _ => None,
    }
}
