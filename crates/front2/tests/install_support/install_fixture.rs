use super::mock_call::MockCall;
use super::mock_storage::MockStorage;
use crate::gpt_support::{ENTRY_BYTES, LAST_LBA, PARTITION_SECTORS, SECTOR, SyntheticGpt, seal};
use front2::ImageRecord;
use front2::gpt::{GptCopyKind, PartitionLocation, PartitionMap, SlotActivation, misc_wipe_sector};
use front2::install::{
    GptBackup, IMAGE_PARTITIONS, InstallImage, InstallInputs, InstallPlan, StoredInstallPlan,
};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tempfile::TempDir;

const LUN_COUNT: u8 = 6;
const ACTIVE_SLOT_FLAGS: u64 = 0x107f << 48;
const INACTIVE_SLOT_FLAGS: u64 = 0x107b << 48;
const SUPER_ENTRY_POSITION: usize = 5;
const BACKUP_FOLDER: &str = "backup";
const CANDIDATE_FOLDER: &str = "candidate";
const PLAN_FOLDER: &str = "plan";

pub struct InstallFixture {
    pub directory: TempDir,
    pub inputs: InstallInputs,
    pub storage: MockStorage,
    pub calls: Rc<RefCell<Vec<MockCall>>>,
}

impl InstallFixture {
    pub fn standard() -> Self {
        Self::build(PARTITION_SECTORS, true)
    }

    pub fn with_super_sectors(super_sectors: u64) -> Self {
        Self::build(super_sectors, true)
    }

    pub fn with_identical_boot_lun_tables() -> Self {
        Self::build(PARTITION_SECTORS, false)
    }

    pub fn plan_directory(&self) -> PathBuf {
        self.directory.path().join(PLAN_FOLDER)
    }

    pub fn stored_plan(&self) -> StoredInstallPlan {
        let directory = self.plan_directory();
        InstallPlan::build(&self.inputs)
            .expect("the fixture gives a valid plan")
            .save(&directory)
            .expect("the plan is saved");
        InstallPlan::load(&directory).expect("the saved plan loads")
    }

    pub fn location(&self, name: &str) -> PartitionLocation {
        self.inputs
            .partition_map
            .locate(name)
            .expect("the fixture holds the partition")
    }

    pub fn replace_candidate(&mut self, partition: &str, data: &[u8]) {
        let directory = self.inputs.candidate_directory.clone();
        let image = self
            .inputs
            .images
            .iter_mut()
            .find(|image| image.partition == partition)
            .expect("the fixture holds the image");
        fs::write(directory.join(&image.candidate.file), data).expect("candidate file");
        image.candidate =
            ImageRecord::from_file(&directory, &image.candidate.file).expect("candidate record");
    }

    pub fn overwrite_candidate_file(&self, partition: &str, data: &[u8]) {
        let image = self
            .inputs
            .images
            .iter()
            .find(|image| image.partition == partition)
            .expect("the fixture holds the image");
        let path = self.inputs.candidate_directory.join(&image.candidate.file);
        fs::write(path, data).expect("candidate file");
    }

    fn build(super_sectors: u64, distinct_boot_lun_tables: bool) -> Self {
        let directory = TempDir::new().expect("temporary directory");
        let backup_directory = directory.path().join(BACKUP_FOLDER);
        let candidate_directory = directory.path().join(CANDIDATE_FOLDER);
        let copies = gpt_copies(super_sectors, distinct_boot_lun_tables);
        let mut storage = MockStorage::new(LUN_COUNT, LAST_LBA + 1);
        let gpt_backups = store_gpt_copies(&backup_directory, &mut storage, &copies);
        let partition_map = partition_map_of(&copies);
        let mut images = Vec::new();
        for name in IMAGE_PARTITIONS {
            let location = partition_map.locate(name).expect("image partition");
            let original = store_original(&backup_directory, &mut storage, name, location);
            images.push(install_image(&candidate_directory, name, original));
        }
        let metadata_location = partition_map.locate("metadata").expect("metadata");
        let metadata_original = store_original(
            &backup_directory,
            &mut storage,
            "metadata",
            metadata_location,
        );
        let misc_location = partition_map.locate("misc").expect("misc");
        let misc_original = store_original(&backup_directory, &mut storage, "misc", misc_location);
        let misc_sector = storage.content(misc_location.lun, misc_location.start_sector, 1);
        let userdata_location = partition_map.locate("userdata").expect("userdata");
        let userdata_original = store_original(
            &backup_directory,
            &mut storage,
            "userdata",
            userdata_location,
        );
        let inputs = InstallInputs {
            candidate_directory,
            backup_directory,
            partition_map,
            images,
            metadata_original,
            misc_original,
            userdata_original,
            misc_first_sector: misc_wipe_sector(&misc_sector).expect("misc sector"),
            gpt_backups,
            activation_lun0: activation(0, &copies),
            activation_lun4: activation(4, &copies),
        };
        let calls = storage.calls();
        Self {
            directory,
            inputs,
            storage,
            calls,
        }
    }
}

fn store_original(
    backup_directory: &Path,
    storage: &mut MockStorage,
    name: &str,
    location: PartitionLocation,
) -> ImageRecord {
    let data = pattern(
        &format!("{name}-original"),
        location.sectors as usize * SECTOR,
    );
    let file = format!("partitions/lun{}/{name}.bin", location.lun);
    write_file(&backup_directory.join(&file), &data);
    storage.load(location.lun, location.start_sector, &data);
    ImageRecord::from_file(backup_directory, &file).expect("original record")
}

pub fn pattern(seed: &str, bytes: usize) -> Vec<u8> {
    let mut state = seed.bytes().fold(0x9E37_79B9u32, |state, byte| {
        state.wrapping_mul(31).wrapping_add(u32::from(byte))
    });
    let mut data = Vec::with_capacity(bytes);
    for _ in 0..bytes {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        data.push((state >> 24) as u8);
    }
    data
}

fn write_file(path: &Path, data: &[u8]) {
    fs::create_dir_all(path.parent().expect("parent directory")).expect("directory");
    fs::write(path, data).expect("file");
}

fn tables(distinct_boot_lun_tables: bool) -> Vec<SyntheticGpt> {
    let boot_lun_table = |name: &str| {
        let named = SyntheticGpt::empty().with_entry(name, 0);
        if distinct_boot_lun_tables {
            named
        } else {
            SyntheticGpt::empty()
        }
    };
    vec![
        SyntheticGpt::empty()
            .with_entry("vbmeta_system_a", ACTIVE_SLOT_FLAGS)
            .with_entry("vbmeta_system_b", INACTIVE_SLOT_FLAGS)
            .with_entry("metadata", 0)
            .with_entry("misc", 0)
            .with_entry("userdata", 0)
            .with_entry("super", 0),
        boot_lun_table("xbl_a"),
        boot_lun_table("xbl_b"),
        SyntheticGpt::empty().with_entry("cdt", 0),
        SyntheticGpt::empty()
            .with_entry("boot_a", 0x0077 << 48)
            .with_entry("boot_b", 0x00c3 << 48)
            .with_entry("vbmeta_a", ACTIVE_SLOT_FLAGS)
            .with_entry("vbmeta_b", INACTIVE_SLOT_FLAGS)
            .with_entry("dtbo_b", INACTIVE_SLOT_FLAGS)
            .with_entry("init_boot_b", INACTIVE_SLOT_FLAGS)
            .with_entry("recovery_b", INACTIVE_SLOT_FLAGS)
            .with_entry("vendor_boot_b", INACTIVE_SLOT_FLAGS),
        SyntheticGpt::empty().with_entry("fsg", 0),
    ]
}

fn gpt_copies(super_sectors: u64, distinct_boot_lun_tables: bool) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut copies: Vec<(Vec<u8>, Vec<u8>)> = tables(distinct_boot_lun_tables)
        .iter()
        .map(|table| (table.main(), table.backup()))
        .collect();
    let (main, backup) = &mut copies[0];
    resize_entry(GptCopyKind::Main, main, SUPER_ENTRY_POSITION, super_sectors);
    resize_entry(
        GptCopyKind::Backup,
        backup,
        SUPER_ENTRY_POSITION,
        super_sectors,
    );
    copies
}

fn resize_entry(kind: GptCopyKind, bytes: &mut [u8], position: usize, sectors: u64) {
    let entry = kind.table_offset() + position * ENTRY_BYTES;
    let mut first_lba = [0u8; 8];
    first_lba.copy_from_slice(&bytes[entry + 32..entry + 40]);
    let last_lba = u64::from_le_bytes(first_lba) + sectors - 1;
    bytes[entry + 40..entry + 48].copy_from_slice(&last_lba.to_le_bytes());
    seal(kind, bytes);
}

fn store_gpt_copies(
    backup_directory: &Path,
    storage: &mut MockStorage,
    copies: &[(Vec<u8>, Vec<u8>)],
) -> Vec<GptBackup> {
    let mut backups = Vec::new();
    for (lun, (main, backup)) in (0u8..).zip(copies) {
        storage.load(lun, 0, main);
        storage.load(lun, LAST_LBA - 4, backup);
        backups.push(gpt_backup(backup_directory, lun, GptCopyKind::Main, main));
        backups.push(gpt_backup(
            backup_directory,
            lun,
            GptCopyKind::Backup,
            backup,
        ));
    }
    backups
}

fn gpt_backup(backup_directory: &Path, lun: u8, kind: GptCopyKind, data: &[u8]) -> GptBackup {
    let file = format!("gpt/gpt_{}{lun}.bin", kind.label());
    write_file(&backup_directory.join(&file), data);
    GptBackup {
        lun,
        copy: kind,
        record: ImageRecord::from_file(backup_directory, &file).expect("GPT record"),
    }
}

fn partition_map_of(copies: &[(Vec<u8>, Vec<u8>)]) -> PartitionMap {
    let main_copies: Vec<(u8, &[u8])> = (0u8..)
        .zip(copies)
        .map(|(lun, (main, _))| (lun, main.as_slice()))
        .collect();
    PartitionMap::from_main_copies(&main_copies).expect("partition map")
}

fn activation(lun: u8, copies: &[(Vec<u8>, Vec<u8>)]) -> SlotActivation {
    let (main, backup) = &copies[usize::from(lun)];
    SlotActivation::prepare(lun, main, backup).expect("slot activation")
}

fn install_image(candidate_directory: &Path, name: &str, original: ImageRecord) -> InstallImage {
    let data = pattern(&format!("{name}-candidate"), original.bytes as usize);
    let file = match name {
        "super" => "super.img".to_string(),
        _ => format!("view/{}.img", name.trim_end_matches("_b")),
    };
    write_file(&candidate_directory.join(&file), &data);
    InstallImage {
        partition: name.to_string(),
        candidate: ImageRecord::from_file(candidate_directory, &file).expect("candidate record"),
        original,
    }
}
