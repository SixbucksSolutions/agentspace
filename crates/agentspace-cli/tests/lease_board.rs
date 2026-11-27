//! `agentspace lease create --board --epic`, `--turn` and contract v2 (E0.11-T03-S01, #0238).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every expectation is taken
//! from docs/lease-api.md (contract v2: The board read, The plan block, Plan, Board, Epic, Turn,
//! TTL, `create`, Command lines, Exit codes and errors, Messages, Environment, Schemas, Versioning),
//! docs/state.md (What a plan lease adds, `plan_source`), docs/conformance.md (The stand-in board
//! and its credential) and the schemas under `schemas/`, never from the program's code (D31).
//!
//! The board is a stand-in: a `TcpListener` on `127.0.0.1` and a port the OS picks, in a thread of
//! this file, which answers as localswim does and logs every request it gets (its method, its path
//! and query, and whether it carried the stand-in's token). It is never port 8811, and no test of
//! this file reads or connects to Terry's receipt, credential or board. Each receipt and `bot.json`
//! is the test's own, in a throwaway folder, and the token is made up here, 43 characters of
//! unpadded base64url patterned to read as fake. **The token is never printed, logged or passed as
//! an argument**: it is compared inside the stand-in, and a test reports a mismatch by name.
//!
//! The `bot.json` blob is made with `CryptProtectData` in a module that calls Windows, as the
//! credential's own reader decrypts it: user scope, no entropy, the token in UTF-16LE.
//!
//! The board read is Windows-only, as the contract's caller is (D8), so the file is empty
//! elsewhere.

#![cfg(windows)]

mod common;
#[path = "common/fake_driver.rs"]
mod fake_driver;
#[path = "common/lab.rs"]
mod lab;

use common::{MSG_NO_PROJECT, Run, failure_problems};
use fake_driver::Call;
use lab::{Lab, LockHold, NOW, Ran, fmt_time, schema_problems};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

// ---- What the spec words -------------------------------------------------------------------------

/// lease-api.md, Board: a board's ID, patterned to read as fake.
const BOARD: &str = "0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f";

/// A second board, one the stand-in does not know.
const OTHER_BOARD: &str = "1b2c3d4e-1b2c-4d3e-9f4a-1b2c3d4e5f6a";

/// The epic's ticket on `BOARD`, and its revision.
const EPIC: u64 = 52;
const REVISION: u64 = 7;

/// The plan's own name, which a lease prints as `sized_for.plan`.
const PLAN_NAME: &str = "lease-board-test-plan";

/// The largest ticket and revision localswim gives, 2^53 - 1 (lease-api.md, Epic, The board read).
const MAX_SAFE: u64 = 9_007_199_254_740_991;

/// The text of every card a stand-in answers that the client MUST read nothing of (lease-api.md,
/// The board read, pin P1). None of them is in any output.
const MARKERS: [&str; 9] = [
    "PROSE-MARKER-9f3a",
    "COMMENT-MARKER-ONE",
    "COMMENT-MARKER-TWO",
    "RELATED-MARKER-7c1d",
    "PARENT-MARKER-2b8e",
    "CHILD-MARKER-5d4f",
    "TITLE-MARKER-6a0c",
    "PROBLEM-MARKER-TITLE",
    "PROBLEM-MARKER-DETAIL",
];

const LEASE_ERR: &str = "agentspace lease create failed: ";

const MSG_NO_LOCALAPPDATA: &str = "agentspace lease create failed: LOCALAPPDATA is unset or empty, so localswim's service receipt cannot be found. Nothing was recorded.";
const MSG_NO_RECEIPT: &str = "agentspace lease create failed: localswim's service receipt was not found; localswim may not be running. Nothing was recorded.";
const MSG_BAD_RECEIPT: &str = "agentspace lease create failed: localswim's service receipt could not be read. Nothing was recorded.";
const MSG_BAD_CREDENTIAL: &str = "agentspace lease create failed: The board's credential could not be read. Nothing was recorded.";
const MSG_UNREADABLE_ANSWER: &str =
    "agentspace lease create failed: The board's answer could not be read. Nothing was recorded.";
const MSG_REFUSED: &str = "agentspace lease create failed: The board refused agentspace's credential. Nothing was recorded.";

/// lease-api.md, The board read: the "did not answer" message, with the base's port.
fn msg_no_answer(port: u16) -> String {
    format!("{LEASE_ERR}The board at 127.0.0.1:{port} did not answer. Nothing was recorded.")
}

fn msg_too_big(epic: &str) -> String {
    format!(
        "Epic '{epic}' carries no usable plan: the board's answer for this card is larger than 64 MiB."
    )
}

fn msg_archived(epic: &str) -> String {
    format!("Epic '{epic}' is archived. Name an epic that is being worked.")
}

fn msg_no_usable_plan(epic: &str, why: &str) -> String {
    format!("Epic '{epic}' carries no usable plan: {why}.")
}

fn msg_no_card(epic: &str, board: &str) -> String {
    format!(
        "No card of ticket '{epic}' on board '{board}'. Give the epic's ticket number, as in '52'."
    )
}

fn msg_no_board(board: &str) -> String {
    format!("No board of ID '{board}' that agentspace can read.")
}

fn msg_bad_board(v: &str) -> String {
    format!("Board '{v}' is not a board ID. Give the board's UUID, as in '0a1b2c3d-...'.")
}

fn msg_bad_epic(v: &str) -> String {
    format!("Epic '{v}' is not a ticket number. Give the epic's ticket as digits, as in '52'.")
}

fn msg_bad_turn(v: &str) -> String {
    format!("Turn '{v}' is not a turn name. Give the plan's turn name, as in 'T01'.")
}

fn msg_no_such_turn(v: &str, turns: &[&str]) -> String {
    let list: Vec<String> = turns.iter().map(|t| format!("'{t}'")).collect();
    format!(
        "Turn '{v}' is not a turn of the plan; its turns are {}.",
        list.join(", ")
    )
}

fn msg_plan_file(value: &str, why: &str) -> String {
    format!("Plan '{value}' is not a valid plan: {why}.")
}

const HELP_TAIL: &str = "Run 'agentspace lease create --help'.";

fn msg_pair(first: &str, second: &str) -> String {
    let tail = match (first, second) {
        ("--plan", "--size") => "the plan picks the size.",
        ("--board" | "--epic", "--plan") => "the epic carries the plan.",
        ("--board" | "--epic", "--size") => "the epic's plan picks the size.",
        other => panic!("No pair {other:?}"),
    };
    format!("Options {first} and {second} cannot be given together; {tail} {HELP_TAIL}")
}

const MSG_BOARD_NEEDS_EPIC: &str =
    "Option --board needs --epic, the epic's ticket number. Run 'agentspace lease create --help'.";
const MSG_EPIC_NEEDS_BOARD: &str =
    "Option --epic needs --board, the board's ID. Run 'agentspace lease create --help'.";
const MSG_TURN_NEEDS_PLAN: &str =
    "Option --turn needs a plan, from --plan or --epic. Run 'agentspace lease create --help'.";

fn msg_needs_value(flag: &str) -> String {
    format!("Option --{flag} needs a value. Run 'agentspace lease create --help'.")
}

fn msg_empty_value(flag: &str) -> String {
    format!("Option --{flag} has an empty value. Run 'agentspace lease create --help'.")
}

fn msg_twice(flag: &str) -> String {
    format!("Option --{flag} is given twice. Give it once.")
}

/// lease-api.md, The plan block: the `<why>` of each fault of a description's lines.
const WHY_NO_BLOCK: &str = "its description has no agentspace-plan block";
const WHY_TWO_BLOCKS: &str = "its description has more than one agentspace-plan block";
const WHY_NEAR_MISS: &str =
    "a line that starts an agentspace-plan block is not exactly the fence line";
const WHY_UNCLOSED: &str = "its agentspace-plan block is never closed";

/// lease-api.md, Plan: the `<why>` of each check of a plan's content.
const WHY_LARGER: &str = "the file is larger than 262144 bytes";
const WHY_JSON: &str = "the file is not valid JSON";
const WHY_SCHEMA: &str = "the document does not match the plan schema";

// ---- The token and the blob ----------------------------------------------------------------------

static TOKEN_COUNT: AtomicU32 = AtomicU32::new(0);

/// A made-up token: 43 characters of unpadded base64url, patterned to read as fake, and different
/// for every stand-in.
fn new_token() -> String {
    let n = TOKEN_COUNT.fetch_add(1, Ordering::SeqCst);
    let token = format!(
        "FAKE-TOKEN-NOT-REAL_{:011}{:012}",
        u64::from(std::process::id()),
        n
    );
    assert_eq!(token.len(), 43, "The made-up token is 43 characters");
    token
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Windows' data protection, as localswim protects a token: `CryptProtectData`, user scope (no
/// machine flag), with the entropy given or none. The only module of this file that calls Windows.
#[allow(unsafe_code)]
mod dpapi {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
    };

    /// The blob that protects `plain` for the Windows user the test runs as.
    pub fn protect(plain: &[u8], entropy: Option<&[u8]>) -> Vec<u8> {
        let input = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(plain.len()).expect("The plaintext is small"),
            pbData: plain.as_ptr().cast_mut(),
        };
        let entropy_blob = entropy.map(|e| CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(e.len()).expect("The entropy is small"),
            pbData: e.as_ptr().cast_mut(),
        });
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: null_mut(),
        };
        // SAFETY: `input` and the entropy blob point at slices that outlive the call and are only
        // read; `output` is a valid blob for the call to fill; the other arguments are the NULLs
        // the function allows, and no prompt can show.
        let ok = unsafe {
            CryptProtectData(
                &input,
                null(),
                entropy_blob.as_ref().map_or(null(), |b| b as *const _),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        assert!(ok != 0, "CryptProtectData failed");
        // SAFETY: on success `pbData` points at `cbData` bytes the system allocated for us.
        let blob =
            unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
        // SAFETY: the buffer was allocated by the call above, which is freed with `LocalFree`, and
        // nothing refers to it any more.
        unsafe { LocalFree(output.pbData.cast()) };
        blob
    }
}

/// The text of a good `bot.json` for this blob (localswim's `docs/windows-operations.md`).
fn bot_text(blob: &[u8]) -> String {
    format!(
        r#"{{"format":"localswim-credential","format_version":1,"role":"bot","token_dpapi_hex":"{}"}}"#,
        hex(blob)
    )
}

/// The text of a good `bot.json` for this token.
fn bot_text_for(token: &str) -> String {
    bot_text(&dpapi::protect(&utf16le(token), None))
}

// ---- The stand-in board --------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Auth {
    Ok,
    Wrong,
    None,
}

/// One request the stand-in got. It holds no part of the token: the target and the one header
/// value it keeps, `accept-encoding`, are kept through [`redact`].
#[derive(Clone, Debug)]
struct Req {
    method: String,
    target: String,
    auth: Auth,
    /// The header names, in lowercase, in the order sent.
    names: Vec<String>,
    accept_encoding: Option<String>,
    body_len: usize,
    chunked: bool,
    /// Whether the token's text is in the request line, a body or any header but `Authorization`.
    token_elsewhere: bool,
}

impl Req {
    fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }

    fn query(&self) -> &str {
        self.target.split_once('?').map_or("", |(_, q)| q)
    }

    fn is_ticket_view(&self) -> bool {
        self.query().split('&').any(|p| p == "view=ticket")
    }

    fn is_summary_view(&self) -> bool {
        self.query().split('&').any(|p| p == "view=summary")
    }

    fn has(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }
}

/// What the stand-in answers.
#[derive(Clone)]
enum Reply {
    Respond(Resp),
    /// Reads the request and closes the connection with no answer.
    Close,
    /// Reads the request and holds the connection open with no answer, until the test ends it.
    Hang,
}

#[derive(Clone)]
struct Resp {
    status: u16,
    ctype: Option<String>,
    body: Vec<u8>,
    extra: Vec<(String, String)>,
    /// A `Content-Length` other than the body's, as the headers say.
    declared_len: Option<u64>,
    /// Holds the connection open after the bytes, until the client or the test ends it.
    hold: bool,
}

impl Resp {
    fn new(status: u16) -> Resp {
        Resp {
            status,
            ctype: None,
            body: Vec::new(),
            extra: Vec::new(),
            declared_len: None,
            hold: false,
        }
    }

    fn ctype(mut self, c: &str) -> Resp {
        self.ctype = Some(c.to_string());
        self
    }

    fn body(mut self, b: impl Into<Vec<u8>>) -> Resp {
        self.body = b.into();
        self
    }

    fn header(mut self, name: &str, value: &str) -> Resp {
        self.extra.push((name.to_string(), value.to_string()));
        self
    }

    fn declared(mut self, n: u64) -> Resp {
        self.declared_len = Some(n);
        self
    }

    fn hold(mut self) -> Resp {
        self.hold = true;
        self
    }

    fn reply(self) -> Reply {
        Reply::Respond(self)
    }
}

fn reason_of(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        409 => "Conflict",
        410 => "Gone",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

/// localswim's Problem document, with markers in its `title` and `detail`.
fn problem_text(status: u16, code: &str) -> String {
    json!({
        "type": "about:blank",
        "title": "PROBLEM-MARKER-TITLE",
        "status": status,
        "code": code,
        "detail": "PROBLEM-MARKER-DETAIL",
        "request_id": "0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f",
        "outcome": "not_applied",
        "errors": []
    })
    .to_string()
}

/// An answer with localswim's Problem document, `application/problem+json`.
fn problem(status: u16, code: &str) -> Reply {
    Resp::new(status)
        .ctype("application/problem+json")
        .body(problem_text(status, code))
        .reply()
}

/// An answer with a status, no body and no `Content-Type`.
fn bare(status: u16) -> Reply {
    Resp::new(status).reply()
}

fn code_of(status: u16) -> &'static str {
    match status {
        401 => "unauthorized",
        403 => "forbidden",
        404 => "not_found",
        405 => "method_not_allowed",
        406 => "not_acceptable",
        400 => "invalid_request",
        409 => "conflict",
        _ => "internal_error",
    }
}

/// One card of a stand-in's fixture. `revision`, `archived` and `description` are JSON literals, so
/// that a test can give a value of the wrong type or form.
#[derive(Clone)]
struct CardFx {
    revision: String,
    archived: String,
    description: String,
}

impl CardFx {
    fn good(revision: u64, description: &str) -> CardFx {
        CardFx {
            revision: revision.to_string(),
            archived: "false".to_string(),
            description: serde_json::to_string(description).expect("A string can be written"),
        }
    }

    fn archived(revision: u64, description: &str) -> CardFx {
        CardFx {
            archived: "true".to_string(),
            ..CardFx::good(revision, description)
        }
    }

    fn raw(revision: &str, archived: &str, description_json: &str) -> CardFx {
        CardFx {
            revision: revision.to_string(),
            archived: archived.to_string(),
            description: description_json.to_string(),
        }
    }
}

struct Fixture {
    token: String,
    /// Board ID in lowercase, then the cards by ticket.
    boards: BTreeMap<String, BTreeMap<u64, CardFx>>,
}

fn card_ctx(ticket: u64, title: &str) -> String {
    format!(
        r#"{{"id":"0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f","ticket":{ticket},"revision":2,"title":"{title}","archived":false,"lane":{{"id":"1b2c3d4e-1b2c-4d3e-9f4a-1b2c3d4e5f6a","key":"ready","label":"Ready","role":"ready","retired":false}},"owner":"bot","priority":"normal"}}"#
    )
}

/// localswim's `TicketInspection`, whole, with every part the client has no use for filled in and
/// marked (conformance.md, The stand-in board).
fn inspection_text(board: &str, ticket: u64, card: &CardFx, prose: bool) -> String {
    let prose_part = if prose {
        format!(
            r#","prose":{{"description":{},"comments":[{{"id":"0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f","revision":1,"body":"COMMENT-MARKER-ONE"}},{{"id":"1b2c3d4e-1b2c-4d3e-9f4a-1b2c3d4e5f6a","revision":1,"body":"COMMENT-MARKER-TWO"}}]}}"#,
            card.description
        )
    } else {
        String::new()
    };
    format!(
        r#"{{"board_id":"{board}","board_revision":3,"card":{{"id":"0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f","ticket":{ticket},"revision":{},"title":"TITLE-MARKER-6a0c","archived":{},"lane":{{"id":"1b2c3d4e-1b2c-4d3e-9f4a-1b2c3d4e5f6a","key":"ready","label":"Ready","role":"ready","retired":false}},"owner":"bot","priority":"normal"}},"relationships":[{{"id":"1b2c3d4e-1b2c-4d3e-9f4a-1b2c3d4e5f6a","revision":2,"kind":"blocks","direction":"outgoing","card":{}}}],"parent":{},"children":[{}]{prose_part}}}"#,
        card.revision,
        card.archived,
        card_ctx(60, "RELATED-MARKER-7c1d"),
        card_ctx(10, "PARENT-MARKER-2b8e"),
        card_ctx(61, "CHILD-MARKER-5d4f"),
    )
}

/// What a stand-in that is localswim answers, in the order the conformance suite's stand-in checks
/// a request: the method, the API's prefix, the token, then the board and the card.
fn faithful(fx: &Fixture, req: &Req) -> Reply {
    if req.method != "GET" {
        return problem(405, "method_not_allowed");
    }
    let Some(rest) = req.path().strip_prefix("/api/2026-11-10/boards/") else {
        return bare(404);
    };
    if req.auth != Auth::Ok {
        return problem(401, "unauthorized");
    }
    let board = rest.to_ascii_lowercase();
    let params: BTreeMap<&str, &str> = req
        .query()
        .split('&')
        .filter_map(|p| p.split_once('='))
        .collect();
    let cards = fx.boards.get(&board);
    match params.get("view").copied() {
        Some("summary") => match cards {
            Some(_) => Resp::new(200)
                .ctype("application/json")
                .body(json!({"id": board}).to_string())
                .reply(),
            None => problem(404, "not_found"),
        },
        Some("ticket") => {
            let Some(ticket) = params.get("ticket").and_then(|t| t.parse::<u64>().ok()) else {
                return problem(400, "invalid_request");
            };
            let prose = params.get("include_prose") == Some(&"true");
            match cards.and_then(|c| c.get(&ticket)) {
                Some(card) => Resp::new(200)
                    .ctype("application/json")
                    .body(inspection_text(&board, ticket, card, prose))
                    .reply(),
                None => problem(404, "not_found"),
            }
        }
        _ => problem(400, "invalid_request"),
    }
}

type Hook = Box<dyn Fn(&Req) -> Option<Reply> + Send + Sync>;

/// A hook that answers `reply` to the ticket view of any card and leaves every other request to the
/// stand-in as localswim.
fn on_ticket(reply: Reply) -> Hook {
    Box::new(move |req| req.is_ticket_view().then(|| reply.clone()))
}

/// A hook that answers 404 with the Problem document to the ticket view, and `reply` to the summary
/// view.
fn not_found_then(reply: Reply) -> Hook {
    Box::new(move |req| {
        if req.is_ticket_view() {
            Some(problem(404, "not_found"))
        } else if req.is_summary_view() {
            Some(reply.clone())
        } else {
            None
        }
    })
}

/// A hook that answers `reply` to every request.
fn always(reply: Reply) -> Hook {
    Box::new(move |_| Some(reply.clone()))
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn read_request(s: &mut TcpStream, token: &str, stop: &AtomicBool) -> Option<Req> {
    let began = Instant::now();
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let end = loop {
        if let Some(i) = find(&buf, b"\r\n\r\n") {
            break i + 4;
        }
        if stop.load(Ordering::SeqCst) || began.elapsed() > Duration::from_secs(10) {
            return None;
        }
        match s.read(&mut chunk) {
            Ok(0) | Err(_) if buf.is_empty() => return None,
            Ok(0) => break buf.len(),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return None,
        }
    };
    let head = String::from_utf8_lossy(&buf[..end.saturating_sub(4).min(buf.len())]).into_owned();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split(' ');
    let (method, target, version) = (parts.next(), parts.next(), parts.next());
    let (Some(method), Some(target), Some(_version)) = (method, target, version) else {
        return Some(Req {
            method: "INVALID".to_string(),
            target: "-".to_string(),
            auth: Auth::None,
            names: Vec::new(),
            accept_encoding: None,
            body_len: 0,
            chunked: false,
            token_elsewhere: false,
        });
    };
    let mut names = Vec::new();
    let mut auth = Auth::None;
    let mut accept_encoding = None;
    let mut content_length = 0usize;
    let mut chunked = false;
    let mut token_elsewhere = target.contains(token);
    let expected = format!("Bearer {token}");
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match name.as_str() {
            "authorization" => {
                auth = if value == expected {
                    Auth::Ok
                } else {
                    Auth::Wrong
                };
            }
            other => {
                if value.contains(token) {
                    token_elsewhere = true;
                }
                match other {
                    // Kept with the token replaced, so no later message can hold it.
                    "accept-encoding" => accept_encoding = Some(redact(value, token)),
                    "content-length" => content_length = value.parse().unwrap_or(0),
                    "transfer-encoding" => chunked = value.to_ascii_lowercase().contains("chunked"),
                    _ => {}
                }
            }
        }
        names.push(name);
    }
    let mut body_len = buf.len().saturating_sub(end);
    while body_len < content_length && began.elapsed() < Duration::from_secs(10) {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        match s.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                body_len += n;
                if find(&chunk[..n], token.as_bytes()).is_some() {
                    token_elsewhere = true;
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    Some(Req {
        method: method.to_string(),
        // The target is kept with the token replaced: `token_elsewhere` already records that it
        // was there, and every message that formats the target then holds no part of it.
        target: redact(target, token),
        auth,
        names,
        accept_encoding,
        body_len,
        chunked,
        token_elsewhere,
    })
}

/// `text` with every copy of the stand-in's token replaced by `<token>`, so that a request's
/// target or header value can be formatted into a test's message without the token (#0238: "A
/// token, a stand-in's included, is never printed, logged or passed as an argument").
fn redact(text: &str, token: &str) -> String {
    if token.is_empty() {
        text.to_string()
    } else {
        text.replace(token, "<token>")
    }
}

/// Holds the connection until the client closes it, the test ends the stand-in, or a minute passes.
fn hold_open(s: &mut TcpStream, stop: &AtomicBool) {
    let began = Instant::now();
    let mut chunk = [0u8; 1024];
    while !stop.load(Ordering::SeqCst) && began.elapsed() < Duration::from_secs(60) {
        match s.read(&mut chunk) {
            Ok(0) => return,
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return,
        }
    }
}

fn write_reply(s: &mut TcpStream, reply: Reply, stop: &AtomicBool) {
    match reply {
        Reply::Close => {
            let _ = s.shutdown(Shutdown::Both);
        }
        Reply::Hang => hold_open(s, stop),
        Reply::Respond(r) => {
            let mut head = format!("HTTP/1.1 {} {}\r\n", r.status, reason_of(r.status));
            if let Some(c) = &r.ctype {
                head.push_str(&format!("Content-Type: {c}\r\n"));
            }
            if r.status != 204 {
                let n = r.declared_len.unwrap_or(r.body.len() as u64);
                head.push_str(&format!("Content-Length: {n}\r\n"));
            }
            for (k, v) in &r.extra {
                head.push_str(&format!("{k}: {v}\r\n"));
            }
            head.push_str("Connection: close\r\n\r\n");
            let mut bytes = head.into_bytes();
            bytes.extend_from_slice(&r.body);
            if s.write_all(&bytes).is_err() {
                return;
            }
            let _ = s.flush();
            if r.hold {
                hold_open(s, stop);
            } else {
                let _ = s.shutdown(Shutdown::Write);
                // Lets the client read what was sent before the connection goes.
                let began = Instant::now();
                let mut chunk = [0u8; 256];
                while began.elapsed() < Duration::from_millis(500) {
                    match s.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(e)
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) => {}
                        Err(_) => break,
                    }
                }
            }
        }
    }
}

/// A stand-in board: one process-local listener on `127.0.0.1`, a port the OS picked, in a thread.
/// It is ended when it is dropped.
struct Standin {
    port: u16,
    token: String,
    log: Arc<Mutex<Vec<Req>>>,
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
    conns: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl Standin {
    fn start(fx: Fixture, hook: Option<Hook>) -> Standin {
        let listener = (0..20)
            .map(|_| TcpListener::bind(("127.0.0.1", 0)).expect("A loopback port can be bound"))
            .find(|l| l.local_addr().is_ok_and(|a| a.port() != 8811))
            .expect("A port other than 8811");
        let port = listener
            .local_addr()
            .expect("The listener has an address")
            .port();
        assert_ne!(port, 8811, "The stand-in never uses localswim's own port");
        listener
            .set_nonblocking(true)
            .expect("The listener can be polled");
        let token = fx.token.clone();
        let log = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let conns = Arc::new(Mutex::new(Vec::new()));
        let fx = Arc::new(fx);
        let hook = Arc::new(hook);
        let accept = {
            let (log, stop, conns) = (log.clone(), stop.clone(), conns.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut conn, _)) => {
                            let _ = conn.set_nonblocking(false);
                            let _ = conn.set_read_timeout(Some(Duration::from_millis(50)));
                            let (fx, hook, log, stop) =
                                (fx.clone(), hook.clone(), log.clone(), stop.clone());
                            let handle = std::thread::spawn(move || {
                                let Some(req) = read_request(&mut conn, &fx.token, &stop) else {
                                    return;
                                };
                                log.lock().unwrap().push(req.clone());
                                let reply = hook
                                    .as_ref()
                                    .as_ref()
                                    .and_then(|h| h(&req))
                                    .unwrap_or_else(|| faithful(&fx, &req));
                                write_reply(&mut conn, reply, &stop);
                            });
                            conns.lock().unwrap().push(handle);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            })
        };
        Standin {
            port,
            token,
            log,
            stop,
            accept: Some(accept),
            conns,
        }
    }

    fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn token(&self) -> &str {
        &self.token
    }

    fn log(&self) -> Vec<Req> {
        self.log.lock().unwrap().clone()
    }

    fn count(&self) -> usize {
        self.log.lock().unwrap().len()
    }

    /// Sends this request text to the stand-in as it is, and returns the answer's status line.
    fn send_raw(&self, bytes: &[u8]) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).expect("The stand-in is there");
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        s.write_all(bytes).expect("The request can be sent");
        let mut out = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            match s.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    out.extend_from_slice(&chunk[..n]);
                    if find(&out, b"\r\n\r\n").is_some() {
                        break;
                    }
                }
            }
        }
        String::from_utf8_lossy(&out)
            .lines()
            .next()
            .unwrap_or("")
            .to_string()
    }
}

impl Drop for Standin {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.accept.take() {
            let _ = h.join();
        }
        let handles: Vec<JoinHandle<()>> = std::mem::take(&mut *self.conns.lock().unwrap());
        for h in handles {
            let _ = h.join();
        }
    }
}

// ---- What the log of a stand-in MUST show ---------------------------------------------------------

fn ticket_target(board: &str, ticket: u64) -> String {
    format!("/api/2026-11-10/boards/{board}?view=ticket&ticket={ticket}&include_prose=true")
}

fn summary_target(board: &str) -> String {
    format!("/api/2026-11-10/boards/{board}?view=summary")
}

/// The headers the client MAY send (lease-api.md, The requests): `Authorization`, and `Host`,
/// `User-Agent`, `Accept` and `Connection`, with an `Accept-Encoding` of identity at most.
const ALLOWED_HEADERS: [&str; 6] = [
    "authorization",
    "host",
    "user-agent",
    "accept",
    "connection",
    "accept-encoding",
];

/// The problems with a log against the requests the spec says were made, in order: each a GET to
/// exactly its path and query, with the token and nothing else of the client's that MUST NOT be
/// there (lease-api.md, The board read: "The requests", "The client only reads").
fn log_problems(log: &[Req], expected: &[String]) -> Vec<String> {
    let mut p = Vec::new();
    if log.len() != expected.len() {
        p.push(format!(
            "{} requests were made, not {}: {:?}",
            log.len(),
            expected.len(),
            log.iter()
                .map(|r| format!("{} {}", r.method, r.path()))
                .collect::<Vec<_>>()
        ));
    }
    for (i, r) in log.iter().enumerate() {
        let at = format!("Request {}", i + 1);
        if r.method != "GET" {
            p.push(format!("{at}: the method is {}, not GET", r.method));
        }
        if let Some(want) = expected.get(i)
            && &r.target != want
        {
            p.push(format!("{at}: the target is {:?}, not {want:?}", r.target));
        }
        if r.auth != Auth::Ok {
            p.push(format!(
                "{at}: the Authorization header is {:?}, not the token's",
                r.auth
            ));
        }
        if r.body_len != 0 || r.chunked {
            p.push(format!("{at}: it carried a body"));
        }
        for banned in ["cookie", "origin", "if-none-match", "content-length"] {
            if r.has(banned) {
                p.push(format!("{at}: it carried a {banned} header"));
            }
        }
        for n in &r.names {
            if !ALLOWED_HEADERS.contains(&n.as_str()) {
                p.push(format!(
                    "{at}: it carried a header '{n}' the spec does not allow"
                ));
            }
        }
        if let Some(ae) = &r.accept_encoding {
            let only_identity = ae.split(',').all(|part| {
                part.split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .eq_ignore_ascii_case("identity")
            });
            if !only_identity {
                p.push(format!(
                    "{at}: its Accept-Encoding asks for more than identity: {ae:?}"
                ));
            }
        }
        if r.token_elsewhere {
            p.push(format!(
                "{at}: the token is in the request line, a body or a header but Authorization"
            ));
        }
    }
    p
}

/// Whether a target is `/api/2026-11-10/boards/<board>` and then `?view=summary` or `?view=ticket&
/// ticket=<n>&include_prose=true`, the board in lowercase and the ticket with no leading zero
/// (lease-api.md, The requests).
fn target_is_a_spec_target(target: &str) -> bool {
    let Some(rest) = target.strip_prefix("/api/2026-11-10/boards/") else {
        return false;
    };
    let Some((board, query)) = rest.split_once('?') else {
        return false;
    };
    let b = board.as_bytes();
    let uuid = b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_digit() || (b'a'..=b'f').contains(c),
        });
    if !uuid {
        return false;
    }
    if query == "view=summary" {
        return true;
    }
    query
        .strip_prefix("view=ticket&ticket=")
        .and_then(|q| q.strip_suffix("&include_prose=true"))
        .is_some_and(|n| {
            !n.is_empty() && !n.starts_with('0') && n.bytes().all(|c| c.is_ascii_digit())
        })
}

#[track_caller]
fn assert_log(standin: &Standin, expected: &[String]) {
    let p = log_problems(&standin.log(), expected);
    assert!(p.is_empty(), "\n{}\n", p.join("\n"));
}

#[track_caller]
fn assert_no_request(standin: &Standin) {
    assert_eq!(standin.count(), 0, "The board was read, and it MUST NOT be");
}

// ---- Plans and descriptions ---------------------------------------------------------------------

fn sub(name: &str) -> Value {
    json!({"name": name, "model": "Opus", "effort": "high"})
}

fn story(name: &str, subs: Vec<Value>) -> Value {
    json!({"name": name, "subagents": subs})
}

/// A story of `n` subagents none of which starts after another: its peak is `n`.
fn wide(name: &str, n: usize) -> Value {
    story(
        name,
        (0..n).map(|i| sub(&format!("a{i}"))).collect::<Vec<_>>(),
    )
}

fn turn(name: &str, budget: Option<u32>, stories: Vec<Value>) -> Value {
    let mut t = json!({"name": name, "stories": stories});
    if let Some(b) = budget {
        t["budget_minutes"] = json!(b);
    }
    t
}

/// The plan most tests read: six turns, a busiest one (T02) whose budget is not the others', and
/// budgets that round, that sit at the bounds and that are not given.
fn plan_a() -> Value {
    json!({"name": PLAN_NAME, "turns": [
        turn("T01", Some(180), vec![wide("S1", 3), wide("S2", 1)]),
        turn("T02", Some(45), vec![wide("S1", 9)]),
        turn("T03", None, vec![wide("S1", 1)]),
        turn("T04", Some(1), vec![wide("S1", 2)]),
        turn("T05", Some(960), vec![wide("S1", 1)]),
        turn("T06", Some(3), vec![wide("S1", 1)])
    ]})
}

/// Each turn of `plan_a`: its name, the size its agents get, its agents at once and its TTL in
/// seconds, as lease-api.md's TTL table gives them (ceil(budget * 3 / 2) minutes, or 8 hours).
const TURNS_A: [(&str, &str, u64, u64); 6] = [
    ("T01", "m", 5, 16_200),
    ("T02", "l", 10, 4_080),
    ("T03", "s", 2, 28_800),
    ("T04", "s", 3, 120),
    ("T05", "s", 2, 86_400),
    ("T06", "s", 2, 300),
];

/// The busiest turn of `plan_a`.
const BUSIEST_A: (&str, &str, u64, u64) = ("T02", "l", 10, 4_080);

fn compact(plan: &Value) -> String {
    serde_json::to_string(plan).expect("A plan can be written")
}

/// An epic's description: prose before and after, the plan block between, and prose that disagrees
/// with it (the block is the plan, lease-api.md, The plan block).
fn description_of(plan_text: &str) -> String {
    format!(
        "# The epic\n\nPROSE-MARKER-9f3a says every turn has 40 agents.\n\n```agentspace-plan\n{plan_text}\n```\n\nThe execution plan by turn follows. PROSE-MARKER-9f3a again.\n"
    )
}

fn plan_description(plan: &Value) -> String {
    description_of(&compact(plan))
}

// ---- The rig -------------------------------------------------------------------------------------

/// The program's own file in the lab's `bin` folder, which a plan lease's `create` looks beside for
/// agentspace's ProxyCommand program (lease-api.md, Messages: "Way to reach the host not found").
const PROXY_PROGRAM: &str = "agentspace-ssh-proxy.exe";

fn install_proxy(lab: &Lab) {
    let bin = lab.exe.parent().expect("The program has a folder");
    std::fs::create_dir_all(bin).expect("The program's folder can be made");
    std::fs::write(bin.join(PROXY_PROGRAM), b"").expect("The proxy program can be written");
}

fn receipt_text(base: &str, creds: &Path) -> String {
    json!({
        "executable": "C:\\fake\\localswim.exe",
        "data_directory": "C:\\fake\\data",
        "base": base,
        "credential_directory": creds.to_string_lossy()
    })
    .to_string()
}

fn write_text(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("A file has a folder")).unwrap();
    std::fs::write(path, text.as_bytes()).expect("The file can be written");
}

fn as_run(ran: &Ran) -> Run {
    Run {
        status: Some(i32::from(ran.code)),
        stdout: ran.stdout.clone(),
        stderr: ran.stderr.clone(),
    }
}

/// The problems with a run that was meant to end in this error: the shape and the exit code of
/// lease-api.md's Output, the message given, and the error document against v2's error schema.
fn error_problems(
    root: &Path,
    ran: &Ran,
    code: &str,
    field: Option<&str>,
    message: &str,
) -> Vec<String> {
    let mut p = failure_problems(&as_run(ran), code, field, Some(message));
    match serde_json::from_slice::<Value>(ran.stderr.trim_ascii_end()) {
        Ok(doc) => {
            p.extend(schema_problems("error.output.schema.json", &doc));
            if code == "internal"
                && let Some(m) = doc["message"].as_str()
            {
                p.extend(lab::leak_problems(m, root));
            }
        }
        Err(_) => p.push("Stderr is not JSON".to_string()),
    }
    p
}

#[track_caller]
fn assert_no_problems(problems: Vec<String>) {
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

/// One board, its stand-in, a lab with a fake driver, and the receipt and credential the lease CLI
/// reads, all in the lab's throwaway folder.
struct Rig {
    lab: Lab,
    h: fake_driver::Handle,
    standin: Standin,
    appdata: PathBuf,
    creds: PathBuf,
}

impl Rig {
    /// The standard rig: `BOARD`'s card `EPIC` holds `plan_a`.
    fn new() -> Rig {
        Rig::with_plan(&plan_a())
    }

    fn with_plan(plan: &Value) -> Rig {
        Rig::with_description(&plan_description(plan))
    }

    fn with_description(description: &str) -> Rig {
        Rig::with_cards(vec![(EPIC, CardFx::good(REVISION, description))], None)
    }

    fn with_hook(hook: Hook) -> Rig {
        Rig::with_cards(
            vec![(EPIC, CardFx::good(REVISION, &plan_description(&plan_a())))],
            Some(hook),
        )
    }

    fn with_cards(cards: Vec<(u64, CardFx)>, hook: Option<Hook>) -> Rig {
        let fx = Fixture {
            token: new_token(),
            boards: BTreeMap::from([(BOARD.to_string(), cards.into_iter().collect())]),
        };
        let standin = Standin::start(fx, hook);
        let (mut lab, h) = Lab::with_fake();
        install_proxy(&lab);
        let appdata = lab.root.join("AppData").join("Local");
        let creds = appdata.join("localswim-v2").join("credentials");
        write_text(
            &appdata.join("localswim-v2").join("service.json"),
            &receipt_text(&standin.base(), &creds),
        );
        write_text(&creds.join("bot.json"), &bot_text_for(standin.token()));
        lab.set_var("LOCALAPPDATA", appdata.clone().into_os_string());
        Rig {
            lab,
            h,
            standin,
            appdata,
            creds,
        }
    }

    fn receipt_path(&self) -> PathBuf {
        self.appdata.join("localswim-v2").join("service.json")
    }

    fn bot_path(&self) -> PathBuf {
        self.creds.join("bot.json")
    }

    fn write_receipt(&self, text: &str) {
        write_text(&self.receipt_path(), text);
    }

    /// The receipt of this rig's stand-in, with `base` as given.
    fn receipt_with_base(&self, base: &str) -> String {
        receipt_text(base, &self.creds)
    }

    fn port(&self) -> u16 {
        self.standin.port
    }

    /// Fails when the token, or a marker, is in the run's output. It reports which, never the value.
    #[track_caller]
    fn guard(&self, ran: &Ran) {
        // Whatever the run was, every request the stand-in has had is a GET of one of the two
        // targets the spec gives, with the token and nothing the spec forbids.
        let log = self.standin.log();
        let expected: Vec<String> = log
            .iter()
            .map(|r| {
                if target_is_a_spec_target(&r.target) {
                    r.target.clone()
                } else {
                    "a target of the spec's shape".to_string()
                }
            })
            .collect();
        let p = log_problems(&log, &expected);
        assert!(p.is_empty(), "\n{}\n", p.join("\n"));
        for (what, bytes) in [("stdout", &ran.stdout), ("stderr", &ran.stderr)] {
            let text = String::from_utf8_lossy(bytes);
            assert!(
                !text.contains(self.standin.token()),
                "The token is in the run's {what}"
            );
            for m in MARKERS {
                assert!(!text.contains(m), "The marker {m} is in the run's {what}");
            }
        }
    }

    /// `create --project swim` and these arguments, in process, with the fake driver.
    #[track_caller]
    fn create(&self, more: &[&str]) -> Ran {
        let mut line = vec!["lease", "create", "--project", "swim"];
        line.extend_from_slice(more);
        let ran = self.lab.run(&line);
        self.guard(&ran);
        ran
    }

    /// `create --project swim --board <BOARD> --epic <EPIC>` and these arguments.
    #[track_caller]
    fn board_create(&self, more: &[&str]) -> Ran {
        let epic = EPIC.to_string();
        let mut line = vec!["--board", BOARD, "--epic", &epic];
        line.extend_from_slice(more);
        self.create(&line)
    }

    fn place_calls(&self) -> Vec<Call> {
        self.h
            .calls()
            .into_iter()
            .filter(|c| matches!(c, Call::Place { .. }))
            .collect()
    }

    /// Writes a plan file to the working folder and returns its name.
    fn plan_file(&self, plan: &Value) -> String {
        std::fs::write(self.lab.cwd.join("p.json"), compact(plan)).expect("A plan can be written");
        "p.json".to_string()
    }

    /// `true` when the lab holds no lease and has made no folder: the state folder, the OpenSSH
    /// client folder and the lease folders (lease-api.md, The board read: "records no lease, and
    /// creates and changes nothing").
    fn nothing_made(&self) -> Vec<String> {
        let mut p = Vec::new();
        if self.lab.state.exists() {
            p.push("The state folder was made".to_string());
        }
        if self.lab.ssh.exists() {
            p.push("The OpenSSH client folder was made".to_string());
        }
        if !self.h.calls().is_empty() {
            p.push("A driver was asked".to_string());
        }
        let list = self.lab.run(&["lease", "list"]);
        match serde_json::from_slice::<Value>(list.stdout.trim_ascii_end()) {
            Ok(doc) if doc == json!({"leases": []}) => {}
            _ => p.push("`lease list` shows a lease or fails".to_string()),
        }
        p
    }
}

/// The lease document without what differs between two leases of one plan: the ID and what derives
/// from it.
fn same_lease(mut doc: Value) -> Value {
    let o = doc.as_object_mut().expect("A lease is an object");
    o.remove("id");
    o.remove("ssh");
    doc
}

// ---- A board lease is a --plan lease of the same plan ------------------------------------------------

/// The `Place` call's size and TTL.
fn placed(call: &Call) -> (String, u32) {
    match call {
        Call::Place {
            size, ttl_seconds, ..
        } => (size.clone(), *ttl_seconds),
        other => panic!("Not a place call: {other:?}"),
    }
}

#[test]
fn a_board_lease_gets_its_platforms_default_profile_as_a_plan_file_lease_does() {
    // lease-api.md, Profile: "The default is the platform's whether or not a plan is given, from
    // --plan or --epic" (D93). The same plan from the board and from a file asks the driver for
    // the same platform and profile.
    let cases = [
        (vec![], "linux/arm64", "agent-linux"),
        (
            vec!["--platform", "linux/amd64"],
            "linux/amd64",
            "agent-linux",
        ),
        (
            vec!["--platform", "windows/amd64"],
            "windows/amd64",
            "agent-windows",
        ),
        (
            vec!["--platform", "windows/arm64"],
            "windows/arm64",
            "agent-linux",
        ),
        (
            vec!["--platform", "windows/amd64", "--profile", "agent-linux"],
            "windows/amd64",
            "agent-linux",
        ),
    ];
    let rig = Rig::new();
    let file = rig.plan_file(&plan_a());
    let mut problems = Vec::new();
    for (more, platform, profile) in cases {
        rig.h.clear();
        let mut from_file = vec!["--plan", file.as_str()];
        from_file.extend_from_slice(&more);
        let a = rig.create(&from_file).lease_doc();
        let b = rig.board_create(&more).lease_doc();
        for (src, doc) in [("file", &a), ("board", &b)] {
            if doc["platform"] != platform || doc["profile"] != profile {
                problems.push(format!(
                    "{more:?} ({src}): printed {} and {}",
                    doc["platform"], doc["profile"]
                ));
            }
        }
        let asked: Vec<(String, String)> = rig
            .place_calls()
            .iter()
            .map(|c| match c {
                Call::Place {
                    platform, profile, ..
                } => (platform.clone(), profile.clone()),
                other => panic!("Not a place call: {other:?}"),
            })
            .collect();
        let want = (platform.to_string(), profile.to_string());
        if asked != [want.clone(), want] {
            problems.push(format!("{more:?}: the driver was asked {asked:?}"));
        }
    }
    assert_no_problems(problems);
}

#[test]
fn a_board_lease_of_each_turn_and_of_the_busiest_is_what_the_same_plan_given_with_plan_gives() {
    // lease-api.md, `create`: "the same plan gives the same lease from either source (D83): the
    // same size, `sized_for` and TTL", with `--turn` given for each turn and with none (the busiest).
    let rig = Rig::new();
    let file = rig.plan_file(&plan_a());
    let mut cases: Vec<(Option<&str>, &str, &str, u64, u64)> = TURNS_A
        .iter()
        .map(|(t, size, agents, ttl)| (Some(*t), *t, *size, *agents, *ttl))
        .collect();
    cases.push((None, BUSIEST_A.0, BUSIEST_A.1, BUSIEST_A.2, BUSIEST_A.3));
    let mut problems = Vec::new();
    for (flag, turn_name, size, agents, ttl) in cases {
        let what = flag.unwrap_or("no --turn");
        let turn_args: Vec<&str> = flag.map_or(Vec::new(), |t| vec!["--turn", t]);
        rig.h.clear();
        let before = rig.standin.count();
        let mut file_line = vec!["--plan", file.as_str()];
        file_line.extend_from_slice(&turn_args);
        let from_file = rig.create(&file_line);
        let from_board = rig.board_create(&turn_args);
        if from_file.code != 0 || from_board.code != 0 {
            problems.push(format!(
                "{what}: exits {} and {}, stderr {:?} and {:?}",
                from_file.code,
                from_board.code,
                from_file.stderr_text(),
                from_board.stderr_text()
            ));
            continue;
        }
        let (a, b) = (from_file.lease_doc(), from_board.lease_doc());
        if same_lease(a.clone()) != same_lease(b.clone()) {
            problems.push(format!("{what}: the documents differ: {a} and {b}"));
        }
        for (src, doc) in [("file", &a), ("board", &b)] {
            if doc["size"] != size {
                problems.push(format!("{what} ({src}): size {}, not {size}", doc["size"]));
            }
            let want = json!({"plan": PLAN_NAME, "turn": turn_name, "agents": agents});
            if doc["sized_for"] != want {
                problems.push(format!("{what} ({src}): sized_for {}", doc["sized_for"]));
            }
            if doc["ttl_seconds"] != ttl {
                problems.push(format!(
                    "{what} ({src}): ttl_seconds {}, not {ttl}",
                    doc["ttl_seconds"]
                ));
            }
            if doc["expires_at"] != fmt_time(NOW + i64::try_from(ttl).unwrap()) {
                problems.push(format!("{what} ({src}): expires_at {}", doc["expires_at"]));
            }
            if doc["state"] != "ready" {
                problems.push(format!("{what} ({src}): state {}", doc["state"]));
            }
        }
        let calls = rig.place_calls();
        let asked: Vec<(String, u32)> = calls.iter().map(placed).collect();
        let want = (size.to_string(), u32::try_from(ttl).unwrap());
        if asked != [want.clone(), want] {
            problems.push(format!("{what}: the driver was asked {asked:?}"));
        }
        if rig.standin.count() != before + 1 {
            problems.push(format!(
                "{what}: the board was read {} times",
                rig.standin.count() - before
            ));
        }
    }
    assert_no_problems(problems);
    assert_log(
        &rig.standin,
        &vec![ticket_target(BOARD, EPIC); rig.standin.count()],
    );
}

#[test]
fn a_board_lease_without_turn_is_for_the_busiest_turn_and_uses_its_budget_not_another_turns() {
    // lease-api.md, TTL: "The turn is the lease's: the one --turn names, or the plan's busiest".
    // The busiest turn of plan_a, T02, has 10 agents and 45 minutes: 68 minutes, 4080 s. T01's
    // budget is 180 minutes: 16200 s.
    let rig = Rig::new();
    let busiest = rig.board_create(&[]).lease_doc();
    assert_eq!(busiest["ttl_seconds"], 4080);
    assert_eq!(busiest["sized_for"]["turn"], "T02");
    let t01 = rig.board_create(&["--turn", "T01"]).lease_doc();
    assert_eq!(
        t01["ttl_seconds"], 16_200,
        "--turn's budget, not the busiest's"
    );
    assert_eq!(t01["sized_for"]["turn"], "T01");
    assert_eq!(t01["sized_for"]["agents"], 5);
    assert_eq!(t01["size"], "m");
    let t03 = rig.board_create(&["--turn", "T03"]).lease_doc();
    assert_eq!(
        t03["ttl_seconds"], 28_800,
        "A turn with no budget gets 8 hours, whatever the plan's other turns have"
    );
}

#[test]
fn a_plan_file_takes_turn_too() {
    // lease-api.md, Turn: "It applies alike to a plan from --plan <file> and to one from --epic".
    let rig = Rig::new();
    let file = rig.plan_file(&plan_a());
    for (t, size, agents, ttl) in TURNS_A {
        let doc = rig.create(&["--plan", &file, "--turn", t]).lease_doc();
        assert_eq!(doc["size"], size, "{t}");
        assert_eq!(
            doc["sized_for"],
            json!({"plan": PLAN_NAME, "turn": t, "agents": agents}),
            "{t}"
        );
        assert_eq!(doc["ttl_seconds"], ttl, "{t}");
    }
    assert_no_request(&rig.standin);
}

#[test]
fn every_ttl_a_plan_gives_is_the_budget_and_half_again_rounded_up_to_the_minute() {
    // lease-api.md, TTL: ceil(budget_minutes * 3 / 2) * 60, the table, and "8 hours" for a turn with
    // none, with a `null` budget, or with the member left out. From a file and from an epic alike.
    let rows: [(Option<Value>, u64); 10] = [
        (Some(json!(1)), 120),
        (Some(json!(2)), 180),
        (Some(json!(45)), 4_080),
        (Some(json!(120)), 10_800),
        (Some(json!(180)), 16_200),
        (Some(json!(210)), 18_900),
        (Some(json!(959)), 86_340),
        (Some(json!(960)), 86_400),
        (Some(Value::Null), 28_800),
        (None, 28_800),
    ];
    let mut problems = Vec::new();
    for (budget, ttl) in rows {
        let mut t = json!({"name": "T1", "stories": [wide("S", 2)]});
        if let Some(b) = &budget {
            t["budget_minutes"] = b.clone();
        }
        let plan = json!({"turns": [t]});
        let rig = Rig::with_plan(&plan);
        let file = rig.plan_file(&plan);
        for (src, ran) in [
            ("board", rig.board_create(&[])),
            ("file", rig.create(&["--plan", &file])),
        ] {
            let what = format!("{budget:?} ({src})");
            if ran.code != 0 {
                problems.push(format!(
                    "{what}: exit {}, stderr {}",
                    ran.code,
                    ran.stderr_text()
                ));
                continue;
            }
            let doc = ran.lease_doc();
            if doc["ttl_seconds"] != ttl {
                problems.push(format!(
                    "{what}: ttl_seconds {}, not {ttl}",
                    doc["ttl_seconds"]
                ));
            }
            if doc["expires_at"] != fmt_time(NOW + i64::try_from(ttl).unwrap()) {
                problems.push(format!("{what}: expires_at {}", doc["expires_at"]));
            }
            if doc
                .get("sized_for")
                .is_some_and(|s| s.get("plan").is_some())
            {
                problems.push(format!(
                    "{what}: sized_for.plan is there for a plan with no name"
                ));
            }
        }
    }
    assert_no_problems(problems);
}

#[test]
fn ttl_wins_over_the_budget_and_over_the_default() {
    // lease-api.md, TTL: "--ttl wins over both": the TTL --ttl gives, checked as any --ttl is.
    let rig = Rig::new();
    let file = rig.plan_file(&plan_a());
    let cases: [(&[&str], u64); 6] = [
        (&["--ttl", "2h"], 7_200),
        (&["--turn", "T01", "--ttl", "2h"], 7_200),
        (&["--turn", "T03", "--ttl", "90m"], 5_400),
        (&["--turn", "T05", "--ttl", "1m"], 60),
        (&["--turn", "T04", "--ttl", "24h"], 86_400),
        (&["--ttl", "PT1H"], 3_600),
    ];
    for (more, ttl) in cases {
        let from_board = rig.board_create(more).lease_doc();
        assert_eq!(from_board["ttl_seconds"], ttl, "{more:?}");
        let mut line = vec!["--plan", file.as_str()];
        line.extend_from_slice(more);
        let from_file = rig.create(&line).lease_doc();
        assert_eq!(from_file["ttl_seconds"], ttl, "{more:?} (file)");
        assert_eq!(same_lease(from_board), same_lease(from_file), "{more:?}");
    }
}

#[test]
fn a_too_large_turn_is_plan_too_large_with_the_message_of_how_it_was_asked_for() {
    // lease-api.md, Fields: `Plan's busiest turn '<turn>' has <n> agents at once; the largest size,
    // xl, holds <m>.` without --turn, and `Plan's turn '<turn>' ...` with it, even for the busiest;
    // the lease is unsatisfiable, its size xl, and plan_too_large is its only reason. Its TTL is the
    // turn's: 60 minutes and half again, 5400 s.
    let plan = json!({"name": "big", "turns": [
        turn("T1", None, vec![wide("S", 2)]),
        turn("T2", Some(60), vec![wide("A", 32), wide("B", 9)])
    ]});
    let rig = Rig::with_plan(&plan);
    let file = rig.plan_file(&plan);
    let busiest = "Plan's busiest turn 'T2' has 42 agents at once; the largest size, xl, holds 32.";
    let named = "Plan's turn 'T2' has 42 agents at once; the largest size, xl, holds 32.";
    let cases: [(&[&str], &str); 2] = [(&[], busiest), (&["--turn", "T2"], named)];
    for (more, message) in cases {
        let mut line = vec!["--plan", file.as_str()];
        line.extend_from_slice(more);
        for (src, ran) in [
            ("board", rig.board_create(more)),
            ("file", rig.create(&line)),
        ] {
            let doc = ran.lease_doc();
            assert_eq!(doc["state"], "unsatisfiable", "{src} {more:?}");
            assert_eq!(doc["size"], "xl", "{src} {more:?}");
            assert_eq!(
                doc["sized_for"],
                json!({"plan": "big", "turn": "T2", "agents": 42}),
                "{src} {more:?}"
            );
            assert_eq!(
                doc["reasons"].as_array().map(Vec::len),
                Some(1),
                "{src} {more:?}"
            );
            assert_eq!(
                doc["reasons"][0]["code"], "plan_too_large",
                "{src} {more:?}"
            );
            assert_eq!(doc["reasons"][0]["message"], message, "{src} {more:?}");
            assert_eq!(doc["ttl_seconds"], 5_400, "{src} {more:?}");
        }
    }
    // The turn that fits is sized for alone, though the plan has one that does not.
    for (src, ran) in [
        ("board", rig.board_create(&["--turn", "T1"])),
        ("file", rig.create(&["--plan", &file, "--turn", "T1"])),
    ] {
        let doc = ran.lease_doc();
        assert_eq!(doc["state"], "ready", "{src}");
        assert_eq!(doc["size"], "s", "{src}");
        assert_eq!(
            doc["sized_for"],
            json!({"plan": "big", "turn": "T1", "agents": 3}),
            "{src}"
        );
        assert_eq!(doc["ttl_seconds"], 28_800, "{src}");
    }
    assert!(
        rig.place_calls().iter().all(|c| placed(c).0 == "s"),
        "No driver is asked to place a plan_too_large lease"
    );
}

#[test]
fn a_board_and_an_epic_are_normalized_before_they_are_read() {
    // lease-api.md, Board and Epic: the board in lowercase and the ticket in decimal with no leading
    // zero, both read and recorded in that form, from a value with surrounding whitespace.
    let rig = Rig::new();
    for (board, epic) in [
        (BOARD.to_uppercase(), "52"),
        (format!(" {BOARD}\n"), "0052"),
        (BOARD.to_string(), " 52 "),
        (BOARD.to_string(), "0000000000000052"),
        (
            format!("\t{}\u{2028}", BOARD.to_uppercase()),
            "\u{3000}052\u{a0}",
        ),
    ] {
        let before = rig.standin.count();
        let ran = rig.create(&["--board", &board, "--epic", epic]);
        assert_eq!(ran.lease_doc()["sized_for"]["turn"], "T02");
        let log = rig.standin.log();
        assert_eq!(log.len(), before + 1);
        assert_eq!(log[before].target, ticket_target(BOARD, EPIC));
    }
    assert_log(&rig.standin, &vec![ticket_target(BOARD, EPIC); 5]);
    for rec in rig.lab.records() {
        assert_eq!(
            rec["plan_source"],
            json!({"board": BOARD, "ticket": EPIC, "revision": REVISION})
        );
    }
}

#[test]
fn the_largest_ticket_and_revision_are_read_and_recorded() {
    // lease-api.md, Epic and The board read: a ticket up to 9007199254740991, a revision from 1 to
    // 9007199254740991 written as digits alone.
    for revision in [1, MAX_SAFE] {
        let rig = Rig::with_cards(
            vec![(
                MAX_SAFE,
                CardFx::good(revision, &plan_description(&plan_a())),
            )],
            None,
        );
        let ran = rig.create(&["--board", BOARD, "--epic", "9007199254740991"]);
        ran.lease_doc();
        assert_log(&rig.standin, &[ticket_target(BOARD, MAX_SAFE)]);
        assert_eq!(
            rig.lab.records()[0]["plan_source"],
            json!({"board": BOARD, "ticket": MAX_SAFE, "revision": revision})
        );
    }
}

// ---- The built program -----------------------------------------------------------------------------

/// A throwaway home and a working folder, under the temp folder, for runs of the built `agentspace`,
/// which reads the real DPAPI, the real network and its environment as a process does. Every
/// variable it reads is passed through `Command::env`, and `TMP` and `TEMP` are kept.
struct Bin {
    _tmp: tempfile::TempDir,
    home: PathBuf,
}

impl Bin {
    fn new() -> Bin {
        let tmp = tempfile::Builder::new()
            .prefix("aslb-")
            .tempdir()
            .expect("A throwaway folder can be made");
        assert!(tmp.path().starts_with(std::env::temp_dir()));
        let home = tmp.path().join("home");
        std::fs::create_dir_all(home.join("AppData").join("Local")).unwrap();
        std::fs::create_dir_all(home.join("AppData").join("Roaming")).unwrap();
        Bin { _tmp: tmp, home }
    }

    fn appdata(&self) -> PathBuf {
        self.home.join("AppData").join("Local")
    }

    fn command(&self, line: &[&str]) -> Command {
        let mut cmd = Command::new(common::BIN);
        cmd.args(line)
            .current_dir(&self.home)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("USERPROFILE", &self.home)
            .env("HOME", &self.home)
            .env("LOCALAPPDATA", self.appdata())
            .env("APPDATA", self.home.join("AppData").join("Roaming"))
            .env(
                "AGENTSPACE_PROVISION_CMD",
                self.home.join("no-such-run.cmd"),
            )
            .env("AGENTSPACE_PROXY_COMMAND", "no-such-forwarder --stdio");
        for var in [
            "AGENTSPACE_STATE_DIR",
            "AGENTSPACE_SSH_DIR",
            "AGENTSPACE_OPERATOR_CMD",
            "AGENTSPACE_CHECK_SEAM",
        ] {
            cmd.env_remove(var);
        }
        cmd
    }

    fn run(&self, line: &[&str]) -> Run {
        run_to_end(self.command(line))
    }

    /// Writes a receipt, and a `bot.json` for the stand-in's token, in this home.
    fn credential_for(&self, standin: &Standin) {
        let creds = self.appdata().join("localswim-v2").join("credentials");
        write_text(
            &self.appdata().join("localswim-v2").join("service.json"),
            &receipt_text(&standin.base(), &creds),
        );
        write_text(&creds.join("bot.json"), &bot_text_for(standin.token()));
    }
}

/// Runs a command to its end, and fails the test when it takes more than `limit`, ending it by its
/// own handle.
fn run_within(mut cmd: Command, limit: Duration) -> Run {
    let mut child = cmd.spawn().expect("The binary starts");
    let began = Instant::now();
    loop {
        match child.try_wait().expect("The child can be polled") {
            Some(_) => break,
            None if began.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("The binary did not end within {limit:?}");
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let out = child.wait_with_output().expect("The output can be read");
    Run {
        status: out.status.code(),
        stdout: out.stdout,
        stderr: out.stderr,
    }
}

fn run_to_end(cmd: Command) -> Run {
    run_within(cmd, Duration::from_secs(60))
}

// ---- Usage errors, and no board read for any of them ------------------------------------------------

/// One command line after `lease create`, and the usage error it MUST end in.
struct Usage {
    line: Vec<&'static str>,
    field: Option<&'static str>,
    message: String,
}

fn usage(line: &[&'static str], field: Option<&'static str>, message: impl Into<String>) -> Usage {
    Usage {
        line: line.to_vec(),
        field,
        message: message.into(),
    }
}

/// lease-api.md, Command lines ("Which of create's pairs and partners is reported", rules 1 to 7,
/// each worked example there) and Exit codes and errors (the thirteen cases that carry `field`).
fn usage_cases() -> Vec<Usage> {
    const P: [&str; 2] = ["--project", "x"];
    let line = |parts: &[&'static str]| -> Vec<&'static str> {
        let mut v = P.to_vec();
        v.extend_from_slice(parts);
        v
    };
    let u = |parts: &[&'static str], field: Option<&'static str>, m: String| Usage {
        line: line(parts),
        field,
        message: m,
    };
    let bp = || msg_pair("--board", "--plan");
    let bs = || msg_pair("--board", "--size");
    let ep = || msg_pair("--epic", "--plan");
    let es = || msg_pair("--epic", "--size");
    let ps = || msg_pair("--plan", "--size");
    vec![
        // Missing partners (rule 6), and the field of each.
        u(
            &["--board", BOARD],
            Some("epic"),
            MSG_BOARD_NEEDS_EPIC.into(),
        ),
        u(&["--epic", "7"], Some("board"), MSG_EPIC_NEEDS_BOARD.into()),
        u(&["--turn", "T01"], Some("turn"), MSG_TURN_NEEDS_PLAN.into()),
        u(
            &["--board", BOARD, "--turn", "T01"],
            Some("epic"),
            MSG_BOARD_NEEDS_EPIC.into(),
        ),
        u(
            &["--epic", "7", "--turn", "T01"],
            Some("board"),
            MSG_EPIC_NEEDS_BOARD.into(),
        ),
        u(
            &["--turn", "T01", "--epic", "7"],
            Some("board"),
            MSG_EPIC_NEEDS_BOARD.into(),
        ),
        u(
            &["--BOARD", BOARD],
            Some("epic"),
            MSG_BOARD_NEEDS_EPIC.into(),
        ),
        u(&["--Epic=7"], Some("board"), MSG_EPIC_NEEDS_BOARD.into()),
        // A missing --project is reported before a missing partner.
        usage(&["--board", BOARD], Some("project"), MSG_NO_PROJECT),
        usage(&["--epic", "7"], Some("project"), MSG_NO_PROJECT),
        usage(&["--turn", "T01"], Some("project"), MSG_NO_PROJECT),
        usage(
            &["--board", BOARD, "--epic", "7"],
            Some("project"),
            MSG_NO_PROJECT,
        ),
        // Pairs, each in both orders, with the board-side flag alone and with both.
        u(&["--board", BOARD, "--plan", "p.json"], Some("board"), bp()),
        u(&["--plan", "p.json", "--board", BOARD], Some("board"), bp()),
        u(&["--board", BOARD, "--size", "m"], Some("board"), bs()),
        u(&["--size", "m", "--board", BOARD], Some("board"), bs()),
        u(&["--epic", "7", "--plan", "p.json"], Some("epic"), ep()),
        u(&["--plan", "p.json", "--epic", "7"], Some("epic"), ep()),
        u(&["--epic", "7", "--size", "m"], Some("epic"), es()),
        u(&["--size", "m", "--epic", "7"], Some("epic"), es()),
        u(
            &[
                "--board=0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f",
                "--plan=p.json",
            ],
            Some("board"),
            bp(),
        ),
        // Rule 3: two pairs end at one flag; the pair whose earlier flag stands first on the line.
        u(
            &["--board", BOARD, "--epic", "7", "--plan", "p.json"],
            Some("board"),
            bp(),
        ),
        u(
            &["--epic", "7", "--board", BOARD, "--plan", "p.json"],
            Some("board"),
            ep(),
        ),
        u(
            &["--board", BOARD, "--epic", "7", "--size", "m"],
            Some("board"),
            bs(),
        ),
        u(
            &["--epic", "7", "--board", BOARD, "--size", "m"],
            Some("board"),
            es(),
        ),
        // Rule 4: with several, the first pair in line order.
        u(
            &["--plan", "p.json", "--board", BOARD, "--size", "m"],
            Some("board"),
            bp(),
        ),
        u(
            &["--plan", "p.json", "--size", "m", "--board", BOARD],
            Some("plan"),
            ps(),
        ),
        u(
            &["--size", "m", "--plan", "p.json", "--board", BOARD],
            Some("plan"),
            ps(),
        ),
        u(
            &[
                "--board", BOARD, "--epic", "7", "--plan", "p.json", "--size", "m",
            ],
            Some("board"),
            bp(),
        ),
        // Rule 5: the field is the board-side flag, never the later one.
        u(
            &["--epic", "7", "--plan", "p.json", "--board", BOARD],
            Some("board"),
            ep(),
        ),
        u(
            &["--plan", "p.json", "--epic", "7", "--board", BOARD],
            Some("board"),
            ep(),
        ),
        u(
            &["--size", "m", "--epic", "7", "--board", BOARD],
            Some("board"),
            es(),
        ),
        u(
            &["--size", "m", "--board", BOARD, "--epic", "7"],
            Some("board"),
            bs(),
        ),
        // Rule 2: the later flag of a pair with no value or an empty one is reported as that.
        u(
            &["--size", "m", "--board"],
            Some("board"),
            msg_needs_value("board"),
        ),
        u(
            &["--board", BOARD, "--epic", "7", "--plan"],
            Some("plan"),
            msg_needs_value("plan"),
        ),
        u(
            &["--plan", "p.json", "--board", ""],
            Some("board"),
            msg_empty_value("board"),
        ),
        u(
            &["--size", "m", "--epic", ""],
            Some("epic"),
            msg_empty_value("epic"),
        ),
        u(
            &["--board", "--epic", "7"],
            Some("board"),
            msg_needs_value("board"),
        ),
        // A flag with no value, an empty value, or given twice, for each new flag.
        u(&["--board"], Some("board"), msg_needs_value("board")),
        u(&["--epic"], Some("epic"), msg_needs_value("epic")),
        u(&["--turn"], Some("turn"), msg_needs_value("turn")),
        u(
            &["--epic", "7", "--board", BOARD, "--turn"],
            Some("turn"),
            msg_needs_value("turn"),
        ),
        u(
            &["--plan", "p.json", "--turn", ""],
            Some("turn"),
            msg_empty_value("turn"),
        ),
        u(&["--epic="], Some("epic"), msg_empty_value("epic")),
        u(
            &["--board", BOARD, "--board", BOARD, "--epic", "7"],
            Some("board"),
            msg_twice("board"),
        ),
        u(
            &["--epic", "7", "--epic", "7", "--board", BOARD],
            Some("epic"),
            msg_twice("epic"),
        ),
        u(
            &["--plan", "p.json", "--turn", "T1", "--TURN=T1"],
            Some("turn"),
            msg_twice("turn"),
        ),
        // Other usage errors keep their place in line order.
        u(
            &["--bogus", "--board", BOARD, "--plan", "p.json"],
            None,
            common::msg_unknown_option_sub("--bogus", "create"),
        ),
        u(
            &["--board", BOARD, "--plan", "p.json", "--bogus"],
            Some("board"),
            bp(),
        ),
        u(
            &["--board", BOARD, "--epic", "7", "extra"],
            None,
            common::msg_unexpected("extra", "create"),
        ),
    ]
}

#[track_caller]
fn check_usage_in_process(rig: &Rig, cases: &[Usage]) {
    let mut problems = Vec::new();
    for c in cases {
        let mut line = vec!["lease", "create"];
        line.extend_from_slice(&c.line);
        let ran = rig.lab.run(&line);
        rig.guard(&ran);
        for p in error_problems(&rig.lab.root, &ran, "usage", c.field, &c.message) {
            problems.push(format!("{:?}: {p}", c.line));
        }
    }
    assert_no_problems(problems);
}

#[test]
fn every_usage_pair_and_missing_partner_is_reported_with_its_field_and_reads_no_board() {
    // lease-api.md, Command lines and Exit codes and errors. A stand-in is listening, and its log
    // MUST be empty after every line: a usage error reads no board.
    let rig = Rig::new();
    check_usage_in_process(&rig, &usage_cases());
    assert_no_request(&rig.standin);
    assert_eq!(rig.nothing_made(), Vec::<String>::new());
}

#[test]
fn the_built_program_reports_every_usage_pair_and_missing_partner_alike_and_reads_no_board() {
    let standin = Standin::start(
        Fixture {
            token: new_token(),
            boards: BTreeMap::new(),
        },
        None,
    );
    let bin = Bin::new();
    bin.credential_for(&standin);
    let mut problems = Vec::new();
    for c in usage_cases() {
        let mut line = vec!["lease", "create"];
        line.extend_from_slice(&c.line);
        let run = bin.run(&line);
        for p in failure_problems(&run, "usage", c.field, Some(&c.message)) {
            problems.push(format!("{:?}: {p}", c.line));
        }
    }
    assert_no_problems(problems);
    assert_no_request(&standin);
}

#[test]
fn help_anywhere_on_a_board_line_prints_create_help_and_reads_no_board() {
    // lease-api.md, Command lines, "Which error is reported", 1: help wins over everything, an
    // earlier usage error included, and "Nothing else on the line is checked or done".
    let rig = Rig::new();
    for line in [
        vec!["--board", BOARD, "--epic", "7", "--help"],
        vec!["--board", BOARD, "--plan", "p.json", "-h"],
        vec!["--turn", "--help"],
        vec!["--epic", "7", "--turn", "-H"],
    ] {
        let mut full = vec!["lease", "create"];
        full.extend_from_slice(&line);
        let ran = rig.lab.run(&full);
        let p = common::help_problems(&as_run(&ran), "agentspace lease create");
        assert!(p.is_empty(), "{line:?}: {p:?}");
    }
    assert_no_request(&rig.standin);
    assert_eq!(rig.nothing_made(), Vec::<String>::new());
}

// ---- The forms of --board, --epic and --turn -----------------------------------------------------------

#[test]
fn a_board_outside_its_form_is_refused_before_any_request() {
    // lease-api.md, Board: a UUIDv4 alone, in any case, with surrounding whitespace.
    let rig = Rig::new();
    let cases: [(&str, &str); 14] = [
        ("nope", "nope"),
        ("swimcoach", "swimcoach"),
        (
            "0a1b2c3d-0a1b-1c2d-8e3f-0a1b2c3d4e5f",
            "0a1b2c3d-0a1b-1c2d-8e3f-0a1b2c3d4e5f",
        ),
        (
            "0a1b2c3d-0a1b-5c2d-8e3f-0a1b2c3d4e5f",
            "0a1b2c3d-0a1b-5c2d-8e3f-0a1b2c3d4e5f",
        ),
        (
            "0a1b2c3d-0a1b-4c2d-ce3f-0a1b2c3d4e5f",
            "0a1b2c3d-0a1b-4c2d-ce3f-0a1b2c3d4e5f",
        ),
        (
            "{0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f}",
            "{0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f}",
        ),
        (
            "urn:uuid:0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f",
            "urn:uuid:0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f",
        ),
        (
            "0a1b2c3d0a1b4c2d8e3f0a1b2c3d4e5f",
            "0a1b2c3d0a1b4c2d8e3f0a1b2c3d4e5f",
        ),
        (
            "0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f0",
            "0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f0",
        ),
        (
            "0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5",
            "0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5",
        ),
        (
            "\u{feff}0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f",
            "\u{feff}0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f",
        ),
        ("  ", "  "),
        ("a\nb", "a\\u000ab"),
        (
            "0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5g",
            "0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5g",
        ),
    ];
    let mut problems = Vec::new();
    for (value, quoted) in cases {
        let ran = rig.create(&["--board", value, "--epic", "52"]);
        for p in error_problems(
            &rig.lab.root,
            &ran,
            "invalid_value",
            Some("board"),
            &msg_bad_board(quoted),
        ) {
            problems.push(format!("{quoted:?}: {p}"));
        }
    }
    assert_no_problems(problems);
    assert_no_request(&rig.standin);
    assert_eq!(rig.nothing_made(), Vec::<String>::new());
}

#[test]
fn an_epic_outside_its_form_is_refused_before_any_request() {
    // lease-api.md, Epic: 1 to 16 ASCII digits, value 1 to 9007199254740991, no `#`, sign, fraction
    // or exponent; every refusal is invalid_value for epic, `out_of_range` stays the TTL's alone.
    let rig = Rig::new();
    let cases: [(&str, &str); 19] = [
        ("#0052", "#0052"),
        ("52.0", "52.0"),
        ("0", "0"),
        ("0000", "0000"),
        ("-5", "-5"),
        ("+5", "+5"),
        ("5e1", "5e1"),
        ("0x34", "0x34"),
        ("52a", "52a"),
        ("a52", "a52"),
        ("12345678901234567", "12345678901234567"),
        ("00000000000000052", "00000000000000052"),
        ("9007199254740992", "9007199254740992"),
        ("99999999999999999999", "99999999999999999999"),
        (" ", " "),
        ("\u{ff15}\u{ff12}", "\u{ff15}\u{ff12}"),
        ("5 2", "5 2"),
        ("5\n2", "5\\u000a2"),
        ("\u{feff}52", "\u{feff}52"),
    ];
    let mut problems = Vec::new();
    for (value, quoted) in cases {
        let ran = rig.create(&["--board", BOARD, "--epic", value]);
        for p in error_problems(
            &rig.lab.root,
            &ran,
            "invalid_value",
            Some("epic"),
            &msg_bad_epic(quoted),
        ) {
            problems.push(format!("{quoted:?}: {p}"));
        }
    }
    assert_no_problems(problems);
    assert_no_request(&rig.standin);
    assert_eq!(rig.nothing_made(), Vec::<String>::new());
}

#[test]
fn a_turn_outside_its_form_is_refused_before_any_plan_is_matched_or_board_read() {
    // lease-api.md, Turn: not trimmed; whitespace at either end, longer than 64, a character outside
    // printable ASCII, U+FEFF and a value of only whitespace are outside the form.
    let rig = Rig::new();
    let file = rig.plan_file(&plan_a());
    let too_long = "a".repeat(65);
    let cases: [(&str, String); 10] = [
        (" T01", " T01".into()),
        ("T01 ", "T01 ".into()),
        ("\tT01", "\\u0009T01".into()),
        ("T\u{e9}", "T\u{e9}".into()),
        ("\u{feff}T01", "\u{feff}T01".into()),
        (&too_long, too_long.clone()),
        ("   ", "   ".into()),
        ("T\n1", "T\\u000a1".into()),
        ("T\u{2028}", "T\u{2028}".into()),
        ("\u{7f}", "\\u007f".into()),
    ];
    let mut problems = Vec::new();
    for (value, quoted) in &cases {
        let from_board = rig.board_create(&["--turn", value]);
        let from_file = rig.create(&["--plan", &file, "--turn", value]);
        for (src, ran) in [("board", from_board), ("file", from_file)] {
            for p in error_problems(
                &rig.lab.root,
                &ran,
                "invalid_value",
                Some("turn"),
                &msg_bad_turn(quoted),
            ) {
                problems.push(format!("{quoted:?} ({src}): {p}"));
            }
        }
    }
    assert_no_problems(problems);
    assert_no_request(&rig.standin);
    assert_eq!(rig.nothing_made(), Vec::<String>::new());
}

#[test]
fn a_turn_is_compared_exactly_and_never_trimmed_or_folded() {
    // lease-api.md, Turn: `T01` and `t01` are two names; a name may hold a space inside it, and may
    // be 64 characters.
    let long = "x".repeat(64);
    let plan = json!({"turns": [
        turn("T01", None, vec![wide("S", 1)]),
        turn("my turn", None, vec![wide("S", 2)]),
        turn(&long, None, vec![wide("S", 3)])
    ]});
    let rig = Rig::with_plan(&plan);
    for (name, agents) in [("T01", 2), ("my turn", 3), (long.as_str(), 4)] {
        let doc = rig.board_create(&["--turn", name]).lease_doc();
        assert_eq!(doc["sized_for"]["turn"], name);
        assert_eq!(doc["sized_for"]["agents"], agents);
    }
    let ran = rig.board_create(&["--turn", "t01"]);
    assert_no_problems(error_problems(
        &rig.lab.root,
        &ran,
        "invalid_value",
        Some("turn"),
        &msg_no_such_turn("t01", &["T01", "my turn", &long]),
    ));
}

// ---- The order in which a command line is checked --------------------------------------------------------

#[test]
fn a_bad_ttl_or_size_reads_no_board() {
    // lease-api.md, Exit codes and errors, the worked example: `create --project x --epic 7 --board B
    // --ttl 8` is invalid_value for ttl, and reads no board. A TTL out of range is out_of_range, and
    // a --size beside --board or --epic is a usage error.
    let rig = Rig::new();
    let cases: [(&[&str], &str, &str, String); 5] = [
        (
            &["--ttl", "8"],
            "invalid_value",
            "ttl",
            lab::msg_ttl_no_unit("8"),
        ),
        (
            &["--ttl", "48h"],
            "out_of_range",
            "ttl",
            lab::msg_ttl_range("48h"),
        ),
        (
            &["--ttl", "8M"],
            "invalid_value",
            "ttl",
            lab::msg_ttl_m("8M"),
        ),
        (
            &["--ttl", "soon"],
            "invalid_value",
            "ttl",
            lab::msg_ttl_not_duration("soon"),
        ),
        (
            &["--turn", "T01", "--ttl", "0m"],
            "out_of_range",
            "ttl",
            lab::msg_ttl_range("0m"),
        ),
    ];
    let mut problems = Vec::new();
    for (more, code, field, message) in cases {
        let ran = rig.board_create(more);
        for p in error_problems(&rig.lab.root, &ran, code, Some(field), &message) {
            problems.push(format!("{more:?}: {p}"));
        }
    }
    // The spec's own line, in its own order.
    let ran = rig.create(&["--epic", "7", "--board", BOARD, "--ttl", "8"]);
    for p in error_problems(
        &rig.lab.root,
        &ran,
        "invalid_value",
        Some("ttl"),
        &lab::msg_ttl_no_unit("8"),
    ) {
        problems.push(format!("the spec's line: {p}"));
    }
    for (more, field, message) in [
        (vec!["--size", "??"], "board", msg_pair("--board", "--size")),
        (vec!["--size", "m"], "board", msg_pair("--board", "--size")),
        (
            vec!["--size", "xxl"],
            "board",
            msg_pair("--board", "--size"),
        ),
    ] {
        let ran = rig.board_create(&more);
        for p in error_problems(&rig.lab.root, &ran, "usage", Some(field), &message) {
            problems.push(format!("{more:?}: {p}"));
        }
    }
    assert_no_problems(problems);
    assert_no_request(&rig.standin);
    assert_eq!(rig.nothing_made(), Vec::<String>::new());
}

#[test]
fn project_platform_and_profile_are_checked_before_the_board_forms_and_the_board_read() {
    // lease-api.md, Exit codes and errors, step 2: project, platform, profile, plan, board, epic,
    // turn, size, ttl, then the board read, then the turn against the plan.
    let rig = Rig::new();
    let mut problems = Vec::new();
    let mut check = |what: &str, line: &[&str], code: &str, field: &str, message: String| {
        let mut full = vec!["lease", "create"];
        full.extend_from_slice(line);
        let ran = rig.lab.run(&full);
        rig.guard(&ran);
        for p in error_problems(&rig.lab.root, &ran, code, Some(field), &message) {
            problems.push(format!("{what}: {p}"));
        }
    };
    check(
        "project before a bad board",
        &["--project", "-bad-", "--board", "nope", "--epic", "x"],
        "invalid_value",
        "project",
        lab::msg_bad_project("-bad-"),
    );
    check(
        "platform before a bad board",
        &[
            "--project",
            "x",
            "--platform",
            "??",
            "--board",
            "nope",
            "--epic",
            "52",
        ],
        "invalid_value",
        "platform",
        lab::msg_bad_platform("??"),
    );
    check(
        "profile before a bad epic",
        &[
            "--project",
            "x",
            "--profile",
            "-x",
            "--board",
            BOARD,
            "--epic",
            "x",
        ],
        "invalid_value",
        "profile",
        lab::msg_bad_profile("-x"),
    );
    check(
        "a good board line with a bad project reads no board",
        &["--project", "-bad-", "--board", BOARD, "--epic", "52"],
        "invalid_value",
        "project",
        lab::msg_bad_project("-bad-"),
    );
    check(
        "board before epic",
        &[
            "--project",
            "x",
            "--board",
            "nope",
            "--epic",
            "x",
            "--turn",
            " t",
        ],
        "invalid_value",
        "board",
        msg_bad_board("nope"),
    );
    check(
        "epic before turn",
        &[
            "--project",
            "x",
            "--board",
            BOARD,
            "--epic",
            "x",
            "--turn",
            " t",
        ],
        "invalid_value",
        "epic",
        msg_bad_epic("x"),
    );
    check(
        "turn before ttl",
        &[
            "--project",
            "x",
            "--board",
            BOARD,
            "--epic",
            "52",
            "--turn",
            " t",
            "--ttl",
            "8",
        ],
        "invalid_value",
        "turn",
        msg_bad_turn(" t"),
    );
    check(
        "board before ttl",
        &[
            "--project",
            "x",
            "--board",
            "nope",
            "--epic",
            "52",
            "--ttl",
            "8",
        ],
        "invalid_value",
        "board",
        msg_bad_board("nope"),
    );
    drop(check);
    assert_no_problems(problems);
    assert_no_request(&rig.standin);
    assert_eq!(rig.nothing_made(), Vec::<String>::new());
}

#[test]
fn a_plan_file_is_read_before_the_turn_form_and_after_the_project_platform_and_profile() {
    // lease-api.md, `create`: the flags stand in the order their values are checked: project,
    // platform, profile, plan, board, epic, turn, size and TTL.
    let rig = Rig::new();
    let mut problems = Vec::new();
    for (what, line, code, field, message) in [
        (
            "a missing plan before a bad turn form",
            vec!["--project", "x", "--plan", "missing.json", "--turn", " t"],
            "invalid_value",
            "plan",
            msg_plan_file("missing.json", "the file could not be read"),
        ),
        (
            "a bad platform before a missing plan",
            vec![
                "--project",
                "x",
                "--platform",
                "??",
                "--plan",
                "missing.json",
            ],
            "invalid_value",
            "platform",
            lab::msg_bad_platform("??"),
        ),
        (
            "a bad turn form before a bad ttl",
            vec![
                "--project",
                "x",
                "--plan",
                "p.json",
                "--turn",
                " t",
                "--ttl",
                "8",
            ],
            "invalid_value",
            "turn",
            msg_bad_turn(" t"),
        ),
    ] {
        rig.plan_file(&plan_a());
        let mut full = vec!["lease", "create"];
        full.extend_from_slice(&line);
        let ran = rig.lab.run(&full);
        for p in error_problems(&rig.lab.root, &ran, code, Some(field), &message) {
            problems.push(format!("{what}: {p}"));
        }
    }
    assert_no_problems(problems);
    assert_no_request(&rig.standin);
}

#[test]
fn a_turn_the_plan_lacks_is_reported_only_after_the_board_read() {
    // lease-api.md, Turn: "refused only after the plan is read, from either source, and after every
    // other value is checked. Its message names the plan's turns". The board is read first, so a
    // refusal of the board read comes before it, and a bad TTL before the read.
    let all = ["T01", "T02", "T03", "T04", "T05", "T06"];
    let rig = Rig::new();
    let ran = rig.board_create(&["--turn", "T99"]);
    assert_no_problems(error_problems(
        &rig.lab.root,
        &ran,
        "invalid_value",
        Some("turn"),
        &msg_no_such_turn("T99", &all),
    ));
    assert_log(&rig.standin, &[ticket_target(BOARD, EPIC)]);
    assert_eq!(rig.nothing_made(), Vec::<String>::new());
    // From a plan file, the same.
    let file = rig.plan_file(&plan_a());
    let ran = rig.create(&["--plan", &file, "--turn", "T99"]);
    assert_no_problems(error_problems(
        &rig.lab.root,
        &ran,
        "invalid_value",
        Some("turn"),
        &msg_no_such_turn("T99", &all),
    ));
    // A bad ttl is reported before the read, so the board is read no more.
    let before = rig.standin.count();
    let ran = rig.board_create(&["--turn", "T99", "--ttl", "8"]);
    assert_no_problems(error_problems(
        &rig.lab.root,
        &ran,
        "invalid_value",
        Some("ttl"),
        &lab::msg_ttl_no_unit("8"),
    ));
    assert_eq!(rig.standin.count(), before, "A bad TTL reads no board");
    // An epic the board has no card of is reported before the turn is matched.
    let ran = rig.create(&["--board", BOARD, "--epic", "99", "--turn", "T99"]);
    assert_no_problems(error_problems(
        &rig.lab.root,
        &ran,
        "invalid_value",
        Some("epic"),
        &msg_no_card("99", BOARD),
    ));
    // The turn is matched against the plan as read: a turn of another case is no turn.
    let ran = rig.board_create(&["--turn", "t01"]);
    assert_no_problems(error_problems(
        &rig.lab.root,
        &ran,
        "invalid_value",
        Some("turn"),
        &msg_no_such_turn("t01", &all),
    ));
}

// ---- Finding the board: the receipt ----------------------------------------------------------------------

/// A problem-collecting check of one run against an `internal` error with this message.
fn internal_problems(rig: &Rig, ran: &Ran, message: &str) -> Vec<String> {
    error_problems(&rig.lab.root, ran, "internal", None, message)
}

/// The receipt's variables that LOCALAPPDATA is unset, empty or only whitespace, run as the built
/// program with the variable given by `Command::env`, so that the run never reads the caller's own.
/// A receipt and a credential are in the home's `AppData\Local`, where a build that fell back to the
/// profile would find them, and the stand-in's log MUST stay empty.
#[test]
fn localappdata_unset_empty_or_only_whitespace_is_an_internal_error_and_reads_no_board() {
    let standin = Standin::start(
        Fixture {
            token: new_token(),
            boards: BTreeMap::from([(
                BOARD.to_string(),
                BTreeMap::from([(EPIC, CardFx::good(REVISION, &plan_description(&plan_a())))]),
            )]),
        },
        None,
    );
    let bin = Bin::new();
    bin.credential_for(&standin);
    let mut problems = Vec::new();
    for (what, value) in [
        ("unset", None),
        ("empty", Some("")),
        ("a space", Some(" ")),
        ("spaces", Some("   ")),
        ("a tab", Some("\t")),
        ("mixed", Some(" \t ")),
    ] {
        let mut cmd = bin.command(&[
            "lease",
            "create",
            "--project",
            "swim",
            "--board",
            BOARD,
            "--epic",
            "52",
        ]);
        match value {
            None => cmd.env_remove("LOCALAPPDATA"),
            Some(v) => cmd.env("LOCALAPPDATA", v),
        };
        let run = run_to_end(cmd);
        for p in failure_problems(&run, "internal", None, Some(MSG_NO_LOCALAPPDATA)) {
            problems.push(format!("{what}: {p}"));
        }
    }
    assert_no_problems(problems);
    assert_no_request(&standin);
}

#[test]
fn no_receipt_is_an_internal_error_and_reads_no_board() {
    // lease-api.md, The board read: "No file at the receipt's path". The receipt is
    // <LOCALAPPDATA>\localswim-v2\service.json and nowhere else, and no variable of agentspace's
    // moves it.
    let mut problems = Vec::new();
    for what in [
        "no file",
        "no folder",
        "the receipt one folder up",
        "a backup beside it only",
        "an AGENTSPACE variable naming a receipt",
    ] {
        let rig = Rig::new();
        let (dir, path) = (rig.appdata.join("localswim-v2"), rig.receipt_path());
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        match what {
            "no folder" => std::fs::remove_dir_all(&dir).unwrap(),
            "the receipt one folder up" => {
                std::fs::write(rig.appdata.join("service.json"), &text).unwrap();
            }
            "a backup beside it only" => {
                std::fs::write(dir.join("service.json.bak-1"), &text).unwrap();
            }
            _ => {}
        }
        let mut rig = rig;
        if what.starts_with("an AGENTSPACE") {
            std::fs::write(rig.lab.root.join("elsewhere.json"), &text).unwrap();
            for var in [
                "AGENTSPACE_RECEIPT",
                "AGENTSPACE_LOCALSWIM_RECEIPT",
                "AGENTSPACE_BOARD_RECEIPT",
            ] {
                rig.lab
                    .set_var(var, rig.lab.root.join("elsewhere.json").into_os_string());
            }
        }
        let ran = rig.board_create(&[]);
        for p in internal_problems(&rig, &ran, MSG_NO_RECEIPT) {
            problems.push(format!("{what}: {p}"));
        }
        if rig.standin.count() != 0 {
            problems.push(format!("{what}: the board was read"));
        }
        problems.extend(
            rig.nothing_made()
                .into_iter()
                .map(|p| format!("{what}: {p}")),
        );
    }
    assert_no_problems(problems);
}

#[test]
fn a_folder_at_the_receipts_path_is_refused_as_an_internal_error_naming_no_path() {
    // lease-api.md, The board read, the table: a folder at the receipt's path is "The receipt
    // cannot be read", never "No file at the receipt's path", which means nothing is there
    // (architect, 2026-11-25, for v0.11-T03, pin P8). The board is not read.
    let rig = Rig::new();
    std::fs::remove_file(rig.receipt_path()).unwrap();
    std::fs::create_dir_all(rig.receipt_path()).unwrap();
    let ran = rig.board_create(&[]);
    let p = internal_problems(&rig, &ran, MSG_BAD_RECEIPT);
    assert!(p.is_empty(), "{p:?}");
    assert_no_request(&rig.standin);
}

#[test]
fn a_receipt_that_breaks_localswims_rules_or_has_no_usable_base_is_refused_and_reads_no_board() {
    // lease-api.md, The board read: one JSON object in UTF-8 (a leading byte order mark accepted),
    // `base` and `credential_directory` each a string and each once, other members ignored; and a
    // base that is exactly http://127.0.0.1:<port>, "character by character". The stand-in listens
    // on the port the bases name, so that a build that accepted one would reach it.
    let rig = Rig::new();
    let p = rig.port();
    let good_base = rig.standin.base();
    let cd = rig.creds.to_string_lossy().into_owned();
    let cd_json = serde_json::to_string(&cd).unwrap();
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
    let text = |s: String| s.into_bytes();
    for (what, t) in [
        ("empty", String::new()),
        ("not JSON", "not json".to_string()),
        ("an array", "[]".to_string()),
        ("null", "null".to_string()),
        ("a number", "42".to_string()),
        ("a string", "\"x\"".to_string()),
        (
            "no base",
            format!(r#"{{"credential_directory":{cd_json}}}"#),
        ),
        (
            "no credential_directory",
            format!(r#"{{"base":"{good_base}"}}"#),
        ),
        (
            "a base that is a number",
            format!(r#"{{"base":5,"credential_directory":{cd_json}}}"#),
        ),
        (
            "a base that is null",
            format!(r#"{{"base":null,"credential_directory":{cd_json}}}"#),
        ),
        (
            "a base that is an array",
            format!(r#"{{"base":["{good_base}"],"credential_directory":{cd_json}}}"#),
        ),
        (
            "a credential_directory that is a number",
            format!(r#"{{"base":"{good_base}","credential_directory":5}}"#),
        ),
        (
            "a credential_directory that is null",
            format!(r#"{{"base":"{good_base}","credential_directory":null}}"#),
        ),
        (
            "credential_directory twice",
            format!(
                r#"{{"base":"{good_base}","credential_directory":{cd_json},"credential_directory":{cd_json}}}"#
            ),
        ),
        (
            "base twice",
            format!(
                r#"{{"base":"{good_base}","base":"{good_base}","credential_directory":{cd_json}}}"#
            ),
        ),
        (
            "a member named as base in another case",
            format!(
                r#"{{"base":"{good_base}","BASE":"{good_base}","credential_directory":{cd_json}}}"#
            ),
        ),
        (
            "two documents",
            format!(
                r#"{{"base":"{good_base}","credential_directory":{cd_json}}}{{"base":"{good_base}","credential_directory":{cd_json}}}"#
            ),
        ),
        (
            "text after the document",
            format!(r#"{{"base":"{good_base}","credential_directory":{cd_json}}} x"#),
        ),
    ] {
        cases.push((what.to_string(), text(t)));
    }
    // Encodings other than UTF-8.
    let good = rig.receipt_with_base(&good_base);
    let mut utf16 = vec![0xFF, 0xFE];
    utf16.extend(utf16le(&good));
    cases.push(("UTF-16LE with a mark".into(), utf16));
    cases.push(("UTF-16LE with no mark".into(), utf16le(&good)));
    let mut bad_utf8 = good.clone().into_bytes();
    bad_utf8.extend_from_slice(b" \xff");
    cases.push(("not valid UTF-8".into(), bad_utf8));
    // Bases.
    let bases: Vec<(&str, String)> = vec![
        ("localhost", format!("http://localhost:{p}")),
        ("https", format!("https://127.0.0.1:{p}")),
        ("an upper-case scheme", format!("HTTP://127.0.0.1:{p}")),
        ("127.0.0.2", format!("http://127.0.0.2:{p}")),
        ("an IPv6 loopback", format!("http://[::1]:{p}")),
        ("user information", format!("http://user@127.0.0.1:{p}")),
        ("a trailing slash", format!("http://127.0.0.1:{p}/")),
        ("a path", format!("http://127.0.0.1:{p}/x")),
        ("a query", format!("http://127.0.0.1:{p}?x=1")),
        ("a fragment", format!("http://127.0.0.1:{p}#x")),
        ("port 0", "http://127.0.0.1:0".to_string()),
        ("port 65536", "http://127.0.0.1:65536".to_string()),
        ("a leading zero", format!("http://127.0.0.1:0{p}")),
        ("a letter in the port", format!("http://127.0.0.1:{p}x")),
        ("an empty port", "http://127.0.0.1:".to_string()),
        ("no port", "http://127.0.0.1".to_string()),
        ("a leading space", format!(" http://127.0.0.1:{p}")),
        ("a trailing space", format!("http://127.0.0.1:{p} ")),
        ("a trailing line feed", format!("http://127.0.0.1:{p}\n")),
        ("a trailing dot", format!("http://127.0.0.1.:{p}")),
        ("127.1", format!("http://127.1:{p}")),
        ("a hex octet", format!("http://0x7f.0.0.1:{p}")),
        ("a name", format!("http://board.invalid:{p}")),
        ("no scheme", format!("127.0.0.1:{p}")),
        ("another scheme", format!("ftp://127.0.0.1:{p}")),
        ("one slash", format!("http:/127.0.0.1:{p}")),
        (
            "a port of 5 digits over the range",
            "http://127.0.0.1:99999".to_string(),
        ),
        ("a signed port", format!("http://127.0.0.1:+{p}")),
        ("an empty base", String::new()),
    ];
    for (what, base) in bases {
        cases.push((
            format!("base: {what}"),
            rig.receipt_with_base(&base).into_bytes(),
        ));
    }
    let mut problems = Vec::new();
    for (what, bytes) in &cases {
        std::fs::write(rig.receipt_path(), bytes).unwrap();
        let ran = rig.board_create(&[]);
        for p in internal_problems(&rig, &ran, MSG_BAD_RECEIPT) {
            problems.push(format!("{what}: {p}"));
        }
    }
    assert_no_problems(problems);
    assert_no_request(&rig.standin);
    assert_eq!(rig.nothing_made(), Vec::<String>::new());
}

#[test]
fn a_receipt_that_is_localswims_is_read_whatever_else_it_holds_and_however_it_is_laid_out() {
    // lease-api.md, The board read: "a leading byte order mark accepted ... any other member
    // ignored"; localswim: whitespace, line breaks and member order carry no meaning.
    let rig = Rig::new();
    let base = rig.standin.base();
    let cd_json = serde_json::to_string(&rig.creds.to_string_lossy().into_owned()).unwrap();
    let mut with_mark = vec![0xEF, 0xBB, 0xBF];
    with_mark.extend(rig.receipt_with_base(&base).into_bytes());
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("a byte order mark", with_mark),
        (
            "other members, in another order, spread over lines",
            format!(
                "{{\n  \"zzz\": [1, 2, {{\"base\": 5}}],\n  \"credential_directory\": {cd_json},\n  \"executable\": 5,\n  \"base\": \"{base}\",\n  \"data_directory\": null\n}}\n"
            )
            .into_bytes(),
        ),
        (
            "escapes in a string",
            format!(
                "{{\"base\":\"http:\\/\\/127.0.0.1:{}\",\"credential_directory\":{cd_json}}}",
                rig.port()
            )
            .into_bytes(),
        ),
    ];
    let mut problems = Vec::new();
    for (what, bytes) in cases {
        std::fs::write(rig.receipt_path(), bytes).unwrap();
        let before = rig.standin.count();
        let ran = rig.board_create(&[]);
        if ran.code != 0 || rig.standin.count() != before + 1 {
            problems.push(format!(
                "{what}: exit {}, {} requests, stderr {:?}",
                ran.code,
                rig.standin.count() - before,
                ran.stderr_text()
            ));
        }
    }
    assert_no_problems(problems);
}

#[test]
fn a_relative_localappdata_is_resolved_against_the_working_folder() {
    // lease-api.md, The board read: "A relative LOCALAPPDATA is resolved against the working folder
    // of the command."
    let mut rig = Rig::new();
    let relative = rig.lab.cwd.join("rel").join("local");
    std::fs::create_dir_all(relative.join("localswim-v2")).unwrap();
    std::fs::rename(
        rig.receipt_path(),
        relative.join("localswim-v2").join("service.json"),
    )
    .unwrap();
    rig.lab
        .set_var("LOCALAPPDATA", std::ffi::OsString::from("rel\\local"));
    let ran = rig.board_create(&[]);
    ran.lease_doc();
    assert_log(&rig.standin, &[ticket_target(BOARD, EPIC)]);
}

// ---- Finding the token: bot.json -----------------------------------------------------------------------

#[test]
fn a_credential_that_breaks_localswims_rules_or_cannot_be_decrypted_is_refused_before_any_request()
{
    // lease-api.md, The board read, "The credential": bot.json is one JSON object of exactly the
    // members `format`, `format_version`, `role` and `token_dpapi_hex`, `role` being `bot`, a
    // token of 43 characters of unpadded base64url once decrypted. "A file that breaks them, or
    // that cannot be decrypted, is refused", and the stand-in's log shows the token reached nobody.
    let rig = Rig::new();
    let token = rig.standin.token().to_string();
    let blob = dpapi::protect(&utf16le(&token), None);
    let good_hex = hex(&blob);
    let with = |hex_text: &str| -> String {
        format!(
            r#"{{"format":"localswim-credential","format_version":1,"role":"bot","token_dpapi_hex":{hex_text}}}"#
        )
    };
    let quoted = |s: &str| format!("\"{s}\"");
    let protect_text = |text: &str| bot_text(&dpapi::protect(&utf16le(text), None));
    let mut flipped = blob.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 0xFF;
    let mut with_bom = vec![0xFF, 0xFE];
    with_bom.extend(utf16le(&token));
    let mut with_nul = utf16le(&token);
    with_nul.extend([0, 0]);
    let swapped = token.replacen('-', "+", 1);
    assert_eq!(swapped.len(), 43);
    let cases: Vec<(&str, Option<String>)> = vec![
        ("not JSON", Some("nope".into())),
        ("empty", Some(String::new())),
        ("an array", Some("[]".into())),
        (
            "a fifth member",
            Some(format!(
                r#"{{"format":"localswim-credential","format_version":1,"role":"bot","token_dpapi_hex":"{good_hex}","extra":1}}"#
            )),
        ),
        (
            "no format_version",
            Some(format!(
                r#"{{"format":"localswim-credential","role":"bot","token_dpapi_hex":"{good_hex}"}}"#
            )),
        ),
        (
            "another format",
            Some(with(&quoted(&good_hex)).replace("localswim-credential", "other-credential")),
        ),
        (
            "another case of format",
            Some(with(&quoted(&good_hex)).replace("localswim-credential", "Localswim-Credential")),
        ),
        (
            "format_version 1.0",
            Some(
                with(&quoted(&good_hex)).replace("\"format_version\":1", "\"format_version\":1.0"),
            ),
        ),
        (
            "format_version 1e0",
            Some(
                with(&quoted(&good_hex)).replace("\"format_version\":1", "\"format_version\":1e0"),
            ),
        ),
        (
            "format_version \"1\"",
            Some(
                with(&quoted(&good_hex))
                    .replace("\"format_version\":1", "\"format_version\":\"1\""),
            ),
        ),
        (
            "format_version 2",
            Some(with(&quoted(&good_hex)).replace("\"format_version\":1", "\"format_version\":2")),
        ),
        (
            "role human",
            Some(with(&quoted(&good_hex)).replace("\"bot\"", "\"human\"")),
        ),
        (
            "role Bot",
            Some(with(&quoted(&good_hex)).replace("\"bot\"", "\"Bot\"")),
        ),
        (
            "a member named in another case",
            Some(with(&quoted(&good_hex)).replace("\"role\"", "\"Role\"")),
        ),
        (
            "role twice",
            Some(
                with(&quoted(&good_hex))
                    .replace("\"role\":\"bot\"", "\"role\":\"bot\",\"role\":\"bot\""),
            ),
        ),
        ("odd hex digits", Some(with("\"abc\""))),
        ("non-hex digits", Some(with("\"zz\""))),
        ("an empty hex string", Some(with("\"\""))),
        ("hex that is a number", Some(with("12"))),
        ("hex that is null", Some(with("null"))),
        ("a blob DPAPI refuses", Some(bot_text(&[0u8; 100]))),
        ("a truncated blob", Some(bot_text(&blob[..blob.len() / 2]))),
        ("a corrupted blob", Some(bot_text(&flipped))),
        (
            "a blob made with entropy",
            Some(bot_text(&dpapi::protect(
                &utf16le(&token),
                Some(b"some entropy"),
            ))),
        ),
        ("a token of 42 characters", Some(protect_text(&token[..42]))),
        (
            "a token of 44 characters",
            Some(protect_text(&format!("{token}A"))),
        ),
        ("a token with a plus", Some(protect_text(&swapped))),
        (
            "a token with a space",
            Some(protect_text(&format!("{} ", &token[..42]))),
        ),
        (
            "a token with a line feed",
            Some(protect_text(&format!("{}\n", &token[..42]))),
        ),
        (
            "a token in UTF-8",
            Some(bot_text(&dpapi::protect(token.as_bytes(), None))),
        ),
        (
            "a token in UTF-16LE with a byte order mark",
            Some(bot_text(&dpapi::protect(&with_bom, None))),
        ),
        (
            "a token in UTF-16LE with a terminator",
            Some(bot_text(&dpapi::protect(&with_nul, None))),
        ),
        ("no file", None),
    ];
    let mut problems = Vec::new();
    for (what, text) in cases {
        match &text {
            Some(t) => std::fs::write(rig.bot_path(), t).unwrap(),
            None => {
                let _ = std::fs::remove_file(rig.bot_path());
            }
        }
        let ran = rig.board_create(&[]);
        for p in internal_problems(&rig, &ran, MSG_BAD_CREDENTIAL) {
            problems.push(format!("{what}: {p}"));
        }
    }
    // A bot.json that is a folder, and a credential folder that is not there.
    let _ = std::fs::remove_file(rig.bot_path());
    std::fs::create_dir_all(rig.bot_path()).unwrap();
    let ran = rig.board_create(&[]);
    for p in internal_problems(&rig, &ran, MSG_BAD_CREDENTIAL) {
        problems.push(format!("a folder: {p}"));
    }
    std::fs::remove_dir_all(&rig.creds).unwrap();
    let ran = rig.board_create(&[]);
    for p in internal_problems(&rig, &ran, MSG_BAD_CREDENTIAL) {
        problems.push(format!("no credential folder: {p}"));
    }
    assert_no_problems(problems);
    assert_no_request(&rig.standin);
    assert_eq!(rig.nothing_made(), Vec::<String>::new());
}

#[test]
fn a_credential_that_is_localswims_is_decrypted_and_sent_in_the_authorization_header_alone() {
    // The token is sent as `Authorization: Bearer <token>` and nowhere else (lease-api.md, The
    // board read). The stand-in compares it inside, and answers 401 to any other.
    let rig = Rig::new();
    let token = rig.standin.token().to_string();
    let blob = dpapi::protect(&utf16le(&token), None);
    let upper = hex(&blob).to_uppercase();
    let mut mark = vec![0xEF, 0xBB, 0xBF];
    mark.extend(bot_text(&blob).into_bytes());
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("lowercase hex", bot_text(&blob).into_bytes()),
        (
            "other whitespace and another member order",
            format!(
                "{{\n  \"token_dpapi_hex\": \"{}\",\n  \"role\": \"bot\",\n  \"format_version\": 1,\n  \"format\": \"localswim-credential\"\n}}\n",
                hex(&blob)
            )
            .into_bytes(),
        ),
        (
            "upper-case hex digits",
            bot_text(&blob).replace(&hex(&blob), &upper).into_bytes(),
        ),
        ("a byte order mark", mark),
    ];
    let mut problems = Vec::new();
    for (what, bytes) in cases {
        std::fs::write(rig.bot_path(), bytes).unwrap();
        let before = rig.standin.count();
        let ran = rig.board_create(&[]);
        if ran.code != 0 || rig.standin.count() != before + 1 {
            problems.push(format!(
                "{what}: exit {}, {} requests, stderr {:?}",
                ran.code,
                rig.standin.count() - before,
                ran.stderr_text()
            ));
        }
    }
    assert_no_problems(problems);
    assert_log(
        &rig.standin,
        &vec![ticket_target(BOARD, EPIC); rig.standin.count()],
    );
}

// ---- The board's answers: each outcome --------------------------------------------------------------------

/// What a run of `create --board --epic` MUST end in.
enum Want {
    Refused,
    NoAnswer,
    Unreadable,
    /// An `invalid_value` for `epic` or `board`, with this message.
    Value(&'static str, String),
}

impl Want {
    fn problems(&self, rig: &Rig, ran: &Ran) -> Vec<String> {
        match self {
            Want::Refused => internal_problems(rig, ran, MSG_REFUSED),
            Want::NoAnswer => internal_problems(rig, ran, &msg_no_answer(rig.port())),
            Want::Unreadable => internal_problems(rig, ran, MSG_UNREADABLE_ANSWER),
            Want::Value(field, m) => {
                error_problems(&rig.lab.root, ran, "invalid_value", Some(field), m)
            }
        }
    }
}

/// A body of a ticket view with these three members as given, and everything else a whole
/// `TicketInspection` holds.
fn answer_body(revision: &str, archived: &str, description_json: &str) -> Vec<u8> {
    inspection_text(
        BOARD,
        EPIC,
        &CardFx::raw(revision, archived, description_json),
        true,
    )
    .into_bytes()
}

fn good_description_json() -> String {
    serde_json::to_string(&plan_description(&plan_a())).unwrap()
}

fn good_body() -> Vec<u8> {
    answer_body("7", "false", &good_description_json())
}

fn json200(body: impl Into<Vec<u8>>) -> Reply {
    Resp::new(200)
        .ctype("application/json")
        .body(body.into())
        .reply()
}

#[test]
fn every_status_the_board_answers_is_an_outcome_of_the_table_and_a_refused_one_is_not_retried() {
    // lease-api.md, The board read, "Each outcome": 401 and 403 are "refused agentspace's credential"
    // (not tried again); any other status but 200, a 404 that is not localswim's `not_found`
    // included, is "did not answer"; a 3xx is never followed. A card is read once.
    let decoy = Standin::start(
        Fixture {
            token: new_token(),
            boards: BTreeMap::new(),
        },
        None,
    );
    let location = format!("{}{}", decoy.base(), ticket_target(BOARD, EPIC));
    let mut cases: Vec<(String, Hook, Want)> = Vec::new();
    let mut push = |what: &str, hook: Hook, want: Want| cases.push((what.to_string(), hook, want));
    for s in [401u16, 403] {
        push(
            &format!("{s} with a Problem document"),
            always(problem(s, code_of(s))),
            Want::Refused,
        );
        push(&format!("{s} with no body"), always(bare(s)), Want::Refused);
    }
    for s in [400u16, 405, 406, 409, 410, 422, 429, 500, 502, 503] {
        push(
            &format!("{s} with a Problem document"),
            always(problem(s, code_of(s))),
            Want::NoAnswer,
        );
    }
    push("500 with no body", always(bare(500)), Want::NoAnswer);
    push(
        "200 with an empty body",
        always(Resp::new(200).ctype("application/json").reply()),
        Want::Unreadable,
    );
    for s in [201u16, 202, 204, 206] {
        let mut r = Resp::new(s).ctype("application/json");
        if s != 204 {
            r = r.body(good_body());
        }
        push(
            &format!("{s}, a 2xx that is not 200"),
            always(r.reply()),
            Want::NoAnswer,
        );
    }
    for s in [301u16, 302, 303, 307, 308] {
        push(
            &format!("{s} to another stand-in"),
            always(Resp::new(s).header("Location", &location).reply()),
            Want::NoAnswer,
        );
    }
    push("404 with no body", always(bare(404)), Want::NoAnswer);
    push(
        "404 whose Problem document has another code",
        always(problem(404, "forbidden")),
        Want::NoAnswer,
    );
    push(
        "404 whose code is in another case",
        always(
            Resp::new(404)
                .ctype("application/problem+json")
                .body(problem_text(404, "NOT_FOUND"))
                .reply(),
        ),
        Want::NoAnswer,
    );
    push(
        "404 not_found as application/json",
        always(
            Resp::new(404)
                .ctype("application/json")
                .body(problem_text(404, "not_found"))
                .reply(),
        ),
        Want::NoAnswer,
    );
    push(
        "404 not_found as text/plain",
        always(
            Resp::new(404)
                .ctype("text/plain")
                .body(problem_text(404, "not_found"))
                .reply(),
        ),
        Want::NoAnswer,
    );
    push(
        "404 not_found with no Content-Type",
        always(Resp::new(404).body(problem_text(404, "not_found")).reply()),
        Want::NoAnswer,
    );
    push(
        "404 problem+json that is not JSON",
        always(
            Resp::new(404)
                .ctype("application/problem+json")
                .body("nope")
                .reply(),
        ),
        Want::NoAnswer,
    );
    push(
        "404 problem+json that is an array",
        always(
            Resp::new(404)
                .ctype("application/problem+json")
                .body("[]")
                .reply(),
        ),
        Want::NoAnswer,
    );
    push(
        "404 problem+json with no code",
        always(
            Resp::new(404)
                .ctype("application/problem+json")
                .body(r#"{"type":"about:blank","status":404}"#)
                .reply(),
        ),
        Want::NoAnswer,
    );
    push(
        "404 problem+json whose code is a number",
        always(
            Resp::new(404)
                .ctype("application/problem+json")
                .body(r#"{"code":404}"#)
                .reply(),
        ),
        Want::NoAnswer,
    );
    push(
        "no answer: the connection closed",
        always_close(),
        Want::NoAnswer,
    );
    push(
        "no answer: closed before the whole body",
        always(
            Resp::new(200)
                .ctype("application/json")
                .body(&good_body()[..20])
                .declared(good_body().len() as u64)
                .reply(),
        ),
        Want::NoAnswer,
    );
    let mut problems = Vec::new();
    for (what, hook, want) in cases {
        let rig = Rig::with_hook(hook);
        let ran = rig.board_create(&[]);
        for p in want.problems(&rig, &ran) {
            problems.push(format!("{what}: {p}"));
        }
        if rig.standin.count() != 1 {
            problems.push(format!(
                "{what}: {} requests, and a card is read once",
                rig.standin.count()
            ));
        }
        problems.extend(
            rig.nothing_made()
                .into_iter()
                .map(|p| format!("{what}: {p}")),
        );
    }
    assert_no_problems(problems);
    assert_no_request(&decoy);
}

fn always_close() -> Hook {
    always(Reply::Close)
}

#[test]
fn a_404_that_is_localswims_not_found_then_the_summary_view_tells_a_card_from_a_board() {
    // lease-api.md, The board read: after a `not_found` 404, one more GET of the summary view; a 200
    // there is an unknown ticket, the same 404 an unknown board; any other outcome there is the row
    // of that outcome. The card is read once and at most two GETs are made.
    let mut cases: Vec<(String, Hook, Want)> = Vec::new();
    let mut push = |what: &str, hook: Hook, want: Want| cases.push((what.to_string(), hook, want));
    let no_card = || Want::Value("epic", msg_no_card("52", BOARD));
    push(
        "summary 200",
        not_found_then(json200(r#"{"id":"x"}"#)),
        no_card(),
    );
    push(
        "summary 200 that is not JSON",
        not_found_then(json200("not json")),
        no_card(),
    );
    push(
        "summary 200 with no body",
        not_found_then(Resp::new(200).reply()),
        no_card(),
    );
    push(
        "summary 404 not_found",
        not_found_then(problem(404, "not_found")),
        Want::Value("board", msg_no_board(BOARD)),
    );
    push(
        "summary 401",
        not_found_then(problem(401, "unauthorized")),
        Want::Refused,
    );
    push(
        "summary 403",
        not_found_then(problem(403, "forbidden")),
        Want::Refused,
    );
    push(
        "summary 500",
        not_found_then(problem(500, "internal_error")),
        Want::NoAnswer,
    );
    push(
        "summary 400",
        not_found_then(problem(400, "invalid_request")),
        Want::NoAnswer,
    );
    push(
        "summary 405",
        not_found_then(problem(405, "method_not_allowed")),
        Want::NoAnswer,
    );
    push(
        "summary bare 404",
        not_found_then(bare(404)),
        Want::NoAnswer,
    );
    push(
        "summary 404 with another code",
        not_found_then(problem(404, "forbidden")),
        Want::NoAnswer,
    );
    push(
        "summary 302",
        not_found_then(Resp::new(302).header("Location", "/x").reply()),
        Want::NoAnswer,
    );
    push(
        "summary 201",
        not_found_then(Resp::new(201).body("{}").reply()),
        Want::NoAnswer,
    );
    push(
        "summary connection closed",
        not_found_then(Reply::Close),
        Want::NoAnswer,
    );
    // The media type of the first 404 is compared by type and subtype, without regard to case.
    for ctype in [
        "Application/Problem+JSON; charset=utf-8",
        "APPLICATION/PROBLEM+JSON",
        "application/problem+json;charset=UTF-8",
        "application/problem+json ; charset=utf-8",
    ] {
        push(
            &format!("a 404 as {ctype}"),
            on_ticket(
                Resp::new(404)
                    .ctype(ctype)
                    .body(problem_text(404, "not_found"))
                    .reply(),
            ),
            no_card(),
        );
    }
    let mut problems = Vec::new();
    for (what, hook, want) in cases {
        let rig = Rig::with_hook(hook);
        let ran = rig.board_create(&[]);
        for p in want.problems(&rig, &ran) {
            problems.push(format!("{what}: {p}"));
        }
        for p in log_problems(
            &rig.standin.log(),
            &[ticket_target(BOARD, EPIC), summary_target(BOARD)],
        ) {
            problems.push(format!("{what}: {p}"));
        }
        problems.extend(
            rig.nothing_made()
                .into_iter()
                .map(|p| format!("{what}: {p}")),
        );
    }
    assert_no_problems(problems);
}

#[test]
fn an_unknown_ticket_and_an_unknown_board_are_told_apart_by_a_second_request_and_quote_the_value_as_given()
 {
    // lease-api.md, The board read: `<value>` is the --epic value as given in an `epic` row, the
    // --board value as given in the `board` row, `<board>` the normalized board ID.
    let rig = Rig::new();
    let ran = rig.create(&["--board", BOARD, "--epic", "0099"]);
    assert_no_problems(error_problems(
        &rig.lab.root,
        &ran,
        "invalid_value",
        Some("epic"),
        &msg_no_card("0099", BOARD),
    ));
    assert_log(
        &rig.standin,
        &[ticket_target(BOARD, 99), summary_target(BOARD)],
    );
    let loud = format!(" {} ", OTHER_BOARD.to_uppercase());
    let before = rig.standin.count();
    let ran = rig.create(&["--board", &loud, "--epic", " 52 "]);
    assert_no_problems(error_problems(
        &rig.lab.root,
        &ran,
        "invalid_value",
        Some("board"),
        &msg_no_board(&loud),
    ));
    let log = rig.standin.log();
    assert_eq!(log.len(), before + 2);
    assert_eq!(log[before].target, ticket_target(OTHER_BOARD, 52));
    assert_eq!(log[before + 1].target, summary_target(OTHER_BOARD));
    assert_eq!(rig.nothing_made(), Vec::<String>::new());
    // An archived card quotes the epic as given too.
    let rig = Rig::with_cards(
        vec![(
            EPIC,
            CardFx::archived(REVISION, &plan_description(&plan_a())),
        )],
        None,
    );
    let ran = rig.create(&["--board", BOARD, "--epic", "0052"]);
    assert_no_problems(error_problems(
        &rig.lab.root,
        &ran,
        "invalid_value",
        Some("epic"),
        &msg_archived("0052"),
    ));
    assert_log(&rig.standin, &[ticket_target(BOARD, EPIC)]);
    assert_eq!(rig.nothing_made(), Vec::<String>::new());
}

#[test]
fn a_200_answer_larger_than_64_mib_is_refused_for_the_epic_before_it_is_read() {
    // lease-api.md, The board read, Bounds: "A body that a Content-Length says is larger is refused
    // before it is read". The stand-in declares 67108865 bytes, sends none of them and holds the
    // connection open: a build that read on would wait for them.
    let rig = Rig::with_hook(on_ticket(
        Resp::new(200)
            .ctype("application/json")
            .body("{")
            .declared(67_108_865)
            .hold()
            .reply(),
    ));
    let began = Instant::now();
    let ran = rig.board_create(&[]);
    assert_no_problems(error_problems(
        &rig.lab.root,
        &ran,
        "invalid_value",
        Some("epic"),
        &msg_too_big("52"),
    ));
    assert!(
        began.elapsed() < Duration::from_secs(20),
        "The client waited for a body it was told is larger than the bound"
    );
    assert_log(&rig.standin, &[ticket_target(BOARD, EPIC)]);
    assert_eq!(rig.nothing_made(), Vec::<String>::new());
}

#[test]
fn a_200_answer_that_is_not_the_shape_is_an_answer_that_could_not_be_read() {
    // lease-api.md, The board read, "What is read from a 200 answer": one JSON object whose
    // card.revision (a JSON integer of digits alone, 1 to 9007199254740991), card.archived (a
    // boolean) and prose.description (a string) are all there; `5.0`, `5e0`, `-5`, `0`, a string
    // and a number above the bound are refused; a description with an unpaired surrogate refuses
    // the whole answer, and is never invalid_value for epic.
    let desc = good_description_json();
    let mut bodies: Vec<(String, Vec<u8>)> = Vec::new();
    let mut add = |what: &str, body: Vec<u8>| bodies.push((what.to_string(), body));
    add("an empty body", Vec::new());
    add("not JSON", b"not json".to_vec());
    add("an array", b"[]".to_vec());
    add("a string", b"\"x\"".to_vec());
    add("a number", b"5".to_vec());
    add("null", b"null".to_vec());
    add("an empty object", b"{}".to_vec());
    add("not UTF-8", vec![0xFF, 0xFE, 0xFD]);
    let mut trailing = good_body();
    trailing.extend_from_slice(b" x");
    add("text after the document", trailing);
    let mut two = good_body();
    two.extend(good_body());
    add("two documents", two);
    for rev in [
        "5.0",
        "5e0",
        "5E0",
        "1e1",
        "\"5\"",
        "0",
        "-5",
        "9007199254740992",
        "null",
        "true",
        "5.5",
        "[5]",
        "05",
        "+5",
        "-0",
    ] {
        add(&format!("revision {rev}"), answer_body(rev, "false", &desc));
    }
    for arch in ["\"false\"", "0", "null", "1", "\"true\""] {
        add(&format!("archived {arch}"), answer_body("7", arch, &desc));
    }
    for d in [
        "null",
        "5",
        "[]",
        "{}",
        "\"\\ud800\"",
        "\"\\udc00\"",
        "\"a\\ud800b\"",
        "\"\\ud83d\"",
    ] {
        add(&format!("description {d}"), answer_body("7", "false", d));
    }
    let good: Value = serde_json::from_slice(&good_body()).unwrap();
    let mutate = |f: &dyn Fn(&mut Value)| {
        let mut v = good.clone();
        f(&mut v);
        serde_json::to_vec(&v).unwrap()
    };
    add(
        "no card",
        mutate(&|v| {
            v.as_object_mut().unwrap().remove("card");
        }),
    );
    add(
        "no revision",
        mutate(&|v| {
            v["card"].as_object_mut().unwrap().remove("revision");
        }),
    );
    add(
        "no archived",
        mutate(&|v| {
            v["card"].as_object_mut().unwrap().remove("archived");
        }),
    );
    add(
        "no prose",
        mutate(&|v| {
            v.as_object_mut().unwrap().remove("prose");
        }),
    );
    add(
        "no description",
        mutate(&|v| {
            v["prose"].as_object_mut().unwrap().remove("description");
        }),
    );
    add(
        "a card that is a string",
        mutate(&|v| v["card"] = json!("x")),
    );
    add(
        "prose that is a string",
        mutate(&|v| v["prose"] = json!("x")),
    );
    let mut problems = Vec::new();
    for (what, body) in bodies {
        let rig = Rig::with_hook(on_ticket(json200(body)));
        let ran = rig.board_create(&[]);
        for p in internal_problems(&rig, &ran, MSG_UNREADABLE_ANSWER) {
            problems.push(format!("{what}: {p}"));
        }
        if rig.standin.count() != 1 {
            problems.push(format!("{what}: {} requests", rig.standin.count()));
        }
        problems.extend(
            rig.nothing_made()
                .into_iter()
                .map(|p| format!("{what}: {p}")),
        );
    }
    assert_no_problems(problems);
}

#[test]
fn a_client_reads_three_members_and_ignores_every_other_whatever_it_holds() {
    // lease-api.md, The board read: "Every other member MUST be ignored, whatever it holds":
    // prose.comments, relationships, parent, children and related_card among them, and the card's
    // title, lane, owner and priority. A card whose ignored members change gives the same lease.
    let good: Value = serde_json::from_slice(&good_body()).unwrap();
    let mutate = |f: &dyn Fn(&mut Value)| {
        let mut v = good.clone();
        f(&mut v);
        serde_json::to_vec(&v).unwrap()
    };
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("the answer as it is", good_body()),
        (
            "relationships a number",
            mutate(&|v| v["relationships"] = json!(5)),
        ),
        ("parent a string", mutate(&|v| v["parent"] = json!("x"))),
        ("children null", mutate(&|v| v["children"] = Value::Null)),
        (
            "comments a number",
            mutate(&|v| v["prose"]["comments"] = json!(7)),
        ),
        (
            "comments missing",
            mutate(&|v| {
                v["prose"].as_object_mut().unwrap().remove("comments");
            }),
        ),
        (
            "a title that is a number",
            mutate(&|v| v["card"]["title"] = json!(5)),
        ),
        (
            "a lane that is a string",
            mutate(&|v| v["card"]["lane"] = json!("x")),
        ),
        (
            "an owner that is a list",
            mutate(&|v| v["card"]["owner"] = json!([1])),
        ),
        (
            "board_revision a string",
            mutate(&|v| v["board_revision"] = json!("x")),
        ),
        (
            "board_id missing",
            mutate(&|v| {
                v.as_object_mut().unwrap().remove("board_id");
            }),
        ),
        (
            "a related_card of any shape",
            mutate(&|v| v["related_card"] = json!({"x": [1, 2]})),
        ),
        (
            "members the answer did not have",
            mutate(&|v| {
                v["extra"] = json!({"a": [1, 2, {"b": null}]});
                v["card"]["extra"] = json!(true);
                v["prose"]["extra"] = json!(1);
            }),
        ),
    ];
    let mut problems = Vec::new();
    let mut leases = Vec::new();
    for (what, body) in cases {
        let rig = Rig::with_hook(on_ticket(json200(body)));
        let ran = rig.board_create(&[]);
        if ran.code != 0 {
            problems.push(format!(
                "{what}: exit {}, stderr {:?}",
                ran.code,
                ran.stderr_text()
            ));
            continue;
        }
        leases.push(same_lease(ran.lease_doc()));
        if rig.lab.records()[0]["plan_source"]["revision"] != REVISION {
            problems.push(format!("{what}: the revision is not card.revision's"));
        }
    }
    assert_no_problems(problems);
    assert!(leases.windows(2).all(|w| w[0] == w[1]), "The leases differ");
}

#[test]
fn a_board_that_never_answers_ends_the_command_at_30_seconds_with_the_did_not_answer_error() {
    // lease-api.md, Bounds: "Each request has 30 seconds, from the start of its connection to the
    // last byte of its answer." Run as the built program, which a test can end, since no in-process
    // run could be. The stand-in reads the request, logs it and holds the connection open.
    let standin = Standin::start(
        Fixture {
            token: new_token(),
            boards: BTreeMap::new(),
        },
        Some(always(Reply::Hang)),
    );
    let bin = Bin::new();
    bin.credential_for(&standin);
    let began = Instant::now();
    let cmd = bin.command(&[
        "lease",
        "create",
        "--project",
        "swim",
        "--board",
        BOARD,
        "--epic",
        "52",
    ]);
    let run = run_within(cmd, Duration::from_secs(75));
    let took = began.elapsed();
    assert_no_problems(failure_problems(
        &run,
        "internal",
        None,
        Some(&msg_no_answer(standin.port)),
    ));
    assert!(
        took >= Duration::from_secs(28) && took < Duration::from_secs(60),
        "The run took {took:?}, not 30 seconds"
    );
    assert_log(&standin, &[ticket_target(BOARD, EPIC)]);
}

#[test]
fn no_connection_and_a_closed_port_are_the_did_not_answer_error_and_name_the_receipts_port() {
    // lease-api.md, The board read: "No connection, a connection refused ..." with the base's port.
    let rig = Rig::new();
    let dead = {
        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        l.local_addr().unwrap().port()
    };
    assert_ne!(dead, rig.port());
    // No test connects to 127.0.0.1:8811, Terry's board (#0238).
    assert_ne!(dead, 8811);
    rig.write_receipt(&rig.receipt_with_base(&format!("http://127.0.0.1:{dead}")));
    let ran = rig.board_create(&[]);
    assert_no_problems(internal_problems(&rig, &ran, &msg_no_answer(dead)));
    assert_no_request(&rig.standin);
    assert_eq!(rig.nothing_made(), Vec::<String>::new());
}

#[test]
fn a_redirect_is_never_followed() {
    // lease-api.md, Bounds: "A redirect is never followed." A second stand-in is where the first
    // sends the client, and it MUST see nothing.
    let decoy = Standin::start(
        Fixture {
            token: new_token(),
            boards: BTreeMap::new(),
        },
        None,
    );
    let to = format!("{}{}", decoy.base(), ticket_target(BOARD, EPIC));
    let rig = Rig::with_hook(always(Resp::new(307).header("Location", &to).reply()));
    let ran = rig.board_create(&[]);
    assert_no_problems(internal_problems(&rig, &ran, &msg_no_answer(rig.port())));
    assert_log(&rig.standin, &[ticket_target(BOARD, EPIC)]);
    assert_no_request(&decoy);
}

// ---- The plan block -----------------------------------------------------------------------------------------

/// A plan with one turn, `T1`, of one story of one subagent: 2 agents at once, size `s`.
fn tiny() -> Value {
    json!({"name": "tiny", "turns": [{"name": "T1", "stories": [{"name": "S", "subagents": [
        {"name": "a", "model": "Opus", "effort": "high"}]}]}]})
}

fn tiny_text() -> String {
    compact(&tiny())
}

/// A fenced block holding `content` as lines, and a line feed after the closing line.
fn blk(content: &str) -> String {
    format!("```agentspace-plan\n{content}\n```\n")
}

/// Runs `create --board --epic` against a card with this description, and returns the problems with
/// the outcome `want`, and with the requests (one GET) and the lab (nothing made).
fn description_problems(description: &str, want: Result<(), String>) -> Vec<String> {
    let rig = Rig::with_description(description);
    let ran = rig.board_create(&[]);
    let mut p = Vec::new();
    match want {
        Ok(()) => {
            if ran.code != 0 {
                p.push(format!("exit {}, stderr {:?}", ran.code, ran.stderr_text()));
            } else {
                let doc = ran.lease_doc();
                if doc["sized_for"] != json!({"plan": "tiny", "turn": "T1", "agents": 2}) {
                    p.push(format!("sized_for {}", doc["sized_for"]));
                }
            }
        }
        Err(why) => {
            p.extend(error_problems(
                &rig.lab.root,
                &ran,
                "invalid_value",
                Some("epic"),
                &msg_no_usable_plan("52", &why),
            ));
            p.extend(rig.nothing_made());
        }
    }
    p.extend(log_problems(
        &rig.standin.log(),
        &[ticket_target(BOARD, EPIC)],
    ));
    p
}

#[track_caller]
fn check_descriptions(cases: Vec<(String, String, Result<(), String>)>) {
    let mut problems = Vec::new();
    for (what, description, want) in cases {
        for p in description_problems(&description, want) {
            problems.push(format!("{what}: {p}"));
        }
    }
    assert_no_problems(problems);
}

fn refused(what: &str, description: String, why: &str) -> (String, String, Result<(), String>) {
    (what.to_string(), description, Err(why.to_string()))
}

fn accepted(what: &str, description: String) -> (String, String, Result<(), String>) {
    (what.to_string(), description, Ok(()))
}

#[test]
fn a_description_with_no_block_is_refused_and_text_that_only_looks_like_one_is_text() {
    // lease-api.md, The plan block: the opening line is exactly ```agentspace-plan; a line that
    // starts with anything else, a space, a tab, `>`, U+FEFF, a tilde fence, another case, or fewer
    // than three backticks, is text for people, and no Markdown is parsed.
    let ok = tiny_text();
    check_descriptions(vec![
        refused("an empty description", String::new(), WHY_NO_BLOCK),
        refused("only prose", "Just words.\nMore.\n".into(), WHY_NO_BLOCK),
        refused("another fence", "```json\n{}\n```\n".into(), WHY_NO_BLOCK),
        refused("a lone fence", "```\n".into(), WHY_NO_BLOCK),
        refused(
            "an indented block",
            format!(" ```agentspace-plan\n{ok}\n```\n"),
            WHY_NO_BLOCK,
        ),
        refused(
            "a tab-indented block",
            format!("\t```agentspace-plan\n{ok}\n```\n"),
            WHY_NO_BLOCK,
        ),
        refused(
            "a quoted block",
            format!("> ```agentspace-plan\n> {ok}\n> ```\n"),
            WHY_NO_BLOCK,
        ),
        refused(
            "a tilde fence",
            format!("~~~agentspace-plan\n{ok}\n~~~\n"),
            WHY_NO_BLOCK,
        ),
        refused(
            "another case",
            format!("```Agentspace-Plan\n{ok}\n```\n"),
            WHY_NO_BLOCK,
        ),
        refused(
            "a byte order mark before the fence",
            format!("\u{feff}```agentspace-plan\n{ok}\n```\n"),
            WHY_NO_BLOCK,
        ),
        refused(
            "a space between the backticks and the name",
            format!("``` agentspace-plan\n{ok}\n```\n"),
            WHY_NO_BLOCK,
        ),
        refused(
            "two backticks",
            format!("``agentspace-plan\n{ok}\n``\n"),
            WHY_NO_BLOCK,
        ),
        refused(
            "a word before the backticks",
            format!("x```agentspace-plan\n{ok}\n```\n"),
            WHY_NO_BLOCK,
        ),
        refused(
            "a fence with another name",
            format!("```agentspace\n{ok}\n```\n"),
            WHY_NO_BLOCK,
        ),
        refused("the name alone", "agentspace-plan\n".into(), WHY_NO_BLOCK),
    ]);
}

#[test]
fn text_that_only_looks_like_a_block_is_skipped_beside_a_real_block() {
    // lease-api.md, The plan block, "Finding the block", 3: "any other line is text for people, and
    // is skipped. That includes every other fence", and a block that is quoted or indented is text.
    let ok = tiny_text();
    let real = blk(&ok);
    check_descriptions(vec![
        accepted(
            "a json fence before",
            format!("```json\n{{\"turns\":1}}\n```\n{real}"),
        ),
        accepted("a lone fence before", format!("```\n{real}")),
        accepted(
            "a quoted block before",
            format!("> ```agentspace-plan\n> {{\"turns\":1}}\n> ```\n{real}"),
        ),
        accepted(
            "an indented block before",
            format!("  ```agentspace-plan\n  x\n  ```\n{real}"),
        ),
        accepted(
            "a tilde fence before",
            format!("~~~agentspace-plan\nx\n~~~\n{real}"),
        ),
        accepted(
            "another case before",
            format!("```Agentspace-Plan\nx\n```\n{real}"),
        ),
        accepted("a block with no prose", real.clone()),
        accepted(
            "the closing line is the last, with no line feed after it",
            format!("```agentspace-plan\n{ok}\n```"),
        ),
        accepted(
            "text with every other line terminator in it",
            format!("a\u{b}b\u{c}c\u{85}d\u{2028}e\u{2029}f\n{real}after\u{2028}\n"),
        ),
        accepted(
            "text after the block",
            format!("{real}Some prose after.\n```json\n{{}}\n```\n"),
        ),
    ]);
}

#[test]
fn a_second_block_is_refused_and_is_counted_by_lines_alone() {
    // lease-api.md, The plan block: "An opening line inside another fence ... is an opening line,
    // since that fence is not parsed", and the content is checked only after the whole description
    // has been read with no line fault.
    let ok = tiny_text();
    let real = blk(&ok);
    check_descriptions(vec![
        refused("two blocks", format!("{real}\n{real}"), WHY_TWO_BLOCKS),
        refused(
            "three blocks",
            format!("{real}{real}{real}"),
            WHY_TWO_BLOCKS,
        ),
        refused(
            "an example inside another fence, and the block",
            format!("```markdown\n{real}```\n\n{real}"),
            WHY_TWO_BLOCKS,
        ),
        refused(
            "a first block that is not a plan, and a second",
            format!("{}{real}", blk("nope")),
            WHY_TWO_BLOCKS,
        ),
        refused(
            "a block with no closing line and then none",
            format!("{real}```agentspace-plan\n{ok}\n"),
            WHY_TWO_BLOCKS,
        ),
    ]);
}

#[test]
fn a_line_that_nearly_starts_a_block_is_refused_wherever_it_stands_outside_one() {
    // lease-api.md, The plan block: a near miss is a line that is not the opening line and starts
    // with three or more backticks followed at once by `agentspace-plan`; "a line its writer most
    // likely meant as an opening line is never skipped as text". Only LF and CRLF end a line.
    let ok = tiny_text();
    let real = blk(&ok);
    let nears = [
        "```agentspace-plan ",
        "```agentspace-plan\t",
        "````agentspace-plan",
        "`````agentspace-plan",
        "```agentspace-plan json",
        "```agentspace-planner",
        "```agentspace-plan:",
        "```agentspace-plan\u{b}",
        "```agentspace-plan\u{c}",
        "```agentspace-plan\u{85}",
        "```agentspace-plan\u{2028}",
        "```agentspace-plan\u{2029}",
        "```agentspace-plan\u{feff}",
    ];
    let mut cases = Vec::new();
    for near in nears {
        let q = near.escape_debug().to_string();
        cases.push(refused(
            &format!("{q} before a block"),
            format!("intro\n{near}\n{real}"),
            WHY_NEAR_MISS,
        ));
        cases.push(refused(
            &format!("{q} after a block"),
            format!("{real}{near}\n"),
            WHY_NEAR_MISS,
        ));
        cases.push(refused(
            &format!("{q} alone"),
            format!("{near}\n"),
            WHY_NEAR_MISS,
        ));
    }
    // A second carriage return before the line feed is text, so the line is a near miss; and a
    // near miss stops the reading before a second block would.
    cases.push(refused(
        "a carriage return before the line feed that is the line end",
        format!("```agentspace-plan\r\r\n{ok}\n```\n"),
        WHY_NEAR_MISS,
    ));
    cases.push(refused(
        "a near miss before a second block",
        format!("{real}```agentspace-planner\n{real}"),
        WHY_NEAR_MISS,
    ));
    cases.push(refused(
        "a near miss after a block whose content is not a plan",
        format!("{}```agentspace-plan x\n", blk("nope")),
        WHY_NEAR_MISS,
    ));
    check_descriptions(cases);
}

#[test]
fn a_block_that_is_never_closed_is_refused_and_so_is_one_with_a_second_opening_inside() {
    // lease-api.md, The plan block: "Inside a block, an opening line or a near miss is refused as
    // the block before it never being closed", as is the end of the description inside one; the
    // closing line is exactly ``` and nothing else.
    let ok = tiny_text();
    check_descriptions(vec![
        refused(
            "no closing line",
            format!("```agentspace-plan\n{ok}\n"),
            WHY_UNCLOSED,
        ),
        refused(
            "no closing line or line feed",
            format!("```agentspace-plan\n{ok}"),
            WHY_UNCLOSED,
        ),
        refused(
            "an opening line only",
            "```agentspace-plan\n".into(),
            WHY_UNCLOSED,
        ),
        refused(
            "an opening line inside",
            format!("```agentspace-plan\n{ok}\n```agentspace-plan\n{ok}\n```\n"),
            WHY_UNCLOSED,
        ),
        refused(
            "a near miss inside",
            format!("```agentspace-plan\n{ok}\n```agentspace-plan \n```\n"),
            WHY_UNCLOSED,
        ),
        refused(
            "four backticks do not close",
            format!("```agentspace-plan\n{ok}\n````\n"),
            WHY_UNCLOSED,
        ),
        refused(
            "backticks and a space do not close",
            format!("```agentspace-plan\n{ok}\n``` \n"),
            WHY_UNCLOSED,
        ),
        refused(
            "a closing line that names a language does not close",
            format!("```agentspace-plan\n{ok}\n```json\n"),
            WHY_UNCLOSED,
        ),
        refused(
            "an indented closing line does not close",
            format!("```agentspace-plan\n{ok}\n ```\n"),
            WHY_UNCLOSED,
        ),
        refused(
            "a carriage return before a lone line feed is the line end, but a second is text",
            format!("```agentspace-plan\n{ok}\n```\r\r\n"),
            WHY_UNCLOSED,
        ),
    ]);
}

#[test]
fn lines_end_at_lf_or_crlf_and_the_content_is_every_character_between_the_fences() {
    // lease-api.md, The plan block, "Lines" and "The block": mixed LF and CRLF line ends; the
    // content is the plan document's bytes, a leading byte order mark accepted and ignored.
    let ok = tiny_text();
    let pretty = serde_json::to_string_pretty(&tiny()).unwrap();
    check_descriptions(vec![
        accepted(
            "CRLF throughout",
            format!("# Epic\r\n```agentspace-plan\r\n{ok}\r\n```\r\nafter\r\n"),
        ),
        accepted(
            "mixed LF and CRLF",
            format!("intro\r\n```agentspace-plan\n{ok}\r\n```\nafter\r\n"),
        ),
        accepted(
            "a plan over many lines",
            format!("```agentspace-plan\n{pretty}\n```\n"),
        ),
        accepted(
            "a plan over many CRLF lines",
            format!(
                "```agentspace-plan\r\n{}\r\n```\r\n",
                pretty.replace('\n', "\r\n")
            ),
        ),
        accepted(
            "a byte order mark at the start of the content",
            format!("```agentspace-plan\n\u{feff}{ok}\n```\n"),
        ),
        accepted(
            "a lone carriage return in prose",
            format!("a\rb\n```agentspace-plan\n{ok}\n```\n"),
        ),
        accepted(
            "blank lines around the document",
            format!("```agentspace-plan\n\n\n{ok}\n\n```\n"),
        ),
    ]);
}

#[test]
fn a_content_that_is_not_a_plan_is_refused_with_the_why_of_the_first_check_it_fails() {
    // lease-api.md, The plan block and Plan: the content is read as a plan file's bytes: check 1
    // (size), 4 (JSON), 5 (a member named twice), 6 (the schema), 7 to 11 (the names), each `<why>`
    // word for word. Check 3 cannot be reached for a block, and check 2 never applies.
    let ok = tiny_text();
    let with_budget = |b: &str| {
        ok.replace(
            "\"name\":\"T1\"",
            &format!("\"name\":\"T1\",\"budget_minutes\":{b}"),
        )
    };
    let plan_of = |turns: &str| format!("{{\"turns\":{turns}}}");
    let one_sub = |name: &str, after: &str| {
        format!("{{\"name\":\"{name}\",\"model\":\"Opus\",\"effort\":\"high\"{after}}}")
    };
    let story_of = |name: &str, subs: &[String]| {
        format!("{{\"name\":\"{name}\",\"subagents\":[{}]}}", subs.join(","))
    };
    let turn_of = |name: &str, stories: &[String]| {
        format!(
            "{{\"name\":\"{name}\",\"stories\":[{}]}}",
            stories.join(",")
        )
    };
    let s1 = story_of("S", &[one_sub("a", "")]);
    // The document and white space after it, so that the content, the line feed after it and a
    // three-byte byte order mark before it (the caller's) come to `total` bytes.
    let padded = |total: usize| {
        let pad = total - 3 - ok.len() - 1;
        format!("{ok}{}", " ".repeat(pad))
    };
    let big = |extra: usize| {
        // The content is `X` and a line feed: `extra` more bytes than the bound holds.
        let pad = 262_144 + extra - ok.len() - 1;
        format!("{ok}{}", " ".repeat(pad))
    };
    let mut cases = vec![
        refused(
            "empty content",
            "```agentspace-plan\n```\n".into(),
            WHY_JSON,
        ),
        refused("blank content", blk("  \n\n"), WHY_JSON),
        refused("text after the document", blk(&format!("{ok} x")), WHY_JSON),
        refused("two documents", blk(&format!("{ok}\n{ok}")), WHY_JSON),
        refused("not JSON", blk("nope"), WHY_JSON),
        refused(
            "U+FEFF after white space",
            blk(&format!(" \u{feff}{ok}")),
            WHY_JSON,
        ),
        refused(
            "U+FEFF inside the document, outside a string",
            blk(&ok.replace("\"turns\":", "\"turns\":\u{feff}")),
            WHY_JSON,
        ),
        refused(
            "a number it cannot hold",
            blk(&with_budget("1e400")),
            WHY_JSON,
        ),
        refused(
            "a trailing comma",
            blk(&ok.replace("}]}]}", "}],}]}")),
            WHY_JSON,
        ),
        refused(
            "content larger than 262144 bytes",
            format!("```agentspace-plan\n{}\n```\n", big(1)),
            WHY_LARGER,
        ),
        refused(
            "content larger than the bound, which is not JSON either",
            format!("```agentspace-plan\n{}\n```\n", "x".repeat(262_145)),
            WHY_LARGER,
        ),
        refused(
            "a byte order mark counted in the bound",
            format!("```agentspace-plan\n\u{feff}{}\n```\n", padded(262_145)),
            WHY_LARGER,
        ),
        accepted(
            "a byte order mark and content of exactly 262144 bytes",
            format!("```agentspace-plan\n\u{feff}{}\n```\n", padded(262_144)),
        ),
        accepted(
            "content of exactly 262144 bytes",
            format!("```agentspace-plan\n{}\n```\n", big(0)),
        ),
        refused(
            "a member named twice",
            blk(&ok.replacen("{\"name\":\"tiny\"", "{\"name\":\"tiny\",\"name\":\"x\"", 1)),
            "an object has two members named 'name'",
        ),
        refused(
            "a member named twice once its escapes are read",
            blk(&ok.replacen(
                "{\"name\":\"tiny\"",
                "{\"na\\u006de\":\"x\",\"name\":\"tiny\"",
                1,
            )),
            "an object has two members named 'name'",
        ),
        refused("no turns member", blk("{}"), WHY_SCHEMA),
        refused("an empty turns", blk(&plan_of("[]")), WHY_SCHEMA),
        refused("an array", blk("[]"), WHY_SCHEMA),
        refused("a number", blk("5"), WHY_SCHEMA),
        refused("null", blk("null"), WHY_SCHEMA),
        refused(
            "a turn with no stories",
            blk(&plan_of("[{\"name\":\"T\",\"stories\":[]}]")),
            WHY_SCHEMA,
        ),
        refused(
            "a turn with no stories member",
            blk(&plan_of("[{\"name\":\"T\"}]")),
            WHY_SCHEMA,
        ),
        refused(
            "a member that is not in the shape",
            blk(&ok.replacen("{\"name\":\"tiny\"", "{\"zzz\":1,\"name\":\"tiny\"", 1)),
            WHY_SCHEMA,
        ),
        refused(
            "a name with a space at its end",
            blk(&ok.replace("\"name\":\"T1\"", "\"name\":\"T1 \"")),
            WHY_SCHEMA,
        ),
        refused(
            "a name that is not ASCII",
            blk(&ok.replace("\"name\":\"T1\"", "\"name\":\"T\u{e9}\"")),
            WHY_SCHEMA,
        ),
        refused(
            "a name holding U+FEFF",
            blk(&ok.replace("\"name\":\"T1\"", "\"name\":\"T\u{feff}1\"")),
            WHY_SCHEMA,
        ),
        refused(
            "an effort that is not of its form",
            blk(&ok.replace("\"high\"", "\"hi gh\"")),
            WHY_SCHEMA,
        ),
        refused(
            "an after that names one twice",
            blk(&plan_of(&format!(
                "[{}]",
                turn_of(
                    "T",
                    &[story_of(
                        "S",
                        &[one_sub("a", ""), one_sub("b", ",\"after\":[\"a\",\"a\"]")]
                    )]
                )
            ))),
            WHY_SCHEMA,
        ),
        refused("turns that are null", blk("{\"turns\":null}"), WHY_SCHEMA),
    ];
    // A budget_minutes that is not an integer of digits alone from 1 to 960 (check 6), though the
    // plan's schema accepts `90.0` and `9e1`.
    for budget in [
        "90.0", "9e1", "9E1", "-0", "0", "-1", "961", "1e3", "\"90\"", "90.5", "true", "[]", "{}",
        "1.0e1", "0.0",
    ] {
        cases.push(refused(
            &format!("budget_minutes {budget}"),
            blk(&with_budget(budget)),
            WHY_SCHEMA,
        ));
    }
    for budget in ["90", "1", "960", "null"] {
        cases.push(accepted(
            &format!("budget_minutes {budget}"),
            blk(&with_budget(budget)),
        ));
    }
    let names = [
        (
            "a turn named twice",
            plan_of(&format!(
                "[{},{}]",
                turn_of("T1", std::slice::from_ref(&s1)),
                turn_of("T1", std::slice::from_ref(&s1))
            )),
            "turn 'T1' is named twice",
        ),
        (
            "a story named twice",
            plan_of(&format!("[{}]", turn_of("T1", &[s1.clone(), s1.clone()]))),
            "story 'S' is named twice in turn 'T1'",
        ),
        (
            "a subagent named twice",
            plan_of(&format!(
                "[{}]",
                turn_of(
                    "T1",
                    &[story_of("S", &[one_sub("a", ""), one_sub("a", "")])]
                )
            )),
            "subagent 'a' is named twice in story 'S'",
        ),
        (
            "an after naming no subagent of its story",
            plan_of(&format!(
                "[{}]",
                turn_of(
                    "T1",
                    &[story_of(
                        "S",
                        &[one_sub("a", ""), one_sub("b", ",\"after\":[\"zz\"]")]
                    )]
                )
            )),
            "subagent 'b' in story 'S' starts after 'zz', which is no subagent of that story",
        ),
        (
            "a cycle",
            plan_of(&format!(
                "[{}]",
                turn_of(
                    "T1",
                    &[story_of(
                        "S",
                        &[
                            one_sub("a", ",\"after\":[\"b\"]"),
                            one_sub("b", ",\"after\":[\"a\"]")
                        ]
                    )]
                )
            )),
            "the subagents of story 'S' start after each other in a cycle",
        ),
        (
            "a subagent that starts after itself",
            plan_of(&format!(
                "[{}]",
                turn_of(
                    "T1",
                    &[story_of("S", &[one_sub("a", ",\"after\":[\"a\"]")])]
                )
            )),
            "the subagents of story 'S' start after each other in a cycle",
        ),
    ];
    for (what, text, why) in names {
        cases.push(refused(what, blk(&text), why));
    }
    check_descriptions(cases);
}

#[test]
fn the_why_of_a_refused_description_never_quotes_the_description_and_no_marker_is_echoed() {
    // lease-api.md, The plan block: "A <why> never quotes the description, the block or a line of
    // either, beyond the names Plan's own <why>s quote." `Rig::guard` fails on any marker in any
    // output; this runs a refusal of each line fault with markers in the prose and in the block.
    let prose = "PROSE-MARKER-9f3a\n";
    let cases = vec![
        refused("no block", format!("{prose}more"), WHY_NO_BLOCK),
        refused(
            "two blocks",
            format!("{prose}{}{}", blk("nope PROSE-MARKER-9f3a"), blk("x")),
            WHY_TWO_BLOCKS,
        ),
        refused(
            "a near miss",
            format!("{prose}```agentspace-plan PROSE-MARKER-9f3a\n"),
            WHY_NEAR_MISS,
        ),
        refused(
            "never closed",
            format!("{prose}```agentspace-plan\nPROSE-MARKER-9f3a\n"),
            WHY_UNCLOSED,
        ),
        refused(
            "not JSON",
            format!("{prose}{}", blk("PROSE-MARKER-9f3a")),
            WHY_JSON,
        ),
    ];
    check_descriptions(cases);
}

// ---- Before the lease records ------------------------------------------------------------------------------

#[test]
fn the_board_is_read_before_the_lease_records_and_its_internal_comes_first() {
    // lease-api.md, Exit codes and errors, step 2, item 5: "The board read's internal is the one
    // internal that step 2 reports, since the board is read before the lease records are; it is ...
    // never after a not_found or anything that the lease records give." So a state file that is not
    // valid, or no home, never hides a refusal of the board read, and a good board read is followed
    // by the records' own errors.
    let mut problems = Vec::new();
    // The state file is not valid.
    {
        let rig = Rig::with_hook(always(problem(500, "internal_error")));
        rig.lab.seed_raw(b"this is not a state file");
        let ran = rig.board_create(&[]);
        for p in internal_problems(&rig, &ran, &msg_no_answer(rig.port())) {
            problems.push(format!("a board that fails, over a bad state file: {p}"));
        }
        if rig.standin.count() != 1 {
            problems.push("the board was not read once".to_string());
        }
    }
    {
        let rig = Rig::with_cards(
            vec![(
                EPIC,
                CardFx::archived(REVISION, &plan_description(&plan_a())),
            )],
            None,
        );
        rig.lab.seed_raw(b"this is not a state file");
        let ran = rig.board_create(&[]);
        for p in error_problems(
            &rig.lab.root,
            &ran,
            "invalid_value",
            Some("epic"),
            &msg_archived("52"),
        ) {
            problems.push(format!("an archived epic, over a bad state file: {p}"));
        }
    }
    {
        let rig = Rig::new();
        rig.lab.seed_raw(b"this is not a state file");
        let ran = rig.board_create(&["--turn", "T99"]);
        for p in error_problems(
            &rig.lab.root,
            &ran,
            "invalid_value",
            Some("turn"),
            &msg_no_such_turn("T99", &["T01", "T02", "T03", "T04", "T05", "T06"]),
        ) {
            problems.push(format!("a turn the plan lacks, over a bad state file: {p}"));
        }
        let ran = rig.board_create(&[]);
        for p in internal_problems(&rig, &ran, lab::MSG_NOT_VALID) {
            problems.push(format!("a good board, then the bad state file: {p}"));
        }
        if rig.standin.count() != 2 {
            problems.push(format!("{} requests, not 2", rig.standin.count()));
        }
    }
    // No home folder.
    {
        let mut rig = Rig::with_hook(always(problem(500, "internal_error")));
        rig.lab.unset_var("USERPROFILE");
        rig.lab.unset_var("AGENTSPACE_STATE_DIR");
        let ran = rig.board_create(&[]);
        for p in internal_problems(&rig, &ran, &msg_no_answer(rig.port())) {
            problems.push(format!("a board that fails, with no home: {p}"));
        }
    }
    {
        let mut rig = Rig::with_cards(
            vec![(
                EPIC,
                CardFx::archived(REVISION, &plan_description(&plan_a())),
            )],
            None,
        );
        rig.lab.unset_var("USERPROFILE");
        rig.lab.unset_var("AGENTSPACE_STATE_DIR");
        let ran = rig.board_create(&[]);
        for p in error_problems(
            &rig.lab.root,
            &ran,
            "invalid_value",
            Some("epic"),
            &msg_archived("52"),
        ) {
            problems.push(format!("an archived epic, with no home: {p}"));
        }
    }
    {
        let mut rig = Rig::new();
        rig.lab.unset_var("USERPROFILE");
        rig.lab.unset_var("AGENTSPACE_STATE_DIR");
        let ran = rig.board_create(&[]);
        for p in internal_problems(&rig, &ran, &lab::msg_no_home("create")) {
            problems.push(format!("a good board, with no home: {p}"));
        }
        if rig.standin.count() != 1 {
            problems.push("with no home the board was not read once".to_string());
        }
    }
    assert_no_problems(problems);
}

#[test]
fn a_refused_epic_never_waits_for_the_lease_records_and_never_changes_them() {
    // lease-api.md, Concurrency and atomicity: "create reads the board ... before it starts to wait
    // too ... so a slow or refused board read never holds the lease records, and a refused epic
    // never touches them." The lease state lock is held by this test, and the state file seeded.
    let archived = || {
        Rig::with_cards(
            vec![(
                EPIC,
                CardFx::archived(REVISION, &plan_description(&plan_a())),
            )],
            None,
        )
    };
    let rigs: Vec<(&str, Rig, Want)> = vec![
        (
            "an archived epic",
            archived(),
            Want::Value("epic", msg_archived("52")),
        ),
        (
            "a board that refuses",
            Rig::with_hook(always(problem(401, "unauthorized"))),
            Want::Refused,
        ),
        (
            "an unknown ticket",
            Rig::with_hook(not_found_then(json200("{}"))),
            Want::Value("epic", msg_no_card("52", BOARD)),
        ),
    ];
    for (what, rig, want) in rigs {
        let seeded = lab::Rec::ready(&lab::uuid_n(1), "other");
        rig.lab.seed(&[seeded]);
        let before = rig.lab.state_bytes().unwrap();
        let hold = LockHold::take(&rig.lab);
        let began = Instant::now();
        let ran = rig.board_create(&[]);
        let took = began.elapsed();
        drop(hold);
        let p = want.problems(&rig, &ran);
        assert!(p.is_empty(), "{what}: {p:?}");
        assert!(
            took < Duration::from_secs(10),
            "{what}: the command waited {took:?} for the lease records"
        );
        assert_eq!(
            rig.lab.state_bytes().unwrap(),
            before,
            "{what}: the state file changed"
        );
        assert!(rig.h.calls().is_empty(), "{what}: a driver was asked");
    }
}

// ---- What is recorded, and what no output carries ------------------------------------------------------------------

/// Every file under a folder, found by walking it.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                todo.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// The files under `dir` that hold `needle`, as ASCII or as UTF-16LE. A path is returned, never the
/// needle.
fn files_holding(dir: &Path, needle: &str) -> Vec<PathBuf> {
    let (ascii, wide) = (needle.as_bytes().to_vec(), utf16le(needle));
    files_under(dir)
        .into_iter()
        .filter(|f| {
            std::fs::read(f).is_ok_and(|b| find(&b, &ascii).is_some() || find(&b, &wide).is_some())
        })
        .collect()
}

#[test]
fn the_sweep_for_the_token_finds_a_planted_copy_in_either_encoding() {
    // Proof that the sweep can see what it looks for, before its zero is believed.
    let tmp = tempfile::Builder::new()
        .prefix("aslb-sweep-")
        .tempdir()
        .unwrap();
    let token = new_token();
    assert!(files_holding(tmp.path(), &token).is_empty());
    std::fs::create_dir_all(tmp.path().join("a").join("b")).unwrap();
    std::fs::write(tmp.path().join("a").join("one.txt"), format!("xx{token}xx")).unwrap();
    std::fs::write(
        tmp.path().join("a").join("b").join("two.bin"),
        utf16le(&token),
    )
    .unwrap();
    std::fs::write(tmp.path().join("three.txt"), "nothing").unwrap();
    assert_eq!(files_holding(tmp.path(), &token).len(), 2);
}

#[test]
fn a_board_leases_record_keeps_its_plan_and_where_it_came_from_and_no_output_prints_either() {
    // lease-api.md, What is recorded; state.md, `plan_source`: the normalized board ID, the ticket
    // as an integer and card.revision as the answer gave it (not board_revision), beside the plan as
    // read; written last, after `plan`; only on a board lease; kept as they are by renew, release,
    // get and list; and in no output.
    let rig = Rig::new();
    let file = rig.plan_file(&plan_a());
    let board = rig
        .create(&[
            "--board",
            &BOARD.to_uppercase(),
            "--epic",
            "0052",
            "--turn",
            "T01",
        ])
        .lease_doc();
    let from_file = rig.create(&["--plan", &file, "--turn", "T01"]).lease_doc();
    let plain = rig.create(&[]).lease_doc();
    let board_id = board["id"].as_str().unwrap().to_string();
    let state_text = String::from_utf8(rig.lab.state_bytes().unwrap()).unwrap();
    assert!(
        schema_problems(
            "state/leases.state.schema.json",
            &serde_json::from_str::<Value>(&state_text).unwrap()
        )
        .is_empty(),
        "The state file breaks its schema"
    );
    let record = |id: &Value| rig.lab.record_of(id.as_str().unwrap()).unwrap();
    let r = record(&board["id"]);
    assert_eq!(
        r["plan_source"],
        json!({"board": BOARD, "ticket": 52, "revision": 7}),
        "The board normalized, the ticket as an integer, card.revision as read"
    );
    assert!(r["plan_source"]["ticket"].is_u64() && r["plan_source"]["revision"].is_u64());
    assert_eq!(r["plan"], plan_a(), "The plan as the block held it");
    assert_eq!(r["sized_for"], board["sized_for"]);
    assert_eq!(r["driver"], "fake");
    assert!(
        record(&from_file["id"]).get("plan_source").is_none(),
        "A --plan lease has none"
    );
    assert_eq!(record(&from_file["id"])["plan"], plan_a());
    assert!(record(&plain["id"]).get("plan_source").is_none());
    assert!(record(&plain["id"]).get("plan").is_none());
    // Written last: after `driver` and `plan`, in the bytes of the record.
    let at = |needle: &str| {
        let boards: Vec<usize> = state_text.match_indices(needle).map(|(i, _)| i).collect();
        boards
    };
    let first_source = at("\"plan_source\":")[0];
    assert!(
        at("\"plan\":")[0] < first_source,
        "plan_source is written after plan"
    );
    assert!(
        at("\"driver\":")[0] < first_source,
        "plan_source is written after driver"
    );
    let tail = &state_text[first_source..];
    let end = tail.find('}').unwrap();
    assert_eq!(
        &tail[..=end],
        &format!("\"plan_source\":{{\"board\":\"{BOARD}\",\"ticket\":52,\"revision\":7}}")[..],
        "plan_source holds the three members and no more, in this order"
    );
    assert!(
        tail[end + 1..].starts_with("}"),
        "plan_source is the last member of its record"
    );
    // The commands that keep it.
    let requests = rig.standin.count();
    let before = record(&board["id"]);
    let got = rig.lab.run(&["lease", "get", &board_id]);
    let renewed = rig.lab.run(&["lease", "renew", &board_id]);
    let listed = rig.lab.run(&["lease", "list"]);
    let released = rig.lab.run(&["lease", "release", &board_id]);
    let docs = [
        got.lease_doc(),
        renewed.lease_doc(),
        released.lease_doc(),
        board.clone(),
    ];
    let list = listed.list_doc();
    assert_eq!(
        renewed.lease_doc()["ttl_seconds"],
        16_200,
        "renew uses the lease's own TTL"
    );
    assert_eq!(
        rig.standin.count(),
        requests,
        "No other command reads the board"
    );
    let after = record(&board["id"]);
    for member in ["plan", "plan_source", "sized_for", "ttl_seconds"] {
        assert_eq!(
            before[member], after[member],
            "{member} stays as create wrote it"
        );
    }
    for ran in [&got, &renewed, &listed, &released] {
        let text = ran.stdout_text();
        for word in [
            BOARD,
            "plan_source",
            "\"revision\"",
            "\"ticket\"",
            "\"board\"",
            "\"epic\"",
            "127.0.0.1",
        ] {
            assert!(!text.contains(word), "An output holds {word}");
        }
    }
    for doc in docs.iter().chain(list["leases"].as_array().unwrap()) {
        for schema in ["lease.output.schema.json", "v1/lease.output.schema.json"] {
            let p = schema_problems(schema, doc);
            assert!(p.is_empty(), "{schema}: {p:?}\n{doc}");
        }
    }
    let p = schema_problems("v1/lease-list.output.schema.json", &list);
    assert!(p.is_empty(), "{p:?}");
    let p = schema_problems("lease-list.output.schema.json", &list);
    assert!(p.is_empty(), "{p:?}");
}

#[test]
fn a_board_lease_has_sized_for_plan_the_plans_own_name_and_none_when_it_has_none() {
    // lease-api.md, What is recorded: "sized_for.plan is the plan document's own name ... never the
    // board's ID, the board's name, the epic's ticket or the card's title", and is left out when the
    // plan has none.
    let named = Rig::new();
    let doc = named.board_create(&[]).lease_doc();
    assert_eq!(doc["sized_for"]["plan"], PLAN_NAME);
    let unnamed_plan = json!({"turns": [turn("T1", None, vec![wide("S", 2)])]});
    let unnamed = Rig::with_plan(&unnamed_plan);
    let doc = unnamed.board_create(&[]).lease_doc();
    assert_eq!(doc["sized_for"], json!({"turn": "T1", "agents": 3}));
    let null_name = json!({"name": null, "turns": [turn("T1", None, vec![wide("S", 2)])]});
    let rig = Rig::with_plan(&null_name);
    assert_eq!(
        rig.board_create(&[]).lease_doc()["sized_for"],
        json!({"turn": "T1", "agents": 3})
    );
}

#[test]
fn the_token_is_in_no_file_of_the_home_no_output_and_no_state() {
    // lease-api.md, The credential: "The token MUST NOT be printed, logged, written to agentspace's
    // state or to any file". The home of the lab is swept for the token's text and its UTF-16LE
    // bytes after a good run, a refused one and a failed one, and the state file for the base URL.
    let rig = Rig::new();
    let _ = rig.board_create(&[]).lease_doc();
    let _ = rig.board_create(&["--turn", "T99"]);
    let _ = rig.create(&["--board", BOARD, "--epic", "99"]);
    let hits = files_holding(&rig.lab.root, rig.standin.token());
    assert!(hits.is_empty(), "{} files hold the token", hits.len());
    let state = String::from_utf8(rig.lab.state_bytes().unwrap()).unwrap();
    for word in [
        "127.0.0.1",
        "http://",
        "localswim",
        "Bearer",
        "service.json",
        "bot.json",
    ] {
        assert!(!state.contains(word), "The state file holds {word}");
    }
    assert!(
        !state.contains(&rig.port().to_string()) || !state.contains(&format!(":{}", rig.port())),
        "The state file holds the base's port"
    );
}

// ---- The GET-only log, and the checks that read it -----------------------------------------------------------

fn standard_standin() -> Standin {
    Standin::start(
        Fixture {
            token: new_token(),
            boards: BTreeMap::from([(
                BOARD.to_string(),
                BTreeMap::from([(EPIC, CardFx::good(REVISION, &plan_description(&plan_a())))]),
            )]),
        },
        None,
    )
}

/// A request text as an HTTP client sends it, with these extra header lines and this body.
fn raw_get(method: &str, target: &str, token: Option<&str>, extra: &[&str], body: &str) -> String {
    let mut s = format!("{method} {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n");
    if let Some(t) = token {
        s.push_str(&format!("Authorization: Bearer {t}\r\n"));
    }
    for e in extra {
        s.push_str(e);
        s.push_str("\r\n");
    }
    s.push_str("Connection: close\r\n\r\n");
    s.push_str(body);
    s
}

#[test]
fn the_get_only_check_sees_a_post_and_every_header_the_spec_forbids() {
    // conformance.md, "What the board cases MUST show", 3: "a control proves the check can see
    // one". The stand-in is sent each planted request, answers every method but GET with a 405 and
    // logs it, and the check that the client sent nothing but GETs of the spec's targets fails on
    // each.
    let t = ticket_target(BOARD, EPIC);
    type Plant = (&'static str, Box<dyn Fn(&str) -> String>, &'static str);
    let plants: Vec<Plant> = vec![
        (
            "a good GET",
            Box::new(|tok| raw_get("GET", &ticket_target(BOARD, EPIC), Some(tok), &[], "")),
            "",
        ),
        (
            "a POST",
            Box::new(|tok| {
                raw_get(
                    "POST",
                    &ticket_target(BOARD, EPIC),
                    Some(tok),
                    &["Content-Length: 0"],
                    "",
                )
            }),
            "the method is POST",
        ),
        (
            "a PUT",
            Box::new(|tok| raw_get("PUT", &ticket_target(BOARD, EPIC), Some(tok), &[], "")),
            "the method is PUT",
        ),
        (
            "a PATCH",
            Box::new(|tok| raw_get("PATCH", &ticket_target(BOARD, EPIC), Some(tok), &[], "")),
            "the method is PATCH",
        ),
        (
            "a DELETE",
            Box::new(|tok| raw_get("DELETE", &ticket_target(BOARD, EPIC), Some(tok), &[], "")),
            "the method is DELETE",
        ),
        (
            "a HEAD",
            Box::new(|tok| raw_get("HEAD", &ticket_target(BOARD, EPIC), Some(tok), &[], "")),
            "the method is HEAD",
        ),
        (
            "an OPTIONS",
            Box::new(|tok| raw_get("OPTIONS", &ticket_target(BOARD, EPIC), Some(tok), &[], "")),
            "the method is OPTIONS",
        ),
        (
            "a GET with a body",
            Box::new(|tok| {
                raw_get(
                    "GET",
                    &ticket_target(BOARD, EPIC),
                    Some(tok),
                    &["Content-Length: 2"],
                    "{}",
                )
            }),
            "it carried a body",
        ),
        (
            "a GET with a chunked body",
            Box::new(|tok| {
                raw_get(
                    "GET",
                    &ticket_target(BOARD, EPIC),
                    Some(tok),
                    &["Transfer-Encoding: chunked"],
                    "0\r\n\r\n",
                )
            }),
            "it carried a body",
        ),
        (
            "a GET with a cookie",
            Box::new(|tok| {
                raw_get(
                    "GET",
                    &ticket_target(BOARD, EPIC),
                    Some(tok),
                    &["Cookie: a=b"],
                    "",
                )
            }),
            "cookie",
        ),
        (
            "a GET with an Origin",
            Box::new(|tok| {
                raw_get(
                    "GET",
                    &ticket_target(BOARD, EPIC),
                    Some(tok),
                    &["Origin: http://x"],
                    "",
                )
            }),
            "origin",
        ),
        (
            "a GET with If-None-Match",
            Box::new(|tok| {
                raw_get(
                    "GET",
                    &ticket_target(BOARD, EPIC),
                    Some(tok),
                    &["If-None-Match: \"x\""],
                    "",
                )
            }),
            "if-none-match",
        ),
        (
            "a GET that asks for gzip",
            Box::new(|tok| {
                raw_get(
                    "GET",
                    &ticket_target(BOARD, EPIC),
                    Some(tok),
                    &["Accept-Encoding: gzip, identity"],
                    "",
                )
            }),
            "Accept-Encoding",
        ),
        (
            "a GET that asks for identity",
            Box::new(|tok| {
                raw_get(
                    "GET",
                    &ticket_target(BOARD, EPIC),
                    Some(tok),
                    &["Accept-Encoding: identity"],
                    "",
                )
            }),
            "",
        ),
        (
            "a GET with another header",
            Box::new(|tok| {
                raw_get(
                    "GET",
                    &ticket_target(BOARD, EPIC),
                    Some(tok),
                    &["X-Extra: 1"],
                    "",
                )
            }),
            "x-extra",
        ),
        (
            "a GET with no token",
            Box::new(|_| raw_get("GET", &ticket_target(BOARD, EPIC), None, &[], "")),
            "the Authorization header",
        ),
        (
            "a GET with another token",
            Box::new(|_| {
                raw_get(
                    "GET",
                    &ticket_target(BOARD, EPIC),
                    Some("not-the-token"),
                    &[],
                    "",
                )
            }),
            "the Authorization header",
        ),
        (
            "a GET with the token in a header that is not Authorization",
            Box::new(|tok| {
                raw_get(
                    "GET",
                    &ticket_target(BOARD, EPIC),
                    Some(tok),
                    &[&format!("X-Token: {tok}")],
                    "",
                )
            }),
            "the token is in",
        ),
        (
            "a GET with the token in the query",
            Box::new(|tok| {
                raw_get(
                    "GET",
                    &format!("{}&token={tok}", ticket_target(BOARD, EPIC)),
                    Some(tok),
                    &[],
                    "",
                )
            }),
            "the target",
        ),
        (
            "a GET of another path",
            Box::new(|tok| {
                raw_get(
                    "GET",
                    &format!("/boards/{BOARD}?view=ticket&ticket={EPIC}&include_prose=true"),
                    Some(tok),
                    &[],
                    "",
                )
            }),
            "the target",
        ),
        (
            "a GET of the right path with another query",
            Box::new(|tok| {
                raw_get(
                    "GET",
                    &format!("/api/2026-11-10/boards/{BOARD}?view=ticket&ticket={EPIC}"),
                    Some(tok),
                    &[],
                    "",
                )
            }),
            "the target",
        ),
    ];
    let mut problems = Vec::new();
    for (what, text, expect) in plants {
        let standin = standard_standin();
        let status = standin.send_raw(text(standin.token()).as_bytes());
        let found = log_problems(&standin.log(), std::slice::from_ref(&t));
        // No problem the check writes may hold the token, whichever part of a request carried it.
        if found.iter().any(|f| f.contains(standin.token())) {
            problems.push(format!("{what}: a problem the check wrote holds the token"));
        }
        if expect.is_empty() {
            if !found.is_empty() {
                problems.push(format!(
                    "{what}: the check refused a good request: {found:?}"
                ));
            }
        } else {
            if found.is_empty() {
                problems.push(format!("{what}: the check did not see it"));
            } else if !found
                .iter()
                .any(|f| f.to_lowercase().contains(&expect.to_lowercase()))
            {
                problems.push(format!(
                    "{what}: the check's problems do not name {expect:?}: {found:?}"
                ));
            }
            let not_a_get = [
                "a PUT",
                "a PATCH",
                "a DELETE",
                "a HEAD",
                "an OPTIONS",
                "a POST",
            ]
            .iter()
            .any(|m| what.starts_with(m));
            if not_a_get && !status.contains("405") {
                problems.push(format!("{what}: the stand-in answered {status:?}, not 405"));
            }
        }
    }
    assert_no_problems(problems);
}

#[test]
fn the_check_that_no_board_was_read_sees_a_read() {
    // The order tests rest on `assert_no_request`. A planted read, a line that is good, is seen.
    let rig = Rig::new();
    let _ = rig.board_create(&["--ttl", "8"]);
    assert_no_request(&rig.standin);
    let _ = rig.board_create(&[]);
    let seen = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_no_request(&rig.standin);
    }));
    assert!(
        seen.is_err(),
        "The check did not see a request that was made"
    );
}

// ---- Contract v1 and v0 are kept ------------------------------------------------------------------------------------

fn both_versions(schema: &str, doc: &Value) -> Vec<String> {
    let mut p: Vec<String> = schema_problems(schema, doc)
        .into_iter()
        .map(|e| format!("v2: {e}"))
        .collect();
    p.extend(
        schema_problems(&format!("v1/{schema}"), doc)
            .into_iter()
            .map(|e| format!("v1: {e}")),
    );
    p.extend(
        schema_problems(&format!("v0/{schema}"), doc)
            .into_iter()
            .map(|e| format!("v0: {e}")),
    );
    p
}

fn v2_and_v1(schema: &str, doc: &Value) -> Vec<String> {
    let mut p: Vec<String> = schema_problems(schema, doc)
        .into_iter()
        .map(|e| format!("v2: {e}"))
        .collect();
    p.extend(
        schema_problems(&format!("v1/{schema}"), doc)
            .into_iter()
            .map(|e| format!("v1: {e}")),
    );
    p
}

#[test]
fn every_document_of_a_line_with_none_of_the_new_flags_validates_against_v2_v1_and_v0() {
    // lease-api.md, Versioning, "What v2 keeps of v1" and Schemas: every document a command line
    // with none of --board, --epic and --turn prints validates against v1's schema, and one with no
    // --plan against v0's too; v2's conformance MUST show it. For create, get, renew, release and
    // list, and for the errors they fail with.
    let rig = Rig::new();
    let lab = &rig.lab;
    let created = lab.run(&["lease", "create", "--project", "swim"]);
    let created = created.success();
    let id = |d: &Value| d["id"].as_str().unwrap().to_string();
    let pending = {
        rig.h
            .script_place(Ok(agentspace_driver_api::Placed::Pending));
        lab.run(&["lease", "create", "--project", "swim", "--size", "l"])
            .success()
    };
    let unsat = {
        rig.h
            .script_place(Ok(agentspace_driver_api::Placed::Unsatisfiable {
                reasons: vec![fake_driver::reason(
                    "unsupported_size",
                    "Size 'xxl' is not supported; v0 supports sizes s, m, l and xl.",
                )],
            }));
        lab.run(&["lease", "create", "--project", "swim", "--size", "xxl"])
            .success()
    };
    let short = lab
        .run(&["lease", "create", "--project", "swim", "--ttl", "1m"])
        .success();
    let mut docs = vec![
        created.clone(),
        pending.clone(),
        unsat.clone(),
        short.clone(),
    ];
    for d in [&created, &pending, &unsat] {
        docs.push(lab.run(&["lease", "get", &id(d)]).success());
    }
    docs.push(
        lab.run(&["lease", "renew", &id(&created), "--ttl", "1h"])
            .success(),
    );
    let list = lab.run(&["lease", "list"]).success();
    let by_state = lab
        .run(&["lease", "list", "--state", "unsatisfiable"])
        .success();
    let by_project = lab.run(&["lease", "list", "--project", "other"]).success();
    for d in [&pending, &unsat, &created] {
        docs.push(lab.run(&["lease", "release", &id(d)]).success());
    }
    let mut problems = Vec::new();
    for (i, d) in docs.iter().enumerate() {
        for p in both_versions("lease.output.schema.json", d) {
            problems.push(format!("lease document {i}: {p}"));
        }
        if d.get("sized_for").is_some() {
            problems.push(format!("lease document {i}: sized_for without a plan"));
        }
    }
    for (what, d) in [
        ("list", &list),
        ("list by state", &by_state),
        ("list by project", &by_project),
    ] {
        for p in both_versions("lease-list.output.schema.json", d) {
            problems.push(format!("{what}: {p}"));
        }
    }
    // The errors, each with a field the contract v0 had, or none.
    let unknown = "0c7f2b5e-8a64-4f19-b3d2-6e1a9c4d8f07";
    let released = id(&created);
    let errors: Vec<(&str, Vec<&str>)> = vec![
        ("no subcommand", vec!["lease"]),
        ("no project", vec!["lease", "create"]),
        (
            "a flag with no value",
            vec!["lease", "create", "--project", "x", "--ttl"],
        ),
        ("no id", vec!["lease", "get"]),
        ("an unknown option", vec!["lease", "list", "--bogus"]),
        ("a bad id", vec!["lease", "get", "nope"]),
        (
            "a bad size",
            vec!["lease", "create", "--project", "x", "--size", "??"],
        ),
        ("a bad state", vec!["lease", "list", "--state", "nope"]),
        (
            "a ttl out of range",
            vec!["lease", "create", "--project", "x", "--ttl", "48h"],
        ),
        ("no such lease", vec!["lease", "get", unknown]),
        ("release of none", vec!["lease", "release", unknown]),
        (
            "renew of a released lease",
            vec!["lease", "renew", &released],
        ),
    ];
    for (what, line) in errors {
        let ran = lab.run(&line);
        match serde_json::from_slice::<Value>(ran.stderr.trim_ascii_end()) {
            Ok(doc) => {
                for p in both_versions("error.output.schema.json", &doc) {
                    problems.push(format!("{what}: {p}"));
                }
            }
            Err(e) => problems.push(format!("{what}: not JSON: {e}")),
        }
    }
    assert_no_problems(problems);
}

#[test]
fn a_plan_lease_validates_against_v2_and_v1_and_not_v0_and_so_does_a_board_lease() {
    // lease-api.md, Versioning: "every lease and list document v2 prints validates against v1's
    // schemas, whatever made the lease"; a lease with sized_for fails v0's. A plan with a
    // budget_minutes, which v1 refused, still prints a v1 lease.
    let rig = Rig::new();
    let file = rig.plan_file(&plan_a());
    let tiny = json!({"turns": [turn("T1", None, vec![wide("S", 2)])]});
    std::fs::write(rig.lab.cwd.join("t.json"), compact(&tiny)).unwrap();
    let docs = vec![
        (
            "--plan, no budget",
            rig.create(&["--plan", "t.json"]).lease_doc(),
        ),
        (
            "--plan, budgets",
            rig.create(&["--plan", &file]).lease_doc(),
        ),
        (
            "--plan --turn",
            rig.create(&["--plan", &file, "--turn", "T03"]).lease_doc(),
        ),
        (
            "--plan --ttl",
            rig.create(&["--plan", &file, "--ttl", "2h"]).lease_doc(),
        ),
        ("a board lease", rig.board_create(&[]).lease_doc()),
        (
            "a board lease --turn",
            rig.board_create(&["--turn", "T01"]).lease_doc(),
        ),
        (
            "a board lease --ttl",
            rig.board_create(&["--ttl", "90m"]).lease_doc(),
        ),
    ];
    let mut problems = Vec::new();
    for (what, d) in &docs {
        for p in v2_and_v1("lease.output.schema.json", d) {
            problems.push(format!("{what}: {p}"));
        }
        if schema_problems("v0/lease.output.schema.json", d).is_empty() {
            problems.push(format!(
                "{what}: v0's schema accepts a lease with sized_for"
            ));
        }
        let got = rig.lab.run(&["lease", "get", d["id"].as_str().unwrap()]);
        for p in v2_and_v1("lease.output.schema.json", &got.success()) {
            problems.push(format!("{what}, get: {p}"));
        }
    }
    let list = rig.lab.run(&["lease", "list"]).success();
    for p in v2_and_v1("lease-list.output.schema.json", &list) {
        problems.push(format!("list: {p}"));
    }
    assert_no_problems(problems);
}

#[test]
fn an_error_naming_board_epic_or_turn_validates_against_v2_and_fails_v1() {
    // lease-api.md, Schemas: "v2's conformance MUST show that an error naming board, epic or turn
    // validates against v2's error schema and fails v1's, so a validator that accepted everything
    // could not pass the check above." Each error is the one the CLI prints, for each field and for
    // both `usage` and `invalid_value`.
    let rig = Rig::new();
    let lines: Vec<(&str, Vec<&str>, &str, &str)> = vec![
        (
            "usage board",
            vec!["--board", BOARD, "--plan", "p.json"],
            "usage",
            "board",
        ),
        ("usage epic", vec!["--board", BOARD], "usage", "epic"),
        ("usage turn", vec!["--turn", "T1"], "usage", "turn"),
        (
            "board",
            vec!["--board", "nope", "--epic", "52"],
            "invalid_value",
            "board",
        ),
        (
            "epic",
            vec!["--board", BOARD, "--epic", "x"],
            "invalid_value",
            "epic",
        ),
        (
            "turn",
            vec!["--board", BOARD, "--epic", "52", "--turn", " x"],
            "invalid_value",
            "turn",
        ),
        (
            "an epic the board refuses",
            vec!["--board", BOARD, "--epic", "99"],
            "invalid_value",
            "epic",
        ),
        (
            "a turn the plan lacks",
            vec!["--board", BOARD, "--epic", "52", "--turn", "T99"],
            "invalid_value",
            "turn",
        ),
    ];
    let mut problems = Vec::new();
    for (what, line, code, field) in lines {
        let ran = rig.create(&line);
        let doc = match serde_json::from_slice::<Value>(ran.stderr.trim_ascii_end()) {
            Ok(d) => d,
            Err(e) => {
                problems.push(format!("{what}: not JSON: {e}"));
                continue;
            }
        };
        if doc["code"] != code || doc["field"] != field {
            problems.push(format!(
                "{what}: the error is {} for {}",
                doc["code"], doc["field"]
            ));
        }
        for p in schema_problems("error.output.schema.json", &doc) {
            problems.push(format!("{what}: v2: {p}"));
        }
        if schema_problems("v1/error.output.schema.json", &doc).is_empty() {
            problems.push(format!("{what}: v1's error schema accepts it"));
        }
    }
    assert_no_problems(problems);
}

// ---- The built program, with the real DPAPI, in a moved home -----------------------------------------------------

/// The lease document a successful run of the built program printed, checked against v2's schema.
#[track_caller]
fn run_doc(run: &Run) -> Value {
    assert_eq!(
        run.status,
        Some(0),
        "Exit {:?}, stderr {:?}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(run.stderr.is_empty());
    let text = std::str::from_utf8(&run.stdout).expect("Stdout is UTF-8");
    assert!(text.ends_with('\n') && text.matches('\n').count() == 1);
    let doc: Value = serde_json::from_str(text.trim_end()).expect("Stdout is one JSON document");
    for schema in ["lease.output.schema.json", "v1/lease.output.schema.json"] {
        let p = schema_problems(schema, &doc);
        assert!(p.is_empty(), "{schema}: {p:?}");
    }
    doc
}

#[test]
fn the_built_program_decrypts_the_credential_in_a_moved_home_and_makes_a_board_lease() {
    // The program under test reads bot.json with the real DPAPI, as the process it is. Its
    // USERPROFILE, HOME, LOCALAPPDATA and APPDATA are all in the throwaway home (`Command::env`),
    // TMP and TEMP are kept, and the project is not on the project list, so that the lease is
    // `unsatisfiable` with `no_capacity` and needs no CA, no host and no run (conformance.md, What
    // v0.11 changes). If DPAPI could not decrypt under the moved home, the run would fail with the
    // credential's message, and that is the report (N-something), never worked around here.
    let standin = standard_standin();
    let bin = Bin::new();
    bin.credential_for(&standin);
    let run = bin.run(&[
        "lease",
        "create",
        "--project",
        "swim",
        "--board",
        BOARD,
        "--epic",
        "52",
        "--turn",
        "T01",
    ]);
    let guard = |bytes: &[u8]| {
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains(standin.token()), "The token is in an output");
        for m in MARKERS {
            assert!(!text.contains(m), "The marker {m} is in an output");
        }
    };
    guard(&run.stdout);
    guard(&run.stderr);
    let doc = run_doc(&run);
    assert_eq!(doc["state"], "unsatisfiable");
    assert_eq!(doc["reasons"][0]["code"], "no_capacity");
    assert_eq!(doc["size"], "m");
    assert_eq!(
        doc["sized_for"],
        json!({"plan": PLAN_NAME, "turn": "T01", "agents": 5})
    );
    assert_eq!(doc["ttl_seconds"], 16_200);
    assert_log(&standin, &[ticket_target(BOARD, EPIC)]);
    // The record, in the home's own state folder.
    let state = bin.home.join(".agentspace").join("leases.json");
    let text = std::fs::read_to_string(&state).expect("The record is in the throwaway home");
    let file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        file["leases"][0]["plan_source"],
        json!({"board": BOARD, "ticket": 52, "revision": 7})
    );
    assert_eq!(file["leases"][0]["plan"], plan_a());
    assert!(
        schema_problems("state/leases.state.schema.json", &file).is_empty(),
        "The state file breaks its schema"
    );
    let hits = files_holding(&bin.home, standin.token());
    assert!(hits.is_empty(), "{} files hold the token", hits.len());
    for word in ["127.0.0.1", "http://", "Bearer"] {
        assert!(!text.contains(word), "The state file holds {word}");
    }
    // `get` and `list` of the lease, and a `--plan` lease of the same plan, from the program.
    let id = doc["id"].as_str().unwrap();
    let got = run_doc(&bin.run(&["lease", "get", id]));
    assert_eq!(got, doc);
    std::fs::write(bin.home.join("p.json"), compact(&plan_a())).unwrap();
    let from_file = run_doc(&bin.run(&[
        "lease",
        "create",
        "--project",
        "swim",
        "--plan",
        "p.json",
        "--turn",
        "T01",
    ]));
    // The two creates ran the built program at two moments of the real clock, which may fall on
    // either side of a second, so their times are left out; `ttl_seconds` is still compared.
    let untimed = |doc: Value| {
        let mut doc = same_lease(doc);
        let o = doc.as_object_mut().expect("A lease is an object");
        o.remove("created_at");
        o.remove("expires_at");
        doc
    };
    assert_eq!(untimed(from_file), untimed(doc));
    assert_eq!(standin.count(), 1, "Only the one create read the board");
}

#[test]
fn the_built_program_never_goes_through_a_proxy_whatever_the_environment_says() {
    // lease-api.md, The board read: "The connection is made to 127.0.0.1, at the base's port,
    // directly, never through a proxy, whatever the environment says: HTTP_PROXY, HTTPS_PROXY,
    // ALL_PROXY and NO_PROXY, in any case, change nothing." A second stand-in is every proxy.
    let board = standard_standin();
    let proxy = standard_standin();
    let url = format!("http://127.0.0.1:{}", proxy.port);
    let bin = Bin::new();
    bin.credential_for(&board);
    let mut cmd = bin.command(&[
        "lease",
        "create",
        "--project",
        "swim",
        "--board",
        BOARD,
        "--epic",
        "52",
    ]);
    for name in [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        cmd.env(name, &url);
    }
    for name in ["NO_PROXY", "no_proxy"] {
        cmd.env(name, "example.invalid");
    }
    let doc = run_doc(&run_to_end(cmd));
    assert_eq!(doc["state"], "unsatisfiable");
    assert_log(&board, &[ticket_target(BOARD, EPIC)]);
    assert_no_request(&proxy);
}

#[test]
fn the_built_program_reads_no_variable_of_localswims_and_none_of_its_own_for_the_board() {
    // conformance.md, The stand-in credential: "a case sets LOCALSWIM_URL to another loopback port,
    // where nothing listens, and LOCALSWIM_TOKEN to another value, and the read still reaches the
    // stand-in with the credential's token", and no AGENTSPACE_* variable names the receipt.
    let board = standard_standin();
    let bin = Bin::new();
    bin.credential_for(&board);
    let dead = {
        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        l.local_addr().unwrap().port()
    };
    // No test connects to 127.0.0.1:8811, Terry's board (#0238).
    assert_ne!(dead, 8811);
    let mut cmd = bin.command(&[
        "lease",
        "create",
        "--project",
        "swim",
        "--board",
        BOARD,
        "--epic",
        "52",
    ]);
    cmd.env("LOCALSWIM_URL", format!("http://127.0.0.1:{dead}"))
        .env(
            "LOCALSWIM_TOKEN",
            "NOT-THE-STAND-INS-TOKEN-0123456789ABCDEFG",
        )
        .env("AGENTSPACE_RECEIPT", bin.home.join("none.json"))
        .env(
            "AGENTSPACE_LOCALSWIM_URL",
            format!("http://127.0.0.1:{dead}"),
        );
    let doc = run_doc(&run_to_end(cmd));
    assert_eq!(doc["state"], "unsatisfiable");
    assert_log(&board, &[ticket_target(BOARD, EPIC)]);
}

#[test]
fn the_built_program_refuses_a_bad_epic_and_a_board_that_does_not_answer_and_makes_nothing() {
    // Each refusal as the built program: its exit code, its one error document, nothing on stdout,
    // and nothing made in the home (lease-api.md, "Every one records no lease, and creates and
    // changes nothing").
    let mut problems = Vec::new();
    let line = |epic: &str| -> Vec<String> {
        [
            "lease",
            "create",
            "--project",
            "swim",
            "--board",
            BOARD,
            "--epic",
            epic,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    };
    type Refusal = (
        &'static str,
        Option<Hook>,
        Vec<String>,
        &'static str,
        Option<&'static str>,
        String,
    );
    let cases: Vec<Refusal> = vec![
        (
            "an unknown ticket",
            None,
            line("99"),
            "invalid_value",
            Some("epic"),
            msg_no_card("99", BOARD),
        ),
        (
            "a ticket outside its form",
            None,
            line("#52"),
            "invalid_value",
            Some("epic"),
            msg_bad_epic("#52"),
        ),
        (
            "a board that answers 500",
            Some(always(problem(500, "internal_error"))),
            line("52"),
            "internal",
            None,
            String::new(),
        ),
        (
            "a board that refuses the credential",
            Some(always(problem(401, "unauthorized"))),
            line("52"),
            "internal",
            None,
            MSG_REFUSED.to_string(),
        ),
        (
            "a description with no block",
            Some(on_ticket(json200(answer_body(
                "7",
                "false",
                "\"no plan here\"",
            )))),
            line("52"),
            "invalid_value",
            Some("epic"),
            msg_no_usable_plan("52", WHY_NO_BLOCK),
        ),
        (
            "a 200 that is not a ticket view",
            Some(on_ticket(json200("[]"))),
            line("52"),
            "internal",
            None,
            MSG_UNREADABLE_ANSWER.to_string(),
        ),
    ];
    for (what, hook, line, code, field, message) in cases {
        let mut fx = Fixture {
            token: new_token(),
            boards: BTreeMap::new(),
        };
        fx.boards.insert(
            BOARD.to_string(),
            BTreeMap::from([(EPIC, CardFx::good(REVISION, &plan_description(&plan_a())))]),
        );
        let standin = Standin::start(fx, hook);
        let bin = Bin::new();
        bin.credential_for(&standin);
        let refs: Vec<&str> = line.iter().map(String::as_str).collect();
        let run = bin.run(&refs);
        let message = if message.is_empty() {
            msg_no_answer(standin.port)
        } else {
            message
        };
        for p in failure_problems(&run, code, field, Some(&message)) {
            problems.push(format!("{what}: {p}"));
        }
        let text = String::from_utf8_lossy(&run.stderr).into_owned();
        if text.contains(standin.token()) || MARKERS.iter().any(|m| text.contains(m)) {
            problems.push(format!("{what}: a secret or a marker is in stderr"));
        }
        if bin.home.join(".agentspace").exists() {
            problems.push(format!("{what}: the state folder was made"));
        }
        if standin.count() > 2 {
            problems.push(format!("{what}: {} requests", standin.count()));
        }
    }
    assert_no_problems(problems);
}

// ---- A process-level LOCALAPPDATA is never read -----------------------------------------------------------------------

/// What the planted run saw: the requests each stand-in got, and what the child test said.
struct Planted {
    a: Vec<Req>,
    b: Vec<Req>,
    child_exit: Option<i32>,
    child_ran: bool,
}

/// Which LOCALAPPDATA the child's world gives `execute_in`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Given {
    /// A's, the stand-in the world means.
    WorldA,
    /// B's: what a build that read the process environment would use.
    WorldB,
    /// None: the world holds no LOCALAPPDATA at all.
    Nothing,
}

const CHILD: &str = "planted_localappdata_child";

/// The helper test: when the parent started this test executable for it, it runs `create` in
/// process with a `World` whose `var` gives LOCALAPPDATA as the parent said, and prints the exit
/// code. When it is run on its own, it does nothing. It never changes the process's environment.
#[test]
fn planted_localappdata_child() {
    if std::env::var_os("LB_PLANTED_CHILD").is_none() {
        return;
    }
    let (mut lab, _h) = Lab::with_fake();
    install_proxy(&lab);
    if let Some(v) = std::env::var_os("LB_PLANTED_WORLD_LOCALAPPDATA")
        && !v.is_empty()
    {
        lab.set_var("LOCALAPPDATA", v);
    }
    let ran = lab.run(&[
        "lease",
        "create",
        "--project",
        "swim",
        "--board",
        BOARD,
        "--epic",
        "52",
    ]);
    println!("child-exit={}", ran.code);
}

/// Starts this test executable as a child process, filtered to the helper test, with the process's
/// own LOCALAPPDATA set to folder B's and its home variables in a throwaway home, and the world's
/// `var` given as `given` says.
fn run_planted(given: Given) -> Planted {
    let a = standard_standin();
    let b = standard_standin();
    let tmp = tempfile::Builder::new()
        .prefix("aslb-planted-")
        .tempdir()
        .expect("A throwaway folder can be made");
    let write_for = |name: &str, s: &Standin| -> PathBuf {
        let local = tmp.path().join(name).join("AppData").join("Local");
        let creds = local.join("localswim-v2").join("credentials");
        write_text(
            &local.join("localswim-v2").join("service.json"),
            &receipt_text(&s.base(), &creds),
        );
        write_text(&creds.join("bot.json"), &bot_text_for(s.token()));
        local
    };
    let (a_local, b_local) = (write_for("a", &a), write_for("b", &b));
    let home = tmp.path().join("home");
    std::fs::create_dir_all(home.join("AppData").join("Roaming")).unwrap();
    let exe = std::env::current_exe().expect("The test executable has a path");
    let mut cmd = Command::new(exe);
    cmd.args([CHILD, "--exact", "--nocapture", "--test-threads=1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LB_PLANTED_CHILD", "1")
        .env("LOCALAPPDATA", &b_local)
        .env("USERPROFILE", &home)
        .env("HOME", &home)
        .env("APPDATA", home.join("AppData").join("Roaming"));
    match given {
        Given::WorldA => cmd.env("LB_PLANTED_WORLD_LOCALAPPDATA", &a_local),
        Given::WorldB => cmd.env("LB_PLANTED_WORLD_LOCALAPPDATA", &b_local),
        Given::Nothing => cmd.env("LB_PLANTED_WORLD_LOCALAPPDATA", ""),
    };
    for var in [
        "AGENTSPACE_STATE_DIR",
        "AGENTSPACE_SSH_DIR",
        "AGENTSPACE_PROXY_COMMAND",
        "AGENTSPACE_PROVISION_CMD",
    ] {
        cmd.env_remove(var);
    }
    let out = run_to_end(cmd);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    // With --nocapture the helper's line follows the harness's own on one line.
    let child_exit = text
        .lines()
        .find_map(|l| l.split("child-exit=").nth(1))
        .and_then(|n| n.trim().parse().ok());
    let child_ran = child_exit.is_some() && text.contains("1 passed");
    Planted {
        a: a.log(),
        b: b.log(),
        child_exit,
        child_ran,
    }
}

/// The problems with a planted run that was meant to read A's board and never B's: the helper test
/// ran, A's stand-in got the one GET with the token, and B's got none.
fn planted_problems(p: &Planted) -> Vec<String> {
    let mut problems = Vec::new();
    if !p.child_ran {
        problems.push("The helper test did not run as a child, or did not pass".to_string());
    }
    if p.child_exit != Some(0) {
        problems.push(format!(
            "The child's create exited {:?}, not 0",
            p.child_exit
        ));
    }
    problems.extend(
        log_problems(&p.a, &[ticket_target(BOARD, EPIC)])
            .into_iter()
            .map(|e| format!("A's stand-in: {e}")),
    );
    if !p.b.is_empty() {
        problems.push(format!(
            "B's stand-in got {} requests, and MUST get none",
            p.b.len()
        ));
    }
    problems
}

#[test]
fn a_localappdata_of_the_process_is_never_read_the_worlds_is() {
    // The architect's ruling for E0.11-T03-S01: this test executable is run again as a child with
    // `Command::env("LOCALAPPDATA", <folder B>)`, filtered to the helper test, which runs `create`
    // in process with a World whose `var` gives LOCALAPPDATA as folder A. B's receipt points at a
    // second stand-in. It passes only when A's stand-in got the GET and B's got none. Nothing sets
    // an environment variable of this process.
    let planted = run_planted(Given::WorldA);
    assert_no_problems(planted_problems(&planted));
}

#[test]
fn a_world_with_no_localappdata_is_unset_whatever_the_process_has() {
    // The world's `var` is the program's whole environment: with none given the receipt cannot be
    // found, though the process's LOCALAPPDATA names folder B, whose receipt is good.
    let planted = run_planted(Given::Nothing);
    assert!(planted.child_ran, "The helper test did not run as a child");
    assert_eq!(
        planted.child_exit,
        Some(1),
        "The child's create MUST fail: LOCALAPPDATA is unset"
    );
    assert!(planted.a.is_empty(), "A's stand-in was read");
    assert!(
        planted.b.is_empty(),
        "B's stand-in was read: the process's LOCALAPPDATA was used"
    );
}

#[test]
fn the_planted_localappdata_check_fails_for_a_build_that_reads_the_process_environment() {
    // Proof that the check above can fail: a child whose world gives folder B, as a build that read
    // the process's LOCALAPPDATA would behave, reaches B's stand-in and not A's, and the check
    // reports it.
    let planted = run_planted(Given::WorldB);
    assert!(planted.child_ran);
    let problems = planted_problems(&planted);
    assert!(!problems.is_empty(), "The check passed a run that read B");
    assert!(problems.iter().any(|p| p.contains("B's stand-in got")));
    assert!(problems.iter().any(|p| p.contains("A's stand-in")));
}

#[test]
fn the_receipt_and_the_credential_are_read_afresh_for_each_create_and_nothing_is_kept() {
    // lease-api.md, The board read: the receipt "is read afresh for each create", the credential is
    // "dropped once the read ends, whatever its outcome", and "Nothing read from a board is kept for
    // a later command, except what the lease's record keeps".
    let rig = Rig::new();
    rig.board_create(&[]).lease_doc();
    assert_eq!(rig.standin.count(), 1);
    let good_bot = std::fs::read_to_string(rig.bot_path()).unwrap();
    let good_receipt = std::fs::read_to_string(rig.receipt_path()).unwrap();
    std::fs::write(rig.bot_path(), "nope").unwrap();
    let ran = rig.board_create(&[]);
    assert_no_problems(internal_problems(&rig, &ran, MSG_BAD_CREDENTIAL));
    std::fs::write(rig.bot_path(), &good_bot).unwrap();
    std::fs::remove_file(rig.receipt_path()).unwrap();
    let ran = rig.board_create(&[]);
    assert_no_problems(internal_problems(&rig, &ran, MSG_NO_RECEIPT));
    std::fs::write(rig.receipt_path(), &good_receipt).unwrap();
    rig.board_create(&[]).lease_doc();
    assert_eq!(
        rig.standin.count(),
        2,
        "Each good create read the board once"
    );
    // Two creates in a row each read the card, once.
    let first = rig.board_create(&["--turn", "T01"]).lease_doc();
    let second = rig.board_create(&["--turn", "T01"]).lease_doc();
    assert_eq!(same_lease(first), same_lease(second));
    assert_eq!(rig.standin.count(), 4);
}
