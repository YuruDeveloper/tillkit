use front2::apply_script::render;
use front2::digest::sha256_bytes;
use front2::profiles::toss_front2::PROFILE;
use front2::{
    DeviceProfile, Front2Error, ImageRecord, PartitionTarget, PlannedWrite, RootAccess,
    SupportCheck, SupportOverride, WritePlan, WritePurpose,
};

const PLAIN_SCRIPT_SHA256: &str =
    "450df869906fb2394ca7cf391694349dbbbd901bed1eef300ab4cb20eab01f58";
const ACCEPTING_SCRIPT_SHA256: &str =
    "8739d882be1501c519849cc51845874b6bcab974cb8a4ed181ad01fadb20e123";

fn planned(base: &str, node: &str, bytes: u64) -> PlannedWrite {
    let record = |seed: &str| ImageRecord {
        file: format!("{base}.img"),
        bytes,
        sha256: sha256_bytes(format!("{base} {seed}").as_bytes()),
    };
    PlannedWrite {
        target: PartitionTarget {
            label: format!("{base}_b"),
            by_name: format!("/dev/block/by-name/{base}_b"),
            node: node.to_string(),
            bytes,
        },
        new_image: record("new"),
        old_image: record("old"),
        staged_new: format!("{base}.new.img"),
        staged_old: format!("{base}.old.img"),
        state_while_writing: format!("WRITING_{}", base.to_ascii_uppercase()),
    }
}

fn fixed_plan(support_override: SupportOverride) -> WritePlan {
    WritePlan {
        schema: front2::write_plan::PLAN_SCHEMA,
        id: "1762900000-write".into(),
        created_unix: 1_762_900_000,
        purpose: WritePurpose::ApplyCandidate,
        profile: "toss-front2".into(),
        support_override,
        new_pair: "1762900000-candidate".into(),
        old_pair: "1762800000-readback".into(),
        slot_suffix: "_b".into(),
        device_state: "locked".into(),
        verified_boot_state: "green".into(),
        root_access: RootAccess::Su,
        remote_stage: "/data/local/tmp/front2-write-1762900000-write".into(),
        writes: vec![
            planned("init_boot", "/dev/block/sda21", 8 * 1024 * 1024),
            planned("vbmeta", "/dev/block/sda27", 64 * 1024),
        ],
        top_rollback_index: 0,
        script_sha256: String::new(),
        wipes_user_data: false,
        changes_active_slot: false,
        changes_bootloader_lock: false,
        restore_tested_on_device: false,
        power_loss_recovery: front2::write_plan::POWER_LOSS_RECOVERY.into(),
    }
}

fn accepting_plan() -> WritePlan {
    fixed_plan(SupportOverride {
        accepted: vec![SupportCheck::exact(
            "ro.vendor.build.fingerprint",
            "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/475:user/dev-keys",
            "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/476:user/dev-keys",
        )],
    })
}

static OTHER_DEVICE: DeviceProfile = DeviceProfile {
    id: "other-device",
    name: "Other device",
    identity: &[("ro.product.vendor.device", "other")],
    slot_suffix: "_a",
    device_state: "locked",
    verified_boot_state: "green",
    storage_block_bytes: 512,
    top_vbmeta_algorithm: "SHA256_RSA4096",
    top_vbmeta_key_spec: "testkey_rsa4096",
    remote_stage_parent: "/data/local/tmp",
    supports_gsi: false,
};

#[test]
fn a_plan_loads_only_with_the_profile_that_made_it() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut plan = fixed_plan(SupportOverride::none());
    plan.script_sha256 = sha256_bytes(render(&plan, &PROFILE).as_bytes());
    let digest = plan.save(directory.path()).expect("save the plan");
    let (loaded, loaded_digest) = WritePlan::load(directory.path(), &PROFILE).expect("load");
    assert_eq!((loaded, loaded_digest), (plan, digest));
    let refused = WritePlan::load(directory.path(), &OTHER_DEVICE);
    assert!(matches!(refused, Err(Front2Error::Plan(_))), "{refused:?}");
}

#[test]
fn a_plan_with_an_unknown_profile_has_no_script() {
    let mut plan = fixed_plan(SupportOverride::none());
    plan.script_sha256 = sha256_bytes(render(&plan, &PROFILE).as_bytes());
    assert!(plan.script().is_ok());
    plan.profile = OTHER_DEVICE.id.into();
    assert!(matches!(plan.script(), Err(Front2Error::Plan(_))));
}

#[test]
fn the_storage_unit_of_the_profile_sets_the_dd_block_size() {
    let plan = fixed_plan(SupportOverride::none());
    let script = render(&plan, &OTHER_DEVICE);
    assert!(script.contains("bs=512 count=16384 conv=fsync"), "{script}");
    assert!(script.contains("[ \"$(getprop ro.product.vendor.device)\" = 'other' ]"));
    assert!(!script.contains("ro.soc.model"));
}

#[test]
fn the_rendered_script_is_the_same_as_before_the_profile_change() {
    let plain = render(&fixed_plan(SupportOverride::none()), &PROFILE);
    let accepting = render(&accepting_plan(), &PROFILE);
    assert_eq!(sha256_bytes(plain.as_bytes()), PLAIN_SCRIPT_SHA256);
    assert_eq!(sha256_bytes(accepting.as_bytes()), ACCEPTING_SCRIPT_SHA256);
}
