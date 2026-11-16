use super::mock_call::MockCall;
use super::mock_storage::MockStorage;
use crate::gpt_support::{ENTRY_BYTES, LAST_LBA, PARTITION_SECTORS, SECTOR, SyntheticGpt, seal};
use front2::ImageRecord;
use front2::gpt::{
    BootSlot, GptCopyKind, PartitionLocation, PartitionMap, SlotActivation, misc_wipe_sector,
};
use front2::gsi::install_source_slot;
use front2::install::{
    GptBackup, InstallImage, InstallInputs, InstallPlan, StoredInstallPlan, image_partitions,
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
const STOCK_B_BOOT_LUN: u8 = 2;
const DUAL_SLOT_CAPACITY: usize = 16;
const PHYSICAL_BASES: [&str; 5] = ["boot", "dtbo", "init_boot", "recovery", "vendor_boot"];

pub struct InstallFixture {
    pub directory: TempDir,
    pub inputs: InstallInputs,
    pub storage: MockStorage,
    pub calls: Rc<RefCell<Vec<MockCall>>>,
}

pub struct InstallFixtureBuilder {
    super_sectors: u64,
    distinct_boot_lun_tables: bool,
    stock_slot: BootSlot,
    target_slot: BootSlot,
}

impl InstallFixtureBuilder {
    pub fn with_super_sectors(mut self, super_sectors: u64) -> Self {
        self.super_sectors = super_sectors;
        self
    }

    pub fn with_identical_boot_lun_tables(mut self) -> Self {
        self.distinct_boot_lun_tables = false;
        self
    }

    pub fn build(self) -> InstallFixture {
        InstallFixture::assemble(
            self.super_sectors,
            self.distinct_boot_lun_tables,
            self.stock_slot,
            self.target_slot,
        )
    }
}

impl InstallFixture {
    pub fn new(stock_slot: BootSlot, target_slot: BootSlot) -> Self {
        Self::assemble(PARTITION_SECTORS, true, stock_slot, target_slot)
    }

    pub fn builder(stock_slot: BootSlot, target_slot: BootSlot) -> InstallFixtureBuilder {
        InstallFixtureBuilder {
            super_sectors: PARTITION_SECTORS,
            distinct_boot_lun_tables: true,
            stock_slot,
            target_slot,
        }
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

    fn assemble(
        super_sectors: u64,
        distinct_boot_lun_tables: bool,
        stock_slot: BootSlot,
        target_slot: BootSlot,
    ) -> Self {
        let directory = TempDir::new().expect("temporary directory");
        let backup_directory = directory.path().join(BACKUP_FOLDER);
        let candidate_directory = directory.path().join(CANDIDATE_FOLDER);
        let copies = gpt_copies(
            super_sectors,
            distinct_boot_lun_tables,
            stock_slot,
            target_slot,
        );
        let mut storage = MockStorage::new(LUN_COUNT, LAST_LBA + 1);
        if stock_slot == BootSlot::B {
            storage.start_with_boot_lun(STOCK_B_BOOT_LUN);
        }
        let gpt_backups = store_gpt_copies(&backup_directory, &mut storage, &copies);
        let partition_map = partition_map_of(&copies);
        let source_slot =
            install_source_slot(stock_slot, target_slot).expect("install source slot");
        let in_place = source_slot == target_slot;
        let mut images = Vec::new();
        for name in image_partitions(target_slot) {
            let location = partition_map.locate(&name).expect("image partition");
            let original = store_original(&backup_directory, &mut storage, &name, location);
            images.push(install_image(
                &candidate_directory,
                &name,
                original,
                in_place,
                target_slot,
            ));
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
            stock_slot,
            target_slot,
            partition_map,
            images,
            metadata_original,
            misc_original,
            userdata_original,
            misc_first_sector: misc_wipe_sector(&misc_sector).expect("misc sector"),
            gpt_backups,
            activation_lun0: slot_activation(stock_slot, target_slot, 0, &copies),
            activation_lun4: slot_activation(stock_slot, target_slot, 4, &copies),
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

fn tables(
    distinct_boot_lun_tables: bool,
    stock_slot: BootSlot,
    target_slot: BootSlot,
) -> Vec<SyntheticGpt> {
    let pair = |active: u64, inactive: u64| match stock_slot {
        BootSlot::A => (active, inactive),
        BootSlot::B => (inactive, active),
    };
    let (boot_a_flags, boot_b_flags) = pair(0x0077 << 48, 0x00c3 << 48);
    let (vbmeta_a_flags, vbmeta_b_flags) = pair(ACTIVE_SLOT_FLAGS, INACTIVE_SLOT_FLAGS);
    let (system_a_flags, system_b_flags) = pair(ACTIVE_SLOT_FLAGS, INACTIVE_SLOT_FLAGS);
    let (xbl_a_flags, xbl_b_flags) = match stock_slot {
        BootSlot::A => (ACTIVE_SLOT_FLAGS, 0),
        BootSlot::B => (0, ACTIVE_SLOT_FLAGS),
    };
    let boot_lun_table = |name: &str, flags: u64| {
        let named = SyntheticGpt::empty().with_entry(name, flags);
        if distinct_boot_lun_tables {
            named
        } else {
            SyntheticGpt::empty()
        }
    };
    vec![
        SyntheticGpt::empty()
            .with_entry("vbmeta_system_a", system_a_flags)
            .with_entry("vbmeta_system_b", system_b_flags)
            .with_entry("metadata", 0)
            .with_entry("misc", 0)
            .with_entry("userdata", 0)
            .with_entry("super", 0),
        boot_lun_table("xbl_a", xbl_a_flags),
        boot_lun_table("xbl_b", xbl_b_flags),
        SyntheticGpt::empty().with_entry("cdt", 0),
        physical_table(
            boot_a_flags,
            boot_b_flags,
            vbmeta_a_flags,
            vbmeta_b_flags,
            stock_slot,
            target_slot,
        ),
        SyntheticGpt::empty().with_entry("fsg", 0),
    ]
}

// A switch to slot A activates every slot A physical partition, and the
// activation requires a slot B partner for each. The slot A fixture holds the
// full set of physical pairs. A target slot B fixture keeps the smaller table
// that the stock slot B and first conversion tests expect.
fn physical_table(
    boot_a_flags: u64,
    boot_b_flags: u64,
    vbmeta_a_flags: u64,
    vbmeta_b_flags: u64,
    stock_slot: BootSlot,
    target_slot: BootSlot,
) -> SyntheticGpt {
    let base = SyntheticGpt::empty()
        .with_entry("boot_a", boot_a_flags)
        .with_entry("boot_b", boot_b_flags)
        .with_entry("vbmeta_a", vbmeta_a_flags)
        .with_entry("vbmeta_b", vbmeta_b_flags);
    if target_slot == BootSlot::B {
        return base
            .with_entry("dtbo_b", INACTIVE_SLOT_FLAGS)
            .with_entry("init_boot_b", INACTIVE_SLOT_FLAGS)
            .with_entry("recovery_b", INACTIVE_SLOT_FLAGS)
            .with_entry("vendor_boot_b", INACTIVE_SLOT_FLAGS);
    }
    let (secondary_a_flags, secondary_b_flags) = match stock_slot {
        BootSlot::A => (ACTIVE_SLOT_FLAGS, INACTIVE_SLOT_FLAGS),
        BootSlot::B => (INACTIVE_SLOT_FLAGS, ACTIVE_SLOT_FLAGS),
    };
    base.with_capacity(DUAL_SLOT_CAPACITY)
        .with_entry("dtbo_a", secondary_a_flags)
        .with_entry("dtbo_b", secondary_b_flags)
        .with_entry("init_boot_a", secondary_a_flags)
        .with_entry("init_boot_b", secondary_b_flags)
        .with_entry("recovery_a", secondary_a_flags)
        .with_entry("recovery_b", secondary_b_flags)
        .with_entry("vendor_boot_a", secondary_a_flags)
        .with_entry("vendor_boot_b", secondary_b_flags)
}

fn gpt_copies(
    super_sectors: u64,
    distinct_boot_lun_tables: bool,
    stock_slot: BootSlot,
    target_slot: BootSlot,
) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut copies: Vec<(Vec<u8>, Vec<u8>)> =
        tables(distinct_boot_lun_tables, stock_slot, target_slot)
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

fn slot_activation(
    stock_slot: BootSlot,
    target_slot: BootSlot,
    lun: u8,
    copies: &[(Vec<u8>, Vec<u8>)],
) -> Option<SlotActivation> {
    if stock_slot == target_slot {
        return None;
    }
    let (main, backup) = &copies[usize::from(lun)];
    Some(SlotActivation::prepare(lun, target_slot, main, backup).expect("slot activation"))
}

fn install_image(
    candidate_directory: &Path,
    name: &str,
    original: ImageRecord,
    in_place: bool,
    target_slot: BootSlot,
) -> InstallImage {
    let base = name.strip_suffix(target_slot.suffix());
    let is_physical = base.is_some_and(|base| PHYSICAL_BASES.contains(&base));
    let identity = in_place && is_physical;
    let seed = if identity {
        format!("{name}-original")
    } else {
        format!("{name}-candidate")
    };
    let data = pattern(&seed, original.bytes as usize);
    let file = match name {
        "super" => "super.img".to_string(),
        _ => format!("view/{}.img", base.unwrap_or(name)),
    };
    write_file(&candidate_directory.join(&file), &data);
    InstallImage {
        partition: name.to_string(),
        candidate: ImageRecord::from_file(candidate_directory, &file).expect("candidate record"),
        original,
    }
}
