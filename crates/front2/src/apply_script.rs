use crate::planned_write::PlannedWrite;
use crate::profile::BLOCK_SIZE;
use crate::root_access::quote;
use crate::support_report::SupportReport;
use crate::write_plan::WritePlan;
use std::fmt::Write;

pub const SCRIPT_FILE: &str = "apply.sh";
pub const STATE_FILE: &str = "apply-state.txt";
pub const LOG_FILE: &str = "apply.log";
pub const CHECK_PASSED: &str = "ALL_PRECONDITIONS_PASSED";

pub fn render(plan: &WritePlan) -> String {
    let mut script = String::new();
    let mut line = |text: String| {
        script.push_str(&text);
        script.push('\n');
    };
    line("#!/system/bin/sh".into());
    line("set -eu".into());
    line("umask 077".into());
    line(format!("BASE={}", quote(&plan.remote_stage)));
    line(format!("STATE=\"$BASE/{STATE_FILE}\""));
    line("file_sha256() { sha256sum \"$1\" | cut -d ' ' -f 1; }".into());
    line("[ \"$#\" = 1 ]".into());
    line("case \"$1\" in --check|--apply) ;; *) exit 2 ;; esac".into());
    line("[ \"$(id -u)\" = 0 ]".into());
    for (property, expected) in SupportReport::expected_properties() {
        let value = plan.support_override.observed(property).unwrap_or(expected);
        line(format!("[ \"$(getprop {property})\" = {} ]", quote(value)));
    }
    for (index, write) in plan.writes.iter().enumerate() {
        line(preconditions(index, write));
    }
    line("[ ! -e \"$STATE\" ]".into());
    line("if [ \"$1\" = --check ]; then".into());
    line(format!("  echo {CHECK_PASSED}"));
    line("  exit 0".into());
    line("fi".into());
    line("STARTED=0".into());
    line("cleanup() {".into());
    line("  RESULT=$?".into());
    line("  trap - EXIT".into());
    line("  if [ \"$RESULT\" != 0 ] && [ \"$STARTED\" = 1 ]; then".into());
    line("    set +e".into());
    line("    echo APPLY_FAILED_RESTORING >\"$STATE\"".into());
    for (index, write) in plan.writes.iter().enumerate() {
        line(format!("    {}", copy(index, write, &write.staged_old)));
    }
    line("    sync".into());
    line(format!("    if {}; then", all_hold_old(&plan.writes)));
    line("      echo ROLLBACK_VERIFIED >\"$STATE\"".into());
    line("    else".into());
    line("      echo ROLLBACK_FAILED_DO_NOT_REBOOT >\"$STATE\"".into());
    line("    fi".into());
    line("  fi".into());
    line("  exit \"$RESULT\"".into());
    line("}".into());
    line("trap cleanup EXIT".into());
    line("STARTED=1".into());
    for (index, write) in plan.writes.iter().enumerate() {
        line(format!("echo {} >\"$STATE\"", write.state_while_writing));
        line(copy(index, write, &write.staged_new));
        line(format!(
            "[ \"$(file_sha256 \"$TARGET{index}\")\" = {} ]",
            write.new_image.sha256
        ));
    }
    line("sync".into());
    line("echo VERIFIED_PENDING_REBOOT >\"$STATE\"".into());
    line("STARTED=0".into());
    line("trap - EXIT".into());
    script
}

fn preconditions(index: usize, write: &PlannedWrite) -> String {
    let bytes = write.target.bytes;
    let mut text = String::new();
    let _ = writeln!(text, "TARGET{index}={}", quote(&write.target.by_name));
    let _ = writeln!(
        text,
        "[ \"$(readlink -f \"$TARGET{index}\")\" = {} ]",
        quote(&write.target.node)
    );
    let _ = writeln!(
        text,
        "[ \"$(blockdev --getsize64 \"$TARGET{index}\")\" = {bytes} ]"
    );
    let _ = writeln!(text, "[ \"$(blockdev --getro \"$TARGET{index}\")\" = 0 ]");
    for (staged, sha256) in [
        (&write.staged_new, &write.new_image.sha256),
        (&write.staged_old, &write.old_image.sha256),
    ] {
        let _ = writeln!(text, "[ \"$(stat -c %s \"$BASE/{staged}\")\" = {bytes} ]");
        let _ = writeln!(text, "[ \"$(file_sha256 \"$BASE/{staged}\")\" = {sha256} ]");
    }
    let _ = write!(
        text,
        "[ \"$(file_sha256 \"$TARGET{index}\")\" = {} ]",
        write.old_image.sha256
    );
    text
}

fn copy(index: usize, write: &PlannedWrite, staged: &str) -> String {
    format!(
        "dd if=\"$BASE/{staged}\" of=\"$TARGET{index}\" bs={BLOCK_SIZE} count={} conv=fsync",
        write.target.bytes / BLOCK_SIZE
    )
}

fn all_hold_old(writes: &[PlannedWrite]) -> String {
    writes
        .iter()
        .enumerate()
        .map(|(index, write)| {
            format!(
                "[ \"$(file_sha256 \"$TARGET{index}\")\" = {} ]",
                write.old_image.sha256
            )
        })
        .collect::<Vec<_>>()
        .join(" && ")
}
