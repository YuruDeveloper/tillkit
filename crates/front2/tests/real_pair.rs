use front2::candidate::build_signed_candidate;
use front2::digest::sha256_file;
use front2::profiles::toss_front2::PROFILE;
use front2::{PairKind, PairRequest, PairStore};
use std::path::PathBuf;

const READBACK: &str = "1-readback";
const CANDIDATE: &str = "2-candidate";

#[test]
fn candidate_from_a_real_readback_when_available() {
    let Some(source) = std::env::var_os("FRONT2_REAL_PAIR_DIR").map(PathBuf::from) else {
        eprintln!("skipped: FRONT2_REAL_PAIR_DIR is not set");
        return;
    };
    let directory = tempfile::tempdir().expect("tempdir");
    let store = PairStore::new(directory.path().join("pairs"), &PROFILE);
    let readback_directory = store.begin(READBACK).expect("begin");
    for (from, to) in [
        ("current-init_boot_b.img", "init_boot.img"),
        ("current-vbmeta_b.img", "vbmeta.img"),
    ] {
        std::fs::copy(source.join(from), readback_directory.join(to)).expect("copy readback");
    }
    let readback = store
        .commit(READBACK, PairKind::DeviceReadback, 1, "_b", None, "real")
        .expect("commit readback");
    readback.avb.require_trust(&PROFILE).expect("front2 trust");
    front2::pair_verification::verify_pair_images(&readback_directory, &PROFILE)
        .expect("the real readback passes the write boundary verification");
    println!("readback avb: {:#?}", readback.avb);

    let request = PairRequest {
        id: CANDIDATE,
        created_unix: 2,
        note: "real",
    };
    let candidate = build_signed_candidate(
        &store,
        READBACK,
        &source.join("magisk-patched-init_boot_b.img"),
        request,
    )
    .expect("candidate");
    println!("candidate avb: {:#?}", candidate.avb);
    println!("candidate init_boot: {:?}", candidate.init_boot);
    println!("candidate vbmeta: {:?}", candidate.vbmeta);
    assert_eq!(
        candidate.avb.top_rollback_index,
        readback.avb.top_rollback_index
    );

    let reference_init_boot = source.join("signed").join("init_boot.img");
    let reference_vbmeta = source.join("signed").join("vbmeta.img");
    if reference_init_boot.exists() && reference_vbmeta.exists() {
        let reference = front2::AvbSummary::inspect(&reference_init_boot, &reference_vbmeta)
            .expect("reference summary");
        let payload = candidate.avb.init_boot_payload_bytes as usize;
        let built = std::fs::read(
            store
                .directory(CANDIDATE)
                .expect("directory")
                .join("init_boot.img"),
        )
        .expect("candidate image");
        let verified = std::fs::read(&reference_init_boot).expect("reference image");
        assert_eq!(reference, candidate.avb, "reference summary");
        assert!(
            built[..payload] == verified[..payload],
            "payload bytes differ from the reference"
        );
        println!("reference summary equals candidate summary: true; payload bytes identical: true");
        front2::top_signature::verify_top_signature(&reference_vbmeta, "testkey_rsa4096")
            .expect("reference signature");
    }

    for (name, record) in [
        ("init_boot.img", &candidate.init_boot),
        ("vbmeta.img", &candidate.vbmeta),
    ] {
        let reference = source.join("signed").join(name);
        if !reference.exists() {
            continue;
        }
        let (bytes, sha256) = sha256_file(&reference).expect("reference digest");
        println!(
            "{name}: reference {bytes} bytes {sha256}, identical to candidate: {}",
            sha256 == record.sha256
        );
    }
}
