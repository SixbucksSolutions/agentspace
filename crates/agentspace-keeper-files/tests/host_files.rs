//! The box's holder files are unchanged, and another host's record is in the box's form
//! (docs/state.md, The holder record, Another host's holder; docs/keeper.md, Stopping and
//! starting the keeper; E0.07-T03-S05, #0154).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. D55 pins the box's files
//! as v0.06 left them, so that swimcoach's side reads the same and a keeper upgraded with the box's
//! holder running adopts it from `board-holder.json`: the names do not change, and v0.06's bytes
//! are read, and written, exactly as before. Another host's record, `board-holder.<project>.json`,
//! is "in the box's file's form": the same members, bytes and reader rules, with the host's own
//! lease and alias in it.
//!
//! The functions that name each host's files are tested in `host_names.rs`.

use agentspace_keeper_api::HolderRecord;
use agentspace_keeper_files::{
    HOLDER_RECORD_FILE, KEEPER_LOCK_FILE, PASS_LOCK_FILE, parse_holder_record,
    serialize_holder_record,
};

/// A holder record exactly as v0.06 wrote it: compact JSON in the order `pid`,
/// `process_started_at`, `lease_id`, `alias`, then one LF.
const V006_RECORD: &str = "{\"pid\":24112,\"process_started_at\":\"2026-11-18T05:07:00.123Z\",\
\"lease_id\":\"0b6f4c1e-8d1a-4f0e-9c3b-2a5d7e9f1c44\",\
\"alias\":\"agentspace-swimcoach-0b6f4c1e\"}\n";

fn v006_record() -> HolderRecord {
    HolderRecord {
        pid: 24112,
        process_started_at: "2026-11-18T05:07:00.123Z".to_string(),
        lease_id: "0b6f4c1e-8d1a-4f0e-9c3b-2a5d7e9f1c44".to_string(),
        alias: "agentspace-swimcoach-0b6f4c1e".to_string(),
    }
}

/// The box's file names keep v0.06's, with no project in them (D55, "The box's own files"), and
/// the keeper's two lock files, which serve every host, keep theirs.
#[test]
fn the_boxs_file_names_are_unchanged() {
    assert_eq!(HOLDER_RECORD_FILE, "board-holder.json");
    assert_eq!(PASS_LOCK_FILE, "board-holder.lock");
    assert_eq!(KEEPER_LOCK_FILE, "board-holder-keeper.lock");
}

/// An upgraded keeper reads v0.06's record: the bytes v0.06 wrote are a record, with the members
/// as written.
#[test]
fn v006s_record_is_read_by_the_upgraded_reader() {
    assert_eq!(
        parse_holder_record(V006_RECORD.as_bytes()),
        Some(v006_record())
    );
}

/// The writer's bytes for that record are v0.06's, to the byte, so that nothing about the box's
/// record moves.
#[test]
fn the_writer_gives_v006s_bytes_for_the_box() {
    assert_eq!(
        serialize_holder_record(&v006_record()).unwrap(),
        V006_RECORD.as_bytes()
    );
}

/// Another host's record is the same form with the host's own lease and alias in it: the strict
/// writer takes an alias of that host's project, hyphens included, and the reader gives the
/// record back.
#[test]
fn another_hosts_record_is_in_the_boxs_form() {
    for alias in [
        "agentspace-alpha-3f2a9c1e",
        "agentspace-agentspace-3f2a9c1e",
        "agentspace-my-project-3f2a9c1e",
    ] {
        let record = HolderRecord {
            pid: 31337,
            process_started_at: "2026-11-20T08:09:10.001Z".to_string(),
            lease_id: "3f2a9c1e-6b7d-4c8e-9f01-2d3e4f5a6b7c".to_string(),
            alias: alias.to_string(),
        };
        let bytes = serialize_holder_record(&record).unwrap_or_else(|e| panic!("{alias}: {e:?}"));
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(
            text.starts_with("{\"pid\":31337,\"process_started_at\":"),
            "{text}"
        );
        assert!(
            text.ends_with("}\n") && text.matches('\n').count() == 1,
            "{text:?}"
        );
        assert_eq!(parse_holder_record(&bytes), Some(record), "{alias}");
    }
}
