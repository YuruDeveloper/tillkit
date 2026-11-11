use front2::apply_script::render;
use front2::digest::sha256_bytes;
use front2::{
    ImageRecord, PartitionTarget, PlannedWrite, RootAccess, SupportCheck, SupportOverride,
    WritePlan, WritePurpose,
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

#[test]
fn the_rendered_script_is_the_same_as_before_the_profile_change() {
    let plain = render(&fixed_plan(SupportOverride::none()));
    let accepting = render(&accepting_plan());
    assert_eq!(sha256_bytes(plain.as_bytes()), PLAIN_SCRIPT_SHA256);
    assert_eq!(sha256_bytes(accepting.as_bytes()), ACCEPTING_SCRIPT_SHA256);
}
