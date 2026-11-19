//! The output rules and the failure rules (lease-api.md, The program, Output, Exit codes and
//! errors, Messages): one error document on stderr and nothing on stdout, exit codes from the table
//! only, messages with no control character, a quoted value escaped, and a run that ends in an
//! unexpected failure still ending in one `internal` document.
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. A success document cannot be
//! tested here, because every subcommand answers `internal` in this story (E0.05-T03-S02, #0106,
//! replaces that); help is the one success output, and `help.rs` checks it.

mod common;

use common::*;
use serde_json::Value;
use std::ffi::OsString;

fn doc_of(run: &Run) -> Value {
    serde_json::from_slice(&run.stderr).expect("Stderr is one JSON document")
}

#[test]
fn a_failure_writes_nothing_to_stdout_and_one_compact_document_and_line_feed_to_stderr() {
    let lines = [
        args(&[]),
        args(&["lease"]),
        args(&["frob"]),
        args(&["lease", "create"]),
        args(&["lease", "get", ID, "--ttl", "8h"]),
        args(&["lease", "create", "--project", "x"]),
        args(&["lease", "get", ID]),
        args(&["lease", "renew", ID]),
        args(&["lease", "release", ID]),
        args(&["lease", "list"]),
    ];
    let mut failures = Vec::new();
    for line in &lines {
        let run = run_bin(line);
        let doc = serde_json::from_slice::<Value>(&run.stderr);
        let code = doc
            .ok()
            .and_then(|d| d["code"].as_str().map(str::to_string))
            .unwrap_or_default();
        if code.is_empty() {
            failures.push(format!("{}: stderr holds no error document", show(line)));
            continue;
        }
        let p = failure_problems(&run, &code, None, None);
        let p: Vec<String> = p
            .into_iter()
            .filter(|m| !m.starts_with("An unwanted field") && !m.starts_with("No field"))
            .collect();
        if !p.is_empty() {
            failures.push(format!("{}: {}", show(line), p.join("; ")));
        }
    }
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}

#[test]
fn every_exit_code_is_in_the_table_and_comes_with_its_error_code() {
    let lines = [
        args(&[]),
        args(&["lease"]),
        args(&["--bogus"]),
        args(&["lease", "create"]),
        args(&["lease", "create", "--project", "x"]),
        args(&["lease", "get", ID]),
        args(&["lease", "list", "--state", "ready"]),
        args(&["lease", "renew", ID, "--ttl", "8h"]),
        args(&["lease", "release", ID]),
        args(&["--help"]),
        args(&["lease", "get", "--help"]),
    ];
    for line in &lines {
        let run = run_bin(line);
        let status = run
            .status
            .unwrap_or_else(|| panic!("{}: ended by a signal", show(line)));
        assert!(
            (0..=4).contains(&status),
            "{}: exit {status} is not in the table",
            show(line)
        );
        if status == 0 {
            assert!(
                run.stderr.is_empty(),
                "{}: nothing on stderr on success",
                show(line)
            );
        } else {
            let doc = doc_of(&run);
            let code = doc["code"].as_str().expect("A code");
            assert_eq!(
                exit_for(code),
                status,
                "{}: the code and the exit code agree",
                show(line)
            );
        }
    }
}

#[test]
fn the_error_document_is_valid_against_the_error_schema_for_every_usage_error_shape() {
    // The schema file itself is read, so its lists of codes, fields and states bind the check.
    let schema = error_schema();
    assert_eq!(
        schema["properties"]["code"]["enum"]
            .as_array()
            .map(Vec::len),
        Some(6)
    );
    let lines = [
        args(&[]),
        args(&["lease", "create", "--project"]),
        args(&["lease", "get"]),
        args(&["lease", "create"]),
        args(&["lease", "list", "--state", ""]),
        args(&[
            "lease",
            "create",
            "--project",
            "x",
            "--ttl",
            "1",
            "--TTL",
            "2",
        ]),
        args(&["lease", "get", ID]),
    ];
    for line in &lines {
        let run = run_bin(line);
        let doc = doc_of(&run);
        let p = shape_problems(&doc, &schema);
        assert!(p.is_empty(), "{}: {}", show(line), p.join("; "));
    }
    // The checker MUST be able to find something: a document that breaks each rule is refused.
    let bad = [
        r#"{"code":"usage"}"#,
        r#"{"code":"usage","message":" x"}"#,
        r#"{"code":"usage","message":""}"#,
        r#"{"code":"usage","message":"a\nb"}"#,
        r#"{"code":"nope","message":"x"}"#,
        r#"{"code":"usage","message":"x","field":"nope"}"#,
        r#"{"code":"usage","message":"x","state":"ready"}"#,
        r#"{"code":"internal","message":"x","field":"ttl"}"#,
        r#"{"code":"invalid_value","message":"x"}"#,
        r#"{"code":"usage","message":"x","extra":1}"#,
    ];
    for text in bad {
        let doc: Value = serde_json::from_str(text).expect("JSON");
        assert!(
            !shape_problems(&doc, &schema).is_empty(),
            "{text} MUST be refused"
        );
    }
}

#[test]
fn a_usage_error_carries_a_code_and_a_message_and_a_field_only_in_the_five_cases() {
    let with_field: [(&[&str], &str); 5] = [
        (&["lease", "create", "--project"], "project"),
        (&["lease", "create", "--project="], "project"),
        (
            &["lease", "create", "--project", "x", "--project", "y"],
            "project",
        ),
        (&["lease", "get"], "id"),
        (&["lease", "create"], "project"),
    ];
    for (line, field) in with_field {
        let run = run_str(line);
        let doc = doc_of(&run);
        assert_eq!(doc["code"], "usage");
        assert_eq!(doc["field"], field, "{line:?}");
        assert_eq!(
            doc.as_object().map(|o| o.len()),
            Some(3),
            "{line:?}: code, message and field only"
        );
    }
    let without_field: [&[&str]; 5] = [
        &[],
        &["lease"],
        &["frob"],
        &["lease", "list", "--bogus"],
        &["lease", "list", "extra"],
    ];
    for line in without_field {
        let doc = doc_of(&run_str(line));
        assert_eq!(doc["code"], "usage");
        assert_eq!(
            doc.as_object().map(|o| o.len()),
            Some(2),
            "{line:?}: code and message only"
        );
    }
}

#[test]
fn an_internal_error_carries_only_a_code_and_a_message() {
    let run = run_str(&["lease", "get", ID]);
    let doc = doc_of(&run);
    assert_eq!(doc.as_object().map(|o| o.len()), Some(2));
    assert_eq!(doc["code"], "internal");
    assert_eq!(doc["message"], msg_stub("get").as_str());
}

#[test]
fn the_subcommand_in_an_internal_message_is_lowercase() {
    let run = run_str(&["LEASE", "RELEASE", ID]);
    let doc = doc_of(&run);
    assert_eq!(doc["message"], msg_stub("release").as_str());
}

#[test]
fn a_message_is_in_agentspaces_own_words_with_no_path_and_no_other_programs_text() {
    for line in [
        args(&["lease", "get", ID]),
        args(&["lease", "list"]),
        args(&["lease", "create", "--project", "x"]),
    ] {
        let run = run_bin(&line);
        let message = doc_of(&run)["message"]
            .as_str()
            .expect("A message")
            .to_string();
        assert!(
            !message.contains('/'),
            "{}: no path in {message:?}",
            show(&line)
        );
        assert!(
            !message.contains('\\'),
            "{}: no path in {message:?}",
            show(&line)
        );
        assert!(!message.contains(".exe"), "{message:?}");
        assert!(!message.contains("panicked"), "{message:?}");
        assert!(!message.contains("RUST_BACKTRACE"), "{message:?}");
    }
}

// ---- A quoted value: control characters as \u and four lowercase hex digits --------------------

fn unknown_command_run(arg: &str) -> (String, String) {
    let run = run_str(&[arg]);
    let p = failure_problems(&run, "usage", None, None);
    assert!(p.is_empty(), "{arg:?}: {}", p.join("; "));
    let text = String::from_utf8(run.stderr).expect("UTF-8");
    let message = serde_json::from_str::<Value>(&text).expect("JSON")["message"]
        .as_str()
        .expect("A message")
        .to_string();
    (message, text)
}

#[test]
fn a_control_character_in_a_quoted_value_is_written_as_backslash_u_and_four_lowercase_hex_digits() {
    let (message, raw) = unknown_command_run("a\nb");
    assert_eq!(message, msg_unknown_command("a\\u000ab"));
    // In the JSON document that backslash is itself escaped.
    assert!(
        raw.contains("a\\\\u000ab"),
        "The document holds \\\\u000a: {raw}"
    );
    assert_eq!(
        raw.matches('\n').count(),
        1,
        "No other line feed in the document"
    );

    let all: String = (1u32..=0x1f)
        .chain([0x7f])
        .filter_map(char::from_u32)
        .collect();
    let (message, _) = unknown_command_run(&all);
    let want: String = (1u32..=0x1f)
        .chain([0x7f])
        .map(|c| format!("\\u{c:04x}"))
        .collect();
    assert_eq!(message, msg_unknown_command(&want));
    assert!(
        message.contains("\\u001b") && message.contains("\\u001f") && message.contains("\\u007f")
    );
    assert!(!message.chars().any(|c| (c as u32) <= 0x1f || c == '\u{7f}'));
}

#[test]
fn a_control_character_is_escaped_in_every_message_that_quotes_a_value() {
    let cases: Vec<(Vec<&str>, String)> = vec![
        (vec!["--bo\tgus"], msg_unknown_option_top("--bo\\u0009gus")),
        (
            vec!["lease", "--bo\rgus"],
            msg_unknown_option_lease("--bo\\u000dgus"),
        ),
        (
            vec!["lease", "FR\u{1b}OB"],
            msg_unknown_subcommand("FR\\u001bOB"),
        ),
        (
            vec!["lease", "list", "--bo\ngus=a\nb"],
            msg_unknown_option_sub("--bo\\u000agus=a\\u000ab", "list"),
        ),
        (
            vec!["lease", "list", "ext\u{7f}ra"],
            msg_unexpected("ext\\u007fra", "list"),
        ),
    ];
    for (line, message) in cases {
        let run = run_str(&line);
        let p = failure_problems(&run, "usage", None, Some(&message));
        assert!(p.is_empty(), "{line:?}: {}", p.join("; "));
    }
}

#[test]
fn a_character_that_is_not_a_control_character_is_quoted_as_given() {
    // Only U+0000 to U+001F and U+007F are control characters here: C1 controls, U+2028, a byte order
    // mark in the middle, a quote, a backslash and non-ASCII letters stay as they are.
    let value = "\u{85}\u{a0}\u{2028}\u{feff}'\\\u{e9}\u{1f600}";
    let (message, raw) = unknown_command_run(value);
    assert_eq!(message, msg_unknown_command(value));
    assert!(std::str::from_utf8(raw.as_bytes()).is_ok());
}

#[test]
fn an_unknown_option_is_quoted_whole_with_its_equals_value() {
    let run = run_str(&["--help=yes"]);
    let doc = doc_of(&run);
    assert_eq!(
        doc["message"],
        msg_unknown_option_top("--help=yes").as_str()
    );
}

// ---- Unexpected failure ------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[test]
fn a_failure_to_write_stdout_still_ends_in_one_internal_error_document_and_exit_1() {
    // /dev/full fails every write with ENOSPC, which is an unexpected failure that no caller caused.
    let full = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .expect("/dev/full opens");
    for line in [args(&["--help"]), args(&["lease", "create", "--help"])] {
        let full = full.try_clone().expect("The handle clones");
        let mut cmd = command(&line);
        cmd.stdout(full).stderr(std::process::Stdio::piped());
        let run = run_command(cmd);
        let p = failure_problems(&run, "internal", None, None);
        // Stdout is the full device, so only the stderr and exit rules can be seen here.
        assert!(p.is_empty(), "{}: {}", show(&line), p.join("; "));
        let doc = doc_of(&run);
        let message = doc["message"].as_str().expect("A message");
        assert!(
            message.starts_with("agentspace lease"),
            "{}: the message names agentspace lease: {message:?}",
            show(&line)
        );
        assert!(!message.contains("panicked"), "{message:?}");
    }
}

#[test]
fn main_with_never_panics_on_any_arguments() {
    let long = "a".repeat(200_000);
    let mut lines: Vec<Vec<OsString>> = vec![
        args(&[]),
        args(&[""]),
        args(&["", "", ""]),
        args(&["lease", "create", "--project", &long]),
        args(&[&long]),
        args(&["lease", &long]),
        args(&["lease", "list", "--state", "\n,\r,\0"]),
        args(&["--", "--", "--"]),
        args(&["-", "-", "-"]),
        args(&["lease", "create", "--=", "--=="]),
        args(&[
            "lease",
            "create",
            "--project",
            "\u{1f600}",
            "--ttl",
            "\u{feff}",
        ]),
        args(&["lease", "get", "\u{0}"]),
    ];
    let many: Vec<OsString> = (0..5000).map(|i| os(&format!("--flag{i}"))).collect();
    lines.push(many);
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        lines.push(vec![OsString::from_vec(vec![0xff, 0xfe, 0x80])]);
        lines.push(vec![os("lease"), OsString::from_vec(b"cr\xc3".to_vec())]);
        lines.push(vec![
            os("lease"),
            os("create"),
            os("--project"),
            OsString::from_vec(vec![0xed, 0xa0, 0x80]),
        ]);
    }
    for line in lines {
        let result = std::panic::catch_unwind(|| in_process(&line));
        let code =
            result.unwrap_or_else(|_| panic!("main_with panicked on {} arguments", line.len()));
        assert!((0..=4).contains(&code), "Exit {code} is not in the table");
    }
}

#[test]
fn a_run_reads_nothing_from_standard_input() {
    // Standard input stays open and silent. A program that reads it would wait for the timeout.
    let mut cmd = command(&args(&["lease", "get", ID]));
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().expect("The binary starts");
    let (tx, rx) = std::sync::mpsc::channel();
    let held = child.stdin.take();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let out = rx
        .recv_timeout(std::time::Duration::from_secs(20))
        .expect("The binary ends without input")
        .expect("Output");
    drop(held);
    assert_eq!(out.status.code(), Some(1));
}

#[cfg(unix)]
#[test]
fn the_program_name_is_never_read_as_an_argument() {
    use std::os::unix::process::CommandExt;
    // The first argument the operating system gives is the program's name, whatever it says.
    for name in ["-h", "--help", "--bogus", "lease", "frob"] {
        let line = args(&["lease", "get", ID]);
        let mut cmd = command(&line);
        cmd.arg0(name)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let run = run_command(cmd);
        let p = failure_problems(&run, "internal", None, Some(&msg_stub("get")));
        assert!(p.is_empty(), "argv[0] {name:?}: {}", p.join("; "));
    }
}
