use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Mutex;

use phi_kernel::ports::JournalPort;

/// Largest amount read while scanning backwards for the last newline.
const REPAIR_CHUNK_BYTES: u64 = 8 * 1024;

/// Discard a torn trailing record so the next append starts on a fresh line.
fn truncate_torn_tail(file: &mut File) -> Result<(), String> {
    let len = file
        .metadata()
        .map_err(|e| format!("stat journal: {e}"))?
        .len();
    if len == 0 {
        return Ok(());
    }
    file.seek(SeekFrom::End(-1))
        .map_err(|e| format!("seek journal: {e}"))?;
    let mut last = [0u8; 1];
    file.read_exact(&mut last)
        .map_err(|e| format!("read journal: {e}"))?;
    if last[0] == b'\n' {
        return Ok(());
    }

    // Scan backwards in bounded chunks so a large journal is never loaded into
    // memory just to repair its tail.
    let mut end = len;
    let mut cut = 0u64;
    while end > 0 {
        let start = end.saturating_sub(REPAIR_CHUNK_BYTES);
        let size = (end - start) as usize;
        let mut buf = vec![0u8; size];
        file.seek(SeekFrom::Start(start))
            .map_err(|e| format!("seek journal: {e}"))?;
        file.read_exact(&mut buf)
            .map_err(|e| format!("read journal: {e}"))?;
        if let Some(pos) = buf.iter().rposition(|&byte| byte == b'\n') {
            cut = start + pos as u64 + 1;
            break;
        }
        end = start;
    }

    file.set_len(cut)
        .map_err(|e| format!("truncate journal: {e}"))?;
    Ok(())
}

/// Append-only JSONL journal. Every append is flushed and synced before the
/// call returns, so subscribers only ever observe durable events.
pub struct JsonlJournal {
    path: PathBuf,
    lock: Mutex<()>,
}

impl JsonlJournal {
    pub fn new(path: PathBuf) -> Self {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        Self {
            path,
            lock: Mutex::new(()),
        }
    }
}

impl JournalPort for JsonlJournal {
    fn append(&self, line: &str) -> Result<(), String> {
        let _guard = self.lock.lock().unwrap();
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&self.path)
            .map_err(|e| format!("open journal: {e}"))?;
        // Repair a torn tail first. A crash can leave the last record without
        // its terminating newline; appending directly would concatenate two
        // events into one unparseable line. Truncating the fragment keeps the
        // journal readable by replay, which only tolerates a bad final line.
        truncate_torn_tail(&mut file)?;
        file.seek(SeekFrom::End(0))
            .map_err(|e| format!("seek journal: {e}"))?;
        file.write_all(line.as_bytes())
            .map_err(|e| format!("write journal: {e}"))?;
        file.write_all(b"\n")
            .map_err(|e| format!("write journal: {e}"))?;
        file.flush().map_err(|e| format!("flush journal: {e}"))?;
        file.sync_data().map_err(|e| format!("sync journal: {e}"))?;
        Ok(())
    }

    fn read_all(&self) -> Result<Vec<String>, String> {
        let _guard = self.lock.lock().unwrap();
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        // Read bytes and decode lossily: a tail truncated in the middle of a
        // UTF-8 sequence must not make the whole journal unreadable. The
        // resulting invalid line simply fails JSON parsing and is skipped.
        let raw = fs::read(&self.path).map_err(|e| format!("read journal: {e}"))?;
        let text = String::from_utf8_lossy(&raw);
        Ok(text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.to_string())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Arc;

    use phi_kernel::event_bus::EventBus;

    use crate::system::{StderrLog, SystemClock, UuidIds};

    fn temp_journal(name: &str) -> (PathBuf, JsonlJournal) {
        let dir = std::env::temp_dir().join(format!("phi-journal-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("events.jsonl");
        (path.clone(), JsonlJournal::new(path))
    }

    fn assert_all_json(lines: &[String]) {
        for line in lines {
            serde_json::from_str::<serde_json::Value>(line).unwrap();
        }
    }

    #[test]
    fn repairs_torn_tail_before_appending() {
        let (path, journal) = temp_journal("torn");
        // Simulate a crash between writing the JSON payload and its newline.
        fs::write(&path, b"{\"partial\":true}").unwrap();
        journal.append("{\"ok\":1}").unwrap();

        let lines = journal.read_all().unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "{\"ok\":1}");
    }

    #[test]
    fn keeps_complete_line_and_discards_partial_tail() {
        let (path, journal) = temp_journal("partial-tail");
        fs::write(&path, b"{\"a\":1}\n{\"partial\":true}").unwrap();
        journal.append("{\"ok\":1}").unwrap();

        let lines = journal.read_all().unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "{\"a\":1}");
        assert_eq!(lines[1], "{\"ok\":1}");
    }

    #[test]
    fn torn_tail_repair_survives_restart() {
        let (path, journal) = temp_journal("restart");
        fs::write(&path, b"{\"partial\":true}").unwrap();
        journal.append("{\"event\":1}").unwrap();

        // A fresh journal for the same path must append after the repaired
        // prefix, leaving only complete and parseable lines.
        let second = JsonlJournal::new(path.clone());
        second.append("{\"event\":2}").unwrap();

        let lines = second.read_all().unwrap();
        assert_eq!(lines.len(), 2);
        assert_all_json(&lines);
    }

    #[test]
    fn event_bus_restore_succeeds_after_torn_tail_repair() {
        let (path, _journal) = temp_journal("bus-restore");
        fs::write(&path, b"{\"partial\":true}").unwrap();

        // Publishing repairs the torn tail before writing the first complete
        // record, so replay sees only well-formed lines.
        let bus = EventBus::new(
            Arc::new(JsonlJournal::new(path.clone())),
            Arc::new(SystemClock::new()),
            Arc::new(UuidIds),
            Arc::new(StderrLog),
        );
        bus.publish("sess_1", "test", &serde_json::json!({ "type": "Test" }))
            .unwrap();

        let restored = EventBus::new(
            Arc::new(JsonlJournal::new(path.clone())),
            Arc::new(SystemClock::new()),
            Arc::new(UuidIds),
            Arc::new(StderrLog),
        );
        let records = restored.restore().unwrap();
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn tolerates_truncated_utf8_tail() {
        let (path, journal) = temp_journal("utf8");
        let mut bytes = b"{\"a\":1}\n".to_vec();
        // A multi-byte character cut in half, with no terminating newline.
        bytes.extend_from_slice(&[0xE4, 0xB8]);
        fs::write(&path, &bytes).unwrap();

        let lines = journal.read_all().unwrap();
        assert_eq!(lines[0], "{\"a\":1}");
        assert_eq!(lines.len(), 2);
    }
}
