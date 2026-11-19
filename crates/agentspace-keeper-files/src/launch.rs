//! A holder's launch files: the request, the claim and the result (state.md, A holder's launch
//! files).

use crate::NotValid;
use crate::json::{Schema, file_bytes, is_valid, object, read_value, why_not_valid};
use serde_json::Value;

/// `board-holder-launch.<hex>.json`: the pass's request to its launcher to start a holder. Every
/// path is full, so the launcher never reads the state folder from its environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchRequest {
    /// The full path of the program the holder runs: the ssh program, or `cmd.exe` for a batch
    /// stand-in.
    pub application: String,
    /// The holder's whole command line.
    pub command_line: String,
    /// The full path of `board-holder-ssh.log`.
    pub log: String,
    /// The holder's working folder, a full path: the home folder.
    pub directory: String,
    /// The launcher's deadline, UTC, `YYYY-MM-DDTHH:MM:SS.fffffffZ` (seven fraction digits).
    pub not_after: String,
    /// The full path of the result file.
    pub result: String,
}

fn request_value(r: &LaunchRequest) -> Value {
    object([
        ("application", Value::from(r.application.as_str())),
        ("command_line", Value::from(r.command_line.as_str())),
        ("log", Value::from(r.log.as_str())),
        ("directory", Value::from(r.directory.as_str())),
        ("not_after", Value::from(r.not_after.as_str())),
        ("result", Value::from(r.result.as_str())),
    ])
}

/// The request's bytes: compact JSON in the order `application`, `command_line`, `log`,
/// `directory`, `not_after`, `result`, then one LF. A request that would not validate (a path that
/// is not full, a control character, a `not_after` without seven fraction digits) is refused.
pub fn serialize_launch_request(request: &LaunchRequest) -> Result<Vec<u8>, NotValid> {
    let doc = request_value(request);
    if !is_valid(Schema::Launch, &doc) {
        return Err(why_not_valid(Schema::Launch, &doc, "The launch request"));
    }
    file_bytes(&doc)
}

/// Reads a launch request, which only the keeper writes, so it is read strictly: the bytes MUST be
/// one JSON object that validates against the request's schema, with no member named twice.
pub fn parse_launch_request(bytes: &[u8]) -> Result<LaunchRequest, NotValid> {
    let not_valid = || NotValid("The launch request is not valid".to_string());
    let doc = read_value(bytes).ok_or_else(not_valid)?;
    if !is_valid(Schema::Launch, &doc) {
        return Err(why_not_valid(Schema::Launch, &doc, "The launch request"));
    }
    let o = doc.as_object().ok_or_else(not_valid)?;
    let text = |key: &str| {
        o.get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(not_valid)
    };
    Ok(LaunchRequest {
        application: text("application")?,
        command_line: text("command_line")?,
        log: text("log")?,
        directory: text("directory")?,
        not_after: text("not_after")?,
        result: text("result")?,
    })
}

/// `board-holder-launch.<hex>.result.json`: a launcher's answer, one of two shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchResult {
    /// The holder started, as read from the process the launcher created.
    Started {
        pid: u32,
        /// `YYYY-MM-DDTHH:MM:SS.fffZ`.
        process_started_at: String,
        /// The command line read back from the process, or `None` (written as `null`) when it
        /// could not be read.
        command_line: Option<String>,
    },
    /// No holder started: `<text>` is not empty.
    NotStarted { error: String },
}

fn result_value(r: &LaunchResult) -> Value {
    match r {
        LaunchResult::Started {
            pid,
            process_started_at,
            command_line,
        } => object([
            ("pid", Value::from(*pid)),
            (
                "process_started_at",
                Value::from(process_started_at.as_str()),
            ),
            (
                "command_line",
                command_line.as_deref().map_or(Value::Null, Value::from),
            ),
        ]),
        LaunchResult::NotStarted { error } => object([("error", Value::from(error.as_str()))]),
    }
}

/// The result's bytes: compact JSON, `{"pid":..,"process_started_at":"..","command_line":..}` or
/// `{"error":".."}`, then one LF. A result that would not validate (an empty error, a PID of 0) is
/// refused.
pub fn serialize_launch_result(result: &LaunchResult) -> Result<Vec<u8>, NotValid> {
    let doc = result_value(result);
    if !is_valid(Schema::Result, &doc) {
        return Err(why_not_valid(Schema::Result, &doc, "The launch result"));
    }
    file_bytes(&doc)
}

/// Reads a launch result, which only a launcher writes, so it is read strictly: the bytes MUST be
/// one JSON object that validates against exactly one of the result's two shapes, with no member
/// named twice. A result that is neither shape is an error, which the pass takes as no answer.
pub fn parse_launch_result(bytes: &[u8]) -> Result<LaunchResult, NotValid> {
    let not_valid = || NotValid("The launch result is not valid".to_string());
    let doc = read_value(bytes).ok_or_else(not_valid)?;
    if !is_valid(Schema::Result, &doc) {
        return Err(why_not_valid(Schema::Result, &doc, "The launch result"));
    }
    let o = doc.as_object().ok_or_else(not_valid)?;
    if let Some(error) = o.get("error") {
        return Ok(LaunchResult::NotStarted {
            error: error.as_str().ok_or_else(not_valid)?.to_string(),
        });
    }
    let pid = o
        .get("pid")
        .and_then(Value::as_u64)
        .and_then(|p| u32::try_from(p).ok())
        .ok_or_else(not_valid)?;
    let process_started_at = o
        .get("process_started_at")
        .and_then(Value::as_str)
        .ok_or_else(not_valid)?
        .to_string();
    let command_line = match o.get("command_line").ok_or_else(not_valid)? {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        _ => return Err(not_valid()),
    };
    Ok(LaunchResult::Started {
        pid,
        process_started_at,
        command_line,
    })
}

/// `board-holder-launch.<hex>.claim`: its content, which nothing in agentspace reads but a
/// conformance check does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// A launcher claimed the request: `launcher <pid>`.
    Launcher(u32),
    /// The pass withdrew the request: `abandoned`.
    Abandoned,
}

/// The claim's bytes: UTF-8 without a byte order mark, one line ending in LF: `launcher <pid>` or
/// `abandoned`.
pub fn serialize_claim(claim: Claim) -> Vec<u8> {
    match claim {
        Claim::Launcher(pid) => format!("launcher {pid}\n").into_bytes(),
        Claim::Abandoned => b"abandoned\n".to_vec(),
    }
}

/// Reads a claim's bytes: exactly `launcher <pid>` with the PID in decimal digits, or `abandoned`,
/// then one LF. Anything else, a missing LF and a byte order mark included, is `None`.
pub fn parse_claim(bytes: &[u8]) -> Option<Claim> {
    let line = std::str::from_utf8(bytes.strip_suffix(b"\n")?).ok()?;
    if line == "abandoned" {
        return Some(Claim::Abandoned);
    }
    let digits = line.strip_prefix("launcher ")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u32>().ok().map(Claim::Launcher)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> LaunchRequest {
        LaunchRequest {
            application: r"C:\Tools\OpenSSH\ssh.exe".to_string(),
            command_line: r#""C:\Tools\OpenSSH\ssh.exe" -N -o ExitOnForwardFailure=yes agentspace-demo-0a1b2c3d"#.to_string(),
            log: r"C:\Users\someone\.agentspace\board-holder-ssh.log".to_string(),
            directory: r"C:\Users\someone".to_string(),
            not_after: "2026-11-19T04:05:36.1234567Z".to_string(),
            result: r"C:\Users\someone\.agentspace\board-holder-launch.ab12.result.json"
                .to_string(),
        }
    }

    #[test]
    fn a_request_is_compact_in_member_order_and_round_trips() {
        let bytes = serialize_launch_request(&request()).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(
            text.starts_with(r#"{"application":"C:\\Tools\\OpenSSH\\ssh.exe","command_line":"#)
        );
        assert!(text.contains(r#","log":"C:\\Users"#));
        assert!(text.contains(r#","directory":"C:\\Users\\someone","not_after":"2026-11-19T04:05:36.1234567Z","result":"#));
        assert!(text.ends_with("\"}\n"));
        assert_eq!(parse_launch_request(&bytes), Ok(request()));
    }

    #[test]
    fn a_request_that_the_schema_refuses_is_not_written() {
        let mut r = request();
        r.application = "ssh.exe".to_string();
        assert!(serialize_launch_request(&r).is_err());
        let mut r = request();
        r.log = r"\relative\log".to_string();
        assert!(serialize_launch_request(&r).is_err());
        let mut r = request();
        r.directory = "C:/forward/slashes".to_string();
        assert!(serialize_launch_request(&r).is_err());
        let mut r = request();
        r.command_line = String::new();
        assert!(serialize_launch_request(&r).is_err());
        let mut r = request();
        r.command_line = "ssh\n-N".to_string();
        assert!(serialize_launch_request(&r).is_err());
        let mut r = request();
        r.not_after = "2026-11-19T04:05:36.123Z".to_string();
        assert!(serialize_launch_request(&r).is_err());
        let mut r = request();
        r.result = "C:\\ends\\in\\lf\n".to_string();
        assert!(serialize_launch_request(&r).is_err());
        let mut r = request();
        r.application = r"\\server\share\ssh.exe".to_string();
        r.directory = r"\\?\C:\Users\someone".to_string();
        assert!(serialize_launch_request(&r).is_ok());
    }

    #[test]
    fn a_request_is_read_strictly() {
        let text = String::from_utf8(serialize_launch_request(&request()).unwrap()).unwrap();
        assert!(parse_launch_request(text.replace("\"log\"", "\"logs\"").as_bytes()).is_err());
        assert!(parse_launch_request(text.replace('}', ",\"extra\":1}").as_bytes()).is_err());
        assert!(parse_launch_request(b"[]").is_err());
        assert!(parse_launch_request(b"").is_err());
        let twice = text.replace("\"log\":", "\"log\":\"C:\\\\a\",\"log\":");
        assert!(parse_launch_request(twice.as_bytes()).is_err());
    }

    fn started() -> LaunchResult {
        LaunchResult::Started {
            pid: 321,
            process_started_at: "2026-11-19T04:05:07.456Z".to_string(),
            command_line: Some(r#""C:\Tools\ssh.exe" -N"#.to_string()),
        }
    }

    #[test]
    fn a_started_result_has_its_bytes_and_a_null_command_line_when_it_could_not_be_read() {
        let bytes = serialize_launch_result(&started()).unwrap();
        assert_eq!(
            String::from_utf8(bytes.clone()).unwrap(),
            "{\"pid\":321,\"process_started_at\":\"2026-11-19T04:05:07.456Z\",\
             \"command_line\":\"\\\"C:\\\\Tools\\\\ssh.exe\\\" -N\"}\n"
        );
        assert_eq!(parse_launch_result(&bytes), Ok(started()));
        let unread = LaunchResult::Started {
            pid: 321,
            process_started_at: "2026-11-19T04:05:07.456Z".to_string(),
            command_line: None,
        };
        let bytes = serialize_launch_result(&unread).unwrap();
        assert_eq!(
            bytes,
            b"{\"pid\":321,\"process_started_at\":\"2026-11-19T04:05:07.456Z\",\"command_line\":null}\n"
        );
        assert_eq!(parse_launch_result(&bytes), Ok(unread));
    }

    #[test]
    fn a_not_started_result_has_its_bytes_and_an_error_is_never_empty() {
        let r = LaunchResult::NotStarted {
            error: "NUL could not be opened.".to_string(),
        };
        let bytes = serialize_launch_result(&r).unwrap();
        assert_eq!(bytes, b"{\"error\":\"NUL could not be opened.\"}\n");
        assert_eq!(parse_launch_result(&bytes), Ok(r));
        let empty = LaunchResult::NotStarted {
            error: String::new(),
        };
        assert!(serialize_launch_result(&empty).is_err());
        assert!(parse_launch_result(b"{\"error\":\"\"}\n").is_err());
    }

    #[test]
    fn a_result_of_neither_shape_is_refused() {
        let bad_pid = LaunchResult::Started {
            pid: 0,
            process_started_at: "2026-11-19T04:05:07.456Z".to_string(),
            command_line: None,
        };
        assert!(serialize_launch_result(&bad_pid).is_err());
        let bad_time = LaunchResult::Started {
            pid: 5,
            process_started_at: "2026-11-19T04:05:07.4567890Z".to_string(),
            command_line: None,
        };
        assert!(serialize_launch_result(&bad_time).is_err());
        for text in [
            "{}",
            "{\"pid\":5}",
            "{\"pid\":5,\"process_started_at\":\"2026-11-19T04:05:07.456Z\"}",
            "{\"pid\":5,\"process_started_at\":\"2026-11-19T04:05:07.456Z\",\"command_line\":null,\"error\":\"x\"}",
            "{\"error\":\"x\",\"extra\":1}",
            "{\"error\":null}",
            "{\"error\":\"x\",\"error\":\"y\"}",
            "[]",
            "",
        ] {
            assert!(parse_launch_result(text.as_bytes()).is_err(), "{text}");
        }
    }

    #[test]
    fn a_claim_is_one_line_ending_in_lf() {
        assert_eq!(serialize_claim(Claim::Launcher(4321)), b"launcher 4321\n");
        assert_eq!(serialize_claim(Claim::Abandoned), b"abandoned\n");
        assert_eq!(parse_claim(b"launcher 4321\n"), Some(Claim::Launcher(4321)));
        assert_eq!(parse_claim(b"abandoned\n"), Some(Claim::Abandoned));
        // A claim `launcher 0` is accepted (architect, 2026-11-19, for E0.06-T01-S07).
        assert_eq!(serialize_claim(Claim::Launcher(0)), b"launcher 0\n");
        assert_eq!(parse_claim(b"launcher 0\n"), Some(Claim::Launcher(0)));
        for bad in [
            &b""[..],
            b"abandoned",
            b"abandoned\r\n",
            b"abandoned\n\n",
            b"Abandoned\n",
            b"launcher\n",
            b"launcher \n",
            b"launcher x\n",
            b"launcher -1\n",
            b"launcher 12 \n",
            b"launcher 99999999999\n",
            b"\xEF\xBB\xBFlauncher 1\n",
            b"launcher 1\nabandoned\n",
        ] {
            assert_eq!(parse_claim(bad), None, "{bad:?}");
        }
    }
}
