//! The client's unit tests. Every board here is a stand-in: a scripted [`Get`], or a listener on a
//! port the test picked on 127.0.0.1. No test reads a real receipt or credential, and no token, a
//! planted one included, is printed: a request's head is compared, never shown.

use super::*;
use agentspace_push::Zeroizing;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

const BOARD: &str = "0f1e2d3c-4b5a-4968-8776-655443322110";

/// A planted token, 43 characters of unpadded base64url.
const PLANTED: &str = "planted_TOKEN-0123456789abcdefghijklmnopqrs";

// ----- The texts, from lease-api.md's "Each outcome" table, word for word -----

#[test]
fn internal_texts_are_the_tables() {
    assert_eq!(
        NO_LOCAL_APP_DATA,
        "LOCALAPPDATA is unset or empty, so localswim's service receipt cannot be found"
    );
    assert_eq!(
        NO_RECEIPT,
        "localswim's service receipt was not found; localswim may not be running"
    );
    assert_eq!(BAD_RECEIPT, "localswim's service receipt could not be read");
    assert_eq!(BAD_CREDENTIAL, "The board's credential could not be read");
    assert_eq!(BAD_ANSWER, "The board's answer could not be read");
    assert_eq!(REFUSED, "The board refused agentspace's credential");
    assert_eq!(
        no_answer_text("4321"),
        "The board at 127.0.0.1:4321 did not answer"
    );
    assert_eq!(API_PREFIX, "/api/2026-11-10");
    assert_eq!(BODY_LIMIT, 64 * 1024 * 1024);
}

// ----- The receipt and the credential -----

/// Files from a map; anything not in it is `Missing`. Records every path read.
#[derive(Default)]
struct MapFiles {
    files: HashMap<PathBuf, FileRead>,
    reads: RefCell<Vec<PathBuf>>,
}

impl Files for MapFiles {
    fn read(&self, path: &Path) -> FileRead {
        self.reads.borrow_mut().push(path.to_path_buf());
        self.files.get(path).cloned().unwrap_or(FileRead::Missing)
    }
    fn write_stamp(&self, _path: &Path, _bytes: &[u8]) -> bool {
        false
    }
}

/// An `Unprotect` that gives the planted token in UTF-16LE, or fails.
struct FakeUnprotect(bool);

impl Unprotect for FakeUnprotect {
    fn unprotect(&self, _blob: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
        self.0
            .then(|| Zeroizing::new(PLANTED.encode_utf16().flat_map(u16::to_le_bytes).collect()))
    }
}

const CREDENTIALS: &str = "throwaway-credentials";
const BOT_JSON: &str =
    r#"{"format":"localswim-credential","format_version":1,"role":"bot","token_dpapi_hex":"0a0b"}"#;

fn ctx(local_app_data: Option<&str>) -> Context {
    Context {
        home: PathBuf::from("throwaway-home"),
        local_app_data: local_app_data.map(PathBuf::from),
        system_root: PathBuf::from("throwaway-root"),
        nested: false,
    }
}

fn receipt_text(base: Option<&str>) -> String {
    let mut object = serde_json::Map::new();
    if let Some(base) = base {
        object.insert("base".into(), Value::String(base.into()));
    }
    object.insert(
        "credential_directory".into(),
        Value::String(CREDENTIALS.into()),
    );
    Value::Object(object).to_string()
}

fn files_with(receipt: Option<FileRead>, bot: Option<FileRead>) -> MapFiles {
    let mut files = MapFiles::default();
    let receipt_at = receipt_path(&ctx(Some("throwaway-local"))).expect("Path");
    if let Some(r) = receipt {
        files.files.insert(receipt_at, r);
    }
    if let Some(b) = bot {
        files.files.insert(credential_path(CREDENTIALS), b);
    }
    files
}

fn bytes(text: &str) -> FileRead {
    FileRead::Bytes(text.as_bytes().to_vec())
}

fn good_files() -> MapFiles {
    files_with(
        Some(bytes(&receipt_text(Some("http://127.0.0.1:4321")))),
        Some(bytes(BOT_JSON)),
    )
}

fn found(files: &MapFiles, unprotect: bool, local: Option<&str>) -> Result<String, BoardError> {
    find_board(&ctx(local), files, &FakeUnprotect(unprotect)).map(|(port, bearer)| {
        // Compared, never shown.
        assert!(
            bearer.as_str() == format!("Bearer {PLANTED}"),
            "The bearer is not `Bearer <token>`"
        );
        port
    })
}

#[test]
fn a_good_receipt_and_credential_give_the_port_and_the_bearer() {
    assert_eq!(
        found(&good_files(), true, Some("throwaway-local")),
        Ok("4321".to_owned())
    );
}

#[test]
fn local_app_data_unset_empty_or_whitespace_is_the_first_row() {
    for local in [None, Some(""), Some("   ")] {
        let files = good_files();
        assert_eq!(
            found(&files, true, local),
            Err(internal(NO_LOCAL_APP_DATA)),
            "{local:?}"
        );
        assert!(files.reads.borrow().is_empty(), "Nothing is read");
    }
}

#[test]
fn a_missing_receipt_is_not_found_and_an_unreadable_one_is_not_read() {
    let files = files_with(None, None);
    assert_eq!(
        found(&files, true, Some("throwaway-local")),
        Err(internal(NO_RECEIPT))
    );
    let files = files_with(Some(FileRead::Unreadable), Some(bytes(BOT_JSON)));
    assert_eq!(
        found(&files, true, Some("throwaway-local")),
        Err(internal(BAD_RECEIPT))
    );
}

#[test]
fn a_receipt_that_breaks_the_rules_or_lacks_its_base_is_refused() {
    for receipt in [
        "not json".to_owned(),
        "[]".to_owned(),
        receipt_text(None),
        receipt_text(Some("https://127.0.0.1:4321")),
        receipt_text(Some("http://localhost:4321")),
        receipt_text(Some("http://127.0.0.1")),
        receipt_text(Some("http://127.0.0.1:4321/")),
        receipt_text(Some("http://127.0.0.1:0")),
    ] {
        let files = files_with(Some(bytes(&receipt)), Some(bytes(BOT_JSON)));
        assert_eq!(
            found(&files, true, Some("throwaway-local")),
            Err(internal(BAD_RECEIPT)),
            "{receipt}"
        );
        assert_eq!(files.reads.borrow().len(), 1, "bot.json is not read");
    }
}

#[test]
fn a_receipt_not_in_utf8_is_refused_and_a_utf8_mark_is_accepted() {
    let text = receipt_text(Some("http://127.0.0.1:4321"));
    let mut marked = vec![0xEF, 0xBB, 0xBF];
    marked.extend(text.as_bytes());
    let files = files_with(Some(FileRead::Bytes(marked)), Some(bytes(BOT_JSON)));
    assert_eq!(
        found(&files, true, Some("throwaway-local")),
        Ok("4321".to_owned())
    );
    let utf16 = |mark: [u8; 2], big: bool| {
        let mut out = mark.to_vec();
        for unit in text.encode_utf16() {
            out.extend(if big {
                unit.to_be_bytes()
            } else {
                unit.to_le_bytes()
            });
        }
        out
    };
    for receipt in [utf16([0xFF, 0xFE], false), utf16([0xFE, 0xFF], true)] {
        let files = files_with(Some(FileRead::Bytes(receipt)), Some(bytes(BOT_JSON)));
        assert_eq!(
            found(&files, true, Some("throwaway-local")),
            Err(internal(BAD_RECEIPT))
        );
    }
}

#[test]
fn a_credential_missing_unreadable_bad_or_not_decrypted_is_refused() {
    let receipt = || Some(bytes(&receipt_text(Some("http://127.0.0.1:4321"))));
    for (bot, unprotect) in [
        (None, true),
        (Some(FileRead::Unreadable), true),
        (Some(bytes("{}")), true),
        (Some(bytes(&BOT_JSON.replace("bot", "human"))), true),
        (Some(bytes(BOT_JSON)), false),
    ] {
        let files = files_with(receipt(), bot);
        assert_eq!(
            found(&files, unprotect, Some("throwaway-local")),
            Err(internal(BAD_CREDENTIAL))
        );
    }
}

#[test]
fn a_value_the_cli_never_passes_reads_nothing() {
    for board in [
        "",
        "0F1E2D3C-4B5A-4968-8776-655443322110",
        "0f1e2d3c4b5a49688776655443322110",
        "0f1e2d3c-4b5a-4968-8776-65544332211/",
        "0f1e2d3c-4b5a-4968-8776-655443322110?x",
        "../../x",
    ] {
        let files = good_files();
        let got = read_epic(
            &ctx(Some("throwaway-local")),
            &files,
            &FakeUnprotect(true),
            board,
            52,
        );
        assert_eq!(got, Err(BoardError::UnknownBoard), "{board}");
        assert!(files.reads.borrow().is_empty(), "Nothing is read");
    }
    for ticket in [0, MAX_INTEGER + 1, u64::MAX] {
        let files = good_files();
        let got = read_epic(
            &ctx(Some("throwaway-local")),
            &files,
            &FakeUnprotect(true),
            BOARD,
            ticket,
        );
        assert_eq!(got, Err(BoardError::UnknownTicket), "{ticket}");
        assert!(files.reads.borrow().is_empty(), "Nothing is read");
    }
}

// ----- The requests and their outcomes, over a scripted board -----

/// Each GET a scripted board was asked for: its target, and the statuses whose body it reads.
type Asked = Vec<(String, Vec<u16>)>;

/// A scripted board: each GET takes the next reply, and `NoAnswer` once they run out.
struct Script {
    replies: RefCell<VecDeque<Reply>>,
    asked: RefCell<Asked>,
}

impl Script {
    fn new(replies: Vec<Reply>) -> Self {
        Script {
            replies: RefCell::new(replies.into()),
            asked: RefCell::new(Vec::new()),
        }
    }
}

impl Get for Script {
    fn get(&self, target: &str, read_body_of: &[u16]) -> Reply {
        self.asked
            .borrow_mut()
            .push((target.to_owned(), read_body_of.to_vec()));
        self.replies
            .borrow_mut()
            .pop_front()
            .unwrap_or(Reply::NoAnswer)
    }
}

fn ticket_target() -> String {
    format!("/api/2026-11-10/boards/{BOARD}?view=ticket&ticket=52&include_prose=true")
}

fn summary_target() -> String {
    format!("/api/2026-11-10/boards/{BOARD}?view=summary")
}

fn answer(status: u16, problem_json: bool, body: Option<BodyRead>) -> Reply {
    Reply::Answer {
        status,
        problem_json,
        body,
    }
}

fn ok_body(text: &str) -> Reply {
    answer(200, false, Some(BodyRead::Bytes(text.as_bytes().to_vec())))
}

const NOT_FOUND: &str =
    r#"{"type":"about:blank","title":"Not Found","status":404,"code":"not_found","detail":"x"}"#;

fn not_found() -> Reply {
    answer(
        404,
        true,
        Some(BodyRead::Bytes(NOT_FOUND.as_bytes().to_vec())),
    )
}

const GOOD: &str = r#"{"card":{"revision":7,"archived":false,"title":"t","lane":"x"},"prose":{"description":"The plan","comments":[1,2]},"relationships":null,"parent":{"x":1}}"#;

fn run(replies: Vec<Reply>) -> (Result<Epic, BoardError>, Asked) {
    let script = Script::new(replies);
    let got = read_with(&script, "4321", BOARD, 52);
    (got, script.asked.into_inner())
}

fn no_answer() -> Result<Epic, BoardError> {
    Err(BoardError::Internal(no_answer_text("4321")))
}

#[test]
fn a_200_is_read_with_one_get_of_the_ticket_view() {
    let (got, asked) = run(vec![ok_body(GOOD)]);
    assert_eq!(
        got,
        Ok(Epic {
            description: "The plan".into(),
            archived: false,
            revision: 7,
        })
    );
    assert_eq!(asked, vec![(ticket_target(), vec![200, 404])]);
}

#[test]
fn an_archived_card_is_ok_and_archived() {
    let body = GOOD.replace("\"archived\":false", "\"archived\":true");
    let (got, _) = run(vec![ok_body(&body)]);
    assert!(got.is_ok_and(|epic| epic.archived));
}

#[test]
fn not_found_then_200_is_an_unknown_ticket() {
    let (got, asked) = run(vec![not_found(), answer(200, false, None)]);
    assert_eq!(got, Err(BoardError::UnknownTicket));
    assert_eq!(
        asked,
        vec![
            (ticket_target(), vec![200, 404]),
            (summary_target(), vec![404])
        ],
        "The summary view's 200 body is not read"
    );
}

#[test]
fn not_found_twice_is_an_unknown_board() {
    let (got, asked) = run(vec![not_found(), not_found()]);
    assert_eq!(got, Err(BoardError::UnknownBoard));
    assert_eq!(asked.len(), 2);
}

#[test]
fn not_found_then_any_other_summary_outcome_is_that_outcomes_row() {
    for (summary, expected) in [
        (answer(401, false, None), Err(internal(REFUSED))),
        (answer(403, false, None), Err(internal(REFUSED))),
        (answer(500, false, None), no_answer()),
        (answer(302, false, None), no_answer()),
        (answer(204, false, None), no_answer()),
        (Reply::NoAnswer, no_answer()),
        (
            answer(404, false, Some(BodyRead::Bytes(NOT_FOUND.into()))),
            no_answer(),
        ),
        (answer(404, true, Some(BodyRead::TooLarge)), no_answer()),
        (answer(404, true, Some(BodyRead::Failed)), no_answer()),
    ] {
        let (got, asked) = run(vec![not_found(), summary]);
        assert_eq!(got, expected);
        assert_eq!(asked.len(), 2, "No third request");
    }
}

#[test]
fn a_404_that_is_not_localswims_not_found_is_no_answer_with_one_request() {
    let problem = |text: &str| answer(404, true, Some(BodyRead::Bytes(text.into())));
    for first in [
        // The right body, the wrong media type.
        answer(404, false, Some(BodyRead::Bytes(NOT_FOUND.into()))),
        // The right media type, the wrong body.
        problem(r#"{"code":"gone"}"#),
        problem(r#"{"code":"NOT_FOUND"}"#),
        problem(r#"{"code":null}"#),
        problem(r#"[{"code":"not_found"}]"#),
        problem(r#"{"code":"not_found"} x"#),
        problem("not json"),
        problem(""),
        answer(404, true, Some(BodyRead::TooLarge)),
        answer(404, true, Some(BodyRead::Failed)),
    ] {
        let (got, asked) = run(vec![first]);
        assert_eq!(got, no_answer());
        assert_eq!(asked.len(), 1, "No summary request");
    }
}

#[test]
fn each_status_maps_to_its_row() {
    for status in [401, 403] {
        let (got, asked) = run(vec![answer(status, false, None)]);
        assert_eq!(got, Err(internal(REFUSED)), "{status}");
        assert_eq!(asked.len(), 1);
    }
    for status in [
        100, 101, 199, 201, 202, 204, 206, 299, 300, 301, 302, 303, 304, 307, 308, 400, 402, 405,
        406, 409, 410, 418, 429, 499, 500, 502, 503, 599,
    ] {
        let (got, asked) = run(vec![answer(status, false, None)]);
        assert_eq!(got, no_answer(), "{status}");
        assert_eq!(asked.len(), 1);
    }
    let (got, _) = run(vec![Reply::NoAnswer]);
    assert_eq!(got, no_answer());
}

#[test]
fn a_200_too_large_is_too_large_and_one_cut_short_is_no_answer() {
    assert_eq!(
        run(vec![answer(200, false, Some(BodyRead::TooLarge))]).0,
        Err(BoardError::TooLarge)
    );
    assert_eq!(
        run(vec![answer(200, false, Some(BodyRead::Failed))]).0,
        no_answer()
    );
}

// ----- What is read from a 200 -----

#[test]
fn the_three_members_are_read_and_every_other_is_ignored() {
    let body = r#"{"z":{"deep":[{"code":"not_found"}]},"prose":{"comments":"é","description":"D","x":1},"card":{"archived":false,"revision":9007199254740991,"title":null,"priority":[]},"children":[],"related_card":{}}"#;
    assert_eq!(
        parse_ticket(body.as_bytes()),
        Some(Epic {
            description: "D".into(),
            archived: false,
            revision: 9_007_199_254_740_991,
        })
    );
}

#[test]
fn a_revision_that_is_not_digits_from_1_to_the_bound_is_refused() {
    for revision in [
        "5.0",
        "5e0",
        "5E0",
        "-5",
        "0",
        "-0",
        "\"5\"",
        "9007199254740992",
        "18446744073709551616",
        "null",
        "true",
        "[5]",
    ] {
        let body = GOOD.replace("\"revision\":7", &format!("\"revision\":{revision}"));
        assert_ne!(body, GOOD);
        assert_eq!(parse_ticket(body.as_bytes()), None, "{revision}");
    }
    let one = GOOD.replace("\"revision\":7", "\"revision\":1");
    assert_eq!(parse_ticket(one.as_bytes()).map(|e| e.revision), Some(1));
}

#[test]
fn a_body_of_the_wrong_shape_is_refused() {
    for body in [
        "",
        "not json",
        "[]",
        "null",
        "\"x\"",
        &format!("{GOOD} {{}}"),
        &format!("\u{feff}{GOOD}"),
        r#"{"prose":{"description":"D"}}"#,
        r#"{"card":{"revision":1,"archived":false}}"#,
        r#"{"card":[],"prose":{"description":"D"}}"#,
        r#"{"card":{"revision":1,"archived":false},"prose":"D"}"#,
        r#"{"card":{"archived":false},"prose":{"description":"D"}}"#,
        r#"{"card":{"revision":1},"prose":{"description":"D"}}"#,
        r#"{"card":{"revision":1,"archived":"false"},"prose":{"description":"D"}}"#,
        r#"{"card":{"revision":1,"archived":0},"prose":{"description":"D"}}"#,
        r#"{"card":{"revision":1,"archived":false},"prose":{}}"#,
        r#"{"card":{"revision":1,"archived":false},"prose":{"description":null}}"#,
        r#"{"card":{"revision":1,"archived":false},"prose":{"description":["D"]}}"#,
        // An unpaired surrogate refuses the answer whole, the description's and any other's.
        r#"{"card":{"revision":1,"archived":false},"prose":{"description":"\ud800"}}"#,
        r#"{"card":{"revision":1,"archived":false},"prose":{"description":"D"},"x":"\udc00"}"#,
    ] {
        assert_eq!(parse_ticket(body.as_bytes()), None, "{body}");
        let (got, _) = run(vec![ok_body(body)]);
        assert_eq!(got, Err(internal(BAD_ANSWER)), "{body}");
    }
    // Invalid UTF-8 is refused.
    let mut bad = GOOD.as_bytes().to_vec();
    bad.insert(bad.len() - 3, 0xFF);
    assert_eq!(parse_ticket(&bad), None);
}

// ----- The media type and the body bound -----

#[test]
fn the_problem_media_type_ignores_case_and_parameters() {
    for yes in [
        "application/problem+json",
        "Application/Problem+JSON; charset=utf-8",
        "APPLICATION/PROBLEM+JSON;charset=UTF-8",
        " application/problem+json ",
    ] {
        assert!(is_problem_json(yes), "{yes}");
    }
    for no in [
        "application/json",
        "application/problem+jsonx",
        "application/problem",
        "text/plain; application/problem+json",
        "",
    ] {
        assert!(!is_problem_json(no), "{no}");
    }
}

/// A reader of endless zeros that counts what it gives, or an error after `fail_after` bytes.
struct Counting<'a> {
    given: &'a std::cell::Cell<u64>,
    fail_after: Option<u64>,
    end_after: Option<u64>,
}

impl Read for Counting<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let given = self.given.get();
        if self.fail_after.is_some_and(|n| given >= n) {
            return Err(std::io::Error::other("Cut short"));
        }
        let room = self.end_after.map_or(u64::MAX, |n| n - given);
        let n = buf.len().min(usize::try_from(room).unwrap_or(usize::MAX));
        buf[..n].fill(0);
        self.given.set(given + n as u64);
        Ok(n)
    }
}

fn counting(given: &std::cell::Cell<u64>) -> Counting<'_> {
    Counting {
        given,
        fail_after: None,
        end_after: None,
    }
}

#[test]
fn a_content_length_above_the_bound_is_refused_before_a_byte_is_read() {
    let given = std::cell::Cell::new(0);
    assert_eq!(
        read_bounded(counting(&given), Some(BODY_LIMIT + 1)),
        BodyRead::TooLarge
    );
    assert_eq!(given.get(), 0);
}

#[test]
fn an_endless_body_is_refused_after_one_byte_past_the_bound() {
    let given = std::cell::Cell::new(0);
    assert_eq!(read_bounded(counting(&given), None), BodyRead::TooLarge);
    assert_eq!(
        given.get(),
        67_108_865,
        "Never more than the bound plus one"
    );
}

#[test]
fn a_body_of_exactly_the_bound_is_read_whole() {
    let given = std::cell::Cell::new(0);
    let reader = Counting {
        end_after: Some(BODY_LIMIT),
        ..counting(&given)
    };
    match read_bounded(reader, Some(BODY_LIMIT)) {
        BodyRead::Bytes(bytes) => assert_eq!(bytes.len() as u64, BODY_LIMIT),
        other => panic!("Expected the bytes, got {}", kind(&other)),
    }
}

#[test]
fn a_body_cut_short_failed() {
    let given = std::cell::Cell::new(0);
    let reader = Counting {
        fail_after: Some(10),
        ..counting(&given)
    };
    assert_eq!(read_bounded(reader, None), BodyRead::Failed);
}

fn kind(read: &BodyRead) -> &'static str {
    match read {
        BodyRead::Bytes(_) => "Bytes",
        BodyRead::TooLarge => "TooLarge",
        BodyRead::Failed => "Failed",
    }
}

// ----- The client's settings -----

#[test]
fn the_settings_override_a_proxy_redirects_status_errors_and_the_timeout() {
    let proxy = ureq::Proxy::new("http://127.0.0.1:9").expect("Proxy");
    let busy = Agent::config_builder()
        .proxy(Some(proxy))
        .max_redirects(5)
        .http_status_as_error(true)
        .timeout_global(None);
    let config = settings(busy, REQUEST_TIMEOUT);
    assert!(config.proxy().is_none(), "No proxy, whatever came before");
    assert_eq!(config.max_redirects(), 0);
    assert!(!config.http_status_as_error());
    assert_eq!(config.timeouts().global, Some(Duration::from_secs(30)));
    assert_eq!(REQUEST_TIMEOUT, Duration::from_secs(30));
}

// ----- The one way to a socket -----

/// Patterns built at run time, so this file never matches them itself.
fn pattern(parts: &[&str]) -> String {
    parts.concat()
}

#[test]
fn the_crate_has_one_way_to_a_socket_and_it_sends_get() {
    let client = include_str!("client.rs");
    let sources = [
        ("lib.rs", include_str!("lib.rs")),
        ("client.rs", client),
        ("block.rs", include_str!("block.rs")),
    ];
    // One call of ureq's `get`, in `send_get`, and no other way to a request.
    assert_eq!(client.matches(&pattern(&[".get(", "url)"])).count(), 1);
    assert_eq!(client.matches(&pattern(&["fn send", "_get("])).count(), 1);
    for (name, source) in sources {
        for forbidden in [
            pattern(&[".po", "st("]),
            pattern(&[".pu", "t("]),
            pattern(&[".pat", "ch("]),
            pattern(&[".del", "ete("]),
            pattern(&[".he", "ad("]),
            pattern(&[".opt", "ions("]),
            pattern(&[".trac", "e("]),
            pattern(&[".conn", "ect("]),
            pattern(&[".ru", "n("]),
            pattern(&["Met", "hod"]),
            pattern(&["ureq::", "get"]),
            pattern(&["ureq::", "request"]),
            pattern(&["allow_non_standard", "_methods"]),
            pattern(&["force_send", "_body"]),
            pattern(&["Tcp", "Stream"]),
            pattern(&["Udp", "Socket"]),
        ] {
            assert!(!source.contains(&forbidden), "{name} holds {forbidden}");
        }
        if name != "client.rs" {
            assert!(!source.contains("ureq"), "{name} uses the HTTP client");
        }
    }
}

// ----- Over a socket, to a stand-in on a port the test picked -----

/// One scripted answer: the bytes to send, and whether to hold the connection open afterwards.
struct Scripted {
    head_and_body: Vec<u8>,
    then_endless: bool,
    hold: bool,
}

fn scripted(text: &str) -> Scripted {
    Scripted {
        head_and_body: text.as_bytes().to_vec(),
        then_endless: false,
        hold: false,
    }
}

const STOP: &str = "STOP";

/// A stand-in board on 127.0.0.1, on a port the system picks. Each connection's request head is
/// recorded, and answered with the next scripted answer; a connection past the script is closed.
/// It ends when the test sends [`STOP`] as a head.
struct StandIn {
    port: u16,
    heads: mpsc::Receiver<Vec<String>>,
}

impl StandIn {
    fn start(script: Vec<Scripted>) -> StandIn {
        let listener = TcpListener::bind("127.0.0.1:0").expect("Bind");
        let port = listener.local_addr().expect("Address").port();
        let (send, heads) = mpsc::channel();
        thread::spawn(move || {
            let mut script: VecDeque<Scripted> = script.into();
            let mut seen = Vec::new();
            let mut held = Vec::new();
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                let head = read_head(&stream);
                if head.first().map(String::as_str) == Some(STOP) {
                    break;
                }
                seen.push(head);
                let Some(answer) = script.pop_front() else {
                    continue;
                };
                let _ = stream.write_all(&answer.head_and_body);
                if answer.then_endless {
                    let chunk = vec![b' '; 1 << 20];
                    for _ in 0..80 {
                        if stream.write_all(&chunk).is_err() {
                            break;
                        }
                    }
                }
                let _ = stream.flush();
                if answer.hold {
                    held.push(stream);
                }
            }
            drop(held);
            let _ = send.send(seen.into_iter().flatten().collect());
        });
        StandIn { port, heads }
    }

    /// Stops the stand-in and gives every head's lines, in order, each head ended by "".
    fn stop(self) -> Vec<String> {
        if let Ok(mut stop) = TcpStream::connect(("127.0.0.1", self.port)) {
            let _ = stop.write_all(format!("{STOP}\r\n\r\n").as_bytes());
        }
        self.heads
            .recv_timeout(Duration::from_secs(30))
            .expect("The stand-in ended")
    }
}

fn read_head(stream: &TcpStream) -> Vec<String> {
    let mut reader = BufReader::new(stream);
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let line = line.trim_end_matches(['\r', '\n']).to_owned();
                let end = line.is_empty();
                lines.push(line);
                if end {
                    break;
                }
            }
        }
    }
    lines
}

fn planted_bearer() -> Bearer {
    let files = good_files();
    let (_, bearer) = find_board(&ctx(Some("throwaway-local")), &files, &FakeUnprotect(true))
        .expect("The planted credential");
    bearer
}

fn over_socket(
    script: Vec<Scripted>,
    timeout: Duration,
) -> (Result<Epic, BoardError>, Vec<String>, String) {
    let stand_in = StandIn::start(script);
    let port = stand_in.port.to_string();
    let got = read_at(&port, &planted_bearer(), BOARD, 52, timeout);
    (got, stand_in.stop(), port)
}

fn request_lines(heads: &[String]) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = true;
    for line in heads {
        if start {
            lines.push(line.as_str());
        }
        start = line.is_empty();
    }
    lines
}

/// Checks one request's header lines without showing a value: the header names allowed, the
/// `Authorization` exactly the planted bearer, and any `Accept-Encoding` `identity`.
fn check_headers(heads: &[String]) {
    let allowed = [
        "host",
        "authorization",
        "user-agent",
        "accept",
        "accept-encoding",
        "connection",
    ];
    let mut authorizations = 0;
    let mut start = true;
    for line in heads {
        if start || line.is_empty() {
            start = line.is_empty();
            continue;
        }
        let (name, value) = line.split_once(':').expect("A header line");
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        assert!(allowed.contains(&name.as_str()), "A header named {name}");
        if name == "authorization" {
            authorizations += 1;
            assert!(
                value == format!("Bearer {PLANTED}"),
                "The Authorization is not the bearer"
            );
        }
        if name == "accept-encoding" {
            assert_eq!(value, "identity");
        }
    }
    assert_eq!(authorizations, request_lines(heads).len(), "One each");
}

fn ticket_line() -> String {
    format!("GET /api/2026-11-10/boards/{BOARD}?view=ticket&ticket=52&include_prose=true HTTP/1.1")
}

#[test]
fn a_200_over_a_socket_is_one_get_with_only_the_headers_allowed() {
    let answer = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{GOOD}",
        GOOD.len()
    );
    let (got, heads, _) = over_socket(vec![scripted(&answer)], REQUEST_TIMEOUT);
    assert_eq!(got.map(|e| e.revision), Ok(7));
    assert_eq!(request_lines(&heads), vec![ticket_line().as_str()]);
    check_headers(&heads);
}

#[test]
fn not_found_then_200_over_a_socket_is_two_gets() {
    let not_found = format!(
        "HTTP/1.1 404 Not Found\r\nContent-Type: Application/Problem+JSON; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{NOT_FOUND}",
        NOT_FOUND.len()
    );
    let summary = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
    let (got, heads, _) = over_socket(
        vec![scripted(&not_found), scripted(summary)],
        REQUEST_TIMEOUT,
    );
    assert_eq!(got, Err(BoardError::UnknownTicket));
    let summary_line = format!("GET /api/2026-11-10/boards/{BOARD}?view=summary HTTP/1.1");
    assert_eq!(
        request_lines(&heads),
        vec![ticket_line().as_str(), summary_line.as_str()]
    );
    check_headers(&heads);
}

#[test]
fn a_redirect_is_not_followed() {
    let redirect = format!(
        "HTTP/1.1 302 Found\r\nLocation: /api/2026-11-10/boards/{BOARD}?view=summary\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    let (got, heads, port) = over_socket(vec![scripted(&redirect)], REQUEST_TIMEOUT);
    assert_eq!(got, Err(BoardError::Internal(no_answer_text(&port))));
    assert_eq!(request_lines(&heads).len(), 1, "One request");
}

#[test]
fn a_content_length_above_the_bound_is_refused_without_waiting_for_the_body() {
    let mut answer = scripted(&format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
        BODY_LIMIT + 1
    ));
    answer.hold = true;
    let started = Instant::now();
    let (got, _, _) = over_socket(vec![answer], Duration::from_secs(5));
    assert_eq!(got, Err(BoardError::TooLarge));
    assert!(started.elapsed() < Duration::from_secs(5), "Did not wait");
}

#[test]
fn a_body_growing_past_the_bound_is_too_large() {
    let mut answer = scripted("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n");
    answer.then_endless = true;
    let (got, _, _) = over_socket(vec![answer], REQUEST_TIMEOUT);
    assert_eq!(got, Err(BoardError::TooLarge));
}

#[test]
fn a_board_that_never_answers_is_ended_at_the_timeout() {
    let mut answer = scripted("");
    answer.hold = true;
    let started = Instant::now();
    let (got, _, port) = over_socket(vec![answer], Duration::from_secs(1));
    assert_eq!(got, Err(BoardError::Internal(no_answer_text(&port))));
    assert!(started.elapsed() < Duration::from_secs(20), "Ended in time");
}

#[test]
fn a_port_with_no_listener_is_no_answer() {
    // A port the system gave and that is closed again before the read.
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("Bind");
        listener.local_addr().expect("Address").port().to_string()
    };
    let got = read_at(&port, &planted_bearer(), BOARD, 52, Duration::from_secs(5));
    assert_eq!(got, Err(BoardError::Internal(no_answer_text(&port))));
}
