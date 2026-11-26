//! The tests' shared ground: a stand-in board (a loopback server written here, on a port the OS
//! picks), a stand-in receipt and credential in a throwaway home, and the checks every read's
//! test makes (docs/lease-api.md, The board read; docs/conformance.md, The stand-in board and its
//! credential).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - The stand-in MUST bind `127.0.0.1` port 0, so the OS picks the port, and MUST NOT be, or
//!   reach, localswim's own port 8811.
//! - The token is a stand-in drawn at random for each stand-in, and MUST NOT be printed, logged,
//!   asserted in a message or passed as an argument. The stand-in compares it and records only
//!   whether a request carried exactly `Bearer <token>`.
//! - A test MUST NOT read Terry's receipt, credential or board.

#![allow(dead_code)]

use agentspace_board::{BoardError, Epic, read_epic};
use agentspace_push::{Context, StdFiles, Unprotect, Zeroizing};
use std::collections::BTreeMap;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// localswim's API prefix, as its `docs/openapi.yaml` gives it under `servers`.
pub const API: &str = "/api/2026-11-10/";
/// A board's ID, patterned to read as fake (conformance.md).
pub const BOARD: &str = "0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f";
/// Another board's ID.
pub const OTHER_BOARD: &str = "1b2c3d4e-1b2c-4d3e-9f40-1b2c3d4e5f60";
/// The answer's bound, 64 MiB.
pub const BOUND: u64 = 67_108_864;
/// Strings the stand-in puts in every part of an answer the client has no use for, and in every
/// Problem document. No message of the client may hold one.
pub const MARKERS: [&str; 5] = [
    "TITLE-MARKER-9f2",
    "COMMENT-MARKER-1-7c4",
    "COMMENT-MARKER-2-1d8",
    "RELATED-MARKER-3b6",
    "DETAIL-MARKER-5e0",
];
/// The stand-in credential's blob starts with this, and the fake `Unprotect` strips it.
const BLOB_PREFIX: &[u8] = b"STANDIN-DPAPI:";

// ---------------------------------------------------------------------------------------------
// What a request looked like, never what it carried.

/// One request the stand-in received. It holds whether `Authorization` was exactly `Bearer
/// <token>`, never the header.
#[derive(Clone, Debug)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub query: String,
    pub auth_ok: bool,
    /// Header names, lowercased.
    pub headers: Vec<String>,
    /// The `Accept-Encoding` value, when there was one.
    pub accept_encoding: Option<String>,
    pub has_body: bool,
}

/// What every request of every test MUST have been (lease-api.md, The requests; The client only
/// reads): a GET, with exactly the Bearer header, no body, no cookie, no `Origin`, no
/// `If-None-Match`, and no `Accept-Encoding` but `identity`.
pub fn assert_only_gets(reqs: &[Seen]) {
    for (i, r) in reqs.iter().enumerate() {
        assert_eq!(r.method, "GET", "request {i}'s method");
        assert!(
            r.auth_ok,
            "request {i} did not carry exactly the Bearer header"
        );
        assert!(
            r.path.starts_with(&format!("{API}boards/")),
            "request {i}'s path was not under the API's boards"
        );
        for bad in ["cookie", "origin", "if-none-match"] {
            assert!(
                !r.headers.iter().any(|h| h == bad),
                "request {i} carried a {bad} header"
            );
        }
        assert!(!r.has_body, "request {i} carried a body");
        if let Some(enc) = &r.accept_encoding {
            assert!(
                enc.trim().eq_ignore_ascii_case("identity"),
                "request {i} asked for an encoding other than identity"
            );
        }
    }
}

/// The ticket view's query, exactly.
pub fn ticket_query(ticket: u64) -> String {
    format!("view=ticket&ticket={ticket}&include_prose=true")
}

/// The first request of a read of `board`'s `ticket`.
pub fn assert_ticket_view(r: &Seen, board: &str, ticket: u64) {
    assert_eq!(r.path, format!("{API}boards/{board}"));
    assert_eq!(r.query, ticket_query(ticket));
}

/// The second request of a refused read: the board's summary view.
pub fn assert_summary_view(r: &Seen, board: &str) {
    assert_eq!(r.path, format!("{API}boards/{board}"));
    assert_eq!(r.query, "view=summary");
}

// ---------------------------------------------------------------------------------------------
// The card and the answers.

/// One card of a stand-in board. The three raw members are JSON text, so a test can write what a
/// real answer never would (`5.0`, `"5"`).
#[derive(Clone)]
pub struct Card {
    pub revision_raw: String,
    pub archived_raw: String,
    /// The description as JSON text, quotes included.
    pub description_raw: String,
}

impl Card {
    pub fn new(revision: u64, archived: bool, description: &str) -> Card {
        Card {
            revision_raw: revision.to_string(),
            archived_raw: archived.to_string(),
            description_raw: serde_json::to_string(description).unwrap(),
        }
    }

    pub fn raw(revision: &str, archived: &str, description: &str) -> Card {
        Card {
            revision_raw: revision.to_string(),
            archived_raw: archived.to_string(),
            description_raw: description.to_string(),
        }
    }

    /// A whole `TicketInspection` (localswim's `docs/openapi.yaml`), with every part the client
    /// has no use for filled in, each holding a marker or a decoy value: other cards with other
    /// revisions and an archived parent, comments with a `description` and a `revision` of their
    /// own.
    pub fn body(&self, board: &str, ticket: u64, with_prose: bool) -> String {
        let context = |t: u64, rev: u64, archived: bool, title: &str| {
            serde_json::json!({
                "id": "2f5a1c9e-3b7d-4e21-8a64-0c9d7e5b1a30",
                "ticket": t,
                "revision": rev,
                "archived": archived,
                "title": title,
                "lane": "Backlog",
                "owner": null,
                "priority": "high",
            })
        };
        let mut doc = serde_json::json!({
            "board_id": board,
            "board_revision": 31,
            "card": {
                "id": "9c3d5e7f-1a2b-4c3d-8e4f-5a6b7c8d9e0f",
                "ticket": ticket,
                "revision": "@@REV@@",
                "archived": "@@ARCH@@",
                "title": MARKERS[0],
                "lane": "In Progress",
                "owner": "bot",
                "priority": "low",
            },
            "related_card": context(40, 4040, true, MARKERS[3]),
            "relationships": [{
                "id": "3a4b5c6d-7e8f-4a9b-8c0d-1e2f3a4b5c6d",
                "revision": 3,
                "kind": "blocks",
                "direction": "outgoing",
                "card": context(41, 777, true, MARKERS[3]),
            }],
            "parent": context(10, 999, true, MARKERS[3]),
            "children": [context(53, 888, true, MARKERS[3])],
        });
        if with_prose {
            doc["prose"] = serde_json::json!({
                "description": "@@DESC@@",
                "comments": [
                    {"id": "c1", "body": MARKERS[1], "revision": 4242, "archived": true},
                    {"id": "c2", "body": MARKERS[2], "description": "A decoy description"},
                ],
            });
        }
        doc.to_string()
            .replacen("\"@@REV@@\"", &self.revision_raw, 1)
            .replacen("\"@@ARCH@@\"", &self.archived_raw, 1)
            .replacen("\"@@DESC@@\"", &self.description_raw, 1)
    }
}

/// A Problem document, localswim's shape, with a marker in `title` and `detail`.
pub fn problem_body(status: u16, code: &str) -> String {
    serde_json::json!({
        "type": "about:blank",
        "title": MARKERS[0],
        "status": status,
        "code": code,
        "detail": MARKERS[4],
        "request_id": "req-1",
        "outcome": "rejected",
        "errors": [],
    })
    .to_string()
}

/// How the stand-in answers instead of looking a card up.
#[derive(Clone)]
pub enum Answer {
    /// A whole answer: a status, a `Content-Type` when given, more headers and a body.
    Raw {
        status: u16,
        content_type: Option<String>,
        extra: Vec<(String, String)>,
        body: String,
    },
    /// Reads the request and never answers it, until the stand-in ends.
    Hang,
    /// Reads the request and closes the connection with no byte of answer.
    Close,
    /// Promises a body, sends a part of it and closes: a `Content-Length` of 1000, or a chunked
    /// body with no last chunk.
    Truncated { chunked: bool },
    /// A body of `total` bytes (a whole document, then spaces) sent with a `Content-Length` of
    /// `total`, or in chunks. `send` bytes of it are sent: fewer than `total` with `hold` shows a
    /// refusal before the body is read. After them the connection is held open, or ended.
    Big {
        status: u16,
        problem: bool,
        total: u64,
        chunked: bool,
        send: u64,
        hold: bool,
    },
}

impl Answer {
    /// A localswim Problem document, given with the media type `application/problem+json`.
    pub fn problem(status: u16, code: &str) -> Answer {
        Answer::Raw {
            status,
            content_type: Some("application/problem+json".to_string()),
            extra: Vec::new(),
            body: problem_body(status, code),
        }
    }

    /// An empty body, with no `Content-Type`: what a bare 404 is.
    pub fn bare(status: u16) -> Answer {
        Answer::Raw {
            status,
            content_type: None,
            extra: Vec::new(),
            body: String::new(),
        }
    }

    /// A 200 with this body.
    pub fn ok_body(body: &str) -> Answer {
        Answer::Raw {
            status: 200,
            content_type: Some("application/json".to_string()),
            extra: Vec::new(),
            body: body.to_string(),
        }
    }

    pub fn raw(status: u16, content_type: Option<&str>, body: &str) -> Answer {
        Answer::Raw {
            status,
            content_type: content_type.map(str::to_string),
            extra: Vec::new(),
            body: body.to_string(),
        }
    }

    /// A redirect to `location`.
    pub fn redirect(status: u16, location: &str) -> Answer {
        Answer::Raw {
            status,
            content_type: None,
            extra: vec![("Location".to_string(), location.to_string())],
            body: String::new(),
        }
    }
}

/// What a stand-in knows and how it answers.
#[derive(Clone)]
pub struct Behavior {
    /// Board ID, then ticket, then card.
    pub boards: BTreeMap<String, BTreeMap<u64, Card>>,
    /// Overrides every reply after the method, the prefix and the token checks, the summary view
    /// included unless `summary` is set.
    pub answer: Option<Answer>,
    /// Overrides the reply to `?view=summary` alone.
    pub summary: Option<Answer>,
    /// The media type of the 404 Problem documents the stand-in gives.
    pub problem_media: String,
}

impl Behavior {
    pub fn empty() -> Behavior {
        Behavior {
            boards: BTreeMap::new(),
            answer: None,
            summary: None,
            problem_media: "application/problem+json".to_string(),
        }
    }

    /// One board holding ticket 52, an epic.
    pub fn standard() -> Behavior {
        Behavior::empty().with_card(BOARD, 52, Card::new(7, false, "An epic's description"))
    }

    pub fn with_card(mut self, board: &str, ticket: u64, card: Card) -> Behavior {
        self.boards
            .entry(board.to_string())
            .or_default()
            .insert(ticket, card);
        self
    }

    pub fn with_board(mut self, board: &str) -> Behavior {
        self.boards.entry(board.to_string()).or_default();
        self
    }

    pub fn answer(mut self, a: Answer) -> Behavior {
        self.answer = Some(a);
        self
    }

    pub fn summary(mut self, a: Answer) -> Behavior {
        self.summary = Some(a);
        self
    }

    pub fn media(mut self, media: &str) -> Behavior {
        self.problem_media = media.to_string();
        self
    }
}

// ---------------------------------------------------------------------------------------------
// The stand-in server.

struct Shared {
    behavior: Behavior,
    token: String,
    log: Mutex<Vec<Seen>>,
    stop: AtomicBool,
}

/// A stand-in board on `127.0.0.1` and a port the OS picked. It ends, and its threads with it,
/// when dropped.
pub struct Standin {
    pub port: u16,
    shared: Arc<Shared>,
    accept: Option<JoinHandle<()>>,
    conns: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A stand-in token: 43 characters of unpadded base64url, drawn at random for each call from
/// the standard library's per-instance random hash keys. It is built here and never written in
/// a file of the repository.
pub fn fresh_token() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    (0..43u64)
        .map(|i| {
            let mut h = RandomState::new().build_hasher();
            h.write_u64(n * 64 + i);
            ALPHABET[(h.finish() % 64) as usize] as char
        })
        .collect()
}

impl Standin {
    pub fn start(behavior: Behavior) -> Standin {
        Standin::start_with_token(behavior, fresh_token())
    }

    pub fn start_with_token(behavior: Behavior, token: String) -> Standin {
        let listener = loop {
            let l = TcpListener::bind("127.0.0.1:0").expect("the stand-in binds a loopback port");
            // Never localswim's own port.
            if l.local_addr().unwrap().port() != 8811 {
                break l;
            }
        };
        let port = listener.local_addr().unwrap().port();
        assert_ne!(port, 8811, "the stand-in MUST NOT be localswim's port");
        listener.set_nonblocking(true).unwrap();
        let shared = Arc::new(Shared {
            behavior,
            token,
            log: Mutex::new(Vec::new()),
            stop: AtomicBool::new(false),
        });
        let conns: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
        let accept = {
            let shared = Arc::clone(&shared);
            let conns = Arc::clone(&conns);
            thread::spawn(move || {
                while !shared.stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let _ = stream.set_nonblocking(false);
                            let shared = Arc::clone(&shared);
                            let handle = thread::spawn(move || handle(stream, &shared));
                            conns.lock().unwrap().push(handle);
                        }
                        Err(_) => thread::sleep(Duration::from_millis(5)),
                    }
                }
            })
        };
        Standin {
            port,
            shared,
            accept: Some(accept),
            conns,
        }
    }

    /// The requests so far, in order.
    pub fn requests(&self) -> Vec<Seen> {
        self.shared.log.lock().unwrap().clone()
    }

    /// The stand-in's token, for the credential the test writes and for the leak checks. It MUST
    /// NOT be printed.
    pub fn token(&self) -> &str {
        &self.shared.token
    }

    pub fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for Standin {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.accept.take() {
            let _ = h.join();
        }
        let handles: Vec<_> = self.conns.lock().unwrap().drain(..).collect();
        for h in handles {
            let _ = h.join();
        }
    }
}

/// The head of a request: everything up to the blank line. `None` when the connection sent
/// nothing, or never finished a head.
fn read_head(stream: &mut TcpStream) -> Option<String> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut data: Vec<u8> = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => return None,
            Ok(n) => data.extend_from_slice(&buf[..n]),
        }
        if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            return Some(String::from_utf8_lossy(&data[..at]).into_owned());
        }
        if data.len() > 65536 {
            return None;
        }
    }
}

fn handle(mut stream: TcpStream, shared: &Shared) {
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    let Some(head) = read_head(&mut stream) else {
        return;
    };
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split(' ');
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    let header = |name: &str| {
        headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
    };
    let expected = format!("Bearer {}", shared.token);
    let auth_ok = header("authorization").as_deref() == Some(expected.as_str());
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.clone(), String::new()),
    };
    let has_body = header("content-length").is_some_and(|v| v.trim() != "0")
        || header("transfer-encoding").is_some();
    shared.log.lock().unwrap().push(Seen {
        method: method.clone(),
        path: path.clone(),
        query: query.clone(),
        auth_ok,
        headers: headers.iter().map(|(n, _)| n.clone()).collect(),
        accept_encoding: header("accept-encoding"),
        has_body,
    });

    // The method, then the prefix, then the token, then the lookup (conformance.md).
    if method != "GET" {
        reply(
            &mut stream,
            &Answer::problem(405, "method_not_allowed"),
            shared,
        );
        return;
    }
    let Some(rest) = path.strip_prefix(API) else {
        reply(&mut stream, &Answer::bare(404), shared);
        return;
    };
    if !auth_ok {
        reply(&mut stream, &Answer::problem(401, "unauthorized"), shared);
        return;
    }
    let params: Vec<(&str, &str)> = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| p.split_once('=').unwrap_or((p, "")))
        .collect();
    let param = |name: &str| params.iter().find(|(k, _)| *k == name).map(|(_, v)| *v);
    let view = param("view");
    if view == Some("summary")
        && let Some(a) = &shared.behavior.summary
    {
        reply(&mut stream, a, shared);
        return;
    }
    if let Some(a) = &shared.behavior.answer {
        reply(&mut stream, a, shared);
        return;
    }
    let not_found = Answer::Raw {
        status: 404,
        content_type: Some(shared.behavior.problem_media.clone()),
        extra: Vec::new(),
        body: problem_body(404, "not_found"),
    };
    let Some(board) = rest.strip_prefix("boards/") else {
        reply(
            &mut stream,
            &Answer::problem(400, "invalid_request"),
            shared,
        );
        return;
    };
    let board = board.to_ascii_lowercase();
    let known = shared.behavior.boards.get(&board);
    match view {
        Some("ticket") => {
            let ticket = param("ticket").and_then(|t| t.parse::<u64>().ok());
            let card = known.and_then(|cards| cards.get(&ticket?));
            match (card, ticket) {
                (Some(card), Some(ticket)) => {
                    let prose = param("include_prose") == Some("true");
                    let body = card.body(&board, ticket, prose);
                    reply(&mut stream, &Answer::ok_body(&body), shared);
                }
                _ => reply(&mut stream, &not_found, shared),
            }
        }
        Some("summary") => {
            if known.is_some() {
                let body = serde_json::json!({"id": board}).to_string();
                reply(&mut stream, &Answer::ok_body(&body), shared);
            } else {
                reply(&mut stream, &not_found, shared);
            }
        }
        _ => reply(
            &mut stream,
            &Answer::problem(400, "invalid_request"),
            shared,
        ),
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
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
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

fn wait_for_stop(shared: &Shared) {
    while !shared.stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(50));
    }
}

fn reply(stream: &mut TcpStream, answer: &Answer, shared: &Shared) {
    match answer {
        Answer::Raw {
            status,
            content_type,
            extra,
            body,
        } => {
            let mut head = format!("HTTP/1.1 {status} {}\r\n", reason(*status));
            if let Some(ct) = content_type {
                head.push_str(&format!("Content-Type: {ct}\r\n"));
            }
            for (k, v) in extra {
                head.push_str(&format!("{k}: {v}\r\n"));
            }
            let no_length = *status == 204 || *status == 304 || (100..200).contains(status);
            if !no_length {
                head.push_str(&format!("Content-Length: {}\r\n", body.len()));
            }
            head.push_str("Connection: close\r\n\r\n");
            let _ = stream.write_all(head.as_bytes());
            if !no_length {
                let _ = stream.write_all(body.as_bytes());
            }
            let _ = stream.flush();
            let _ = stream.shutdown(Shutdown::Write);
        }
        Answer::Hang => wait_for_stop(shared),
        Answer::Close => {
            let _ = stream.shutdown(Shutdown::Both);
        }
        Answer::Truncated { chunked } => {
            let head = if *chunked {
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
            } else {
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n"
            };
            let _ = stream.write_all(head.as_bytes());
            if *chunked {
                let _ = stream.write_all(b"10\r\n{\"card\":{\"revisi\r\n");
            } else {
                let _ = stream.write_all(b"{\"card\":{\"revisi");
            }
            let _ = stream.flush();
            let _ = stream.shutdown(Shutdown::Write);
        }
        Answer::Big {
            status,
            problem,
            total,
            chunked,
            send,
            hold,
        } => big(
            stream, shared, *status, *problem, *total, *chunked, *send, *hold,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn big(
    stream: &mut TcpStream,
    shared: &Shared,
    status: u16,
    problem: bool,
    total: u64,
    chunked: bool,
    send: u64,
    hold: bool,
) {
    let (doc, ct) = if problem {
        (
            problem_body(status, "not_found"),
            "application/problem+json",
        )
    } else {
        (
            Card::new(7, false, "A big answer's description").body(BOARD, 52, true),
            "application/json",
        )
    };
    let doc = doc.into_bytes();
    assert!(
        doc.len() as u64 <= total,
        "the document does not fit the total"
    );
    let mut head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {ct}\r\n",
        reason(status)
    );
    if chunked {
        head.push_str("Transfer-Encoding: chunked\r\n");
    } else {
        head.push_str(&format!("Content-Length: {total}\r\n"));
    }
    head.push_str("Connection: close\r\n\r\n");
    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }
    const BLOCK: u64 = 65536;
    let mut written: u64 = 0;
    while written < send {
        let n = BLOCK.min(send - written) as usize;
        let mut block: Vec<u8> = Vec::with_capacity(n);
        if (written as usize) < doc.len() {
            let end = doc.len().min(written as usize + n);
            block.extend_from_slice(&doc[written as usize..end]);
        }
        block.resize(n, b' ');
        let ok = if chunked {
            stream
                .write_all(format!("{n:x}\r\n").as_bytes())
                .and_then(|_| stream.write_all(&block))
                .and_then(|_| stream.write_all(b"\r\n"))
        } else {
            stream.write_all(&block)
        };
        if ok.is_err() {
            return;
        }
        written += n as u64;
    }
    if hold {
        let _ = stream.flush();
        wait_for_stop(shared);
    } else {
        if chunked {
            let _ = stream.write_all(b"0\r\n\r\n");
        }
        let _ = stream.flush();
        let _ = stream.shutdown(Shutdown::Write);
    }
}

// ---------------------------------------------------------------------------------------------
// The stand-in credential, and the throwaway home it sits in.

/// A decryption that gives back what the stand-in credential's blob holds: the blob without its
/// prefix. A blob with another start is one that Windows' data protection would refuse.
pub struct FakeDpapi;

impl Unprotect for FakeDpapi {
    fn unprotect(&self, blob: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
        blob.strip_prefix(BLOB_PREFIX)
            .map(|plain| Zeroizing::new(plain.to_vec()))
    }
}

pub fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The blob the fake decryption accepts for `plaintext`.
pub fn blob_for(plaintext: &[u8]) -> Vec<u8> {
    let mut blob = BLOB_PREFIX.to_vec();
    blob.extend_from_slice(plaintext);
    blob
}

/// `bot.json`'s text, with `token_dpapi_hex` as given.
pub fn bot_json(token_hex: &str) -> String {
    serde_json::json!({
        "format": "localswim-credential",
        "format_version": 1,
        "role": "bot",
        "token_dpapi_hex": token_hex,
    })
    .to_string()
}

/// A context whose `LOCALAPPDATA` is inside `root`. Never the real one.
pub fn ctx_for(root: &Path) -> Context {
    Context {
        home: root.join("home"),
        local_app_data: Some(root.join("AppData").join("Local")),
        system_root: root.join("system"),
        nested: false,
    }
}

/// A throwaway home inside the build's own scratch folder, never `%TEMP%`, with a stand-in
/// receipt and a stand-in `bot.json` that a read can use, and a stand-in board to read.
pub struct World {
    pub standin: Standin,
    pub dir: tempfile::TempDir,
    pub ctx: Context,
}

impl World {
    pub fn new(behavior: Behavior) -> World {
        World::around(Standin::start(behavior))
    }

    /// A world for a stand-in already started (one whose token the test chose, say).
    pub fn around(standin: Standin) -> World {
        let dir = tempfile::Builder::new()
            .prefix("board-")
            .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
            .expect("a throwaway folder in the build's scratch folder");
        let ctx = ctx_for(dir.path());
        let world = World { standin, dir, ctx };
        world.write_receipt_base(serde_json::Value::String(world.standin.base()));
        world.write_bot(&world.good_bot_text());
        world
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    pub fn local(&self) -> PathBuf {
        self.ctx.local_app_data.clone().unwrap()
    }

    pub fn receipt_file(&self) -> PathBuf {
        self.local().join("localswim-v2").join("service.json")
    }

    pub fn cred_dir(&self) -> PathBuf {
        self.root().join("credentials")
    }

    pub fn bot_file(&self) -> PathBuf {
        self.cred_dir().join("bot.json")
    }

    pub fn good_bot_text(&self) -> String {
        let blob = blob_for(&utf16le(self.standin.token()));
        bot_json(&hex(&blob))
    }

    /// The receipt with `base` as given, as a JSON value, and the credential folder.
    pub fn write_receipt_base(&self, base: serde_json::Value) {
        let text = serde_json::json!({
            "base": base,
            "credential_directory": self.cred_dir().to_string_lossy(),
        })
        .to_string();
        self.write_receipt(text.as_bytes());
    }

    pub fn write_receipt(&self, bytes: &[u8]) {
        let file = self.receipt_file();
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let _ = std::fs::remove_dir_all(&file);
        let _ = std::fs::remove_file(&file);
        std::fs::write(&file, bytes).unwrap();
    }

    pub fn remove_receipt(&self) {
        let _ = std::fs::remove_file(self.receipt_file());
    }

    pub fn write_bot(&self, text: &str) {
        self.write_bot_bytes(text.as_bytes());
    }

    pub fn write_bot_bytes(&self, bytes: &[u8]) {
        std::fs::create_dir_all(self.cred_dir()).unwrap();
        let file = self.bot_file();
        let _ = std::fs::remove_dir_all(&file);
        let _ = std::fs::remove_file(&file);
        std::fs::write(&file, bytes).unwrap();
    }

    pub fn remove_bot(&self) {
        let _ = std::fs::remove_file(self.bot_file());
    }

    /// The read: `agentspace_board::read_epic` with the real files and the fake decryption.
    pub fn read(&self, board: &str, ticket: u64) -> Result<Epic, BoardError> {
        read_epic(&self.ctx, &StdFiles, &FakeDpapi, board, ticket)
    }

    /// The requests so far, every one of them a read of the right kind (`assert_only_gets`).
    pub fn requests(&self) -> Vec<Seen> {
        let reqs = self.standin.requests();
        assert_only_gets(&reqs);
        reqs
    }

    /// Checks `msg` against the leaks the spec forbids, without printing it.
    pub fn check_message(&self, msg: &str) {
        leak_check(self.standin.token(), msg, &[&self.root().to_string_lossy()]);
    }

    /// The `Internal` message of `result`, after the leak checks. A result that is not
    /// `Internal` fails with its variant's name.
    pub fn internal(&self, result: Result<Epic, BoardError>) -> String {
        match result {
            Err(BoardError::Internal(msg)) => {
                self.check_message(&msg);
                msg
            }
            other => panic!("expected Internal, got {}", name_of(&other)),
        }
    }

    pub fn ok(&self, result: Result<Epic, BoardError>) -> Epic {
        match result {
            Ok(epic) => epic,
            Err(BoardError::Internal(msg)) => {
                self.check_message(&msg);
                panic!("expected Ok, got Internal({msg})")
            }
            other => panic!("expected Ok, got {}", name_of(&other)),
        }
    }
}

/// A result's variant, never a message.
pub fn name_of(result: &Result<Epic, BoardError>) -> &'static str {
    match result {
        Ok(_) => "Ok",
        Err(BoardError::UnknownBoard) => "UnknownBoard",
        Err(BoardError::UnknownTicket) => "UnknownTicket",
        Err(BoardError::TooLarge) => "TooLarge",
        Err(BoardError::Internal(_)) => "Internal",
    }
}

/// No message holds the token, a part of it (any 8 characters in a row), a marker from a part of
/// the answer or a Problem document, a URL, a path, a header or an operating system's text. It
/// panics without quoting `msg`, which may be the leak.
pub fn leak_check(token: &str, msg: &str, paths: &[&str]) {
    no_token(token, msg);
    for marker in MARKERS {
        assert!(!msg.contains(marker), "a message holds a marker");
    }
    let lower = msg.to_ascii_lowercase();
    for forbidden in [
        "http",
        "/api",
        "bearer",
        "authorization",
        "service.json",
        "bot.json",
        "localswim-v2",
        ".json",
        "\\",
        "problem",
        "not_found",
        "revision",
    ] {
        assert!(
            !lower.contains(forbidden),
            "a message holds a URL, a path, a header or the board's own text"
        );
    }
    for path in paths {
        assert!(!msg.contains(path), "a message holds a path");
    }
}

/// `text` holds neither the token nor any 8 characters in a row of it. It panics without quoting
/// `text`.
pub fn no_token(token: &str, text: &str) {
    assert!(!text.contains(token), "a message holds the whole token");
    let chars: Vec<char> = token.chars().collect();
    for window in chars.windows(8) {
        let part: String = window.iter().collect();
        assert!(!text.contains(&part), "a message holds a part of the token");
    }
}

/// The messages of the spec's table (lease-api.md, The board read, "Each outcome"), between
/// `agentspace lease create failed: ` and `. Nothing was recorded.`, with no final period.
pub const MSG_LOCALAPPDATA: &str =
    "LOCALAPPDATA is unset or empty, so localswim's service receipt cannot be found";
pub const MSG_NO_RECEIPT: &str =
    "localswim's service receipt was not found; localswim may not be running";
pub const MSG_RECEIPT: &str = "localswim's service receipt could not be read";
pub const MSG_CREDENTIAL: &str = "The board's credential could not be read";
pub const MSG_ANSWER: &str = "The board's answer could not be read";
pub const MSG_REFUSED: &str = "The board refused agentspace's credential";

pub fn msg_no_answer(port: u16) -> String {
    format!("The board at 127.0.0.1:{port} did not answer")
}

/// A port nothing listens on: bound, read and closed.
pub fn dead_port() -> u16 {
    loop {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        if port != 8811 {
            return port;
        }
    }
}
