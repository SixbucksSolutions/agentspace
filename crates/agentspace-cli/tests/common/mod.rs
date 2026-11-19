//! Helpers shared by the integration tests of the lease CLI's front (E0.05-T01-S01).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every expectation is taken
//! from docs/lease-api.md and schemas/error.output.schema.json, never from the program's code (D31).
//! The tests run the built binary in a throwaway working folder with a throwaway home, and never read
//! or write the real `~/.agentspace` or `~/.ssh`.

#![allow(dead_code)]

use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// The built binary, `agentspace` (`agentspace.exe` on Windows).
pub const BIN: &str = env!("CARGO_BIN_EXE_agentspace");

/// A well-formed lease ID, for lines that need one.
pub const ID: &str = "3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f";

/// What a program run left behind.
pub struct Run {
    pub status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub fn os(text: &str) -> OsString {
    OsString::from(text)
}

pub fn args(list: &[&str]) -> Vec<OsString> {
    list.iter().map(|a| os(a)).collect()
}

/// `before`, a value that is not valid Unicode (an unpaired surrogate on Windows, a byte that
/// starts no UTF-8 character elsewhere), then `after`.
pub fn not_unicode(before: &str, after: &str) -> OsString {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let mut wide: Vec<u16> = before.encode_utf16().collect();
        wide.push(0xD800);
        wide.extend(after.encode_utf16());
        OsString::from_wide(&wide)
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::ffi::OsStringExt;
        let mut bytes = before.as_bytes().to_vec();
        bytes.push(0xff);
        bytes.extend(after.as_bytes());
        OsString::from_vec(bytes)
    }
}

/// A throwaway folder under the temp folder, used as the working folder and the home of every run.
pub fn scratch_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("agentspace-cli-tests");
    std::fs::create_dir_all(&dir).expect("The scratch folder can be made");
    dir
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The binary's command, with a throwaway home and none of agentspace's own variables set.
pub fn command(line: &[OsString]) -> Command {
    let scratch = scratch_dir();
    let mut cmd = Command::new(BIN);
    cmd.args(line)
        .current_dir(&scratch)
        .env("HOME", &scratch)
        .env("USERPROFILE", &scratch)
        .env_remove("AGENTSPACE_STATE_DIR")
        .env_remove("AGENTSPACE_SSH_DIR")
        .env_remove("AGENTSPACE_PROXY_COMMAND")
        .stdin(Stdio::null());
    cmd
}

/// Runs a prepared command to its end, and fails the test when it takes more than a minute.
pub fn run_command(mut cmd: Command) -> Run {
    let child = cmd.spawn().expect("The binary starts");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let out = rx
        .recv_timeout(Duration::from_secs(60))
        .expect("The binary ends within a minute")
        .expect("The binary's output can be read");
    Run {
        status: out.status.code(),
        stdout: out.stdout,
        stderr: out.stderr,
    }
}

/// Runs the binary with both output streams captured.
pub fn run_bin(line: &[OsString]) -> Run {
    let mut cmd = command(line);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    run_command(cmd)
}

pub fn run_str(line: &[&str]) -> Run {
    run_bin(&args(line))
}

/// The number inside an `ExitCode`, which has no accessor.
pub fn exit_number(code: ExitCode) -> i32 {
    let text = format!("{code:?}");
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("No number in the exit code's debug text '{text}'"))
}

/// Runs the program in this process through `execute_in`, with the arguments after the program's
/// name, in a world of its own (E0.05-T03-S02): `USERPROFILE` is a fresh throwaway folder under the
/// temp folder and no other variable is set, the working folder is that folder, the clock and the IDs
/// are fixed, and there is no driver. It never builds a world from the real process, so it never
/// reads or writes a real home.
pub fn in_process_outcome(line: &[OsString]) -> agentspace_cli::Outcome {
    static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let home = scratch_dir().join(format!("in-process-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&home).expect("The throwaway home can be made");
    let home_var = home.clone().into_os_string();
    let var = move |k: &str| (k == "USERPROFILE").then(|| home_var.clone());
    let now = || 1_800_000_000_i64;
    let new_id = || "3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f".to_string();
    let own_exe = PathBuf::from(BIN);
    let world = agentspace_cli::World {
        var: &var,
        cwd: Some(&home),
        own_exe: &own_exe,
        now: &now,
        new_id: &new_id,
        drivers: &[],
    };
    let out = agentspace_cli::execute_in(line, &world);
    let _ = std::fs::remove_dir_all(&home);
    out
}

/// The exit code of `in_process_outcome`.
pub fn in_process(line: &[OsString]) -> i32 {
    i32::from(in_process_outcome(line).exit_code)
}

/// The exit code that each error code MUST come with (lease-api.md, Exit codes and errors).
pub fn exit_for(code: &str) -> i32 {
    match code {
        "internal" => 1,
        "usage" | "invalid_value" | "out_of_range" => 2,
        "not_found" => 3,
        "invalid_state" => 4,
        other => panic!("No exit code for the error code '{other}'"),
    }
}

/// True when the text holds no whitespace outside its JSON strings (lease-api.md, Output).
pub fn is_compact(text: &str) -> bool {
    let mut in_string = false;
    let mut escaped = false;
    for ch in text.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
        } else if ch == '"' {
            in_string = true;
        } else if matches!(ch, ' ' | '\t' | '\r' | '\n') {
            return false;
        }
    }
    true
}

// ---- The help blocks of docs/lease-api.md ------------------------------------------------------

fn is_level(name: &str) -> bool {
    let parts: Vec<&str> = name.split(' ').collect();
    match parts.as_slice() {
        ["agentspace"] | ["agentspace", "lease"] => true,
        ["agentspace", "lease", sub] => {
            !sub.is_empty() && sub.chars().all(|c| c.is_ascii_lowercase())
        }
        _ => false,
    }
}

/// The help blocks of docs/lease-api.md by level (`agentspace`, `agentspace lease`, ...), each as the
/// text `--help` MUST print: the lines of the block, each ending in a line feed.
pub fn help_blocks() -> BTreeMap<String, String> {
    let path = repo_root().join("docs").join("lease-api.md");
    let text = std::fs::read_to_string(&path)
        .expect("docs/lease-api.md can be read")
        .replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut blocks = BTreeMap::new();
    let mut level: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(name) = line.strip_prefix("### ") {
            if is_level(name) {
                level = Some(name.to_string());
            }
        } else if line == "**Help:**"
            && let Some(name) = level.take()
        {
            let mut j = i + 1;
            while j < lines.len() && lines[j] != "```" {
                j += 1;
            }
            let mut block = String::new();
            j += 1;
            while j < lines.len() && lines[j] != "```" {
                block.push_str(lines[j]);
                block.push('\n');
                j += 1;
            }
            blocks.insert(name, block);
            i = j;
        }
        i += 1;
    }
    blocks
}

/// The seven levels, as the help blocks name them, and the words that reach each.
pub const LEVELS: [(&str, &[&str]); 7] = [
    ("agentspace", &[]),
    ("agentspace lease", &["lease"]),
    ("agentspace lease create", &["lease", "create"]),
    ("agentspace lease get", &["lease", "get"]),
    ("agentspace lease renew", &["lease", "renew"]),
    ("agentspace lease release", &["lease", "release"]),
    ("agentspace lease list", &["lease", "list"]),
];

/// The problems with a run that was meant to print one level's help: exit 0, nothing on stderr, and
/// stdout byte for byte the doc's block.
pub fn help_problems(run: &Run, level: &str) -> Vec<String> {
    let blocks = help_blocks();
    let mut p = Vec::new();
    let Some(block) = blocks.get(level) else {
        return vec![format!("docs/lease-api.md has no help block for '{level}'")];
    };
    if block.is_empty() {
        return vec![format!("The doc's help block for '{level}' is empty")];
    }
    if run.status != Some(0) {
        p.push(format!("Exit {:?}, not 0", run.status));
    }
    if !run.stderr.is_empty() {
        p.push(format!("Stderr holds {} bytes", run.stderr.len()));
    }
    if run.stdout != block.as_bytes() {
        p.push(format!(
            "Stdout is {} bytes and not the doc's {}: {:?}",
            run.stdout.len(),
            block.len(),
            String::from_utf8_lossy(&run.stdout)
        ));
    }
    p
}

// ---- The error document ------------------------------------------------------------------------

pub fn error_schema() -> Value {
    let path = repo_root().join("schemas").join("error.output.schema.json");
    let text = std::fs::read_to_string(path).expect("The error schema can be read");
    serde_json::from_str(&text).expect("The error schema is JSON")
}

fn schema_enum(schema: &Value, member: &str) -> Vec<String> {
    schema["properties"][member]["enum"]
        .as_array()
        .unwrap_or_else(|| panic!("The schema has no enum for '{member}'"))
        .iter()
        .map(|v| v.as_str().expect("An enum holds strings").to_string())
        .collect()
}

fn is_lease_id(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() != 36 {
        return false;
    }
    b.iter().enumerate().all(|(i, c)| match i {
        8 | 13 | 18 | 23 => *c == b'-',
        14 => *c == b'4',
        19 => matches!(*c, b'8' | b'9' | b'a' | b'b'),
        _ => c.is_ascii_digit() || (b'a'..=b'f').contains(c),
    })
}

/// Checks a document against error.output.schema.json by hand: the members and their patterns, and
/// what each code requires and forbids. The `field` rule for `usage` is the spec's, not the schema's.
pub fn shape_problems(doc: &Value, schema: &Value) -> Vec<String> {
    let mut p = Vec::new();
    let Some(obj) = doc.as_object() else {
        return vec!["The document is not an object".to_string()];
    };
    let props = schema["properties"]
        .as_object()
        .expect("The schema has properties");
    for key in obj.keys() {
        if !props.contains_key(key) {
            p.push(format!("An unknown member '{key}'"));
        }
    }
    let code = obj.get("code").and_then(Value::as_str);
    match code {
        Some(c) if schema_enum(schema, "code").iter().any(|k| k == c) => {}
        other => p.push(format!("The code {other:?} is not in the schema's list")),
    }
    match obj.get("message").and_then(Value::as_str) {
        None => p.push("No message string".to_string()),
        Some(m) => {
            let mut chars = m.chars();
            match chars.next() {
                None => p.push("The message is empty".to_string()),
                Some(first) => {
                    if (first as u32) <= 0x20 || first == '\u{7f}' {
                        p.push(
                            "The message starts with a space or a control character".to_string(),
                        );
                    }
                }
            }
            if m.chars().any(|c| (c as u32) <= 0x1f || c == '\u{7f}') {
                p.push("The message holds a control character".to_string());
            }
        }
    }
    if let Some(v) = obj.get("field") {
        match v.as_str() {
            Some(f) if schema_enum(schema, "field").iter().any(|k| k == f) => {}
            _ => p.push(format!("The field {v} is not in the schema's list")),
        }
    }
    if let Some(v) = obj.get("lease_id")
        && !v.as_str().is_some_and(is_lease_id)
    {
        p.push(format!("The lease_id {v} is not a lowercase UUIDv4"));
    }
    if let Some(v) = obj.get("state") {
        match v.as_str() {
            Some(s) if schema_enum(schema, "state").iter().any(|k| k == s) => {}
            _ => p.push(format!("The state {v} is not in the schema's list")),
        }
    }
    let has = |k: &str| obj.contains_key(k);
    match code {
        Some("internal") => {
            if has("field") || has("lease_id") || has("state") {
                p.push("An internal error carries no field, lease_id or state".to_string());
            }
        }
        Some("usage") => {
            if has("lease_id") || has("state") {
                p.push("A usage error carries no lease_id or state".to_string());
            }
        }
        Some("invalid_value" | "out_of_range") => {
            if !has("field") {
                p.push("No field".to_string());
            }
            if has("lease_id") || has("state") {
                p.push("No lease_id or state is allowed".to_string());
            }
        }
        Some("not_found") => {
            if !has("lease_id") || has("field") || has("state") {
                p.push("not_found needs lease_id, and no field or state".to_string());
            }
        }
        Some("invalid_state") if !has("lease_id") || !has("state") || has("field") => {
            p.push("invalid_state needs lease_id and state, and no field".to_string());
        }
        _ => {}
    }
    p
}

/// The problems with a run that was meant to fail (lease-api.md, Output and Exit codes and errors):
/// the exit code of its error code, nothing on stdout, one compact error document on stderr with one
/// line feed, no byte order mark, valid in shape, and the code, field and message expected. A `None`
/// field means the error MUST NOT carry one. A `None` message is not compared.
pub fn failure_problems(
    run: &Run,
    code: &str,
    field: Option<&str>,
    message: Option<&str>,
) -> Vec<String> {
    let mut p = Vec::new();
    let want = exit_for(code);
    if run.status != Some(want) {
        p.push(format!("Exit {:?}, not {want}", run.status));
    }
    if !run.stdout.is_empty() {
        p.push(format!("Stdout holds {} bytes", run.stdout.len()));
    }
    if run.stderr.starts_with(&[0xEF, 0xBB, 0xBF]) {
        p.push("Stderr starts with a byte order mark".to_string());
    }
    let Ok(text) = std::str::from_utf8(&run.stderr) else {
        p.push("Stderr is not valid UTF-8".to_string());
        return p;
    };
    if !(text.ends_with('\n') && text.matches('\n').count() == 1 && !text.contains('\r')) {
        p.push(format!(
            "Stderr is not one line ending in one line feed: {text:?}"
        ));
        return p;
    }
    let body = text.trim_end_matches('\n');
    if !is_compact(body) {
        p.push("The error document is not compact".to_string());
    }
    let doc: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            p.push(format!("Stderr is not one JSON document ({e}): {body:?}"));
            return p;
        }
    };
    p.extend(shape_problems(&doc, &error_schema()));
    if doc["code"] != code {
        p.push(format!("Code is {}, not '{code}'", doc["code"]));
    }
    match (field, doc.get("field")) {
        (None, None) => {}
        (None, Some(f)) => p.push(format!("An unwanted field {f}")),
        (Some(want), None) => p.push(format!("No field, wanted '{want}'")),
        (Some(want), Some(f)) => {
            if f != want {
                p.push(format!("Field is {f}, not '{want}'"));
            }
        }
    }
    if let Some(m) = doc["message"].as_str() {
        if let Some(want) = message
            && m != want
        {
            p.push(format!("Message is {m:?}, not {want:?}"));
        }
        if m.contains('\u{fffd}') {
            p.push("The message holds a replacement character".to_string());
        }
        let bin_dir = Path::new(BIN)
            .parent()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !bin_dir.is_empty() && m.contains(&bin_dir) {
            p.push("The message names the program's folder".to_string());
        }
        if m.contains(scratch_dir().to_string_lossy().as_ref()) {
            p.push("The message names the working folder".to_string());
        }
    }
    p
}

// ---- Cases -------------------------------------------------------------------------------------

/// One command line, and the usage error it MUST end in.
pub struct Case {
    pub line: Vec<OsString>,
    pub field: Option<&'static str>,
    pub message: String,
}

pub fn case(line: &[&str], field: Option<&'static str>, message: &str) -> Case {
    Case {
        line: args(line),
        field,
        message: message.to_string(),
    }
}

pub fn show(line: &[OsString]) -> String {
    format!("agentspace {line:?}")
}

/// Runs each case through the binary and in process through `execute_in` (`in_process`), and fails
/// once with every problem.
pub fn check_usage_cases(cases: Vec<Case>) {
    let mut failures = Vec::new();
    for c in &cases {
        let run = run_bin(&c.line);
        let p = failure_problems(&run, "usage", c.field, Some(&c.message));
        if !p.is_empty() {
            failures.push(format!("{}: {}", show(&c.line), p.join("; ")));
        }
        let code = in_process(&c.line);
        if code != 2 {
            failures.push(format!("{} in process: exit {code}, not 2", show(&c.line)));
        }
    }
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}

// ---- Messages, as lease-api.md's Messages table words them -------------------------------------

pub fn msg_unknown_option_top(v: &str) -> String {
    format!("Unknown option '{v}'. Run 'agentspace --help' for its options.")
}

pub fn msg_unknown_option_lease(v: &str) -> String {
    format!("Unknown option '{v}'. Run 'agentspace lease --help' for its options.")
}

pub fn msg_unknown_option_sub(v: &str, sub: &str) -> String {
    format!("Unknown option '{v}' for agentspace lease {sub}. Run 'agentspace lease {sub} --help'.")
}

pub fn msg_unknown_command(v: &str) -> String {
    format!("Unknown command '{v}'. Run 'agentspace --help' for the commands.")
}

pub fn msg_unknown_subcommand(v: &str) -> String {
    format!("Unknown subcommand '{v}'. Run 'agentspace lease --help' for the subcommands.")
}

pub const MSG_NO_COMMAND: &str = "No command given. Run 'agentspace --help' for the commands.";

pub const MSG_NO_SUBCOMMAND: &str =
    "No subcommand given. Run 'agentspace lease --help' for the subcommands.";

pub fn msg_needs_value(flag: &str, sub: &str) -> String {
    format!("Option --{flag} needs a value. Run 'agentspace lease {sub} --help'.")
}

pub fn msg_empty_value(flag: &str, sub: &str) -> String {
    format!("Option --{flag} has an empty value. Run 'agentspace lease {sub} --help'.")
}

pub fn msg_twice(flag: &str) -> String {
    format!("Option --{flag} is given twice. Give it once.")
}

pub fn msg_unexpected(v: &str, sub: &str) -> String {
    format!(
        "Unexpected argument '{v}' for agentspace lease {sub}. Run 'agentspace lease {sub} --help'."
    )
}

pub fn msg_no_id(sub: &str) -> String {
    format!("No lease ID given. Run 'agentspace lease {sub} --help'.")
}

pub const MSG_NO_PROJECT: &str =
    "Option --project is required. Run 'agentspace lease create --help'.";

pub const MSG_NO_DRIVER: &str =
    "agentspace lease create failed: No lease driver is available. Nothing was recorded.";

pub fn msg_not_found(id: &str) -> String {
    format!("No lease has ID '{id}'.")
}

/// The exit code a valid line to `sub` gets in an empty throwaway home with no driver: `create`
/// fails `internal` (no driver), `list` prints the empty list, and `get`, `renew` and `release` of
/// an ID that names no lease are `not_found`.
pub fn answer_exit(sub: &str) -> i32 {
    match sub {
        "create" => 1,
        "list" => 0,
        _ => 3,
    }
}

/// The problems with a run of a valid line to `sub`, for the lease `id` (normalized) when the line
/// has one, in an empty throwaway home with no driver (see `answer_exit`).
pub fn answer_problems(run: &Run, sub: &str, id: &str) -> Vec<String> {
    match sub {
        "create" => failure_problems(run, "internal", None, Some(MSG_NO_DRIVER)),
        "list" => {
            let mut p = Vec::new();
            if run.status != Some(0) {
                p.push(format!("Exit {:?}, not 0", run.status));
            }
            if !run.stderr.is_empty() {
                p.push(format!("Stderr holds {} bytes", run.stderr.len()));
            }
            if run.stdout != b"{\"leases\":[]}\n" {
                p.push(format!(
                    "Stdout is {:?}, not the empty list",
                    String::from_utf8_lossy(&run.stdout)
                ));
            }
            p
        }
        _ => {
            let mut p = failure_problems(run, "not_found", None, Some(&msg_not_found(id)));
            if let Ok(doc) = serde_json::from_slice::<Value>(&run.stderr)
                && doc["lease_id"] != id
            {
                p.push(format!("lease_id is {}, not '{id}'", doc["lease_id"]));
            }
            p
        }
    }
}

/// The problems with a run of a line that reaches a command but may hold a value the command
/// refuses: not a usage error, and a well-formed answer of whatever kind it is.
pub fn reaches_command_problems(run: &Run) -> Vec<String> {
    if run.status == Some(0) {
        let ok = std::str::from_utf8(&run.stdout)
            .ok()
            .and_then(|t| t.strip_suffix('\n'))
            .is_some_and(|b| serde_json::from_str::<Value>(b).is_ok() && run.stderr.is_empty());
        return if ok {
            Vec::new()
        } else {
            vec!["Exit 0 without one JSON document".to_string()]
        };
    }
    let doc: Value = serde_json::from_slice(&run.stderr).unwrap_or(Value::Null);
    let Some(code) = doc["code"].as_str() else {
        return vec![format!(
            "No error document: {:?}",
            String::from_utf8_lossy(&run.stderr)
        )];
    };
    if code == "usage" {
        return vec![format!("A usage error: {}", doc["message"])];
    }
    failure_problems(run, code, doc["field"].as_str(), None)
}
