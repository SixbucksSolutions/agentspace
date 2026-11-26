//! The stand-in board itself (docs/conformance.md, "The stand-in board"), shown to do what the
//! other tests lean on: refuse and record every method but GET, check the prefix, then the
//! token, then the lookup, and tell a request with the right Bearer header from any other. A
//! check that has never failed is not believed (AGENTS.md), so these also show that
//! `assert_only_gets` fails on a request that was not a good GET.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The token is a stand-in and is never
//! printed; the raw requests below carry it by header only through `World`, never as an
//! argument.

mod common;

use common::*;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// One raw request, and the status line of the answer.
fn raw(port: u16, head: &str) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.write_all(head.as_bytes()).unwrap();
    let mut answer = Vec::new();
    let _ = s.read_to_end(&mut answer);
    let text = String::from_utf8_lossy(&answer).into_owned();
    let status = text
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, text)
}

fn good_path() -> String {
    format!("{API}boards/{BOARD}?view=ticket&ticket=52&include_prose=true")
}

#[test]
fn the_stand_in_binds_loopback_on_a_port_the_os_picked() {
    let standin = Standin::start(Behavior::standard());
    assert_ne!(standin.port, 8811);
    assert!(standin.port > 0);
    assert_eq!(standin.base(), format!("http://127.0.0.1:{}", standin.port));
}

#[test]
fn any_method_but_get_is_405_and_recorded() {
    let standin = Standin::start(Behavior::standard());
    for method in ["POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] {
        let (status, _) = raw(
            standin.port,
            &format!("{method} {} HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", good_path()),
        );
        assert_eq!(status, 405, "{method}");
    }
    let reqs = standin.requests();
    assert_eq!(reqs.len(), 6);
    let methods: Vec<&str> = reqs.iter().map(|r| r.method.as_str()).collect();
    assert_eq!(methods, ["POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"]);
}

#[test]
fn the_reads_only_check_fails_on_a_planted_non_get() {
    let standin = Standin::start(Behavior::standard());
    let _ = raw(
        standin.port,
        &format!("POST {} HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", good_path()),
    );
    let reqs = standin.requests();
    let caught = std::panic::catch_unwind(|| assert_only_gets(&reqs));
    assert!(caught.is_err(), "assert_only_gets passed a POST");
}

#[test]
fn the_reads_only_check_fails_on_a_wrong_or_missing_token_and_on_other_planted_values() {
    let standin = Standin::start(Behavior::standard());
    let head = |extra: &str| format!("GET {} HTTP/1.1\r\nHost: x\r\n{extra}Connection: close\r\n\r\n", good_path());
    // No Authorization.
    let (status, _) = raw(standin.port, &head(""));
    assert_eq!(status, 401);
    // A wrong one, never the stand-in's token.
    let (status, _) = raw(standin.port, &head("Authorization: Bearer wrong\r\n"));
    assert_eq!(status, 401);
    let (status, _) = raw(standin.port, &head("Authorization: wrong\r\n"));
    assert_eq!(status, 401);
    // A good request that carries what a read MUST NOT.
    let token_line = format!("Authorization: Bearer {}\r\n", standin.token());
    let (status, _) = raw(standin.port, &head(&format!("{token_line}Cookie: a=b\r\n")));
    assert_eq!(status, 200);
    let (status, _) = raw(standin.port, &head(&format!("{token_line}Origin: http://x\r\n")));
    assert_eq!(status, 200);
    let (status, _) = raw(standin.port, &head(&format!("{token_line}If-None-Match: x\r\n")));
    assert_eq!(status, 200);
    let (status, _) = raw(standin.port, &head(&format!("{token_line}Accept-Encoding: gzip\r\n")));
    assert_eq!(status, 200);
    // And one that is clean.
    let (status, _) = raw(standin.port, &head(&format!("{token_line}Accept-Encoding: identity\r\n")));
    assert_eq!(status, 200);
    let reqs = standin.requests();
    assert_eq!(reqs.len(), 8);
    assert!(!reqs[0].auth_ok && !reqs[1].auth_ok && !reqs[2].auth_ok);
    for (i, r) in reqs.iter().enumerate().take(7) {
        let one = vec![r.clone()];
        let caught = std::panic::catch_unwind(|| assert_only_gets(&one));
        assert!(caught.is_err(), "assert_only_gets passed planted request {i}");
    }
    assert_only_gets(&reqs[7..]);
}

#[test]
fn a_path_without_the_prefix_is_a_bare_404_before_the_token_is_looked_at() {
    let standin = Standin::start(Behavior::standard());
    let (status, text) = raw(
        standin.port,
        &format!("GET /boards/{BOARD}?view=ticket&ticket=52 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"),
    );
    assert_eq!(status, 404);
    assert!(!text.to_ascii_lowercase().contains("content-type"), "a bare 404 has no content type");
    assert!(text.ends_with("\r\n\r\n"), "a bare 404 has no body");
    // POST with no prefix is 405: the method is checked first.
    let (status, _) = raw(
        standin.port,
        &format!("POST /boards/{BOARD} HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
    );
    assert_eq!(status, 405);
}

#[test]
fn the_token_is_checked_before_the_lookup() {
    // A wrong token for an unknown board answers 401, never 404.
    let standin = Standin::start(Behavior::standard());
    let (status, _) = raw(
        standin.port,
        &format!(
            "GET {API}boards/{OTHER_BOARD}?view=ticket&ticket=1&include_prose=true HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer wrong\r\nConnection: close\r\n\r\n"
        ),
    );
    assert_eq!(status, 401);
}

#[test]
fn the_ticket_view_is_a_whole_inspection_with_every_part_filled_in_and_markers_in_each() {
    let standin = Standin::start(Behavior::standard());
    let head = format!(
        "GET {} HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
        good_path(),
        standin.token()
    );
    let (status, text) = raw(standin.port, &head);
    assert_eq!(status, 200);
    let body = text.split("\r\n\r\n").nth(1).unwrap();
    let doc: serde_json::Value = serde_json::from_str(body).unwrap();
    for member in ["board_id", "board_revision", "card", "relationships", "parent", "children", "prose"] {
        assert!(doc.get(member).is_some(), "the inspection lacks {member}");
    }
    assert!(doc["prose"]["comments"].as_array().unwrap().len() >= 2);
    assert!(!doc["relationships"].as_array().unwrap().is_empty());
    assert!(doc["parent"].is_object());
    assert!(!doc["children"].as_array().unwrap().is_empty());
    for marker in &MARKERS[..4] {
        assert!(body.contains(marker), "the inspection holds no {marker}");
    }
}

#[test]
fn a_404_is_the_same_document_for_an_unknown_board_and_an_unknown_ticket() {
    let standin = Standin::start(Behavior::standard());
    let get = |path: &str| {
        raw(
            standin.port,
            &format!(
                "GET {path} HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
                standin.token()
            ),
        )
    };
    let (s1, t1) = get(&format!("{API}boards/{BOARD}?view=ticket&ticket=99&include_prose=true"));
    let (s2, t2) = get(&format!("{API}boards/{OTHER_BOARD}?view=ticket&ticket=52&include_prose=true"));
    assert_eq!((s1, s2), (404, 404));
    for t in [&t1, &t2] {
        assert!(t.to_ascii_lowercase().contains("content-type: application/problem+json"));
        assert!(t.contains("\"code\":\"not_found\""));
    }
    let (s3, _) = get(&format!("{API}boards/{BOARD}?view=summary"));
    let (s4, _) = get(&format!("{API}boards/{OTHER_BOARD}?view=summary"));
    assert_eq!((s3, s4), (200, 404));
}
