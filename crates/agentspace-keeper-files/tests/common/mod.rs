//! Shared helpers for the keeper-files tests.
//!
//! Every call into `agentspace_keeper_files`' own API (beyond the pinned `agentspace-keeper-api`)
//! is gathered in the "Adapters" section below, so that the dev lead adapts the tests to the
//! implementation by editing only that section. No assertion calls the crate directly.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use agentspace_keeper_api::{HolderRecord, Matcher};
use serde_json::Value;

// ---- Test-side shapes, independent of the implementation's types ----

/// The launch request's members (docs/state.md, A holder's launch files).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub application: String,
    pub command_line: String,
    pub log: String,
    pub directory: String,
    pub not_after: String,
    pub result: String,
}

/// The launch result's two shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Started {
        pid: u32,
        process_started_at: String,
        command_line: Option<String>,
    },
    NotStarted(String),
}

// ---- Fixed fake values (the repo is public: nothing real) ----

pub const LEASE_ID: &str = "0123abcd-4def-4abc-9def-0123456789ab";
pub const ALIAS: &str = "agentspace-demo-0123abcd";
pub const STARTED_AT: &str = "2026-11-19T13:45:07.123Z";
pub const SYSTEM32: &str = r"C:\Windows\System32";

pub fn good_request() -> Request {
    Request {
        application: r"C:\Tools\OpenSSH\ssh.exe".into(),
        command_line: format!(
            r#""C:\Tools\OpenSSH\ssh.exe" -N -o ExitOnForwardFailure=yes {ALIAS}"#
        ),
        log: r"C:\Users\fake\.agentspace\board-holder-ssh.log".into(),
        directory: r"C:\Users\fake".into(),
        not_after: "2026-11-19T13:45:37.1234567Z".into(),
        result: r"C:\Users\fake\.agentspace\board-holder-launch.0a1b2c3d.result.json".into(),
    }
}

pub fn good_record() -> HolderRecord {
    HolderRecord {
        pid: 4242,
        process_started_at: STARTED_AT.into(),
        lease_id: LEASE_ID.into(),
        alias: ALIAS.into(),
    }
}

// ---- Schemas ----

fn schema_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas/state")
        .join(name)
}

pub fn validator(schema_file: &str) -> jsonschema::Validator {
    let text = std::fs::read_to_string(schema_path(schema_file)).expect("The schema is readable");
    let schema: Value = serde_json::from_str(&text).expect("The schema is JSON");
    jsonschema::validator_for(&schema).expect("The schema compiles")
}

/// Asserts that `bytes` are one compact JSON line, UTF-8 without a byte order mark, ending in one
/// LF, and that the document meets the schema in `schema_file`.
pub fn assert_valid_json_file(bytes: &[u8], schema_file: &str) -> Value {
    assert!(
        !bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "A byte order mark leads the file"
    );
    let text = std::str::from_utf8(bytes).expect("The file is UTF-8");
    assert!(text.ends_with('\n'), "The file ends in an LF: {text:?}");
    assert!(
        !text[..text.len() - 1].contains('\n'),
        "Only one LF, at the end: {text:?}"
    );
    let value: Value = serde_json::from_str(text).expect("The file is JSON");
    let compact = serde_json::to_string(&value).unwrap();
    assert_eq!(
        compact,
        &text[..text.len() - 1],
        "The JSON is compact, in the document's own member order"
    );
    let v = validator(schema_file);
    let errors: Vec<String> = v.iter_errors(&value).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "Schema errors: {errors:?} in {text:?}");
    value
}

// ---- Adapters: the only calls into agentspace_keeper_files ----

use agentspace_keeper_files as kf;

/// The holder record's bytes, as a writer MUST write them. A valid record that the writer refuses
/// fails the test.
pub fn holder_record_bytes(r: &HolderRecord) -> Vec<u8> {
    kf::serialize_holder_record(r)
        .unwrap_or_else(|e| panic!("The writer refused a valid holder record: {e:?}"))
}

/// A reader's view of a holder record's bytes: `None` is "no record".
pub fn parse_holder_record(bytes: &[u8]) -> Option<HolderRecord> {
    kf::parse_holder_record(bytes)
}

/// The request's bytes. The writer is strict to the schema, so a request that does not meet the
/// schema (a test of the reader's refusals) is written here the way a writer without that check
/// would, compact with one LF. A request that does meet the schema and that the writer refuses
/// fails the test.
pub fn request_bytes(r: &Request) -> Vec<u8> {
    let request = kf::LaunchRequest {
        application: r.application.clone(),
        command_line: r.command_line.clone(),
        log: r.log.clone(),
        directory: r.directory.clone(),
        not_after: r.not_after.clone(),
        result: r.result.clone(),
    };
    match kf::serialize_launch_request(&request) {
        Ok(bytes) => bytes,
        Err(e) => {
            let doc = serde_json::json!({
                "application": r.application,
                "command_line": r.command_line,
                "log": r.log,
                "directory": r.directory,
                "not_after": r.not_after,
                "result": r.result,
            });
            assert!(
                !validator("board-holder-launch.state.schema.json").is_valid(&doc),
                "The writer refused a request that meets the schema: {e:?}"
            );
            let mut bytes = serde_json::to_vec(&doc).unwrap();
            bytes.push(b'\n');
            bytes
        }
    }
}

pub fn parse_request(bytes: &[u8]) -> Option<Request> {
    kf::parse_launch_request(bytes).ok().map(|r| Request {
        application: r.application,
        command_line: r.command_line,
        log: r.log,
        directory: r.directory,
        not_after: r.not_after,
        result: r.result,
    })
}

pub fn answer_bytes(a: &Answer) -> Vec<u8> {
    kf::serialize_launch_result(&match a {
        Answer::Started {
            pid,
            process_started_at,
            command_line,
        } => kf::LaunchResult::Started {
            pid: *pid,
            process_started_at: process_started_at.clone(),
            command_line: command_line.clone(),
        },
        Answer::NotStarted(e) => kf::LaunchResult::NotStarted { error: e.clone() },
    })
    .unwrap_or_else(|e| panic!("The writer refused a valid launch result: {e:?}"))
}

pub fn parse_answer(bytes: &[u8]) -> Option<Answer> {
    kf::parse_launch_result(bytes).ok().map(|r| match r {
        kf::LaunchResult::Started {
            pid,
            process_started_at,
            command_line,
        } => Answer::Started {
            pid,
            process_started_at,
            command_line,
        },
        kf::LaunchResult::NotStarted { error } => Answer::NotStarted(error),
    })
}

/// The claim a launcher writes: `launcher <pid>` and an LF.
pub fn launcher_claim_bytes(pid: u32) -> Vec<u8> {
    kf::serialize_claim(kf::Claim::Launcher(pid))
}

/// The claim the pass writes to withdraw a request: `abandoned` and an LF.
pub fn abandoned_claim_bytes() -> Vec<u8> {
    kf::serialize_claim(kf::Claim::Abandoned)
}

/// The keeper lock file's content for a keeper's PID.
pub fn keeper_lock_bytes(pid: u32) -> Vec<u8> {
    kf::serialize_keeper_pid(pid)
}

/// What a second keeper reads from the first 32 bytes it can: the digits, as text, when they are
/// 1 to 10 digits after trimming blanks (keeper.md, board-holder-keeper.lock).
pub fn parse_keeper_lock_pid(bytes: &[u8]) -> Option<String> {
    kf::keeper_pid_digits(bytes)
}

/// The matcher, for `<System32>` = [`SYSTEM32`].
pub fn matcher() -> impl Matcher {
    kf::HolderMatcher::new(Path::new(SYSTEM32))
}

/// The command-line splitter, as `CommandLineToArgvW` does (keeper.md, The holder command, rule 1).
pub fn split(line: &str) -> Vec<String> {
    kf::split_command_line(line)
}
