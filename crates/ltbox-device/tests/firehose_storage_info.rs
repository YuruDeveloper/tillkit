use std::io::{self, BufRead, Cursor, Read, Write};

use qdl::types::{FirehoseConfiguration, QdlBackend, QdlChan};

const ACK: &[u8] = b"<data><response value=\"ACK\" /></data>";
const NAK: &[u8] = b"<data><response value=\"NAK\" /></data>";

struct StorageInfoChannel {
    incoming: Cursor<Vec<u8>>,
    commands: Vec<String>,
    config: FirehoseConfiguration,
}

impl StorageInfoChannel {
    fn new(incoming: Vec<u8>) -> Self {
        Self {
            incoming: Cursor::new(incoming),
            commands: Vec::new(),
            config: serial_configuration(),
        }
    }
}

fn serial_configuration() -> FirehoseConfiguration {
    FirehoseConfiguration {
        backend: QdlBackend::Serial,
        ..FirehoseConfiguration::default()
    }
}

impl Read for StorageInfoChannel {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.incoming.read(buffer)
    }
}

impl BufRead for StorageInfoChannel {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        let available = self.incoming.fill_buf()?;
        if available.is_empty() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        Ok(available)
    }

    fn consume(&mut self, amount: usize) {
        self.incoming.consume(amount);
    }
}

impl Write for StorageInfoChannel {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.commands
            .push(String::from_utf8(buffer.to_vec()).expect("command UTF-8"));
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl QdlChan for StorageInfoChannel {
    fn fh_config(&self) -> &FirehoseConfiguration {
        &self.config
    }

    fn mut_fh_config(&mut self) -> &mut FirehoseConfiguration {
        &mut self.config
    }
}

fn log_packet(value: &str) -> Vec<u8> {
    format!("<data><log value=\"{value}\" /></data>").into_bytes()
}

fn response(lines: &[&str], last: &[u8]) -> Vec<u8> {
    let mut incoming: Vec<u8> = lines.iter().flat_map(|line| log_packet(line)).collect();
    incoming.extend_from_slice(last);
    incoming
}

fn sent_command(channel: &StorageInfoChannel) -> (String, String) {
    assert_eq!(channel.commands.len(), 1);
    let document = roxmltree::Document::parse(&channel.commands[0]).expect("command XML");
    let node = document
        .root_element()
        .first_element_child()
        .expect("command node");
    (
        node.tag_name().name().to_string(),
        node.attribute("physical_partition_number")
            .expect("partition attribute")
            .to_string(),
    )
}

#[test]
fn storage_info_log_returns_every_log_line_before_the_ack() {
    let lines = ["UFS Total Active LU: 0x6", "UFS LU Total Blocks: 0x800"];
    let mut channel = StorageInfoChannel::new(response(&lines, ACK));
    assert!(channel.fh_config().skip_firehose_log);

    let received = qdl::firehose_get_storage_info_log(&mut channel, 1).expect("ACK");

    assert_eq!(received, lines);
    assert_eq!(
        sent_command(&channel),
        ("getstorageinfo".to_string(), "1".to_string())
    );
}

#[test]
fn storage_info_log_accepts_the_boot_well_known_lun() {
    let mut channel = StorageInfoChannel::new(response(&["UFS Boot LUN ID: = 0x2"], ACK));

    let received = qdl::firehose_get_storage_info_log(&mut channel, 176).expect("ACK");

    assert_eq!(received, ["UFS Boot LUN ID: = 0x2"]);
    assert_eq!(sent_command(&channel).1, "176");
}

#[test]
fn storage_info_log_rejects_a_nak() {
    let mut channel =
        StorageInfoChannel::new(response(&["ERROR: no such physical partition"], NAK));

    let error = qdl::firehose_get_storage_info_log(&mut channel, 9).unwrap_err();

    assert!(error.to_string().contains("NAKed"), "{error}");
}

#[test]
fn storage_info_log_rejects_log_lines_without_a_response() {
    let mut channel = StorageInfoChannel::new(response(&["UFS LU Total Blocks: 0x800"], b""));

    let error = qdl::firehose_get_storage_info_log(&mut channel, 1).unwrap_err();

    assert_eq!(
        error.downcast_ref::<io::Error>().map(io::Error::kind),
        Some(io::ErrorKind::TimedOut)
    );
}

#[test]
fn storage_info_without_log_capture_still_skips_the_log_lines() {
    let mut channel = StorageInfoChannel::new(response(&["UFS LU Total Blocks: 0x800"], ACK));

    qdl::firehose_get_storage_info(&mut channel, 1).expect("ACK");

    assert_eq!(
        channel.incoming.position() as usize,
        channel.incoming.get_ref().len()
    );
}
