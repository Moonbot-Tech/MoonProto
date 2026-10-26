//! Settings archive transfer, following the core's sequential snapshot client.

use super::{Event, EventDispatcher};
use crate::commands::registry::{read_string, CURRENT_PROTO_CMD_VER};
use crate::state::SettingsEvent;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const RETRY: Duration = Duration::from_secs(2);
const NO_PROGRESS: Duration = Duration::from_secs(300);
// The core's reply reader has the same per-chunk bound.
const CHUNK_SIZE: usize = 128 * 1024;

pub(super) struct Download {
    path: PathBuf,
    file: File,
    id: u64,
    offset: i64,
    total: i64,
    hash: String,
    digest: Sha256,
    last_request: Option<Instant>,
    progress: Instant,
}

struct Reply<'a> {
    id: u64,
    offset: i64,
    total: i64,
    hash: String,
    error: String,
    data: &'a [u8],
}

impl<'a> Reply<'a> {
    fn parse(payload: &'a [u8]) -> Option<Self> {
        let id = u64::from_le_bytes(payload.get(11..19)?.try_into().ok()?);
        let offset = i64::from_le_bytes(payload.get(19..27)?.try_into().ok()?);
        let total = i64::from_le_bytes(payload.get(27..35)?.try_into().ok()?);
        let mut pos = 35;
        let hash = read_string(payload, &mut pos)?;
        let error = read_string(payload, &mut pos)?;
        let data = payload.get(pos..)?;
        (data.len() <= CHUNK_SIZE).then_some(Self {
            id,
            offset,
            total,
            hash,
            error,
            data,
        })
    }
}

impl Download {
    fn create(path: PathBuf) -> std::io::Result<Self> {
        let file = OpenOptions::new().write(true).create_new(true).open(&path)?;
        Ok(Self {
            path,
            file,
            id: rand::random::<u64>().max(1),
            offset: -1,
            total: 0,
            hash: String::new(),
            digest: Sha256::new(),
            last_request: None,
            progress: Instant::now(),
        })
    }

    fn accept(&mut self, reply: Reply<'_>, now: Instant) -> Result<bool, String> {
        if reply.id != self.id {
            return Ok(false);
        }
        if !reply.error.is_empty() {
            return Err(reply.error);
        }
        // A delayed/repeated answer must not write bytes or extend the progress timeout.
        if reply.offset != self.offset || reply.total <= 0 {
            return Ok(false);
        }
        if self.offset == -1 {
            self.hash = reply.hash;
            self.total = reply.total;
            self.offset = 0;
        } else {
            if reply.hash != self.hash || reply.total != self.total {
                return Err("The settings archive changed during transfer".into());
            }
            if self.offset == self.total && reply.data.is_empty() {
                return Ok(true);
            }
            if reply.data.is_empty() || reply.data.len() as i64 > self.total - self.offset {
                return Err("Invalid settings archive chunk".into());
            }
            self.file.write_all(reply.data).map_err(|e| e.to_string())?;
            self.digest.update(reply.data);
            self.offset += reply.data.len() as i64;
            if self.offset == self.total {
                let hash = format!("{:x}", self.digest.clone().finalize());
                if hash != self.hash {
                    return Err("The settings archive checksum does not match".into());
                }
                self.file.flush().map_err(|e| e.to_string())?;
            }
        }
        self.progress = now;
        self.last_request = None;
        Ok(false)
    }

    fn event(self, result: Result<u64, String>) -> Event {
        Event::Settings(SettingsEvent::SettingsSnapshotDownloaded {
            path: self.path,
            result,
        })
    }
}

impl EventDispatcher {
    pub(crate) fn start_settings_snapshot(&mut self, path: PathBuf) {
        // The server owns one active export. Preserve the current transfer when
        // a caller submits a second request instead of silently replacing it.
        let result = if self.settings_snapshot.is_some() {
            Err("A settings snapshot download is already in progress".into())
        } else {
            Download::create(path.clone()).map_err(|e| e.to_string())
        };
        match result {
            Ok(download) => self.settings_snapshot = Some(download),
            Err(error) => self.queue_events([Event::Settings(SettingsEvent::SettingsSnapshotDownloaded {
                path,
                result: Err(error),
            })]),
        }
    }

    pub(crate) fn poll_settings_snapshot(&mut self, client: &crate::client::Client) {
        let Some(download) = &mut self.settings_snapshot else {
            return;
        };
        let now = Instant::now();
        if now.duration_since(download.progress) > NO_PROGRESS {
            let download = self.settings_snapshot.take().unwrap();
            self.queue_events([download.event(Err(
                "No settings archive progress for 5 minutes; check connection and core support".into(),
            ))]);
            return;
        }
        if client.is_authorized()
            && download
                .last_request
                .is_none_or(|last| now.duration_since(last) >= RETRY)
            && client.ui_settings_snapshot_request(download.id, download.offset)
        {
            download.last_request = Some(now);
        }
    }

    pub(super) fn receive_settings_snapshot(&mut self, payload: &[u8], out: &mut Vec<Event>) {
        if payload.len() < 11 {
            Self::push_parse_failed(out, crate::protocol::Command::UI, payload);
            return;
        }
        if u16::from_le_bytes([payload[1], payload[2]]) > CURRENT_PROTO_CMD_VER {
            return;
        }
        let Some(reply) = Reply::parse(payload) else {
            Self::push_parse_failed(out, crate::protocol::Command::UI, payload);
            return;
        };
        let Some(download) = &mut self.settings_snapshot else {
            return;
        };
        let result = match download.accept(reply, Instant::now()) {
            Ok(false) => return,
            Ok(true) => Ok(download.total as u64),
            Err(error) => Err(error),
        };
        out.push(self.settings_snapshot.take().unwrap().event(result));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempPath(PathBuf);
    impl TempPath {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("moonproto-snapshot-{:016x}.zip", rand::random::<u64>())))
        }
    }
    impl Drop for TempPath {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn reply(download: &Download, offset: i64, data: &[u8], archive: &[u8]) -> Vec<u8> {
        let mut out = vec![54, 4, 0];
        out.extend_from_slice(&1u64.to_le_bytes());
        out.extend_from_slice(&download.id.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&(archive.len() as i64).to_le_bytes());
        crate::commands::registry::write_string(&mut out, &format!("{:x}", Sha256::digest(archive)));
        crate::commands::registry::write_string(&mut out, "");
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn snapshot_reordered_repeated_and_foreign_parts_do_not_duplicate_bytes() {
        let path = TempPath::new();
        let mut download = Download::create(path.0.clone()).unwrap();
        let archive = b"settings snapshot archive";
        let metadata = reply(&download, -1, &[], archive);
        let first = reply(&download, 0, &archive[..8], archive);
        let last = reply(&download, 8, &archive[8..], archive);
        let eof = reply(&download, archive.len() as i64, &[], archive);
        let now = Instant::now();
        assert!(!download.accept(Reply::parse(&last).unwrap(), now).unwrap());
        assert!(!download.accept(Reply::parse(&metadata).unwrap(), now).unwrap());
        let mut foreign = first.clone();
        foreign[11..19].copy_from_slice(&download.id.wrapping_add(1).to_le_bytes());
        assert!(!download.accept(Reply::parse(&foreign).unwrap(), now).unwrap());
        assert_eq!(download.offset, 0);
        assert!(!download.accept(Reply::parse(&first).unwrap(), now).unwrap());
        let progress = download.progress;
        assert!(!download
            .accept(Reply::parse(&first).unwrap(), now + Duration::from_secs(200))
            .unwrap());
        assert_eq!(download.progress, progress);
        assert!(!download.accept(Reply::parse(&last).unwrap(), now).unwrap());
        assert_eq!(std::fs::read(&path.0).unwrap(), archive);
        assert!(download.accept(Reply::parse(&eof).unwrap(), now).unwrap());
    }

    #[test]
    fn snapshot_rejects_changed_archive_corruption_and_invalid_chunks() {
        for case in 0..5 {
            let path = TempPath::new();
            let mut download = Download::create(path.0.clone()).unwrap();
            let archive = b"test";
            let metadata = reply(&download, -1, &[], archive);
            download
                .accept(Reply::parse(&metadata).unwrap(), Instant::now())
                .unwrap();
            let packet = reply(&download, 0, archive, archive);
            let mut parsed = Reply::parse(&packet).unwrap();
            match case {
                0 => parsed.hash = "changed".into(),
                1 => parsed.total += 1,
                2 => parsed.data = b"xxxx",
                3 => parsed.data = &[],
                4 => parsed.data = b"longer",
                _ => unreachable!(),
            }
            assert!(download.accept(parsed, Instant::now()).is_err());
        }
    }

    #[test]
    fn snapshot_parser_enforces_core_chunk_bound_and_truncation() {
        let path = TempPath::new();
        let download = Download::create(path.0.clone()).unwrap();
        let packet = reply(&download, -1, &[], b"test");
        for end in 0..packet.len() {
            assert!(Reply::parse(&packet[..end]).is_none());
        }
        let data = vec![0; CHUNK_SIZE];
        assert!(Reply::parse(&reply(&download, 0, &data, &data)).is_some());
        let data = vec![0; CHUNK_SIZE + 1];
        assert!(Reply::parse(&reply(&download, 0, &data, &data)).is_none());
    }

    #[test]
    fn snapshot_completion_errors_timeout_and_existing_file_are_observable() {
        let path = TempPath::new();
        let mut dispatcher = EventDispatcher::new();
        dispatcher.start_settings_snapshot(path.0.clone());
        let download = dispatcher.settings_snapshot.as_ref().unwrap();
        let metadata = reply(download, -1, &[], b"test");
        let data = reply(download, 0, b"test", b"test");
        let eof = reply(download, 4, &[], b"test");
        let mut out = Vec::new();
        dispatcher.receive_settings_snapshot(&metadata, &mut out);
        dispatcher.receive_settings_snapshot(&data, &mut out);
        assert!(
            out.is_empty(),
            "completion requires the EOF response that releases the server export"
        );
        dispatcher.receive_settings_snapshot(&eof, &mut out);
        assert!(matches!(
            &out[..],
            [Event::Settings(SettingsEvent::SettingsSnapshotDownloaded {
                result: Ok(4),
                ..
            })]
        ));
        assert!(dispatcher.settings_snapshot.is_none());
        dispatcher.start_settings_snapshot(path.0.clone());
        assert!(matches!(
            &dispatcher.take_queued_events()[..],
            [Event::Settings(SettingsEvent::SettingsSnapshotDownloaded {
                result: Err(_),
                ..
            })]
        ));
        assert_eq!(std::fs::read(&path.0).unwrap(), b"test");

        let path2 = TempPath::new();
        dispatcher.start_settings_snapshot(path2.0.clone());
        dispatcher.settings_snapshot.as_mut().unwrap().progress = Instant::now() - NO_PROGRESS - Duration::from_secs(1);
        let client =
            crate::client::Client::new(crate::ClientConfig::new("127.0.0.1", 9, [0; 16], [0; 16]).without_ntp());
        dispatcher.poll_settings_snapshot(&client);
        assert!(dispatcher.settings_snapshot.is_none());
        assert!(matches!(&dispatcher.take_queued_events()[..],
            [Event::Settings(SettingsEvent::SettingsSnapshotDownloaded { result: Err(e), .. })] if e.contains("5 minutes")));

        let path3 = TempPath::new();
        dispatcher.start_settings_snapshot(path3.0.clone());
        let packet = reply(dispatcher.settings_snapshot.as_ref().unwrap(), -1, &[], b"test");
        let mut parsed = Reply::parse(&packet).unwrap();
        parsed.error = "Another settings download is in progress".into();
        assert_eq!(
            dispatcher
                .settings_snapshot
                .as_mut()
                .unwrap()
                .accept(parsed, Instant::now())
                .unwrap_err(),
            "Another settings download is in progress"
        );
        drop(dispatcher);
    }
}
