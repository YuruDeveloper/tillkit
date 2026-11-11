use front2::apply_script::{CHECK_PASSED, SCRIPT_FILE, STATE_FILE, render};
use front2::digest::sha256_bytes;
use front2::profiles::toss_front2::PROFILE;
use front2::{
    ImageRecord, PartitionTarget, PlannedWrite, RootAccess, SupportCheck, SupportOverride,
    SupportReport, WritePlan, WritePurpose,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FINGERPRINT: &str = "ro.vendor.build.fingerprint";
const OTHER_BUILD: &str = "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/476:user/dev-keys";
const FAKE_SLOT: &str = "FRONT2_FAKE_RO_BOOT_SLOT_SUFFIX";
const FAKE_BOOT_COMPLETED: &str = "FRONT2_FAKE_SYS_BOOT_COMPLETED";
const FAKE_FINGERPRINT: &str = "FRONT2_FAKE_RO_VENDOR_BUILD_FINGERPRINT";
const ID_STUB: &str = "#!/bin/sh\necho 0\n";
const BLOCKDEV_STUB: &str =
    "#!/bin/sh\ncase \"$1\" in\n  --getsize64) stat -c %s \"$2\" ;;\n  --getro) echo 0 ;;\nesac\n";
const DD_STUB: &str = "#!/bin/sh\nCOUNT=$(cat \"$FRONT2_DD_COUNT\" 2>/dev/null || echo 0)\nCOUNT=$((COUNT + 1))\necho \"$COUNT\" >\"$FRONT2_DD_COUNT\"\nif [ \"$COUNT\" -ge \"${FRONT2_DD_FAIL_FROM:-999}\" ] && [ \"$COUNT\" -le \"${FRONT2_DD_FAIL_TO:-999}\" ]; then\n  exit 1\nfi\n\"$FRONT2_REAL_DD\" \"$@\" || exit 1\nif [ \"$COUNT\" = \"${FRONT2_DD_CORRUPT_AT:-0}\" ]; then\n  for ARGUMENT in \"$@\"; do\n    case \"$ARGUMENT\" in of=*) printf X | \"$FRONT2_REAL_DD\" of=\"${ARGUMENT#of=}\" bs=1 count=1 conv=notrunc ;; esac\n  done\nfi\n";

struct Stage {
    directory: tempfile::TempDir,
    shell: String,
    real_dd: String,
    plan: WritePlan,
    old: [Vec<u8>; 2],
    new: [Vec<u8>; 2],
}

type Breaker = fn(&Stage);
type Environment = &'static [(&'static str, &'static str)];

fn getprop_stub() -> String {
    let mut stub = String::from("#!/bin/sh\n");
    stub.push_str("[ \"$1\" != \"${FRONT2_GETPROP_FAILS:-}\" ] || exit 1\n");
    stub.push_str("[ \"$1\" != \"${FRONT2_GETPROP_EMPTY:-}\" ] || exit 0\n");
    stub.push_str("case \"$1\" in\n");
    for (property, expected) in SupportReport::expected_properties(&PROFILE) {
        let variable = property.replace('.', "_").to_ascii_uppercase();
        stub.push_str(&format!(
            "  {property}) echo \"${{FRONT2_FAKE_{variable}:-{expected}}}\" ;;\n"
        ));
    }
    stub.push_str("esac\n");
    stub
}

fn find_shell() -> Option<String> {
    let shell = std::env::var("FRONT2_TEST_SH").unwrap_or_else(|_| "sh".to_string());
    let probe = Command::new(&shell).args(["-c", "exit 0"]).output();
    probe
        .is_ok_and(|output| output.status.success())
        .then_some(shell)
}

fn shell_text(shell: &str, script: &str, argument: &Path) -> String {
    let output = Command::new(shell)
        .args(["-c", script, "front2-test"])
        .arg(argument)
        .output()
        .expect("run shell");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn pattern(seed: u8, bytes: usize) -> Vec<u8> {
    (0..bytes).map(|index| seed ^ (index % 251) as u8).collect()
}

fn write_stub(directory: &Path, name: &str, text: &str) {
    let path = directory.join(name);
    std::fs::write(&path, text).expect("write stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
}

fn planned(stage: &Path, shell: &str, base: &str, old: &[u8], new: &[u8]) -> PlannedWrite {
    let label = format!("{base}_b");
    let device = stage.join("dev").join(&label);
    std::fs::write(&device, old).expect("write fake partition");
    let staged_new = format!("{base}.new.img");
    let staged_old = format!("{base}.old.img");
    std::fs::write(stage.join("stage").join(&staged_new), new).expect("stage new");
    std::fs::write(stage.join("stage").join(&staged_old), old).expect("stage old");
    let record = |data: &[u8]| ImageRecord {
        file: format!("{base}.img"),
        bytes: data.len() as u64,
        sha256: sha256_bytes(data),
    };
    PlannedWrite {
        target: PartitionTarget {
            label,
            by_name: slashes(&device),
            node: shell_text(shell, "readlink -f \"$1\"", &device),
            bytes: old.len() as u64,
        },
        new_image: record(new),
        old_image: record(old),
        staged_new,
        staged_old,
        state_while_writing: format!("WRITING_{}", base.to_ascii_uppercase()),
    }
}

fn stage() -> Option<Stage> {
    stage_accepting(SupportOverride::none())
}

fn stage_accepting(support_override: SupportOverride) -> Option<Stage> {
    let Some(shell) = find_shell() else {
        eprintln!("skipped: no POSIX sh found (set FRONT2_TEST_SH to run this test)");
        return None;
    };
    let directory = tempfile::tempdir().expect("tempdir");
    for name in ["dev", "stage", "stubs"] {
        std::fs::create_dir(directory.path().join(name)).expect("create directory");
    }
    let real_dd = shell_text(&shell, "command -v dd", Path::new(""));
    let stubs = directory.path().join("stubs");
    write_stub(&stubs, "getprop", &getprop_stub());
    write_stub(&stubs, "id", ID_STUB);
    write_stub(&stubs, "blockdev", BLOCKDEV_STUB);
    write_stub(&stubs, "dd", DD_STUB);
    let old = [pattern(1, 8 * 4096), pattern(2, 4 * 4096)];
    let new = [pattern(3, 8 * 4096), pattern(4, 4 * 4096)];
    let writes = vec![
        planned(directory.path(), &shell, "init_boot", &old[0], &new[0]),
        planned(directory.path(), &shell, "vbmeta", &old[1], &new[1]),
    ];
    let mut plan = WritePlan {
        schema: front2::write_plan::PLAN_SCHEMA,
        id: "test".into(),
        created_unix: 0,
        purpose: WritePurpose::ApplyCandidate,
        profile: PROFILE.id.into(),
        support_override,
        new_pair: "new".into(),
        old_pair: "old".into(),
        slot_suffix: "_b".into(),
        device_state: "locked".into(),
        verified_boot_state: "green".into(),
        root_access: RootAccess::AdbRoot,
        remote_stage: slashes(&directory.path().join("stage")),
        writes,
        top_rollback_index: 0,
        script_sha256: String::new(),
        wipes_user_data: false,
        changes_active_slot: false,
        changes_bootloader_lock: false,
        restore_tested_on_device: false,
        power_loss_recovery: String::new(),
    };
    let script = render(&plan, &PROFILE);
    plan.script_sha256 = sha256_bytes(script.as_bytes());
    std::fs::write(directory.path().join("stage").join(SCRIPT_FILE), script).expect("script");
    Some(Stage {
        directory,
        shell,
        real_dd,
        plan,
        old,
        new,
    })
}

impl Stage {
    fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }

    fn run(&self, argument: &str, environment: &[(&str, &str)]) -> Output {
        let separator = if cfg!(windows) { ";" } else { ":" };
        let search_path = format!(
            "{}{separator}{}",
            self.path("stubs").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut command = Command::new(&self.shell);
        command
            .arg(slashes(&self.path("stage").join(SCRIPT_FILE)))
            .arg(argument)
            .env("PATH", search_path)
            .env("FRONT2_REAL_DD", &self.real_dd)
            .env("FRONT2_DD_COUNT", slashes(&self.path("dd-count")));
        for (key, value) in environment {
            command.env(key, value);
        }
        command.output().expect("run apply script")
    }

    fn targets(&self) -> [Vec<u8>; 2] {
        [
            std::fs::read(self.path("dev").join("init_boot_b")).expect("init_boot"),
            std::fs::read(self.path("dev").join("vbmeta_b")).expect("vbmeta"),
        ]
    }

    fn state(&self) -> String {
        std::fs::read_to_string(self.path("stage").join(STATE_FILE))
            .unwrap_or_default()
            .trim()
            .to_string()
    }
}

#[test]
fn check_passes_without_touching_the_targets() {
    let Some(stage) = stage() else { return };
    let output = stage.run("--check", &[]);
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains(CHECK_PASSED));
    assert_eq!(stage.targets(), stage.old);
    assert_eq!(stage.state(), "");
    assert_eq!(stage.run("--other", &[]).status.code(), Some(2));
    assert_eq!(stage.targets(), stage.old);
}

#[test]
fn apply_writes_both_targets_and_refuses_a_second_run() {
    let Some(stage) = stage() else { return };
    let output = stage.run("--apply", &[]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(stage.targets(), stage.new);
    assert_eq!(stage.state(), "VERIFIED_PENDING_REBOOT");
    assert!(!stage.run("--apply", &[]).status.success());
    assert!(!stage.run("--check", &[]).status.success());
    assert_eq!(stage.targets(), stage.new);
    assert_eq!(stage.state(), "VERIFIED_PENDING_REBOOT");
}

#[test]
fn each_broken_precondition_stops_before_any_write() {
    let no_change: Environment = &[];
    let breakers: [(&str, Breaker, Environment); 7] = [
        ("wrong slot", |_| {}, &[(FAKE_SLOT, "_a")]),
        ("boot not completed", |_| {}, &[(FAKE_BOOT_COMPLETED, "0")]),
        ("another build", |_| {}, &[(FAKE_FINGERPRINT, OTHER_BUILD)]),
        (
            "target differs from the old image",
            |stage| {
                let path = stage.path("dev").join("vbmeta_b");
                let mut data = std::fs::read(&path).expect("read");
                data[5] ^= 1;
                std::fs::write(&path, data).expect("write");
            },
            no_change,
        ),
        (
            "staged new image is corrupt",
            |stage| {
                let path = stage.path("stage").join("init_boot.new.img");
                let mut data = std::fs::read(&path).expect("read");
                data[5] ^= 1;
                std::fs::write(&path, data).expect("write");
            },
            no_change,
        ),
        (
            "staged old image is missing",
            |stage| {
                std::fs::remove_file(stage.path("stage").join("vbmeta.old.img")).expect("remove");
            },
            no_change,
        ),
        (
            "target size changed",
            |stage| {
                let path = stage.path("dev").join("init_boot_b");
                let mut data = std::fs::read(&path).expect("read");
                data.extend_from_slice(&[0; 4096]);
                std::fs::write(&path, data).expect("write");
            },
            no_change,
        ),
    ];
    for (name, breaker, environment) in breakers {
        let Some(stage) = stage() else { return };
        breaker(&stage);
        let before = stage.targets();
        for argument in ["--check", "--apply"] {
            assert!(!stage.run(argument, environment).status.success(), "{name}");
        }
        assert_eq!(stage.targets(), before, "{name}");
        assert_eq!(stage.state(), "", "{name}");
        assert!(!stage.path("dd-count").exists(), "{name}: dd ran");
    }
}

#[test]
fn doc_1054_an_accepted_value_must_be_the_same_when_the_script_runs() {
    let other_build = SupportCheck::exact(
        FINGERPRINT,
        "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/475:user/dev-keys",
        OTHER_BUILD,
    );
    let accepted = SupportOverride {
        accepted: vec![other_build],
    };
    let Some(stage) = stage_accepting(accepted) else {
        return;
    };
    let same_device: Environment = &[(FAKE_FINGERPRINT, OTHER_BUILD)];
    for argument in ["--check", "--apply"] {
        assert!(!stage.run(argument, &[]).status.success(), "{argument}");
    }
    assert_eq!(stage.targets(), stage.old);
    assert!(!stage.path("dd-count").exists(), "dd ran");
    assert!(stage.run("--check", same_device).status.success());
    assert!(stage.run("--apply", same_device).status.success());
    assert_eq!(stage.targets(), stage.new);
}

#[test]
fn doc_1055_a_failed_property_read_is_not_an_accepted_empty_value() {
    let no_build = SupportCheck::exact(
        FINGERPRINT,
        "TOSSPLACE/toss_front2/toss_front2:13/TKQ1.230110.001/475:user/dev-keys",
        "",
    );
    let accepted = SupportOverride {
        accepted: vec![no_build],
    };
    let Some(stage) = stage_accepting(accepted) else {
        return;
    };
    let failed_read: Environment = &[("FRONT2_GETPROP_FAILS", FINGERPRINT)];
    let empty_value: Environment = &[("FRONT2_GETPROP_EMPTY", FINGERPRINT)];
    for argument in ["--check", "--apply"] {
        let output = stage.run(argument, failed_read);
        assert!(!output.status.success(), "{argument}");
    }
    assert_eq!(stage.targets(), stage.old);
    assert!(!stage.path("dd-count").exists(), "dd ran");
    assert!(stage.run("--check", empty_value).status.success());
    assert!(stage.run("--apply", empty_value).status.success());
    assert_eq!(stage.targets(), stage.new);
}

#[test]
fn failed_or_corrupt_second_write_restores_the_old_pair() {
    for environment in [
        [("FRONT2_DD_FAIL_FROM", "2"), ("FRONT2_DD_FAIL_TO", "2")],
        [("FRONT2_DD_CORRUPT_AT", "2"), ("FRONT2_DD_FAIL_TO", "0")],
        [("FRONT2_DD_CORRUPT_AT", "1"), ("FRONT2_DD_FAIL_TO", "0")],
    ] {
        let Some(stage) = stage() else { return };
        let output = stage.run("--apply", &environment);
        assert!(!output.status.success(), "{environment:?}");
        assert_eq!(stage.state(), "ROLLBACK_VERIFIED", "{environment:?}");
        assert_eq!(stage.targets(), stage.old, "{environment:?}");
    }
}

#[test]
fn failed_rollback_is_reported_as_do_not_reboot() {
    let Some(stage) = stage() else { return };
    let output = stage.run("--apply", &[("FRONT2_DD_FAIL_FROM", "2")]);
    assert!(!output.status.success());
    assert_eq!(stage.state(), "ROLLBACK_FAILED_DO_NOT_REBOOT");
    assert_eq!(stage.targets()[0], stage.new[0]);
    assert_eq!(stage.targets()[1], stage.old[1]);
}

#[test]
fn rendered_script_matches_the_plan_digest() {
    let Some(stage) = stage() else { return };
    assert!(stage.plan.script().is_ok());
    let mut edited = stage.plan.clone();
    edited.writes[0].target.node.push('x');
    assert!(edited.script().is_err());
}
