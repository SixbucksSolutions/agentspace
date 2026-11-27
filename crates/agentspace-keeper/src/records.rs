//! The holder records' files, over `agentspace-keeper-files` and `agentspace-state` (docs/state.md,
//! The holder record; Another host's holder; Replacing a file).

use crate::Records;
use crate::hosts::box_key;
use agentspace_hosts::HostKey;
use agentspace_keeper_api::HolderRecord;
use agentspace_keeper_files::{
    holder_record_file_keyed, parse_holder_record_keyed, serialize_holder_record_keyed,
};
use agentspace_state::write_atomically;
use std::io::ErrorKind;
use std::path::Path;

/// The real [`Records`]: `<state folder>\board-holder.json` for the box, and
/// `<state folder>\board-holder.<host name>.json` for another host, each in the same form
/// (state.md, Another host's holder; pin P12).
///
/// - **A read is the tolerant one of state.md**: a record that is missing, cannot be read, cannot
///   be parsed, or whose members do not have their forms is no record (`None`), and so is one
///   whose platform, `linux/arm64` when it holds none, is not the platform of the host its file is
///   named for (keeper.md, Trusting the record, item 1; pins P5 and P16). So a record written
///   before v0.12 is read as a `linux/arm64` host's.
/// - **A write is strict to the schema, and atomic**: the bytes are
///   `serialize_holder_record_keyed`'s, with `platform` only for a host that is not `linux/arm64`,
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
        read_file(state_dir, &box_key())
    }

    fn write(&self, state_dir: &Path, record: &HolderRecord) -> Result<(), String> {
        write_file(state_dir, &box_key(), record)
    }

    fn remove(&self, state_dir: &Path) -> Result<(), String> {
        remove_file(state_dir, &box_key())
    }
}

/// A host's record file, read tolerantly, as the record of a host of `host`'s platform.
pub(crate) fn read_file(state_dir: &Path, host: &HostKey) -> Option<HolderRecord> {
    let bytes = std::fs::read(state_dir.join(holder_record_file_keyed(host))).ok()?;
    parse_holder_record_keyed(&bytes, host)
}

/// Writes a host's record file atomically, strict to the schema.
pub(crate) fn write_file(
    state_dir: &Path,
    host: &HostKey,
    record: &HolderRecord,
) -> Result<(), String> {
    let bytes = serialize_holder_record_keyed(record, host).map_err(|e| e.0)?;
    write_atomically(&state_dir.join(holder_record_file_keyed(host)), &bytes)
        .map_err(|e| e.to_string())
}

/// Removes a host's record file; one already gone is `Ok`.
pub(crate) fn remove_file(state_dir: &Path, host: &HostKey) -> Result<(), String> {
    match std::fs::remove_file(state_dir.join(holder_record_file_keyed(host))) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
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

    #[test]
    fn another_hosts_record_is_its_own_file_and_leaves_the_boxs_alone() {
        let dir = state();
        let records = FileRecords::new();
        let demo = HostKey::linux_arm64("demo");
        records
            .write_host(dir.path(), &demo, &record())
            .expect("It is written");
        assert!(dir.path().join("board-holder.demo.json").is_file());
        assert!(!file(&dir).exists());
        assert_eq!(records.read_host(dir.path(), &demo), Some(record()));
        assert_eq!(records.read_host(dir.path(), &box_key()), None);
        assert_eq!(records.read(dir.path()), None);
        assert_eq!(records.remove_host(dir.path(), &demo), Ok(()));
        assert!(!dir.path().join("board-holder.demo.json").exists());
        assert_eq!(records.remove_host(dir.path(), &demo), Ok(()));
    }

    /// keeper.md, A host's key and name: two hosts of one project have two records, and a host of
    /// another platform's record names its platform; a `linux/arm64` record stays v0.11's bytes
    /// (pins P12, P14 and P16).
    #[test]
    fn two_hosts_of_one_project_have_two_records_and_only_off_linux_arm64_one_names_its_platform() {
        let dir = state();
        let records = FileRecords::new();
        let arm = HostKey::linux_arm64("demo");
        let win = HostKey::new("demo", "windows/amd64");
        let mut other = record();
        other.pid = 77;
        records.write_host(dir.path(), &arm, &record()).unwrap();
        records.write_host(dir.path(), &win, &other).unwrap();
        let arm_file = dir.path().join("board-holder.demo.json");
        let win_file = dir.path().join("board-holder.demo.windows-amd64.json");
        assert!(
            !std::fs::read_to_string(&arm_file)
                .unwrap()
                .contains("platform")
        );
        assert!(
            std::fs::read_to_string(&win_file)
                .unwrap()
                .ends_with(",\"platform\":\"windows/amd64\"}\n")
        );
        assert_eq!(records.read_host(dir.path(), &arm), Some(record()));
        assert_eq!(records.read_host(dir.path(), &win), Some(other.clone()));
        records.remove_host(dir.path(), &win).unwrap();
        assert!(!win_file.exists());
        assert_eq!(records.read_host(dir.path(), &arm), Some(record()));
        // The box's project on another platform is another host, never the box.
        let swim_win = HostKey::new("swimcoach", "windows/amd64");
        records.write_host(dir.path(), &swim_win, &other).unwrap();
        assert!(
            dir.path()
                .join("board-holder.swimcoach.windows-amd64.json")
                .is_file()
        );
        assert_eq!(records.read(dir.path()), None);
    }

    /// keeper.md, Trusting the record, item 1: a record whose platform is not its file's host's is
    /// no record, and a record written before v0.12, with no `platform`, is `linux/arm64`'s (pin
    /// P16).
    #[test]
    fn a_record_whose_platform_is_not_its_files_is_no_record_and_an_old_one_is_linux_arm64s() {
        let dir = state();
        let records = FileRecords::new();
        let win = HostKey::new("demo", "windows/amd64");
        let old = "{\"pid\":4242,\"process_started_at\":\"2026-11-19T04:05:06.123Z\",\"lease_id\":\"0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d\",\"alias\":\"agentspace-demo-0a1b2c3d\"}\n";
        std::fs::write(dir.path().join("board-holder.demo.json"), old).unwrap();
        std::fs::write(file(&dir), old).unwrap();
        std::fs::write(dir.path().join("board-holder.demo.windows-amd64.json"), old).unwrap();
        assert_eq!(
            records.read_host(dir.path(), &HostKey::linux_arm64("demo")),
            Some(record())
        );
        assert_eq!(records.read(dir.path()), Some(record()));
        assert_eq!(records.read_host(dir.path(), &win), None);
        let amd = old.replace("}\n", ",\"platform\":\"linux/amd64\"}\n");
        std::fs::write(
            dir.path().join("board-holder.demo.windows-amd64.json"),
            &amd,
        )
        .unwrap();
        assert_eq!(records.read_host(dir.path(), &win), None);
        let explicit = old.replace("}\n", ",\"platform\":\"linux/arm64\"}\n");
        std::fs::write(dir.path().join("board-holder.demo.json"), &explicit).unwrap();
        assert_eq!(
            records.read_host(dir.path(), &HostKey::linux_arm64("demo")),
            Some(record())
        );
    }
}
