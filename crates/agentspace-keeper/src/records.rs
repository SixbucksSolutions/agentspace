//! The holder record's file, over `agentspace-keeper-files` and `agentspace-state` (docs/state.md,
//! The holder record; Replacing a file).

use crate::Records;
use agentspace_keeper_api::HolderRecord;
use agentspace_keeper_files::{HOLDER_RECORD_FILE, parse_holder_record, serialize_holder_record};
use agentspace_state::write_atomically;
use std::io::ErrorKind;
use std::path::Path;

/// The real [`Records`]: `<state folder>\board-holder.json`.
///
/// - **A read is the tolerant one of state.md**: a record that is missing, cannot be read, cannot
///   be parsed, or whose members do not have their forms is no record (`None`).
/// - **A write is strict to the schema, and atomic**: the bytes are `serialize_holder_record`'s,
///   and they replace the file with `write_atomically`, so a reader never sees half a record. A
///   record the schema refuses is not written.
/// - **A removal of a record that is already gone is `Ok`.**
///
/// It writes to no stream and never panics on bad input.
#[derive(Debug, Default, Clone, Copy)]
pub struct FileRecords {
    _private: (),
}

impl FileRecords {
    pub fn new() -> FileRecords {
        FileRecords { _private: () }
    }
}

impl Records for FileRecords {
    fn read(&self, state_dir: &Path) -> Option<HolderRecord> {
        let bytes = std::fs::read(state_dir.join(HOLDER_RECORD_FILE)).ok()?;
        parse_holder_record(&bytes)
    }

    fn write(&self, state_dir: &Path, record: &HolderRecord) -> Result<(), String> {
        let bytes = serialize_holder_record(record).map_err(|e| e.0)?;
        write_atomically(&state_dir.join(HOLDER_RECORD_FILE), &bytes).map_err(|e| e.to_string())
    }

    fn remove(&self, state_dir: &Path) -> Result<(), String> {
        match std::fs::remove_file(state_dir.join(HOLDER_RECORD_FILE)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> HolderRecord {
        HolderRecord {
            pid: 4242,
            process_started_at: "2026-11-19T04:05:06.123Z".to_string(),
            lease_id: "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d".to_string(),
            alias: "agentspace-demo-0a1b2c3d".to_string(),
        }
    }

    fn state() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("agentspace-keeper-unit-records-")
            .tempdir()
            .expect("A throwaway folder can be made")
    }

    fn file(dir: &tempfile::TempDir) -> std::path::PathBuf {
        dir.path().join("board-holder.json")
    }

    #[test]
    fn a_missing_record_is_no_record() {
        let dir = state();
        assert_eq!(FileRecords::new().read(dir.path()), None);
    }

    #[test]
    fn a_record_written_is_read_back_and_is_the_files_bytes() {
        let dir = state();
        let records = FileRecords::new();
        records.write(dir.path(), &record()).expect("It is written");
        assert_eq!(records.read(dir.path()), Some(record()));
        assert_eq!(
            std::fs::read_to_string(file(&dir)).expect("The file is UTF-8"),
            "{\"pid\":4242,\"process_started_at\":\"2026-11-19T04:05:06.123Z\",\
             \"lease_id\":\"0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d\",\
             \"alias\":\"agentspace-demo-0a1b2c3d\"}\n"
        );
    }

    #[test]
    fn a_write_replaces_the_record_and_leaves_no_other_file() {
        let dir = state();
        let records = FileRecords::new();
        records.write(dir.path(), &record()).unwrap();
        let mut second = record();
        second.pid = 77;
        records.write(dir.path(), &second).unwrap();
        assert_eq!(records.read(dir.path()), Some(second));
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("board-holder.json")]);
    }

    #[test]
    fn a_record_that_is_not_in_its_form_is_no_record() {
        let dir = state();
        for text in [
            "",
            "not json",
            "[]",
            "{\"pid\":0,\"process_started_at\":\"2026-11-19T04:05:06.123Z\",\"lease_id\":\"0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d\",\"alias\":\"agentspace-demo-0a1b2c3d\"}",
            "{\"pid\":7,\"process_started_at\":\"2026-11-19T04:05:06Z\",\"lease_id\":\"0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d\",\"alias\":\"agentspace-demo-0a1b2c3d\"}",
            "{\"pid\":7,\"process_started_at\":\"2026-11-19T04:05:06.123Z\",\"lease_id\":\"x\",\"alias\":\"agentspace-demo-0a1b2c3d\"}",
            "{\"pid\":7,\"process_started_at\":\"2026-11-19T04:05:06.123Z\",\"lease_id\":\"0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d\",\"alias\":\"-bad\"}",
        ] {
            std::fs::write(file(&dir), text).unwrap();
            assert_eq!(FileRecords::new().read(dir.path()), None, "For {text:?}");
        }
    }

    #[test]
    fn a_record_that_cannot_be_read_is_no_record() {
        // A folder where the file belongs cannot be read as a file.
        let dir = state();
        std::fs::create_dir(file(&dir)).unwrap();
        assert_eq!(FileRecords::new().read(dir.path()), None);
    }

    #[test]
    fn a_record_the_schema_refuses_is_not_written() {
        let dir = state();
        let mut bad = record();
        bad.alias = "plain".to_string();
        let error = FileRecords::new()
            .write(dir.path(), &bad)
            .expect_err("The schema refuses it");
        assert!(!error.is_empty());
        assert!(!file(&dir).exists());
    }

    #[test]
    fn a_write_into_a_missing_folder_is_an_error_and_makes_no_folder() {
        let dir = state();
        let missing = dir.path().join("none");
        let error = FileRecords::new()
            .write(&missing, &record())
            .expect_err("The folder is missing");
        assert!(!error.is_empty());
        assert!(!missing.exists());
    }

    #[test]
    fn a_removal_removes_the_record_and_one_already_gone_is_ok() {
        let dir = state();
        let records = FileRecords::new();
        records.write(dir.path(), &record()).unwrap();
        assert_eq!(records.remove(dir.path()), Ok(()));
        assert!(!file(&dir).exists());
        assert_eq!(records.remove(dir.path()), Ok(()));
        // A missing state folder holds no record either.
        assert_eq!(records.remove(&dir.path().join("none")), Ok(()));
    }

    #[test]
    fn a_record_that_cannot_be_removed_is_an_error() {
        // A folder where the file belongs cannot be removed as a file.
        let dir = state();
        std::fs::create_dir(file(&dir)).unwrap();
        let error = FileRecords::new()
            .remove(dir.path())
            .expect_err("A folder is not a record");
        assert!(!error.is_empty());
        assert!(file(&dir).is_dir());
    }
}
