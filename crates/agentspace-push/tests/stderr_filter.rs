//! The caller-mode line rule over the push's ssh's stderr (operator.md, The token push, What the
//! laptop sends, step 2; What a caller sees; #0167, P14, architect, 2026-11-21, for v0.08-T01;
//! built by E0.08-T02-S04, #0176), as `StderrFilter` states it, with no process and no pipe.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! **The rule:** every line passes with the line ending it came with, but a line that is exactly
//! `The host is ready.`, compared without its LF or CR LF, which is dropped. A last line with no
//! ending passes as it is, with none added. Bytes are compared and never decoded. With
//! `AGENTSPACE_DIAGNOSTIC` set, nothing is dropped. The rule MUST NOT depend on how the reads
//! split the stream, so each case is fed whole, one byte at a time, and split at every position.

use agentspace_push::{HOST_READY_LINE, StderrFilter};

const READY: &[u8] = b"The host is ready.";

/// Feeds `chunks` in order, then finishes.
fn filtered(diagnostic: bool, chunks: &[&[u8]]) -> Vec<u8> {
    let mut filter = StderrFilter::new(diagnostic);
    let mut out = Vec::new();
    for chunk in chunks {
        out.extend(filter.feed(chunk));
    }
    out.extend(filter.finish());
    out
}

/// The rule, written out plainly and independently of the filter: the lines of `input` that are
/// not exactly the ready line, each with its own ending.
fn model(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut rest = input;
    while !rest.is_empty() {
        let (line, ended) = match rest.iter().position(|&b| b == b'\n') {
            Some(i) => (&rest[..=i], true),
            None => (rest, false),
        };
        rest = &rest[line.len()..];
        let content = if ended {
            let without_lf = &line[..line.len() - 1];
            without_lf.strip_suffix(b"\r").unwrap_or(without_lf)
        } else {
            line
        };
        if content != READY {
            out.extend_from_slice(line);
        }
    }
    out
}

/// Every way to feed `input`: whole, one byte at a time, and split in two at every position.
fn assert_rule(input: &[u8], want: &[u8], what: &str) {
    assert_eq!(filtered(false, &[input]), want, "{what}: fed whole");
    let bytes: Vec<&[u8]> = input.chunks(1).collect();
    assert_eq!(filtered(false, &bytes), want, "{what}: fed byte by byte");
    for at in 0..=input.len() {
        let (a, b) = input.split_at(at);
        assert_eq!(filtered(false, &[a, b]), want, "{what}: split at {at}");
    }
    assert_eq!(model(input), want, "{what}: the model agrees");
}

#[test]
fn the_ready_lines_text_is_the_specs() {
    assert_eq!(HOST_READY_LINE, READY);
}

#[test]
fn the_exact_line_is_dropped_with_either_ending() {
    assert_rule(b"The host is ready.\n", b"", "LF");
    assert_rule(b"The host is ready.\r\n", b"", "CR LF");
    assert_rule(
        b"a\r\nThe host is ready.\r\nb\nThe host is ready.\nc\r\n",
        b"a\r\nb\nc\r\n",
        "both, among others",
    );
    assert_rule(b"The host is ready.\nThe host is ready.\n", b"", "twice");
}

#[test]
fn every_other_line_passes_with_the_ending_it_came_with() {
    assert_rule(b"one\r\ntwo\nthree\r\n", b"one\r\ntwo\nthree\r\n", "mixed");
    assert_rule(b"\n\r\n\n", b"\n\r\n\n", "empty lines");
    assert_rule(b"", b"", "nothing");
}

#[test]
fn a_last_line_with_no_ending_passes_as_it_is_and_the_ready_line_alone_is_dropped() {
    assert_rule(b"a\nlast", b"a\nlast", "no ending");
    assert_rule(
        b"a\nThe host is ready.",
        b"a\n",
        "the ready line, no ending",
    );
    assert_rule(b"The host is ready.", b"", "only the ready line, no ending");
    assert_rule(b"a\nThe host is", b"a\nThe host is", "a start of it");
    assert_rule(
        b"a\nThe host is ready. ",
        b"a\nThe host is ready. ",
        "it and a space",
    );
    assert_rule(
        b"a\nThe host is ready.\r",
        b"a\nThe host is ready.\r",
        "it and a CR",
    );
}

#[test]
fn a_line_that_differs_by_one_byte_is_kept() {
    let near: [&[u8]; 14] = [
        b"The host is ready. \n",
        b"The host is ready. \r\n",
        b" The host is ready.\n",
        b"\tThe host is ready.\n",
        b"the host is ready.\n",
        b"The Host is ready.\n",
        b"The host is ready\n",
        b"The host is ready..\n",
        b"The host is ready.x\n",
        b"x The host is ready.\n",
        b"agentspace: The host is ready.\n",
        b"The host is ready.\r\r\n",
        b"The host is ready.\xff\n",
        b"The host is  ready.\n",
    ];
    for line in near {
        let mut input = b"top\n".to_vec();
        input.extend_from_slice(line);
        input.extend_from_slice(b"The host is ready.\nbottom\n");
        let mut want = b"top\n".to_vec();
        want.extend_from_slice(line);
        want.extend_from_slice(b"bottom\n");
        assert_rule(&input, &want, &String::from_utf8_lossy(line));
    }
}

#[test]
fn bytes_are_compared_and_never_decoded() {
    // Invalid UTF-8, a NUL, a lone surrogate's bytes and a truncated sequence all pass unchanged;
    // a line whose lossy decoding would look like the ready line is not that line.
    let input: &[u8] = b"\xff\xfe\xc3\x28 raw\r\nThe host is ready.\n\xc0\xaf\x80\x81\r\nnul\x00inside\n\xed\xa0\x80\nThe host is ready.\xef\xbf\xbd\nend\xe2\x82";
    let want: &[u8] = b"\xff\xfe\xc3\x28 raw\r\n\xc0\xaf\x80\x81\r\nnul\x00inside\n\xed\xa0\x80\nThe host is ready.\xef\xbf\xbd\nend\xe2\x82";
    assert_rule(input, want, "raw bytes");
}

#[test]
fn a_long_line_passes_whole() {
    let mut line = vec![b'x'; 100_000];
    line.extend_from_slice(b"\r\n");
    let mut input = b"The host is ready.\n".to_vec();
    input.extend_from_slice(&line);
    assert_eq!(filtered(false, &[&input]), line);
    let chunks: Vec<&[u8]> = input.chunks(4093).collect();
    assert_eq!(filtered(false, &chunks), line);
}

#[test]
fn with_diagnostic_set_every_byte_passes_unchanged() {
    let input: &[u8] =
        b"The host is ready.\r\nkept\nThe host is ready.\n\xff\xfe raw\r\nThe host is ready.";
    assert_eq!(filtered(true, &[input]), input);
    let bytes: Vec<&[u8]> = input.chunks(1).collect();
    assert_eq!(filtered(true, &bytes), input);
    for at in 0..=input.len() {
        let (a, b) = input.split_at(at);
        assert_eq!(filtered(true, &[a, b]), input, "Split at {at}");
    }
}

#[test]
fn any_input_in_any_split_follows_the_rule() {
    // A deterministic mix of the ready line, its near misses, line endings and raw bytes.
    const PIECES: [&[u8]; 14] = [
        b"The host is ready.",
        b"The host is ready",
        b"The host",
        b" is ready.",
        b"\r\n",
        b"\n",
        b"\r",
        b" ",
        b"x",
        b".",
        b"\xff",
        b"The host is ready.\n",
        b"The host is ready.\r\n",
        b"ssh: a warning",
    ];
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as usize
    };
    for case in 0..3000 {
        let mut input = Vec::new();
        for _ in 0..(1 + next() % 12) {
            input.extend_from_slice(PIECES[next() % PIECES.len()]);
        }
        let want = model(&input);
        // A random split into several chunks.
        let mut chunks: Vec<&[u8]> = Vec::new();
        let mut rest: &[u8] = &input;
        while !rest.is_empty() {
            let take = 1 + next() % rest.len();
            let (a, b) = rest.split_at(take);
            chunks.push(a);
            rest = b;
        }
        assert_eq!(
            filtered(false, &chunks),
            want,
            "Case {case}: input {input:?} in chunks {chunks:?}"
        );
        assert_eq!(
            filtered(true, &chunks),
            input,
            "Case {case}: with diagnostic set, nothing is dropped"
        );
    }
}

#[test]
fn a_filter_gives_back_what_it_holds_when_the_stream_ends() {
    // A line that could still be the ready line is held until it is known; the end of the stream
    // gives it back as it is.
    let mut filter = StderrFilter::new(false);
    let mut out = filter.feed(b"The host is rea");
    out.extend(filter.finish());
    assert_eq!(out, b"The host is rea");

    let mut filter = StderrFilter::new(false);
    let mut out = filter.feed(b"The host is ready.");
    out.extend(filter.finish());
    assert_eq!(out, b"");
}
