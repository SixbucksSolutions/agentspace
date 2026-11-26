//! `read_epic`: the one GET that reads an epic (docs/lease-api.md, "The board read", "Each
//! outcome" and "What is read from a 200 answer"), against a stand-in board on a port the OS
//! picked (docs/conformance.md, "The stand-in board").
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every expectation is the spec's, never
//! the code's: the message of each `Internal` is the table's text between `agentspace lease
//! create failed: ` and `. Nothing was recorded.`, word for word, with no final period.
//!
//! Every test asserts that every request the stand-in saw was a GET with exactly the Bearer
//! header (`World::requests`), and counts the requests: one for a card, two for an unknown
//! ticket or board. No test reads Terry's receipt, credential or board, and the stand-in's port
//! is never 8811. The token is a stand-in drawn for each test and never printed.

mod common;

use agentspace_board::BoardError;
use common::*;
use std::time::{Duration, Instant};

const DESCRIPTION: &str = "An epic's description";

fn standard() -> World {
    World::new(Behavior::standard())
}

// ---------------------------------------------------------------------------------------------
// A card.

#[test]
fn a_card_reads_with_its_description_archived_flag_and_revision() {
    let world = standard();
    let epic = world.ok(world.read(BOARD, 52));
    assert_eq!(epic.description, DESCRIPTION);
    assert!(!epic.archived);
    assert_eq!(epic.revision, 7);
    let reqs = world.requests();
    assert_eq!(reqs.len(), 1, "one GET reads a card");
    assert_ticket_view(&reqs[0], BOARD, 52);
}

#[test]
fn the_stand_in_is_never_localswims_port() {
    let world = standard();
    assert_ne!(world.standin.port, 8811);
    let base = world.standin.base();
    assert!(base.starts_with("http://127.0.0.1:"));
    let receipt = std::fs::read_to_string(world.receipt_file()).unwrap();
    assert!(receipt.contains(&base));
}

#[test]
fn every_part_the_client_has_no_use_for_is_ignored() {
    // The stand-in fills comments, relationships, the parent, the children and a related card,
    // each with a revision, an archived flag or a description of its own, so a client that read
    // the wrong one would answer with that value. Only three members are read.
    let world = World::new(Behavior::standard().with_card(BOARD, 53, Card::new(9, true, "Archived one")));
    let epic = world.ok(world.read(BOARD, 52));
    assert_eq!((epic.revision, epic.archived), (7, false));
    assert_eq!(epic.description, DESCRIPTION);
    let epic = world.ok(world.read(BOARD, 53));
    assert_eq!((epic.revision, epic.archived), (9, true));
    assert_eq!(epic.description, "Archived one");
    assert_eq!(world.requests().len(), 2);
}

#[test]
fn each_ticket_is_asked_for_by_its_number_alone() {
    for ticket in [1u64, 52, 4_194_304, 9_007_199_254_740_991] {
        let world = World::new(Behavior::empty().with_card(BOARD, ticket, Card::new(3, false, "x")));
        let epic = world.ok(world.read(BOARD, ticket));
        assert_eq!(epic.revision, 3);
        let reqs = world.requests();
        assert_eq!(reqs.len(), 1);
        assert_ticket_view(&reqs[0], BOARD, ticket);
    }
}

#[test]
fn the_description_comes_back_exactly() {
    let cases = [
        "",
        "Plain prose.",
        "Line one\r\n```agentspace-plan\r\n{\"a\": 1}\r\n```\r\nmixed\nends\n",
        "Unicode: \u{e9} \u{1F600} \u{2028} \u{FEFF}start \t tab",
        "Quotes \" and \\ backslash and / slash",
        "  leading and trailing space  ",
    ];
    for description in cases {
        let world = World::new(Behavior::empty().with_card(BOARD, 52, Card::new(1, false, description)));
        let epic = world.ok(world.read(BOARD, 52));
        assert_eq!(epic.description, description);
        assert_eq!(world.requests().len(), 1);
    }
}

#[test]
fn a_surrogate_pair_in_the_description_is_text() {
    let raw = Card::raw("7", "false", "\"a\\ud83d\\ude00b\"");
    let world = World::new(Behavior::empty().with_card(BOARD, 52, raw));
    let epic = world.ok(world.read(BOARD, 52));
    assert_eq!(epic.description, "a\u{1F600}b");
}

#[test]
fn the_revision_bounds_are_accepted() {
    for revision in [1u64, 2, 9_007_199_254_740_990, 9_007_199_254_740_991] {
        let world = World::new(Behavior::empty().with_card(BOARD, 52, Card::new(revision, false, "x")));
        let epic = world.ok(world.read(BOARD, 52));
        assert_eq!(epic.revision, revision);
    }
}

#[test]
fn an_archived_card_reads_ok_with_archived_true() {
    // The read reports it; the lease CLI is what refuses an archived epic.
    let world = World::new(Behavior::empty().with_card(BOARD, 52, Card::new(12, true, "Archived epic")));
    let epic = world.ok(world.read(BOARD, 52));
    assert!(epic.archived);
    assert_eq!(epic.revision, 12);
    assert_eq!(epic.description, "Archived epic");
    assert_eq!(world.requests().len(), 1);
}

#[test]
fn a_card_on_the_other_of_two_boards_is_read_from_that_board() {
    let behavior = Behavior::empty()
        .with_card(BOARD, 52, Card::new(7, false, "Mine"))
        .with_card(OTHER_BOARD, 52, Card::new(8, false, "Theirs"));
    let world = World::new(behavior);
    let epic = world.ok(world.read(OTHER_BOARD, 52));
    assert_eq!((epic.revision, epic.description.as_str()), (8, "Theirs"));
    assert_ticket_view(&world.requests()[0], OTHER_BOARD, 52);
}

#[test]
fn nothing_is_kept_between_reads() {
    // The receipt and the credential are read afresh for each read (lease-api.md: "read afresh
    // for each create, whole, and closed at once").
    let world = standard();
    world.ok(world.read(BOARD, 52));
    world.remove_receipt();
    let msg = world.internal(world.read(BOARD, 52));
    assert_eq!(msg, MSG_NO_RECEIPT);
    world.write_receipt_base(serde_json::Value::String(world.standin.base()));
    world.ok(world.read(BOARD, 52));
    world.remove_bot();
    let msg = world.internal(world.read(BOARD, 52));
    assert_eq!(msg, MSG_CREDENTIAL);
    world.write_bot(&world.good_bot_text());
    world.ok(world.read(BOARD, 52));
    assert_eq!(world.requests().len(), 3);
}

// ---------------------------------------------------------------------------------------------
// An unknown ticket, an unknown board: a second GET tells them apart.

#[test]
fn an_unknown_ticket_is_told_by_the_summary_view() {
    let world = standard();
    let result = world.read(BOARD, 99);
    assert!(
        matches!(result, Err(BoardError::UnknownTicket)),
        "expected UnknownTicket, got {}",
        name_of(&result)
    );
    let reqs = world.requests();
    assert_eq!(reqs.len(), 2, "a refused read costs two GETs");
    assert_ticket_view(&reqs[0], BOARD, 99);
    assert_summary_view(&reqs[1], BOARD);
}

#[test]
fn an_unknown_board_is_told_by_the_summary_view() {
    let world = standard();
    let result = world.read(OTHER_BOARD, 52);
    assert!(
        matches!(result, Err(BoardError::UnknownBoard)),
        "expected UnknownBoard, got {}",
        name_of(&result)
    );
    let reqs = world.requests();
    assert_eq!(reqs.len(), 2);
    assert_ticket_view(&reqs[0], OTHER_BOARD, 52);
    assert_summary_view(&reqs[1], OTHER_BOARD);
}

#[test]
fn a_known_board_with_no_cards_has_no_such_ticket() {
    let world = World::new(Behavior::empty().with_board(BOARD));
    let result = world.read(BOARD, 1);
    assert!(matches!(result, Err(BoardError::UnknownTicket)), "got {}", name_of(&result));
    assert_eq!(world.requests().len(), 2);
}

#[test]
fn the_media_type_of_a_not_found_is_compared_without_case_and_parameters_are_ignored() {
    for media in [
        "application/problem+json",
        "Application/Problem+JSON; charset=utf-8",
        "APPLICATION/PROBLEM+JSON",
        "application/problem+json;charset=UTF-8",
    ] {
        let world = World::new(Behavior::standard().media(media));
        let result = world.read(BOARD, 99);
        assert!(matches!(result, Err(BoardError::UnknownTicket)), "got {}", name_of(&result));
        assert_eq!(world.requests().len(), 2);
        let result = world.read(OTHER_BOARD, 99);
        assert!(matches!(result, Err(BoardError::UnknownBoard)), "got {}", name_of(&result));
        assert_eq!(world.requests().len(), 4);
    }
}

#[test]
fn a_404_in_another_media_type_is_not_a_not_found() {
    for media in [
        "application/json",
        "text/plain",
        "text/html",
        "application/problem",
        "application/problem+jsonx",
        "application/x-problem+json",
        "application/problem+xml",
    ] {
        let world = World::new(Behavior::standard().media(media));
        let msg = world.internal(world.read(BOARD, 99));
        assert_eq!(msg, msg_no_answer(world.standin.port), "media {media}");
        assert_eq!(world.requests().len(), 1, "no second GET for media {media}");
    }
}

#[test]
fn a_404_with_no_content_type_at_all_is_not_a_not_found() {
    let world = World::new(Behavior::standard().answer(Answer::bare(404)));
    let msg = world.internal(world.read(BOARD, 52));
    assert_eq!(msg, msg_no_answer(world.standin.port));
    assert_eq!(world.requests().len(), 1);
}

#[test]
fn a_404_problem_with_another_code_or_a_body_that_is_not_one_is_not_a_not_found() {
    let media = Some("application/problem+json");
    let bodies = [
        problem_body(404, "conflict"),
        problem_body(404, "invalid_request"),
        problem_body(404, "internal_error"),
        problem_body(404, ""),
        problem_body(404, "NOT_FOUND"),
        problem_body(404, "not_found "),
        String::new(),
        "not json".to_string(),
        "[]".to_string(),
        "null".to_string(),
        "{}".to_string(),
        "{\"code\": 404}".to_string(),
        "{\"code\": null}".to_string(),
        "{\"code\": [\"not_found\"]}".to_string(),
        "{\"status\": 404, \"title\": \"Not Found\"}".to_string(),
    ];
    for (i, body) in bodies.iter().enumerate() {
        let world = World::new(Behavior::standard().answer(Answer::raw(404, media, body)));
        let msg = world.internal(world.read(BOARD, 52));
        assert_eq!(msg, msg_no_answer(world.standin.port), "body {i}");
        assert_eq!(world.requests().len(), 1, "no second GET for body {i}");
    }
}

#[test]
fn a_404_that_is_a_not_found_with_other_members_still_counts() {
    let body = r#"{"type":"about:blank","title":"x","status":404,"code":"not_found","detail":"y","request_id":"r","outcome":"rejected","errors":[],"extra":{"a":[1,2]}}"#;
    let behavior = Behavior::standard().answer(Answer::raw(404, Some("application/problem+json"), body));
    // The `answer` override covers the summary view too, so the second GET is also this 404.
    let world = World::new(behavior);
    let result = world.read(BOARD, 52);
    assert!(
        matches!(result, Err(BoardError::UnknownBoard)),
        "a 404 for the summary view too is an unknown board, got {}",
        name_of(&result)
    );
    assert_eq!(world.requests().len(), 2);
}

#[test]
fn after_a_not_found_the_summary_views_other_outcomes_are_their_own() {
    let cases: Vec<(&str, Answer, Option<String>)> = vec![
        ("401", Answer::problem(401, "unauthorized"), Some(MSG_REFUSED.to_string())),
        ("403", Answer::problem(403, "forbidden"), Some(MSG_REFUSED.to_string())),
        ("401 bare", Answer::bare(401), Some(MSG_REFUSED.to_string())),
        ("400", Answer::problem(400, "invalid_request"), None),
        ("405", Answer::problem(405, "method_not_allowed"), None),
        ("406", Answer::problem(406, "not_acceptable"), None),
        ("500", Answer::problem(500, "internal_error"), None),
        ("503", Answer::bare(503), None),
        ("404 bare", Answer::bare(404), None),
        ("404 other code", Answer::problem(404, "conflict"), None),
        ("302", Answer::redirect(302, "/api/2026-11-10/elsewhere"), None),
        ("201", Answer::raw(201, Some("application/json"), "{}"), None),
        ("closed", Answer::Close, None),
    ];
    for (label, summary, expected) in cases {
        let world = World::new(Behavior::standard().summary(summary));
        let msg = world.internal(world.read(BOARD, 99));
        let expected = expected.unwrap_or_else(|| msg_no_answer(world.standin.port));
        assert_eq!(msg, expected, "summary answer {label}");
        let reqs = world.requests();
        assert_eq!(reqs.len(), 2, "summary answer {label}");
        assert_summary_view(&reqs[1], BOARD);
    }
}

// ---------------------------------------------------------------------------------------------
// A refused credential.

#[test]
fn a_401_or_a_403_is_a_refused_credential_and_is_not_tried_again() {
    for (status, code) in [(401u16, "unauthorized"), (403, "forbidden")] {
        for answer in [Answer::problem(status, code), Answer::bare(status)] {
            let world = World::new(Behavior::standard().answer(answer));
            let msg = world.internal(world.read(BOARD, 52));
            assert_eq!(msg, MSG_REFUSED, "status {status}");
            assert_eq!(world.requests().len(), 1, "status {status} is not tried again");
        }
    }
}

#[test]
fn a_token_the_board_does_not_know_is_a_refused_credential() {
    // bot.json holds a token other than the stand-in's: the stand-in's own check answers 401, so
    // this shows the client sends the token it decrypted, in the Authorization header.
    let world = standard();
    let other = fresh_token();
    assert_ne!(other, world.standin.token());
    world.write_bot(&bot_json(&hex(&blob_for(&utf16le(&other)))));
    let msg = world.internal(world.read(BOARD, 52));
    assert_eq!(msg, MSG_REFUSED);
    let reqs = world.standin.requests();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].method, "GET");
    assert!(!reqs[0].auth_ok, "the request did not carry the stand-in's token");
}

#[test]
fn the_token_is_sent_exactly_as_the_bearer_header() {
    // `World::requests` fails any request whose Authorization was not exactly `Bearer <token>`;
    // a read that works shows it was, and the count shows one request carried it.
    let world = standard();
    world.ok(world.read(BOARD, 52));
    let reqs = world.requests();
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].auth_ok);
    assert!(reqs[0].headers.iter().any(|h| h == "authorization"));
}

// ---------------------------------------------------------------------------------------------
// A board that does not answer.

#[test]
fn a_port_with_no_listener_does_not_answer() {
    let world = standard();
    let port = dead_port();
    assert_ne!(port, 8811);
    world.write_receipt_base(serde_json::Value::String(format!("http://127.0.0.1:{port}")));
    let msg = world.internal(world.read(BOARD, 52));
    assert_eq!(msg, msg_no_answer(port), "the message names the base's port");
    assert!(world.requests().is_empty(), "the stand-in was never asked");
}

#[test]
fn a_connection_closed_with_no_answer_does_not_answer() {
    let world = World::new(Behavior::standard().answer(Answer::Close));
    let msg = world.internal(world.read(BOARD, 52));
    assert_eq!(msg, msg_no_answer(world.standin.port));
    assert_eq!(world.requests().len(), 1, "a read that fails is not tried again");
}

#[test]
fn an_answer_that_stops_short_does_not_answer() {
    for chunked in [false, true] {
        let world = World::new(Behavior::standard().answer(Answer::Truncated { chunked }));
        let msg = world.internal(world.read(BOARD, 52));
        assert_eq!(msg, msg_no_answer(world.standin.port), "chunked: {chunked}");
        assert_eq!(world.requests().len(), 1);
    }
}

/// The one test that waits the full 30 seconds: a board that reads the request and never
/// answers. Each request has 30 seconds, from the start of its connection to the last byte of its
/// answer (lease-api.md, Bounds). The read runs on its own thread, so that a client with no
/// timeout fails this test at 90 seconds and does not wedge the run.
#[test]
fn slow_a_board_that_never_answers_is_given_up_on_after_30_seconds() {
    use std::sync::{Arc, mpsc};
    let world = Arc::new(World::new(Behavior::standard().answer(Answer::Hang)));
    let (tx, rx) = mpsc::channel();
    let started = Instant::now();
    {
        let world = Arc::clone(&world);
        std::thread::spawn(move || {
            let outcome = match world.read(BOARD, 52) {
                Err(BoardError::Internal(msg)) => Ok(msg),
                other => Err(name_of(&other)),
            };
            let _ = tx.send(outcome);
        });
    }
    let outcome = rx
        .recv_timeout(Duration::from_secs(90))
        .expect("the read did not end within 90 seconds");
    let elapsed = started.elapsed();
    let msg = outcome.unwrap_or_else(|name| panic!("expected Internal, got {name}"));
    world.check_message(&msg);
    assert_eq!(msg, msg_no_answer(world.standin.port));
    assert!(
        elapsed >= Duration::from_secs(25),
        "ended after {elapsed:?}, before the 30 seconds were up"
    );
    assert!(
        elapsed < Duration::from_secs(45),
        "ended after {elapsed:?}, long past the 30 seconds"
    );
    assert_eq!(world.requests().len(), 1);
}

#[test]
fn every_other_status_does_not_answer() {
    let json = Some("application/json");
    let ok_body = Card::new(7, false, "x").body(BOARD, 52, true);
    let cases: Vec<(&str, Answer)> = vec![
        ("201", Answer::raw(201, json, &ok_body)),
        ("202", Answer::raw(202, json, &ok_body)),
        ("204", Answer::raw(204, None, "")),
        ("400", Answer::problem(400, "invalid_request")),
        ("400 bare", Answer::bare(400)),
        ("404 bare", Answer::bare(404)),
        ("405", Answer::problem(405, "method_not_allowed")),
        ("406", Answer::problem(406, "not_acceptable")),
        ("409", Answer::problem(409, "conflict")),
        ("429", Answer::bare(429)),
        ("500", Answer::problem(500, "internal_error")),
        ("502", Answer::bare(502)),
        ("503", Answer::problem(503, "unavailable")),
    ];
    for (label, answer) in cases {
        let world = World::new(Behavior::standard().answer(answer));
        let msg = world.internal(world.read(BOARD, 52));
        assert_eq!(msg, msg_no_answer(world.standin.port), "status {label}");
        assert_eq!(world.requests().len(), 1, "status {label}");
    }
}

#[test]
fn a_redirect_is_never_followed() {
    for status in [301u16, 302, 303, 307, 308] {
        let world = World::new(
            Behavior::standard().answer(Answer::redirect(status, "/api/2026-11-10/elsewhere")),
        );
        let msg = world.internal(world.read(BOARD, 52));
        assert_eq!(msg, msg_no_answer(world.standin.port), "status {status}");
        assert_eq!(
            world.standin.requests().len(),
            1,
            "a {status} was followed (a second request reached the stand-in)"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// A 200 whose body is not the shape.

fn description_ok() -> &'static str {
    r#""prose":{"description":"x","comments":[]}"#
}

#[test]
fn a_200_that_is_not_the_shape_could_not_be_read() {
    let good_card = r#""card":{"revision":7,"archived":false}"#;
    let prose = description_ok();
    let bodies: Vec<(&str, String)> = vec![
        ("empty", String::new()),
        ("not json", "not json".to_string()),
        ("an array", "[]".to_string()),
        ("null", "null".to_string()),
        ("a number", "42".to_string()),
        ("a string", "\"x\"".to_string()),
        ("an empty object", "{}".to_string()),
        ("trailing text", format!("{{{good_card},{prose}}} trailing")),
        ("two documents", format!("{{{good_card},{prose}}}{{}}")),
        ("unterminated", format!("{{{good_card},\"prose\":{{\"description\":\"x\"")),
        ("no card", format!("{{{prose}}}")),
        ("card null", format!("{{\"card\":null,{prose}}}")),
        ("card a number", format!("{{\"card\":5,{prose}}}")),
        ("card an array", format!("{{\"card\":[],{prose}}}")),
        ("card empty", format!("{{\"card\":{{}},{prose}}}")),
        ("no revision", format!("{{\"card\":{{\"archived\":false}},{prose}}}")),
        ("no archived", format!("{{\"card\":{{\"revision\":7}},{prose}}}")),
        ("archived a string", format!("{{\"card\":{{\"revision\":7,\"archived\":\"false\"}},{prose}}}")),
        ("archived a string true", format!("{{\"card\":{{\"revision\":7,\"archived\":\"true\"}},{prose}}}")),
        ("archived 0", format!("{{\"card\":{{\"revision\":7,\"archived\":0}},{prose}}}")),
        ("archived 1", format!("{{\"card\":{{\"revision\":7,\"archived\":1}},{prose}}}")),
        ("archived null", format!("{{\"card\":{{\"revision\":7,\"archived\":null}},{prose}}}")),
        ("no prose", format!("{{{good_card}}}")),
        ("prose null", format!("{{{good_card},\"prose\":null}}")),
        ("prose a string", format!("{{{good_card},\"prose\":\"x\"}}")),
        ("prose an array", format!("{{{good_card},\"prose\":[]}}")),
        ("prose empty", format!("{{{good_card},\"prose\":{{}}}}")),
        ("prose with only comments", format!("{{{good_card},\"prose\":{{\"comments\":[]}}}}")),
        ("description null", format!("{{{good_card},\"prose\":{{\"description\":null,\"comments\":[]}}}}")),
        ("description a number", format!("{{{good_card},\"prose\":{{\"description\":5,\"comments\":[]}}}}")),
        ("description an array", format!("{{{good_card},\"prose\":{{\"description\":[\"x\"],\"comments\":[]}}}}")),
        ("description an object", format!("{{{good_card},\"prose\":{{\"description\":{{}},\"comments\":[]}}}}")),
        // The three are looked for where the spec puts them, never at the top level.
        (
            "members at the top level",
            "{\"revision\":7,\"archived\":false,\"description\":\"x\"}".to_string(),
        ),
        (
            "description on the card",
            "{\"card\":{\"revision\":7,\"archived\":false,\"description\":\"x\"}}".to_string(),
        ),
    ];
    for (label, body) in bodies {
        let world = World::new(Behavior::standard().answer(Answer::ok_body(&body)));
        let msg = world.internal(world.read(BOARD, 52));
        assert_eq!(msg, MSG_ANSWER, "body: {label}");
        assert_eq!(world.requests().len(), 1, "body: {label}");
    }
}

#[test]
fn a_revision_that_is_not_digits_alone_from_1_to_the_bound_could_not_be_read() {
    for revision in [
        "5.0",
        "5e0",
        "5E0",
        "5e+0",
        "50e-1",
        "-5",
        "-0",
        "0",
        "\"5\"",
        "\"\"",
        "9007199254740992",
        "9007199254740993",
        "18446744073709551616",
        "99999999999999999999",
        "5.5",
        "0.5",
        "1e2",
        "true",
        "false",
        "null",
        "[5]",
        "{}",
        "05",
        "-",
    ] {
        let card = Card::raw(revision, "false", "\"x\"");
        let world = World::new(Behavior::empty().with_card(BOARD, 52, card));
        let msg = world.internal(world.read(BOARD, 52));
        assert_eq!(msg, MSG_ANSWER, "revision {revision}");
        assert_eq!(world.requests().len(), 1, "revision {revision}");
    }
}

#[test]
fn a_description_that_is_not_valid_text_makes_the_whole_answer_unreadable() {
    // An unpaired surrogate in the description: the answer is read as JSON before the
    // description is looked at, so this is "the answer could not be read", never anything about
    // the plan.
    for description in [
        "\"\\ud800\"",
        "\"\\udc00\"",
        "\"\\ud800x\"",
        "\"ab\\ud800\"",
        "\"\\ud800\\ud800\"",
        "\"\\udc00\\ud800\"",
        "\"```agentspace-plan\\n{}\\n```\\ud800\"",
    ] {
        let card = Card::raw("7", "false", description);
        let world = World::new(Behavior::empty().with_card(BOARD, 52, card));
        let msg = world.internal(world.read(BOARD, 52));
        assert_eq!(msg, MSG_ANSWER, "description {description}");
        assert_eq!(world.requests().len(), 1);
    }
}

// ---------------------------------------------------------------------------------------------
// The 64 MiB bound. A declared length is refused before the body is read, so those cases send no
// body at all (`send: 0`, the connection held open): a client that waited for it would run to
// the 30 seconds. The bodies that are streamed are built block by block from one 64 KiB buffer,
// so no test allocates 64 MiB.

fn big(status: u16, problem: bool, total: u64, chunked: bool, send: u64, hold: bool) -> Answer {
    Answer::Big {
        status,
        problem,
        total,
        chunked,
        send,
        hold,
    }
}

#[test]
fn a_declared_length_over_the_bound_is_refused_before_it_is_read() {
    let world = World::new(Behavior::standard().answer(big(200, false, BOUND + 1, false, 0, true)));
    let started = Instant::now();
    let result = world.read(BOARD, 52);
    assert!(matches!(result, Err(BoardError::TooLarge)), "got {}", name_of(&result));
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "the refusal waited for a body that was never sent"
    );
    assert_eq!(world.requests().len(), 1);
}

#[test]
fn a_much_larger_declared_length_is_refused_too() {
    let world = World::new(Behavior::standard().answer(big(200, false, 10 * BOUND, false, 0, true)));
    let result = world.read(BOARD, 52);
    assert!(matches!(result, Err(BoardError::TooLarge)), "got {}", name_of(&result));
    assert_eq!(world.requests().len(), 1);
}

#[test]
fn a_chunked_body_that_grows_past_the_bound_is_refused_once_the_next_byte_arrives() {
    // The stand-in sends 64 MiB and one byte in chunks, then holds the connection open with no
    // last chunk: a client that read to the end of the body would wait out the 30 seconds.
    let world = World::new(Behavior::standard().answer(big(200, false, BOUND + 1, true, BOUND + 1, true)));
    let started = Instant::now();
    let result = world.read(BOARD, 52);
    assert!(matches!(result, Err(BoardError::TooLarge)), "got {}", name_of(&result));
    assert!(started.elapsed() < Duration::from_secs(25));
    assert_eq!(world.requests().len(), 1);
}

#[test]
fn a_body_of_exactly_the_bound_is_read_as_declared() {
    let world = World::new(Behavior::standard().answer(big(200, false, BOUND, false, BOUND, false)));
    let epic = world.ok(world.read(BOARD, 52));
    assert_eq!(epic.revision, 7);
    assert_eq!(epic.description, "A big answer's description");
    assert_eq!(world.requests().len(), 1);
}

#[test]
fn a_chunked_body_of_exactly_the_bound_is_read() {
    let world = World::new(Behavior::standard().answer(big(200, false, BOUND, true, BOUND, false)));
    let epic = world.ok(world.read(BOARD, 52));
    assert_eq!(epic.revision, 7);
    assert_eq!(epic.description, "A big answer's description");
    assert_eq!(world.requests().len(), 1);
}

#[test]
fn a_404_body_over_the_bound_is_not_a_not_found() {
    // "A 404's body is read under the same bound ...; one larger than the bound is not [localswim's
    // not_found]": "did not answer", never TooLarge, and no second GET.
    let world = World::new(Behavior::standard().answer(big(404, true, BOUND + 1, false, 0, true)));
    let msg = world.internal(world.read(BOARD, 52));
    assert_eq!(msg, msg_no_answer(world.standin.port));
    assert_eq!(world.requests().len(), 1);
}

// ---------------------------------------------------------------------------------------------
// The receipt: found, read, and refused before any request.

#[test]
fn an_unset_empty_or_blank_localappdata_is_refused() {
    for value in [None, Some(""), Some(" "), Some("   \t ")] {
        let mut world = standard();
        world.ctx.local_app_data = value.map(std::path::PathBuf::from);
        let msg = world.internal(world.read(BOARD, 52));
        assert_eq!(msg, MSG_LOCALAPPDATA, "LOCALAPPDATA {value:?}");
        assert!(world.requests().is_empty(), "LOCALAPPDATA {value:?} made a request");
    }
}

#[test]
fn no_receipt_means_localswim_may_not_be_running() {
    let world = standard();
    world.remove_receipt();
    let msg = world.internal(world.read(BOARD, 52));
    assert_eq!(msg, MSG_NO_RECEIPT);
    assert!(world.requests().is_empty());
    // Not even its folder.
    std::fs::remove_dir_all(world.local().join("localswim-v2")).unwrap();
    let msg = world.internal(world.read(BOARD, 52));
    assert_eq!(msg, MSG_NO_RECEIPT);
    // Not even the local application data folder.
    std::fs::remove_dir_all(world.local()).unwrap();
    let msg = world.internal(world.read(BOARD, 52));
    assert_eq!(msg, MSG_NO_RECEIPT);
    assert!(world.requests().is_empty());
}

#[test]
fn a_receipt_that_cannot_be_read_or_breaks_localswims_rules_is_refused() {
    let world = standard();
    let base = world.standin.base();
    let dir = world.cred_dir().to_string_lossy().into_owned();
    let dir_json = serde_json::to_string(&dir).unwrap();
    let base_json = serde_json::to_string(&base).unwrap();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("not json", b"not json".to_vec()),
        ("an array", b"[]".to_vec()),
        ("null", b"null".to_vec()),
        ("an empty object", b"{}".to_vec()),
        ("no credential_directory", format!("{{\"base\":{base_json}}}").into_bytes()),
        (
            "credential_directory twice",
            format!("{{\"base\":{base_json},\"credential_directory\":{dir_json},\"credential_directory\":{dir_json}}}")
                .into_bytes(),
        ),
        (
            "base twice",
            format!("{{\"base\":{base_json},\"base\":{base_json},\"credential_directory\":{dir_json}}}")
                .into_bytes(),
        ),
        (
            "credential_directory not a string",
            format!("{{\"base\":{base_json},\"credential_directory\":5}}").into_bytes(),
        ),
        (
            "trailing text",
            format!("{{\"base\":{base_json},\"credential_directory\":{dir_json}}} x").into_bytes(),
        ),
        ("no base", format!("{{\"credential_directory\":{dir_json}}}").into_bytes()),
        (
            "base not a string",
            format!("{{\"base\":5,\"credential_directory\":{dir_json}}}").into_bytes(),
        ),
        (
            "base null",
            format!("{{\"base\":null,\"credential_directory\":{dir_json}}}").into_bytes(),
        ),
        (
            "not UTF-8",
            [
                b"{\"base\":".as_slice(),
                &[0xFF, 0xFE, 0xFD],
                b",\"credential_directory\":\"x\"}".as_slice(),
            ]
            .concat(),
        ),
    ];
    for (label, bytes) in cases {
        world.write_receipt(&bytes);
        let result = world.read(BOARD, 52);
        assert!(
            matches!(result, Err(BoardError::Internal(_))),
            "receipt {label}: expected Internal, got {}",
            name_of(&result)
        );
        let msg = world.internal(result);
        assert_eq!(msg, MSG_RECEIPT, "receipt: {label}");
    }
    assert!(world.requests().is_empty(), "a refused receipt made a request");
}

/// lease-api.md, "The board read", says the receipt is "one JSON object in UTF-8, a leading byte
/// order mark accepted". A file in UTF-16 is not UTF-8, so this expects it refused. operator.md,
/// "The receipt", has the push's reader choose UTF-16 by a byte order mark, and `agentspace-push`'s
/// `parse_receipt`, which the board read re-uses (pin P5), does so: the two documents disagree,
/// and this test is reported to the architect as that question (it is not weakened).
#[test]
fn a_utf_16_receipt_is_not_utf_8_and_is_refused() {
    let world = standard();
    let dir_json = serde_json::to_string(&world.cred_dir().to_string_lossy()).unwrap();
    let base_json = serde_json::to_string(&world.standin.base()).unwrap();
    let text = format!("{{\"base\":{base_json},\"credential_directory\":{dir_json}}}");
    for (label, bom, big_endian) in [("UTF-16LE", [0xFFu8, 0xFE], false), ("UTF-16BE", [0xFE, 0xFF], true)] {
        let mut bytes = bom.to_vec();
        for unit in text.encode_utf16() {
            let pair = if big_endian { unit.to_be_bytes() } else { unit.to_le_bytes() };
            bytes.extend_from_slice(&pair);
        }
        world.write_receipt(&bytes);
        let result = world.read(BOARD, 52);
        assert!(
            matches!(result, Err(BoardError::Internal(_))),
            "{label}: expected Internal, got {}",
            name_of(&result)
        );
        assert_eq!(world.internal(result), MSG_RECEIPT, "{label}");
    }
    assert!(world.standin.requests().is_empty());
}

#[test]
fn a_folder_at_the_receipts_path_is_refused() {
    let world = standard();
    std::fs::remove_file(world.receipt_file()).unwrap();
    std::fs::create_dir_all(world.receipt_file()).unwrap();
    let msg = world.internal(world.read(BOARD, 52));
    assert_eq!(msg, MSG_RECEIPT);
    assert!(world.requests().is_empty());
}

#[test]
fn a_base_that_is_not_exactly_http_127_0_0_1_and_a_port_is_refused_before_any_request() {
    let world = standard();
    let p = world.standin.port;
    let bases: Vec<(&str, String)> = vec![
        ("localhost", format!("http://localhost:{p}")),
        ("https", format!("https://127.0.0.1:{p}")),
        ("upper-case scheme", format!("HTTP://127.0.0.1:{p}")),
        ("trailing slash", format!("http://127.0.0.1:{p}/")),
        ("a path", format!("http://127.0.0.1:{p}/api")),
        ("a path with the API", format!("http://127.0.0.1:{p}/api/2026-11-10")),
        ("a query", format!("http://127.0.0.1:{p}?x=1")),
        ("a fragment", format!("http://127.0.0.1:{p}#x")),
        ("user information", format!("http://user@127.0.0.1:{p}")),
        ("a trailing dot", format!("http://127.0.0.1.:{p}")),
        ("127.1", format!("http://127.1:{p}")),
        ("127.0.0.2", format!("http://127.0.0.2:{p}")),
        ("IPv6", format!("http://[::1]:{p}")),
        ("leading space", format!(" http://127.0.0.1:{p}")),
        ("trailing space", format!("http://127.0.0.1:{p} ")),
        ("a leading zero", format!("http://127.0.0.1:0{p}")),
        ("a port with a letter", format!("http://127.0.0.1:{p}a")),
        ("port 0", "http://127.0.0.1:0".to_string()),
        ("port 65536", "http://127.0.0.1:65536".to_string()),
        ("port 99999", "http://127.0.0.1:99999".to_string()),
        ("port 100000", "http://127.0.0.1:100000".to_string()),
        ("no port", "http://127.0.0.1".to_string()),
        ("an empty port", "http://127.0.0.1:".to_string()),
        ("no scheme", format!("127.0.0.1:{p}")),
        ("empty", String::new()),
    ];
    for (label, base) in bases {
        world.write_receipt_base(serde_json::Value::String(base));
        let msg = world.internal(world.read(BOARD, 52));
        assert_eq!(msg, MSG_RECEIPT, "base: {label}");
        assert!(
            world.standin.requests().is_empty(),
            "base {label} reached the stand-in: the token went to a board it should not"
        );
    }
}

#[test]
fn a_receipt_with_a_byte_order_mark_and_other_members_is_read() {
    // localswim's receipt holds other members, which are ignored, and a reader accepts a leading
    // byte order mark.
    let world = standard();
    let dir_json = serde_json::to_string(&world.cred_dir().to_string_lossy()).unwrap();
    let base_json = serde_json::to_string(&world.standin.base()).unwrap();
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(
        format!("{{\"pid\":4242,\"base\":{base_json},\"started\":\"x\",\"credential_directory\":{dir_json},\"more\":{{\"a\":[1]}}}}")
            .as_bytes(),
    );
    world.write_receipt(&bytes);
    let epic = world.ok(world.read(BOARD, 52));
    assert_eq!(epic.revision, 7);
    assert_eq!(world.requests().len(), 1);
}

// ---------------------------------------------------------------------------------------------
// The credential: bot.json, refused before any request.

#[test]
fn a_missing_or_unusable_bot_json_is_refused_before_any_request() {
    let world = standard();
    let good_blob = hex(&blob_for(&utf16le(world.standin.token())));
    let v = |members: &str| format!("{{{members}}}");
    let fmt = "\"format\":\"localswim-credential\"";
    let ver = "\"format_version\":1";
    let role = "\"role\":\"bot\"";
    let hex_member = |h: &str| format!("\"token_dpapi_hex\":\"{h}\"");
    // Plaintexts that decrypt, and are no token.
    let plain = |text: &str| bot_json(&hex(&blob_for(&utf16le(text))));
    let forty_three = "A".repeat(43);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("not json", b"not json".to_vec()),
        ("an array", b"[]".to_vec()),
        ("null", b"null".to_vec()),
        ("an empty object", b"{}".to_vec()),
        (
            "a fifth member",
            v(&format!("{fmt},{ver},{role},{},\"extra\":1", hex_member(&good_blob))).into_bytes(),
        ),
        ("no token member", v(&format!("{fmt},{ver},{role}")).into_bytes()),
        ("no role", v(&format!("{fmt},{ver},{}", hex_member(&good_blob))).into_bytes()),
        (
            "role human",
            v(&format!("{fmt},{ver},\"role\":\"human\",{}", hex_member(&good_blob))).into_bytes(),
        ),
        (
            "role Bot",
            v(&format!("{fmt},{ver},\"role\":\"Bot\",{}", hex_member(&good_blob))).into_bytes(),
        ),
        (
            "wrong format",
            v(&format!("\"format\":\"other\",{ver},{role},{}", hex_member(&good_blob))).into_bytes(),
        ),
        (
            "format_version 1.0",
            v(&format!("{fmt},\"format_version\":1.0,{role},{}", hex_member(&good_blob))).into_bytes(),
        ),
        (
            "format_version \"1\"",
            v(&format!("{fmt},\"format_version\":\"1\",{role},{}", hex_member(&good_blob))).into_bytes(),
        ),
        (
            "format_version 2",
            v(&format!("{fmt},\"format_version\":2,{role},{}", hex_member(&good_blob))).into_bytes(),
        ),
        (
            "a member twice",
            v(&format!("{fmt},{ver},{role},{},{}", hex_member(&good_blob), hex_member(&good_blob)))
                .into_bytes(),
        ),
        ("odd hex", v(&format!("{fmt},{ver},{role},{}", hex_member("abc"))).into_bytes()),
        ("non-hex", v(&format!("{fmt},{ver},{role},{}", hex_member("zz"))).into_bytes()),
        ("empty hex", v(&format!("{fmt},{ver},{role},{}", hex_member(""))).into_bytes()),
        (
            "hex not a string",
            v(&format!("{fmt},{ver},{role},\"token_dpapi_hex\":12")).into_bytes(),
        ),
        (
            "a blob the decryption refuses",
            v(&format!("{fmt},{ver},{role},{}", hex_member("0100000000000000"))).into_bytes(),
        ),
        ("a plaintext of 5 characters", plain("short").into_bytes()),
        ("a plaintext of 42 characters", plain(&"A".repeat(42)).into_bytes()),
        ("a plaintext of 44 characters", plain(&"A".repeat(44)).into_bytes()),
        ("a plus in the plaintext", plain(&format!("{}+", "A".repeat(42))).into_bytes()),
        ("a slash in the plaintext", plain(&format!("{}/", "A".repeat(42))).into_bytes()),
        ("padding in the plaintext", plain(&format!("{}=", "A".repeat(42))).into_bytes()),
        (
            "a terminator after the plaintext",
            bot_json(&hex(&blob_for(&[utf16le(&forty_three), vec![0, 0]].concat()))).into_bytes(),
        ),
        (
            "an odd byte count",
            bot_json(&hex(&blob_for(&[utf16le(&forty_three), vec![0x41]].concat()))).into_bytes(),
        ),
        (
            "an unpaired surrogate",
            bot_json(&hex(&blob_for(&[utf16le(&"A".repeat(42)), vec![0x00, 0xD8]].concat()))).into_bytes(),
        ),
        (
            "the plaintext as UTF-8",
            bot_json(&hex(&blob_for(forty_three.as_bytes()))).into_bytes(),
        ),
    ];
    for (label, bytes) in cases {
        world.write_bot_bytes(&bytes);
        let msg = world.internal(world.read(BOARD, 52));
        assert_eq!(msg, MSG_CREDENTIAL, "bot.json: {label}");
    }
    // No file, a folder in its place, and a credential folder with no bot.json.
    world.remove_bot();
    let msg = world.internal(world.read(BOARD, 52));
    assert_eq!(msg, MSG_CREDENTIAL, "no bot.json");
    std::fs::create_dir_all(world.bot_file()).unwrap();
    let msg = world.internal(world.read(BOARD, 52));
    assert_eq!(msg, MSG_CREDENTIAL, "a folder for bot.json");
    std::fs::remove_dir_all(world.bot_file()).unwrap();
    std::fs::remove_dir_all(world.cred_dir()).unwrap();
    let msg = world.internal(world.read(BOARD, 52));
    assert_eq!(msg, MSG_CREDENTIAL, "no credential folder");
    assert!(
        world.standin.requests().is_empty(),
        "an unusable credential still made a request"
    );
}

#[test]
fn a_credential_folder_the_receipt_names_with_no_bot_json_is_refused() {
    let world = standard();
    let empty = world.root().join("an-empty-credential-folder");
    std::fs::create_dir_all(&empty).unwrap();
    let text = serde_json::json!({
        "base": world.standin.base(),
        "credential_directory": empty.to_string_lossy(),
    })
    .to_string();
    world.write_receipt(text.as_bytes());
    let msg = world.internal(world.read(BOARD, 52));
    assert_eq!(msg, MSG_CREDENTIAL);
    assert!(world.requests().is_empty());
}

#[test]
fn the_credential_directory_with_a_trailing_separator_is_read() {
    // localswim's receipt may name its folder with or without the last separator.
    let world = standard();
    let mut dir = world.cred_dir().to_string_lossy().into_owned();
    dir.push(std::path::MAIN_SEPARATOR);
    let text = serde_json::json!({"base": world.standin.base(), "credential_directory": dir}).to_string();
    world.write_receipt(text.as_bytes());
    let epic = world.ok(world.read(BOARD, 52));
    assert_eq!(epic.revision, 7);
}

// ---------------------------------------------------------------------------------------------
// Secrecy: no message holds the token, and nothing else the board said.

#[test]
fn no_message_holds_the_token_or_anything_the_board_said() {
    // Every error outcome with a message, each checked by `World::internal` for the token, any 8
    // characters of it, the markers in the answer's ignored parts and Problem documents, a URL,
    // a path, a header name and an operating system's text. The message's words are asserted as
    // well, and the two outcomes with no message carry none by their type.
    let mut seen = 0;
    let scenarios: Vec<(&str, Behavior, Box<dyn Fn(&World)>)> = vec![
        ("hang-free 500", Behavior::standard().answer(Answer::problem(500, "internal_error")), Box::new(|_| {})),
        ("401", Behavior::standard().answer(Answer::problem(401, "unauthorized")), Box::new(|_| {})),
        ("403", Behavior::standard().answer(Answer::problem(403, "forbidden")), Box::new(|_| {})),
        ("bad shape", Behavior::standard().answer(Answer::ok_body("{\"card\":1}")), Box::new(|_| {})),
        ("closed", Behavior::standard().answer(Answer::Close), Box::new(|_| {})),
        ("no receipt", Behavior::standard(), Box::new(|w| w.remove_receipt())),
        ("no credential", Behavior::standard(), Box::new(|w| w.remove_bot())),
        (
            "bad base",
            Behavior::standard(),
            Box::new(|w| w.write_receipt_base(serde_json::Value::String("http://localhost:1".into()))),
        ),
    ];
    for (label, behavior, tweak) in scenarios {
        let world = World::new(behavior);
        tweak(&world);
        let result = world.read(BOARD, 52);
        match result {
            Err(BoardError::Internal(msg)) => {
                world.check_message(&msg);
                assert!(!msg.ends_with('.'), "{label}: the message has a final period");
                seen += 1;
            }
            other => panic!("{label}: expected Internal, got {}", name_of(&other)),
        }
    }
    assert_eq!(seen, 8);
    // The Problem documents' own words, and the answer's prose, are in no message either: a 404
    // for an unknown board and ticket carry markers, and give unit variants.
    let world = standard();
    assert!(matches!(world.read(BOARD, 99), Err(BoardError::UnknownTicket)));
    assert!(matches!(world.read(OTHER_BOARD, 1), Err(BoardError::UnknownBoard)));
}

#[test]
fn the_leak_check_finds_a_planted_leak() {
    // The checker is shown to fail on a planted value before its pass is believed.
    let token = fresh_token();
    let planted: Vec<String> = vec![
        format!("The board refused {token}"),
        format!("The board refused {}", &token[10..20]),
        format!("The board said {}", MARKERS[1]),
        "The board at http://127.0.0.1:1 did not answer".to_string(),
        "The board refused /api/2026-11-10/boards".to_string(),
        "Could not read C:\\x\\bot.json".to_string(),
    ];
    for msg in planted {
        let token = token.clone();
        let caught = std::panic::catch_unwind(move || leak_check(&token, &msg, &[]));
        assert!(caught.is_err(), "the leak check passed a planted leak");
    }
    leak_check(&token, "The board at 127.0.0.1:1234 did not answer", &[]);
}
