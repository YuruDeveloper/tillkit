use ltbox_device::edl::{EdlError, UfsStorageInfo};

const LUN0_RESPONSE: &str = include_str!("fixtures/ufs-storage-info-lun0.txt");
const LUN1_RESPONSE: &str = include_str!("fixtures/ufs-storage-info-lun1.txt");

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

fn replaced(text: &str, field_line: &str, replacement: &str) -> Vec<String> {
    assert!(text.contains(field_line), "fixture has {field_line}");
    lines(&text.replace(field_line, replacement))
}

fn storage_info_error(lines: &[String]) -> String {
    match UfsStorageInfo::parse(lines) {
        Err(EdlError::StorageInfo(message)) => message,
        other => panic!("expected a storage info error, got {other:?}"),
    }
}

#[test]
fn recorded_data_lun_response_gives_its_blocks_and_block_bytes() {
    let info = UfsStorageInfo::parse(&lines(LUN0_RESPONSE)).expect("recorded response");

    assert_eq!(
        info,
        UfsStorageInfo {
            total_blocks: 0x66d000,
            block_bytes: 4096,
        }
    );
}

#[test]
fn recorded_boot_lun_response_gives_its_blocks() {
    let info = UfsStorageInfo::parse(&lines(LUN1_RESPONSE)).expect("recorded response");

    assert_eq!(info.total_blocks, 0x800);
    assert_eq!(info.block_bytes, 4096);
}

#[test]
fn a_field_that_repeats_with_the_same_value_is_accepted() {
    let mut repeated = lines(LUN1_RESPONSE);
    repeated.extend(lines(LUN1_RESPONSE));

    let info = UfsStorageInfo::parse(&repeated).expect("same values");

    assert_eq!(info.total_blocks, 0x800);
}

#[test]
fn a_field_line_with_a_prefix_is_accepted() {
    let prefixed = replaced(
        LUN1_RESPONSE,
        "UFS LU Total Blocks: 0x800",
        "INFO: UFS LU Total Blocks: 0x800",
    );

    assert_eq!(
        UfsStorageInfo::parse(&prefixed)
            .expect("prefix")
            .total_blocks,
        0x800
    );
}

#[test]
fn a_block_size_other_than_4096_bytes_is_rejected() {
    let other_size = replaced(
        LUN0_RESPONSE,
        "UFS Logical Block Size: 0xc",
        "UFS Logical Block Size: 0x9",
    );

    assert!(storage_info_error(&other_size).contains("0x9"));
}

#[test]
fn a_block_size_written_as_bytes_is_rejected() {
    let in_bytes = replaced(
        LUN0_RESPONSE,
        "UFS Logical Block Size: 0xc",
        "UFS Logical Block Size: 0x1000",
    );

    assert!(storage_info_error(&in_bytes).contains("0x1000"));
}

#[test]
fn a_response_without_total_blocks_is_rejected() {
    let without: Vec<String> = lines(LUN0_RESPONSE)
        .into_iter()
        .filter(|line| !line.contains("UFS LU Total Blocks:"))
        .collect();

    assert!(storage_info_error(&without).contains("UFS LU Total Blocks:"));
}

#[test]
fn a_response_without_block_size_is_rejected() {
    let without: Vec<String> = lines(LUN0_RESPONSE)
        .into_iter()
        .filter(|line| !line.contains("UFS Logical Block Size:"))
        .collect();

    assert!(storage_info_error(&without).contains("UFS Logical Block Size:"));
}

#[test]
fn a_field_that_repeats_with_another_value_is_rejected() {
    let mut mixed = lines(LUN0_RESPONSE);
    mixed.extend(lines(LUN1_RESPONSE));

    assert!(storage_info_error(&mixed).contains("different values"));
}

#[test]
fn zero_total_blocks_are_rejected() {
    let zero = replaced(
        LUN1_RESPONSE,
        "UFS LU Total Blocks: 0x800",
        "UFS LU Total Blocks: 0x0",
    );

    assert!(storage_info_error(&zero).contains("is 0"));
}

#[test]
fn values_that_are_not_prefixed_hexadecimal_numbers_are_rejected() {
    for value in ["800", "0x", "0x+800", "0x80g", "= ", "0x10000000000000000"] {
        let malformed = replaced(
            LUN1_RESPONSE,
            "UFS LU Total Blocks: 0x800",
            &format!("UFS LU Total Blocks: {value}"),
        );

        assert!(
            UfsStorageInfo::parse(&malformed).is_err(),
            "value {value:?} must be rejected"
        );
    }
}

#[test]
fn the_logical_block_count_field_does_not_stand_for_total_blocks() {
    let without: Vec<String> = lines(LUN1_RESPONSE)
        .into_iter()
        .filter(|line| !line.contains("UFS LU Total Blocks:"))
        .collect();
    assert!(
        without
            .iter()
            .any(|line| line.contains("UFS Logical Block Count:"))
    );

    assert!(UfsStorageInfo::parse(&without).is_err());
}
