//! The board read: one GET of an epic's focused ticket view, and after localswim's `not_found` one
//! GET of the board's summary view, and nothing else (docs/lease-api.md, The board read; D86, D89).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! # What this module does
//!
//! - **It finds the board through localswim's service receipt and reads Bot's token from
//!   `bot.json`**, through `agentspace-push`'s [`receipt_path`], [`parse_receipt`],
//!   [`credential_path`] and [`read_token`], re-used and never copied (D89). Both files are read
//!   through the caller's [`Files`], and the token is decrypted through its [`Unprotect`].
//! - **It only reads.** [`send_get`] is the crate's one way to a socket: it takes no method and
//!   calls ureq's `get`, so no code path can send any method but GET (D86). The two requests'
//!   targets are built here and nowhere else.
//! - **Its HTTP client is ureq 3, built with no proxy, no redirect followed, a 30-second bound
//!   from the connection's start to the answer's last byte, and every status handed back as an
//!   answer** (architect, 2026-11-25, for v0.11-T02, pin P3), so each status is this module's to
//!   map, and the environment's proxy variables change nothing.
//! - **It reads a 200's body and a 404's body to at most 64 MiB, counted as received**, and no
//!   other answer's body.
//!
//! # Secrecy
//!
//! The token is held in `agentspace-push`'s [`Token`](agentspace_push::Token) and then its
//! [`Bearer`], each zeroed when dropped, and goes into the requests' `Authorization` header and
//! nowhere else: no message, log, argument, URL, file or error carries it or anything derived from
//! it. The header value given to ureq is marked sensitive, so `http`'s `Debug` of it is redacted.
//! This module never writes to stdout or stderr, and never panics on bad input.

use agentspace_operator_api::Context;
use agentspace_push::{
    Bearer, FileRead, Files, Unprotect, credential_path, parse_receipt, read_token, receipt_path,
};
use serde_json::Value;
use std::io::Read;
use std::time::Duration;
use ureq::config::{Config, ConfigBuilder};
use ureq::http::header::{AUTHORIZATION, CONTENT_TYPE};
use ureq::http::{HeaderValue, Response};
use ureq::typestate::AgentScope;
use ureq::{Agent, Body};

/// localswim's API path, as its `docs/openapi.yaml` gives it under `servers` (D89 as changed for
/// v0.11-T01): one constant.
pub(crate) const API_PREFIX: &str = "/api/2026-11-10";

/// The bound on a 200's and a 404's body, 64 MiB, counted as received (lease-api.md, The board
/// read, Bounds).
pub(crate) const BODY_LIMIT: u64 = 67_108_864;

/// Each request's bound, from the start of its connection to the last byte of its answer
/// (lease-api.md, The board read, Bounds; architect, 2026-11-25, for v0.11-T02, pin P3).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The largest ticket and card revision localswim gives, 2^53 - 1.
const MAX_INTEGER: u64 = 9_007_199_254_740_991;

/// The receipt's base, before its port: localswim's receipt form, which [`parse_receipt`] already
/// enforces (lease-api.md, The board read, "The base").
const BASE_PREFIX: &str = "http://127.0.0.1:";

/// localswim's Problem document's media type.
const PROBLEM_JSON: &str = "application/problem+json";

// Each `internal` outcome's text, as lease-api.md's "Each outcome" table gives it between
// `agentspace lease create failed: ` and `. Nothing was recorded.`, word for word, with no final
// period (architect, 2026-11-25, for v0.11-T02, pin P2). The CLI frames it.

/// `LOCALAPPDATA` unset, empty or only whitespace.
pub(crate) const NO_LOCAL_APP_DATA: &str =
    "LOCALAPPDATA is unset or empty, so localswim's service receipt cannot be found";
/// No file at the receipt's path.
pub(crate) const NO_RECEIPT: &str =
    "localswim's service receipt was not found; localswim may not be running";
/// The receipt cannot be read, breaks localswim's rules, or has its `base` missing or not of the
/// receipt form.
pub(crate) const BAD_RECEIPT: &str = "localswim's service receipt could not be read";
/// `bot.json` missing, unreadable, breaking localswim's rules, or not decrypted to a token.
pub(crate) const BAD_CREDENTIAL: &str = "The board's credential could not be read";
/// A 200 whose body is not the shape the read takes.
pub(crate) const BAD_ANSWER: &str = "The board's answer could not be read";
/// A 401 or a 403.
pub(crate) const REFUSED: &str = "The board refused agentspace's credential";

/// No answer, or any status the table does not name: `<port>` is the base's port as the receipt
/// gives it, the one part of a URL a message carries.
pub(crate) fn no_answer_text(port: &str) -> String {
    format!("The board at 127.0.0.1:{port} did not answer")
}

/// The epic card's three members the lease reads, and no others (lease-api.md, The board read,
/// "What is read from a 200 answer").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Epic {
    /// `prose.description`, from which the plan block is taken.
    pub description: String,
    /// `card.archived`. An archived card is `Ok` with this `true`, and the CLI refuses it
    /// (architect, 2026-11-25, for v0.11-T02, pin P2).
    pub archived: bool,
    /// `card.revision`, 1 to 9007199254740991.
    pub revision: u64,
}

/// How a board read fails (architect, 2026-11-25, for v0.11-T02, pin P2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardError {
    /// A 404 that is localswim's `not_found`, then the same from the summary view.
    UnknownBoard,
    /// A 404 that is localswim's `not_found`, then 200 from the summary view.
    UnknownTicket,
    /// A 200 whose body is larger than 64 MiB.
    TooLarge,
    /// An `internal` outcome: the table's text between `agentspace lease create failed: ` and
    /// `. Nothing was recorded.`, word for word, with no final period. The crate names no
    /// command; the CLI frames it.
    Internal(String),
}

fn internal(text: &str) -> BoardError {
    BoardError::Internal(text.to_owned())
}

/// Reads one epic card from the caller's localswim board (lease-api.md, The board read).
///
/// - `ctx` gives `LOCALAPPDATA`, under which the receipt is found ([`receipt_path`]).
/// - `files` reads the receipt and `bot.json`, and `unprotect` decrypts the token.
/// - `board` is the normalized board ID, a lowercase UUID, and `ticket` the normalized ticket.
///
/// The outcomes are lease-api.md's "Each outcome" table, the first row that applies. Two guards
/// come before it, for values the CLI never passes, since each would otherwise reach the request's
/// URL: a `board` that is not 36 characters of lowercase hex digits and `-` in a UUID's layout is
/// [`BoardError::UnknownBoard`], and a `ticket` outside 1 to 9007199254740991 is
/// [`BoardError::UnknownTicket`], each with nothing read and no request made.
pub fn read_epic(
    ctx: &Context,
    files: &dyn Files,
    unprotect: &dyn Unprotect,
    board: &str,
    ticket: u64,
) -> Result<Epic, BoardError> {
    if !board_is_normalized(board) {
        return Err(BoardError::UnknownBoard);
    }
    if !(1..=MAX_INTEGER).contains(&ticket) {
        return Err(BoardError::UnknownTicket);
    }
    let (port, bearer) = find_board(ctx, files, unprotect)?;
    read_at(&port, &bearer, board, ticket, REQUEST_TIMEOUT)
    // The bearer is dropped, and so zeroed, here, whatever the outcome.
}

/// Whether `board` is a UUID's layout in lowercase: 8, 4, 4, 4 and 12 hex digits, joined by `-`.
fn board_is_normalized(board: &str) -> bool {
    let bytes = board.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(i, &b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_digit() || (b'a'..=b'f').contains(&b),
        })
}

/// The receipt's port and the token's `Authorization` value, or the table's first rows.
pub(crate) fn find_board(
    ctx: &Context,
    files: &dyn Files,
    unprotect: &dyn Unprotect,
) -> Result<(String, Bearer), BoardError> {
    let path = receipt_path(ctx).ok_or_else(|| internal(NO_LOCAL_APP_DATA))?;
    let receipt = match files.read(&path) {
        FileRead::Missing => return Err(internal(NO_RECEIPT)),
        FileRead::Unreadable => return Err(internal(BAD_RECEIPT)),
        FileRead::Bytes(bytes) if is_utf8(&bytes) => {
            parse_receipt(&bytes).ok_or_else(|| internal(BAD_RECEIPT))?
        }
        FileRead::Bytes(_) => return Err(internal(BAD_RECEIPT)),
    };
    // `parse_receipt` refuses a base that is not the receipt form; a missing base is refused here,
    // and the prefix is checked again as a guard before the port reaches a URL.
    let port = receipt
        .base
        .as_deref()
        .and_then(|base| base.strip_prefix(BASE_PREFIX))
        .filter(|port| !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| internal(BAD_RECEIPT))?
        .to_owned();
    let token = match files.read(&credential_path(&receipt.credential_directory)) {
        FileRead::Bytes(bytes) => read_token(&bytes, unprotect),
        FileRead::Missing | FileRead::Unreadable => None,
    }
    .ok_or_else(|| internal(BAD_CREDENTIAL))?;
    Ok((port, token.bearer()))
}

/// Whether the receipt's bytes are UTF-8, after an optional UTF-8 byte order mark. lease-api.md,
/// The board read, takes the receipt as "one JSON object in UTF-8, a leading byte order mark
/// accepted", as localswim's `docs/windows-operations.md`, "The receipt", writes it, while
/// [`parse_receipt`] also reads UTF-16 marked by its byte order mark (operator.md, The receipt), so
/// the board read refuses anything that is not UTF-8 before it is parsed.
fn is_utf8(bytes: &[u8]) -> bool {
    let text = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    std::str::from_utf8(text).is_ok()
}

/// The read over a real connection to `127.0.0.1:<port>`, each request bounded by `timeout`.
pub(crate) fn read_at(
    port: &str,
    bearer: &Bearer,
    board: &str,
    ticket: u64,
    timeout: Duration,
) -> Result<Epic, BoardError> {
    let agent = Agent::new_with_config(settings(Agent::config_builder(), timeout));
    let http = Http {
        agent: &agent,
        port,
        bearer,
    };
    read_with(&http, port, board, ticket)
}

/// The client's settings, laid over `builder` (architect, 2026-11-25, for v0.11-T02, pin P3, each
/// checked against ureq 3.5.2's `src/config.rs`):
/// - `proxy(None)`: ureq's default config reads a proxy from the environment, and the board read
///   MUST go to 127.0.0.1 directly.
/// - `max_redirects(0)`: no redirect is followed, and the 3xx itself is returned.
/// - `http_status_as_error(false)`: every status is an answer, for this module to map.
/// - `timeout_global(Some(timeout))`: one budget from the connection's start to the body's last
///   byte.
/// - `accept_encoding("identity")`: no compressed answer is asked for, whatever ureq's features.
pub(crate) fn settings(builder: ConfigBuilder<AgentScope>, timeout: Duration) -> Config {
    builder
        .proxy(None)
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .accept_encoding("identity")
        .build()
}

/// One body, as far as it was read.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BodyRead {
    /// The whole body, at most [`BODY_LIMIT`] bytes.
    Bytes(Vec<u8>),
    /// A `Content-Length` above the bound, or more bytes than the bound as they arrived.
    TooLarge,
    /// The connection failed, was closed early or ran past its time before the body ended.
    Failed,
}

/// One request's answer, as far as the read takes it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Reply {
    /// No connection, a connection refused, reset or closed before the head arrived, an answer
    /// that is not HTTP, or the time ran out.
    NoAnswer,
    /// An answer: its status, whether its media type is localswim's Problem document's, and its
    /// body when its status is one whose body is read.
    Answer {
        status: u16,
        problem_json: bool,
        body: Option<BodyRead>,
    },
}

/// A board's two GETs. The real one is [`Http`]; the tests give stand-ins. It has no way to name a
/// method.
pub(crate) trait Get {
    /// One GET of `target`, the path and query after the base. The body is read only when the
    /// status is in `read_body_of`.
    fn get(&self, target: &str, read_body_of: &[u16]) -> Reply;
}

/// The real [`Get`], over ureq.
struct Http<'a> {
    agent: &'a Agent,
    port: &'a str,
    bearer: &'a Bearer,
}

impl Get for Http<'_> {
    fn get(&self, target: &str, read_body_of: &[u16]) -> Reply {
        let url = format!("{BASE_PREFIX}{}{target}", self.port);
        let Ok(mut response) = send_get(self.agent, &url, self.bearer) else {
            return Reply::NoAnswer;
        };
        let status = response.status().as_u16();
        let problem_json = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(is_problem_json);
        let body = read_body_of.contains(&status).then(|| {
            let length = response.body().content_length();
            read_bounded(response.body_mut().as_reader(), length)
        });
        // Any other body is left unread, and dropping the response closes its connection.
        Reply::Answer {
            status,
            problem_json,
            body,
        }
    }
}

/// The crate's one way to a socket: a GET of `url`, with `Authorization: <bearer>` and no body.
/// It takes no method, and calls ureq's `get`, so no code path can send another (D86).
fn send_get(agent: &Agent, url: &str, bearer: &Bearer) -> Result<Response<Body>, ()> {
    let mut authorization = HeaderValue::from_str(bearer.as_str()).map_err(|_| ())?;
    authorization.set_sensitive(true);
    agent
        .get(url)
        .header(AUTHORIZATION, authorization)
        .call()
        .map_err(|_| ())
}

/// Whether a `Content-Type` is localswim's Problem document's media type: its type and subtype,
/// compared without regard to case, with any parameters ignored (lease-api.md, The board read;
/// architect, 2026-11-25, for v0.11-T01, pin P26).
pub(crate) fn is_problem_json(content_type: &str) -> bool {
    content_type
        .split(';')
        .next()
        .is_some_and(|media| media.trim().eq_ignore_ascii_case(PROBLEM_JSON))
}

/// Reads a body under [`BODY_LIMIT`]: a `length` above it is refused before a byte is read, and
/// no more than one byte past it is ever read from `reader`.
pub(crate) fn read_bounded(reader: impl Read, length: Option<u64>) -> BodyRead {
    if length.is_some_and(|n| n > BODY_LIMIT) {
        return BodyRead::TooLarge;
    }
    let mut bytes = Vec::new();
    match reader.take(BODY_LIMIT + 1).read_to_end(&mut bytes) {
        Ok(_) if bytes.len() as u64 > BODY_LIMIT => BodyRead::TooLarge,
        Ok(_) => BodyRead::Bytes(bytes),
        Err(_) => BodyRead::Failed,
    }
}

/// The read over any [`Get`]: the ticket view, then after localswim's `not_found` the summary
/// view, mapped to the table's outcomes.
pub(crate) fn read_with(
    get: &dyn Get,
    port: &str,
    board: &str,
    ticket: u64,
) -> Result<Epic, BoardError> {
    let no_answer = || BoardError::Internal(no_answer_text(port));
    let ticket_view =
        format!("{API_PREFIX}/boards/{board}?view=ticket&ticket={ticket}&include_prose=true");
    match get.get(&ticket_view, &[200, 404]) {
        Reply::Answer {
            status: 200, body, ..
        } => match body {
            Some(BodyRead::Bytes(bytes)) => {
                parse_ticket(&bytes).ok_or_else(|| internal(BAD_ANSWER))
            }
            Some(BodyRead::TooLarge) => Err(BoardError::TooLarge),
            Some(BodyRead::Failed) | None => Err(no_answer()),
        },
        Reply::Answer {
            status: 401 | 403, ..
        } => Err(internal(REFUSED)),
        Reply::Answer {
            status: 404,
            problem_json,
            body,
        } if is_not_found(problem_json, body.as_ref()) => {
            let summary_view = format!("{API_PREFIX}/boards/{board}?view=summary");
            match get.get(&summary_view, &[404]) {
                Reply::Answer { status: 200, .. } => Err(BoardError::UnknownTicket),
                Reply::Answer {
                    status: 404,
                    problem_json,
                    body,
                } if is_not_found(problem_json, body.as_ref()) => Err(BoardError::UnknownBoard),
                Reply::Answer {
                    status: 401 | 403, ..
                } => Err(internal(REFUSED)),
                Reply::Answer { .. } | Reply::NoAnswer => Err(no_answer()),
            }
        }
        Reply::Answer { .. } | Reply::NoAnswer => Err(no_answer()),
    }
}

/// Whether a 404 is localswim's `not_found`: the Problem document's media type, and a body that is
/// one JSON object whose `code` is `not_found`. A body that was not read whole is not.
fn is_not_found(problem_json: bool, body: Option<&BodyRead>) -> bool {
    let Some(BodyRead::Bytes(bytes)) = body else {
        return false;
    };
    problem_json
        && serde_json::from_slice::<Value>(bytes).is_ok_and(|value| {
            value
                .as_object()
                .and_then(|object| object.get("code"))
                .is_some_and(|code| code.as_str() == Some("not_found"))
        })
}

/// A 200's body read as localswim's `TicketInspection`, of which three members are taken and every
/// other is ignored. The whole body is read as JSON first, so a description that cannot be read
/// as JSON, such as one holding an unpaired surrogate, refuses the answer whole. `None` is
/// [`BAD_ANSWER`].
pub(crate) fn parse_ticket(bytes: &[u8]) -> Option<Epic> {
    let Value::Object(mut top) = serde_json::from_slice::<Value>(bytes).ok()? else {
        return None;
    };
    let Value::Object(card) = top.remove("card")? else {
        return None;
    };
    let revision = revision(card.get("revision")?)?;
    let archived = card.get("archived")?.as_bool()?;
    let Value::Object(mut prose) = top.remove("prose")? else {
        return None;
    };
    let Value::String(description) = prose.remove("description")? else {
        return None;
    };
    Some(Epic {
        description,
        archived,
        revision,
    })
}

/// `card.revision`: a JSON integer written as digits alone, from 1 to 9007199254740991. `5.0`,
/// `5e0`, `-5`, `0`, `"5"` and a number above the bound are refused (architect, 2026-11-25, for
/// v0.11-T01, pin P26). serde_json gives a `u64` only for a number written as digits alone: it
/// reads `5.0` and `5e0` as floats and `-5` as a negative integer, with or without its
/// `arbitrary_precision` feature.
fn revision(value: &Value) -> Option<u64> {
    let Value::Number(number) = value else {
        return None;
    };
    number
        .as_u64()
        .filter(|revision| (1..=MAX_INTEGER).contains(revision))
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
