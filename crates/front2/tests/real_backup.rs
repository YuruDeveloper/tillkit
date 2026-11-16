use front2::Front2Error;
use front2::gpt::BootSlot;
use front2::gsi::{
    GsiCandidate, GsiCandidateRequest, StockBuild, backup_boot_slot, backup_stock_build,
    find_testkey_slot, install_inputs, install_source_slot,
};
use front2::install::InstallPlan;
use front2::profiles::toss_front2::PROFILE;
use std::path::PathBuf;

const BACKUP_VARIABLE: &str = "FRONT2_REAL_BACKUP_DIR";
const GSI_VARIABLE: &str = "FRONT2_REAL_GSI_IMAGE";
const OUTPUT_VARIABLE: &str = "FRONT2_REAL_OUTPUT_DIR";
const CANDIDATE_FOLDER: &str = "candidate";
const PLAN_FOLDER: &str = "plan";

fn path_from(variable: &str) -> Option<PathBuf> {
    std::env::var_os(variable).map(PathBuf::from)
}

fn target_slot_of(backup: &std::path::Path) -> BootSlot {
    match find_testkey_slot(backup) {
        Ok(slot) => slot,
        Err(Front2Error::Unsupported(_)) => BootSlot::B,
        Err(other) => panic!("the test key slot of the backup: {other}"),
    }
}

#[test]
fn a_candidate_and_a_plan_from_a_real_full_backup_when_available() {
    let (Some(backup), Some(gsi_image), Some(output)) = (
        path_from(BACKUP_VARIABLE),
        path_from(GSI_VARIABLE),
        path_from(OUTPUT_VARIABLE),
    ) else {
        eprintln!("skipped: {BACKUP_VARIABLE}, {GSI_VARIABLE} or {OUTPUT_VARIABLE} is not set");
        return;
    };
    let stock_slot = backup_boot_slot(&backup).expect("the boot slot of the backup");
    let target_slot = target_slot_of(&backup);
    let source_slot = install_source_slot(stock_slot, target_slot).expect("the source slot");
    println!("boot slot {stock_slot:?}, target slot {target_slot:?}, source slot {source_slot:?}");
    let build = backup_stock_build(&backup, source_slot).expect("the stock build of the backup");
    println!("stock build: {build:?}");
    let accepted = match &build {
        StockBuild::Tested => None,
        StockBuild::Untested(fingerprint) => Some(fingerprint.as_str()),
    };

    let candidate_directory = output.join(CANDIDATE_FOLDER);
    let candidate = GsiCandidate::build(&GsiCandidateRequest {
        profile: &PROFILE,
        backup_directory: &backup,
        gsi_image: &gsi_image,
        output_directory: &candidate_directory,
        rollback_evidence: &[],
        accepted_untested_build: accepted,
        stock_slot,
        target_slot,
    })
    .expect("the candidate builds from the real full backup");
    assert_eq!(candidate.stock_slot, stock_slot);
    assert_eq!(candidate.target_slot, target_slot);
    assert_eq!(candidate.source_slot, source_slot);
    println!("top vbmeta: {:?}", candidate.top_vbmeta);
    println!("chained vbmeta: {:?}", candidate.chained_vbmeta);
    for image in &candidate.physical_images {
        println!(
            "physical image: {} from {:?}",
            image.partition, image.backup_source
        );
    }
    println!("avb verification: {:?}", candidate.avb_verification);

    let (loaded, _) = GsiCandidate::load(&candidate_directory).expect("load the saved candidate");
    loaded
        .verify_files(&candidate_directory)
        .expect("the files match their records");
    let inputs = install_inputs(&loaded, &candidate_directory, &backup, &PROFILE)
        .expect("the install inputs from the candidate and the backup");
    let plan_directory = output.join(PLAN_FOLDER);
    let plan = InstallPlan::build(&inputs).expect("the install plan builds");
    plan.save(&plan_directory).expect("save the install plan");
    InstallPlan::load(&plan_directory).expect("load the saved install plan");
    println!("install plan: {} steps", plan.steps.len());
}
