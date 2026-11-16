use crate::gsi::vbmeta_source::VbmetaSource;
use crate::profiles::toss_front2;
use avbtool_rs::info::DescriptorInfo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemOrigin {
    Stock,
    Other(String),
    NoFingerprint,
}

impl SystemOrigin {
    pub fn read(chained: &VbmetaSource) -> Self {
        let Some(value) = chained
            .descriptors
            .iter()
            .find_map(|descriptor| system_fingerprint(&descriptor.info))
        else {
            return Self::NoFingerprint;
        };
        if is_stock_fingerprint(value) {
            Self::Stock
        } else {
            Self::Other(String::from_utf8_lossy(value).into_owned())
        }
    }
}

fn is_stock_fingerprint(value: &[u8]) -> bool {
    let build_number = std::str::from_utf8(value)
        .ok()
        .and_then(|text| text.strip_prefix(toss_front2::STOCK_SYSTEM_FINGERPRINT_PREFIX))
        .and_then(|rest| rest.strip_suffix(toss_front2::STOCK_SYSTEM_FINGERPRINT_SUFFIX))
        .unwrap_or_default();
    !build_number.is_empty() && build_number.bytes().all(|byte| byte.is_ascii_digit())
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
