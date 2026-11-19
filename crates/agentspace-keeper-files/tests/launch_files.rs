//! A holder's launch files: the request, the claim and the result (docs/state.md, A holder's
//! launch files), and the keeper lock's PID line (docs/state.md, The keeper's lock files).
mod common;

use common::*;

const REQUEST_SCHEMA: &str = "board-holder-launch.state.schema.json";
const RESULT_SCHEMA: &str = "board-holder-launch-result.state.schema.json";

fn started() -> Answer {
    Answer::Started {
        pid: 4242,
        process_started_at: STARTED_AT.into(),
        command_line: Some(good_request().command_line),
    }
}

// ---- The launch request ----

#[test]
fn the_request_is_written_as_exact_compact_bytes() {
    let want = concat!(
        r#"{"application":"C:\\Tools\\OpenSSH\\ssh.exe","#,
        r#""command_line":"\"C:\\Tools\\OpenSSH\\ssh.exe\" -N -o ExitOnForwardFailure=yes agentspace-demo-0123abcd","#,
        r#""log":"C:\\Users\\fake\\.agentspace\\board-holder-ssh.log","#,
        r#""directory":"C:\\Users\\fake","#,
        r#""not_after":"2026-11-19T13:45:37.1234567Z","#,
        r#""result":"C:\\Users\\fake\\.agentspace\\board-holder-launch.0a1b2c3d.result.json"}"#,
        "\n"
    );
    assert_eq!(request_bytes(&good_request()), want.as_bytes());
}

#[test]
fn the_written_request_meets_its_schema_and_the_file_rules() {
    assert_valid_json_file(&request_bytes(&good_request()), REQUEST_SCHEMA);
}

#[test]
fn a_unc_path_request_meets_its_schema() {
    let mut r = good_request();
    r.directory = r"\\?\C:\Users\fake".into();
    r.log = r"\\fakehost\share\board-holder-ssh.log".into();
    assert_valid_json_file(&request_bytes(&r), REQUEST_SCHEMA);
}

#[test]
fn a_batch_stand_in_request_carries_cmd_as_its_application() {
    let mut r = good_request();
    r.application = r"C:\Windows\System32\cmd.exe".into();
    r.command_line = format!(
        r#""C:\Windows\System32\cmd.exe" /d /c ""C:\Temp\stub.cmd" -N -o ExitOnForwardFailure=yes {ALIAS}""#
    );
    let b = request_bytes(&r);
    assert_valid_json_file(&b, REQUEST_SCHEMA);
    assert_eq!(parse_request(&b), Some(r));
}

#[test]
fn a_written_request_reads_back_the_same() {
    let r = good_request();
    assert_eq!(parse_request(&request_bytes(&r)), Some(r));
}

#[test]
fn the_request_schema_check_can_fail() {
    let bad: serde_json::Value = serde_json::from_str(
        r#"{"application":"ssh.exe","command_line":"","log":"l","directory":"d","not_after":"x","result":"r"}"#,
    )
    .unwrap();
    assert!(!validator(REQUEST_SCHEMA).is_valid(&bad));
}

#[test]
fn a_request_that_cannot_be_read_is_no_request() {
    for b in [&b""[..], b"not json\n", b"[]\n", b"{}\n", b"\xFF\xFE"] {
        assert_eq!(parse_request(b), None, "{b:?}");
    }
}

#[test]
fn a_request_with_a_member_missing_is_no_request() {
    let full = good_request();
    let members = [
        ("application", full.application.clone()),
        ("command_line", full.command_line.clone()),
        ("log", full.log.clone()),
        ("directory", full.directory.clone()),
        ("not_after", full.not_after.clone()),
        ("result", full.result.clone()),
    ];
    for skip in 0..members.len() {
        let text: Vec<String> = members
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != skip)
            .map(|(_, (k, v))| format!("\"{k}\":{}", serde_json::to_string(v).unwrap()))
            .collect();
        let b = format!("{{{}}}\n", text.join(",")).into_bytes();
        assert_eq!(parse_request(&b), None, "Without {}", members[skip].0);
    }
}

#[test]
fn a_request_with_a_relative_path_is_no_request() {
    // Architect's ruling, 2026-11-19 (state.md, A holder's launch files): a request that does not
    // match its schema is one the launcher cannot read.
    for field in ["application", "log", "directory", "result"] {
        let mut r = good_request();
        match field {
            "application" => r.application = "ssh.exe".into(),
            "log" => r.log = r"board-holder-ssh.log".into(),
            "directory" => r.directory = r"..\x".into(),
            _ => r.result = r"result.json".into(),
        }
        let b = request_bytes(&r);
        assert_eq!(parse_request(&b), None, "A relative {field}");
    }
}

#[test]
fn a_request_with_a_deadline_of_the_wrong_form_is_no_request() {
    // The deadline has seven fraction digits, no more and no fewer (architect, 2026-11-19, for
    // E0.06-T01-S07: a request off its schema is one the launcher cannot read).
    for t in [
        "2026-11-19T13:45:37.123Z",
        "2026-11-19T13:45:37.123456Z",
        "2026-11-19T13:45:37.12345678Z",
        "2026-11-19T13:45:37Z",
        "2026-11-19T13:45:37.1234567",
        "2026-11-19T13:45:37.1234567Z\n",
    ] {
        let mut r = good_request();
        r.not_after = t.into();
        assert_eq!(parse_request(&request_bytes(&r)), None, "{t:?}");
    }
}

// ---- The claim ----

#[test]
fn a_launchers_claim_is_launcher_pid_and_an_lf() {
    assert_eq!(launcher_claim_bytes(4242), b"launcher 4242\n");
    assert_eq!(launcher_claim_bytes(1), b"launcher 1\n");
    // A claim `launcher 0` is written as any other (architect, 2026-11-19, for E0.06-T01-S07).
    assert_eq!(launcher_claim_bytes(0), b"launcher 0\n");
    assert_eq!(launcher_claim_bytes(2147483647), b"launcher 2147483647\n");
}

#[test]
fn the_passes_withdrawal_is_abandoned_and_an_lf() {
    assert_eq!(abandoned_claim_bytes(), b"abandoned\n");
}

#[test]
fn a_claim_is_one_line_of_utf8_without_a_byte_order_mark() {
    for b in [launcher_claim_bytes(4242), abandoned_claim_bytes()] {
        assert!(!b.starts_with(&[0xEF, 0xBB, 0xBF]));
        let t = std::str::from_utf8(&b).unwrap();
        assert!(t.ends_with('\n') && !t.ends_with("\r\n"));
        assert_eq!(t.matches('\n').count(), 1);
    }
}

// ---- The launch result ----

#[test]
fn a_started_result_is_written_as_exact_compact_bytes() {
    let want = concat!(
        r#"{"pid":4242,"process_started_at":"2026-11-19T13:45:07.123Z","#,
        r#""command_line":"\"C:\\Tools\\OpenSSH\\ssh.exe\" -N -o ExitOnForwardFailure=yes agentspace-demo-0123abcd"}"#,
        "\n"
    );
    assert_eq!(answer_bytes(&started()), want.as_bytes());
}

#[test]
fn a_started_result_with_an_unreadable_command_line_writes_null() {
    let a = Answer::Started {
        pid: 4242,
        process_started_at: STARTED_AT.into(),
        command_line: None,
    };
    let want = "{\"pid\":4242,\"process_started_at\":\"2026-11-19T13:45:07.123Z\",\"command_line\":null}\n";
    assert_eq!(answer_bytes(&a), want.as_bytes());
    assert_valid_json_file(&answer_bytes(&a), RESULT_SCHEMA);
}

#[test]
fn a_not_started_result_is_written_as_exact_compact_bytes() {
    let a = Answer::NotStarted("NUL could not be opened.".into());
    assert_eq!(
        answer_bytes(&a),
        b"{\"error\":\"NUL could not be opened.\"}\n"
    );
    assert_valid_json_file(&answer_bytes(&a), RESULT_SCHEMA);
}

#[test]
fn both_result_shapes_meet_their_schema() {
    assert_valid_json_file(&answer_bytes(&started()), RESULT_SCHEMA);
}

#[test]
fn the_result_schema_check_can_fail() {
    let bad: serde_json::Value = serde_json::from_str(r#"{"error":""}"#).unwrap();
    assert!(!validator(RESULT_SCHEMA).is_valid(&bad));
}

#[test]
fn a_written_result_reads_back_the_same() {
    for a in [
        started(),
        Answer::Started {
            pid: 1,
            process_started_at: STARTED_AT.into(),
            command_line: None,
        },
        Answer::NotStarted(
            "The launcher reached its deadline before it started the holder, so it started nothing."
                .into(),
        ),
    ] {
        assert_eq!(parse_answer(&answer_bytes(&a)), Some(a));
    }
}

#[test]
fn an_error_text_with_quotes_and_backslashes_survives() {
    let a = Answer::NotStarted(r#"The system cannot find "C:\x" (error 2)."#.into());
    assert_eq!(parse_answer(&answer_bytes(&a)), Some(a));
}

#[test]
fn a_result_that_fails_a_rule_is_no_result() {
    let bad: [&[u8]; 14] = [
        b"",
        b"not json\n",
        b"[]\n",
        b"{}\n",
        // An empty error.
        b"{\"error\":\"\"}\n",
        // An error that is not text.
        b"{\"error\":7}\n",
        // Started, with a pid out of range or not an integer.
        b"{\"pid\":0,\"process_started_at\":\"2026-11-19T13:45:07.123Z\",\"command_line\":null}\n",
        b"{\"pid\":2147483648,\"process_started_at\":\"2026-11-19T13:45:07.123Z\",\"command_line\":null}\n",
        b"{\"pid\":\"4242\",\"process_started_at\":\"2026-11-19T13:45:07.123Z\",\"command_line\":null}\n",
        // Started, with a start time of the wrong form.
        b"{\"pid\":4242,\"process_started_at\":\"2026-11-19T13:45:07Z\",\"command_line\":null}\n",
        // Started, with the command line missing.
        b"{\"pid\":4242,\"process_started_at\":\"2026-11-19T13:45:07.123Z\"}\n",
        // Started, with a command line that is neither text nor null.
        b"{\"pid\":4242,\"process_started_at\":\"2026-11-19T13:45:07.123Z\",\"command_line\":5}\n",
        // Both shapes at once.
        b"{\"pid\":4242,\"process_started_at\":\"2026-11-19T13:45:07.123Z\",\"command_line\":null,\"error\":\"x\"}\n",
        // The pid missing.
        b"{\"process_started_at\":\"2026-11-19T13:45:07.123Z\",\"command_line\":null}\n",
    ];
    for b in bad {
        assert_eq!(parse_answer(b), None, "{}", String::from_utf8_lossy(b));
    }
}

// ---- The keeper lock's PID line ----

#[test]
fn the_keeper_lock_holds_the_pid_as_digits_and_nothing_else() {
    assert_eq!(keeper_lock_bytes(4242), b"4242");
    assert_eq!(keeper_lock_bytes(1), b"1");
    assert_eq!(keeper_lock_bytes(2147483647), b"2147483647");
}

#[test]
fn a_second_keeper_reads_one_to_ten_digits() {
    assert_eq!(parse_keeper_lock_pid(b"4242"), Some("4242".to_string()));
    assert_eq!(parse_keeper_lock_pid(b"7"), Some("7".to_string()));
    assert_eq!(
        parse_keeper_lock_pid(b"2147483647"),
        Some("2147483647".to_string())
    );
    assert_eq!(
        parse_keeper_lock_pid(b"9999999999"),
        Some("9999999999".to_string())
    );
}

#[test]
fn blanks_around_the_digits_are_trimmed() {
    assert_eq!(parse_keeper_lock_pid(b"  4242 "), Some("4242".to_string()));
    assert_eq!(parse_keeper_lock_pid(b"\t4242\t"), Some("4242".to_string()));
}

#[test]
fn blanks_are_ascii_whitespace_and_nothing_else() {
    // state.md, The holder record: in the keeper lock's PID line, "blanks" are space, tab, CR, LF
    // and form feed (architect, 2026-11-19, for E0.06-T01-S07).
    assert_eq!(
        parse_keeper_lock_pid(b" \t\r\n\x0C4242\x0C\n\r\t "),
        Some("4242".to_string())
    );
    assert_eq!(parse_keeper_lock_pid(b"4242\r\n"), Some("4242".to_string()));
    // A vertical tab, a NUL and a no-break space are not blanks.
    for b in [&b"\x0B4242"[..], b"4242\x0B", b"4242\x00", b"\xC2\xA04242"] {
        assert_eq!(parse_keeper_lock_pid(b), None, "{b:?}");
    }
}

#[test]
fn what_is_not_one_to_ten_digits_reads_as_no_pid() {
    let bad: [&[u8]; 9] = [
        b"",
        b"   ",
        b"11111111111",
        b"12ab",
        b"ab12",
        b"-4242",
        b"42 42",
        b"4242.5",
        b"\xFF\xFE",
    ];
    for b in bad {
        assert_eq!(parse_keeper_lock_pid(b), None, "{b:?}");
    }
}

#[test]
fn only_the_first_32_bytes_are_read() {
    // 4 digits and 28 blanks fill the 32 bytes: the digits read, whatever follows.
    let mut b = b"4242".to_vec();
    b.extend([b' '; 28]);
    b.extend(b"zzzz-not-read");
    assert_eq!(parse_keeper_lock_pid(&b), Some("4242".to_string()));
    // Forty digits give a first 32 that are more than ten digits.
    assert_eq!(parse_keeper_lock_pid(&[b'1'; 40]), None);
}

#[test]
fn the_written_lock_reads_back() {
    for pid in [1u32, 4242, 2147483647] {
        assert_eq!(
            parse_keeper_lock_pid(&keeper_lock_bytes(pid)),
            Some(pid.to_string())
        );
    }
}
