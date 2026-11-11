use crate::apply_outcome::ApplyOutcome;
use crate::apply_script::{CHECK_PASSED, LOG_FILE, SCRIPT_FILE, STATE_FILE};
use crate::apply_state::ApplyState;
use crate::authorization::Authorization;
use crate::device_shell::DeviceShell;
use crate::error::{Front2Error, Result};
use crate::pair_store::PairStore;
use crate::partition_target::device_sha256;
use crate::root_access::{RootAccess, quote};
use crate::scope::Scope;
use crate::write_plan::WritePlan;
use std::fs;
use std::path::Path;

pub const STATE_POLL_LIMIT: u32 = 120;

pub fn stage(
    shell: &mut dyn DeviceShell,
    store: &PairStore,
    plan: &WritePlan,
    plan_directory: &Path,
) -> Result<()> {
    let script = plan.script()?;
    let local_script = plan_directory.join(SCRIPT_FILE);
    fs::write(&local_script, script.as_bytes())?;
    shell.run_checked(&format!("mkdir {}", quote(&plan.remote_stage)))?;
    let new_directory = store.directory(&plan.new_pair)?;
    let old_directory = store.directory(&plan.old_pair)?;
    for write in &plan.writes {
        write.new_image.verify(&new_directory)?;
        write.old_image.verify(&old_directory)?;
        let uploads = [
            (
                new_directory.join(&write.new_image.file),
                &write.staged_new,
                &write.new_image.sha256,
            ),
            (
                old_directory.join(&write.old_image.file),
                &write.staged_old,
                &write.old_image.sha256,
            ),
        ];
        for (local, staged, sha256) in uploads {
            upload(
                shell,
                &local,
                &format!("{}/{staged}", plan.remote_stage),
                sha256,
            )?;
        }
    }
    upload(
        shell,
        &local_script,
        &format!("{}/{SCRIPT_FILE}", plan.remote_stage),
        &plan.script_sha256,
    )
}

fn upload(shell: &mut dyn DeviceShell, local: &Path, remote: &str, sha256: &str) -> Result<()> {
    shell.push(local, remote)?;
    let staged = device_sha256(shell, RootAccess::AdbRoot, remote)?;
    if staged != sha256 {
        return Err(Front2Error::Verification(format!(
            "{remote} has SHA-256 {staged} after upload, expected {sha256}"
        )));
    }
    Ok(())
}

fn require_staged_script(shell: &mut dyn DeviceShell, plan: &WritePlan) -> Result<()> {
    let path = script_path(plan);
    let staged = device_sha256(shell, plan.root_access, &path)?;
    if staged != plan.script_sha256 {
        return Err(Front2Error::Verification(format!(
            "{path} has SHA-256 {staged} before it runs, the plan records {}",
            plan.script_sha256
        )));
    }
    Ok(())
}

pub fn check(shell: &mut dyn DeviceShell, plan: &WritePlan) -> Result<()> {
    require_staged_script(shell, plan)?;
    let command = format!("sh {} --check", quote(&script_path(plan)));
    let output = shell.run(&plan.root_access.command(&command))?;
    if !output.succeeded() || !output.stdout.contains(CHECK_PASSED) {
        return Err(Front2Error::Verification(format!(
            "the device precondition check exited with status {}: {}",
            output.status,
            output.text()
        )));
    }
    Ok(())
}

pub fn apply(
    shell: &mut dyn DeviceShell,
    plan: &WritePlan,
    plan_sha256: &str,
    authorization: &Authorization,
    wait: &mut dyn FnMut(),
) -> Result<ApplyOutcome> {
    authorization.require(plan_sha256, Scope::WritePair)?;
    check(shell, plan)?;
    require_staged_script(shell, plan)?;
    let stage = quote(&plan.remote_stage);
    let start = format!(
        "nohup sh {} --apply >{stage}/{LOG_FILE} 2>&1 </dev/null &",
        quote(&script_path(plan))
    );
    shell.run_checked(&plan.root_access.command(&start))?;
    let mut state = ApplyState::NotStarted;
    for _ in 0..STATE_POLL_LIMIT {
        state = read_state(shell, plan)?;
        if state.is_final() {
            break;
        }
        wait();
    }
    let device_log = shell
        .run(&plan.root_access.command(&format!("cat {stage}/{LOG_FILE}")))?
        .stdout;
    let readback_sha256 = read_targets(shell, plan)?;
    let readback_matches_new_pair = matches_new_pair(plan, &readback_sha256);
    Ok(ApplyOutcome {
        plan_id: plan.id.clone(),
        plan_sha256: plan_sha256.to_string(),
        state,
        device_log,
        readback_sha256,
        readback_matches_new_pair,
        reboot_requested: false,
    })
}

pub fn reboot(
    shell: &mut dyn DeviceShell,
    plan: &WritePlan,
    plan_sha256: &str,
    authorization: &Authorization,
    outcome: &mut ApplyOutcome,
) -> Result<()> {
    authorization.require(plan_sha256, Scope::Reboot)?;
    if outcome.plan_sha256 != plan_sha256 || !outcome.ready_for_reboot() {
        return Err(Front2Error::Verification(format!(
            "the write result is {:?} with readback match {}; reboot is not allowed",
            outcome.state, outcome.readback_matches_new_pair
        )));
    }
    if read_state(shell, plan)? != ApplyState::VerifiedPendingReboot {
        return Err(Front2Error::Verification(
            "the device state file no longer reports a verified write".into(),
        ));
    }
    let readback = read_targets(shell, plan)?;
    if !matches_new_pair(plan, &readback) {
        return Err(Front2Error::Verification(format!(
            "the partitions hold {readback:?} immediately before reboot, not the new pair"
        )));
    }
    outcome.reboot_requested = true;
    shell.reboot()
}

fn script_path(plan: &WritePlan) -> String {
    format!("{}/{SCRIPT_FILE}", plan.remote_stage)
}

fn read_state(shell: &mut dyn DeviceShell, plan: &WritePlan) -> Result<ApplyState> {
    let command = format!(
        "cat {}/{STATE_FILE} 2>/dev/null || true",
        quote(&plan.remote_stage)
    );
    Ok(ApplyState::parse(
        &shell.run(&plan.root_access.command(&command))?.stdout,
    ))
}

fn read_targets(shell: &mut dyn DeviceShell, plan: &WritePlan) -> Result<Vec<String>> {
    plan.writes
        .iter()
        .map(|write| write.target.sha256(shell, plan.root_access))
        .collect()
}

fn matches_new_pair(plan: &WritePlan, readback: &[String]) -> bool {
    readback.len() == plan.writes.len()
        && plan
            .writes
            .iter()
            .zip(readback)
            .all(|(write, sha256)| write.new_image.sha256 == *sha256)
}
