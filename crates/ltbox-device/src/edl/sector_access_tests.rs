use super::{EdlError, EdlSession, UfsStorageInfo};
use qdl::types::{FirehoseConfiguration, QdlBackend, QdlDevice, QdlReadWrite};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead, Cursor, Read, Write};
use std::sync::{Arc, Mutex};

const ACK: &[u8] = b"<data><response value=\"ACK\" /></data>";
const NAK: &[u8] = b"<data><response value=\"NAK\" /></data>";
const SECTOR: usize = 16;
const DATA_LUN: u8 = 4;
const BOOT_WELL_KNOWN_LUN: u8 = 176;
const DISK_SECTORS: usize = 8;

type Commands = Arc<Mutex<Vec<String>>>;
type Disks = Arc<Mutex<BTreeMap<u8, Vec<u8>>>>;

struct SectorTransport {
    disks: Disks,
    commands: Commands,
    refused: BTreeSet<&'static str>,
    storage_lines: Vec<String>,
    responses: Cursor<Vec<u8>>,
    payload_lun: u8,
    payload_offset: usize,
    payload_remaining: usize,
}

impl Read for SectorTransport {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.responses.read(buffer)
    }
}

impl BufRead for SectorTransport {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        let available = self.responses.fill_buf()?;
        if available.is_empty() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        Ok(available)
    }

    fn consume(&mut self, amount: usize) {
        self.responses.consume(amount);
    }
}

impl Write for SectorTransport {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.payload_remaining > 0 {
            return Ok(self.accept_payload(bytes));
        }
        let xml = std::str::from_utf8(bytes).expect("command UTF-8");
        let document = roxmltree::Document::parse(xml).expect("command XML");
        let command = document
            .root_element()
            .first_element_child()
            .expect("command node");
        let (record, response) = self.respond(command);
        self.commands.lock().unwrap().push(record);
        self.responses = Cursor::new(response);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl QdlReadWrite for SectorTransport {}

impl SectorTransport {
    fn respond(&mut self, command: roxmltree::Node) -> (String, Vec<u8>) {
        let name = command.tag_name().name();
        let status = if self.refused.contains(name) {
            NAK
        } else {
            ACK
        };
        match name {
            "read" => self.respond_to_read(command, status),
            "program" => self.respond_to_program(command, status),
            "setbootablestoragedrive" => (
                format!("setbootablestoragedrive {}", number(command, "value")),
                status.to_vec(),
            ),
            "getstorageinfo" => self.respond_to_storage_info(command, status),
            "power" => (
                format!("power {}", command.attribute("value").unwrap_or_default()),
                ACK.to_vec(),
            ),
            other => panic!("unexpected Firehose command: {other}"),
        }
    }

    fn respond_to_read(&self, command: roxmltree::Node, status: &[u8]) -> (String, Vec<u8>) {
        let (lun, start, sectors) = sector_range(command);
        let mut response = status.to_vec();
        if status == ACK {
            let disks = self.disks.lock().unwrap();
            response.extend_from_slice(&disks[&lun][start * SECTOR..(start + sectors) * SECTOR]);
            response.extend_from_slice(ACK);
        }
        (format!("read {lun} {start} {sectors}"), response)
    }

    fn respond_to_program(&mut self, command: roxmltree::Node, status: &[u8]) -> (String, Vec<u8>) {
        let (lun, start, sectors) = sector_range(command);
        if status == ACK {
            self.payload_lun = lun;
            self.payload_offset = start * SECTOR;
            self.payload_remaining = sectors * SECTOR;
        }
        (format!("program {lun} {start} {sectors}"), status.to_vec())
    }

    fn respond_to_storage_info(
        &self,
        command: roxmltree::Node,
        status: &[u8],
    ) -> (String, Vec<u8>) {
        let mut response: Vec<u8> = self
            .storage_lines
            .iter()
            .flat_map(|line| format!("<data><log value=\"{line}\" /></data>").into_bytes())
            .collect();
        response.extend_from_slice(status);
        (
            format!(
                "getstorageinfo {}",
                number(command, "physical_partition_number")
            ),
            response,
        )
    }

    fn accept_payload(&mut self, bytes: &[u8]) -> usize {
        assert!(bytes.len() <= self.payload_remaining);
        let mut disks = self.disks.lock().unwrap();
        let disk = disks.get_mut(&self.payload_lun).expect("payload LUN");
        disk[self.payload_offset..self.payload_offset + bytes.len()].copy_from_slice(bytes);
        self.payload_offset += bytes.len();
        self.payload_remaining -= bytes.len();
        if self.payload_remaining == 0 {
            self.responses = Cursor::new(ACK.to_vec());
        }
        bytes.len()
    }
}

fn number(command: roxmltree::Node, attribute: &str) -> usize {
    command
        .attribute(attribute)
        .unwrap_or_else(|| panic!("{} has {attribute}", command.tag_name().name()))
        .parse()
        .expect("decimal attribute")
}

fn sector_range(command: roxmltree::Node) -> (u8, usize, usize) {
    let lun = u8::try_from(number(command, "physical_partition_number")).expect("LUN");
    (
        lun,
        number(command, "start_sector"),
        number(command, "num_partition_sectors"),
    )
}

fn disk_pattern(seed: u8) -> Vec<u8> {
    (0..DISK_SECTORS * SECTOR)
        .map(|index| seed.wrapping_add(index as u8))
        .collect()
}

fn session_with(refused: &[&'static str], storage_lines: &[&str]) -> (EdlSession, Commands, Disks) {
    let disks: Disks = Arc::new(Mutex::new(BTreeMap::from([
        (DATA_LUN, disk_pattern(0x10)),
        (BOOT_WELL_KNOWN_LUN, disk_pattern(0x80)),
    ])));
    let commands = Commands::default();
    let transport = SectorTransport {
        disks: Arc::clone(&disks),
        commands: Arc::clone(&commands),
        refused: refused.iter().copied().collect(),
        storage_lines: storage_lines.iter().map(|line| line.to_string()).collect(),
        responses: Cursor::new(Vec::new()),
        payload_lun: 0,
        payload_offset: 0,
        payload_remaining: 0,
    };
    let session = EdlSession {
        dev: QdlDevice {
            rw: Box::new(transport),
            fh_cfg: configuration(),
            reset_on_drop: false,
        },
        lun_sectors: BTreeMap::new(),
        exited: false,
    };
    (session, commands, disks)
}

fn configuration() -> FirehoseConfiguration {
    FirehoseConfiguration {
        storage_sector_size: SECTOR,
        send_buffer_size: SECTOR * 2,
        recv_buffer_size: SECTOR * 2,
        backend: QdlBackend::Serial,
        ..FirehoseConfiguration::default()
    }
}

fn recorded(commands: &Commands) -> Vec<String> {
    commands.lock().unwrap().clone()
}

fn session_error(result: Result<impl std::fmt::Debug, EdlError>) -> String {
    match result {
        Err(EdlError::Session(message)) => message,
        other => panic!("expected a session error, got {other:?}"),
    }
}

#[test]
fn read_sectors_returns_the_bytes_of_the_requested_range() {
    let (mut session, commands, disks) = session_with(&[], &[]);

    let data = session.read_sectors(DATA_LUN, 2, 3).expect("read");

    assert_eq!(
        data,
        disks.lock().unwrap()[&DATA_LUN][2 * SECTOR..5 * SECTOR]
    );
    assert_eq!(recorded(&commands), ["read 4 2 3"]);
}

#[test]
fn read_sectors_reaches_the_boot_well_known_lun() {
    let (mut session, commands, disks) = session_with(&[], &[]);

    let data = session
        .read_sectors(BOOT_WELL_KNOWN_LUN, 0, 6)
        .expect("read");

    assert_eq!(
        data,
        disks.lock().unwrap()[&BOOT_WELL_KNOWN_LUN][..6 * SECTOR]
    );
    assert_eq!(recorded(&commands), ["read 176 0 6"]);
}

#[test]
fn read_of_zero_sectors_sends_no_command() {
    let (mut session, commands, _) = session_with(&[], &[]);

    assert!(session_error(session.read_sectors(DATA_LUN, 0, 0)).contains("at least one"));
    assert!(recorded(&commands).is_empty());
}

#[test]
fn refused_read_is_an_error() {
    let (mut session, _, _) = session_with(&["read"], &[]);

    assert!(session_error(session.read_sectors(DATA_LUN, 0, 1)).contains("LUN 4"));
}

#[test]
fn write_sectors_programs_whole_sectors_that_a_read_returns() {
    let (mut session, commands, disks) = session_with(&[], &[]);
    let data = vec![0xa5; 3 * SECTOR];

    session.write_sectors(DATA_LUN, 4, &data).expect("write");

    assert_eq!(
        disks.lock().unwrap()[&DATA_LUN][4 * SECTOR..7 * SECTOR],
        data
    );
    assert_eq!(session.read_sectors(DATA_LUN, 4, 3).expect("read"), data);
    assert_eq!(recorded(&commands), ["program 4 4 3", "read 4 4 3"]);
}

#[test]
fn write_sectors_rejects_empty_and_partial_sector_data_before_any_command() {
    let (mut session, commands, disks) = session_with(&[], &[]);
    let before = disks.lock().unwrap()[&DATA_LUN].clone();

    for length in [0, 1, SECTOR - 1, SECTOR + 1] {
        let message = session_error(session.write_sectors(DATA_LUN, 0, &vec![0; length]));
        assert!(message.contains("whole sectors"), "{message}");
    }

    assert!(recorded(&commands).is_empty());
    assert_eq!(disks.lock().unwrap()[&DATA_LUN], before);
}

#[test]
fn write_sectors_rejects_a_transfer_size_below_one_sector_before_any_command() {
    for transfer_size in [0, SECTOR - 1] {
        let (mut session, commands, disks) = session_with(&[], &[]);
        session.dev.fh_cfg.send_buffer_size = transfer_size;
        let before = disks.lock().unwrap()[&DATA_LUN].clone();

        let message = session_error(session.write_sectors(DATA_LUN, 0, &[0xa5; SECTOR]));

        assert!(message.contains("each transfer"), "{message}");
        assert!(recorded(&commands).is_empty());
        assert_eq!(disks.lock().unwrap()[&DATA_LUN], before);
    }
}

#[test]
fn write_sectors_accepts_a_transfer_size_of_one_sector() {
    let (mut session, commands, disks) = session_with(&[], &[]);
    session.dev.fh_cfg.send_buffer_size = SECTOR;
    let data = vec![0xa5; 2 * SECTOR];

    session.write_sectors(DATA_LUN, 1, &data).expect("write");

    assert_eq!(disks.lock().unwrap()[&DATA_LUN][SECTOR..3 * SECTOR], data);
    assert_eq!(recorded(&commands), ["program 4 1 2"]);
}

#[test]
fn refused_write_is_an_error_and_sends_no_payload() {
    let (mut session, _, disks) = session_with(&["program"], &[]);
    let before = disks.lock().unwrap()[&DATA_LUN].clone();

    assert!(session_error(session.write_sectors(DATA_LUN, 0, &[0; SECTOR])).contains("LUN 4"));
    assert_eq!(disks.lock().unwrap()[&DATA_LUN], before);
}

#[test]
fn set_bootable_storage_drive_sends_the_lun() {
    let (mut session, commands, _) = session_with(&[], &[]);

    session.set_bootable_storage_drive(2).expect("ACK");

    assert_eq!(recorded(&commands), ["setbootablestoragedrive 2"]);
}

#[test]
fn refused_boot_lun_selection_is_an_error() {
    let (mut session, commands, _) = session_with(&["setbootablestoragedrive"], &[]);

    assert!(session_error(session.set_bootable_storage_drive(2)).contains("LUN 2"));
    assert_eq!(
        recorded(&commands),
        ["setbootablestoragedrive 2", "power reset_to_edl"]
    );
}

#[test]
fn storage_info_parses_the_log_lines_of_the_response() {
    let (mut session, commands, _) = session_with(
        &[],
        &[
            "UFS Current LUN Number: = 0x1",
            "UFS Logical Block Size: 0xc",
            "UFS LU Total Blocks: 0x800",
        ],
    );

    let info = session.storage_info(1).expect("storage info");

    assert_eq!(
        info,
        UfsStorageInfo {
            total_blocks: 0x800,
            block_bytes: 4096,
        }
    );
    assert_eq!(recorded(&commands), ["getstorageinfo 1"]);
}

#[test]
fn storage_info_with_an_unknown_block_size_is_an_error() {
    let (mut session, _, _) = session_with(
        &[],
        &[
            "UFS Current LUN Number: = 0x1",
            "UFS Logical Block Size: 0x9",
            "UFS LU Total Blocks: 0x800",
        ],
    );

    assert!(matches!(
        session.storage_info(1),
        Err(EdlError::StorageInfo(_))
    ));
}

#[test]
fn refused_storage_info_is_an_error() {
    let (mut session, _, _) = session_with(
        &["getstorageinfo"],
        &["UFS Logical Block Size: 0xc", "UFS LU Total Blocks: 0x800"],
    );

    assert!(session_error(session.storage_info(9)).contains("LUN 9"));
}

#[test]
fn storage_info_rejects_the_log_of_another_lun() {
    let (mut session, commands, _) = session_with(
        &[],
        &[
            "UFS Current LUN Number: = 0x0",
            "UFS Logical Block Size: 0xc",
            "UFS LU Total Blocks: 0x66d000",
        ],
    );

    assert!(matches!(
        session.storage_info(1),
        Err(EdlError::StorageInfo(message)) if message.contains("LUN 1 was requested")
    ));
    assert_eq!(recorded(&commands), ["getstorageinfo 1"]);
}
