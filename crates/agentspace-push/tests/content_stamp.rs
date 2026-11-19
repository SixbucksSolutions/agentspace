//! The content's exact bytes, the stamp against a known vector, the stamp file's bytes, and the
//! zone's check (operator.md, What the laptop reads, The time zone, The stamp).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;
use common::*;

#[test]
fn content_is_exactly_two_lf_lines() {
    let c = content(BASE, TOKEN);
    assert_eq!(c, CONTENT.as_bytes());
    assert_eq!(c.iter().filter(|&&b| b == b'\n').count(), 2);
    assert!(!c.contains(&b'\r'), "no CR");
    assert!(!c.starts_with(&[0xEF, 0xBB, 0xBF]), "no byte order mark");
    assert!(c.ends_with(b"\n") && !c.ends_with(b"\n\n"));
}

#[test]
fn content_sends_the_url_with_nothing_added_or_removed() {
    assert_eq!(
        content("http://localhost:9/", "t"),
        b"LOCALSWIM_URL=http://localhost:9/\nLOCALSWIM_TOKEN=t\n"
    );
}

#[test]
fn content_carries_the_token_as_utf8() {
    let c = content(BASE, "t\u{e9}k\u{1F600}n");
    assert_eq!(
        c,
        "LOCALSWIM_URL=http://127.0.0.1:8811\nLOCALSWIM_TOKEN=t\u{e9}k\u{1F600}n\n".as_bytes()
    );
}

#[test]
fn stamp_matches_the_known_vectors() {
    // Each expected value is sha256sum of "<instance> LF <zone> LF <content>", computed apart.
    let c = CONTENT.as_bytes();
    assert_eq!(stamp(INSTANCE, ZONE, c), STAMP_LA);
    assert_eq!(stamp(INSTANCE, "", c), STAMP_EMPTY_ZONE);
    assert_eq!(stamp(INSTANCE, "Asia/Kolkata", c), STAMP_KOLKATA);
    assert_eq!(stamp(INSTANCE, "Europe/London", c), STAMP_LONDON);
    assert_eq!(stamp(OTHER_INSTANCE, ZONE, c), STAMP_LA_OTHER_INSTANCE);
}

#[test]
fn stamp_is_64_uppercase_hex_digits() {
    let s = stamp(INSTANCE, ZONE, CONTENT.as_bytes());
    assert_eq!(s.len(), 64);
    assert!(
        s.bytes()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b)),
        "{s}"
    );
}

#[test]
fn stamp_changes_with_the_instance_the_zone_the_url_and_the_token() {
    let base = stamp(INSTANCE, ZONE, &content(BASE, TOKEN));
    assert_ne!(base, stamp(OTHER_INSTANCE, ZONE, &content(BASE, TOKEN)));
    assert_ne!(
        base,
        stamp(INSTANCE, "Europe/London", &content(BASE, TOKEN))
    );
    assert_ne!(
        base,
        stamp(INSTANCE, ZONE, &content("http://127.0.0.1:8812", TOKEN))
    );
    assert_ne!(base, stamp(INSTANCE, ZONE, &content(BASE, TOKEN_B)));
}

#[test]
fn stamp_file_is_compact_json_with_no_bom_and_no_line_end() {
    let b = stamp_file_bytes(STAMP_LA);
    assert_eq!(
        b,
        format!("{{\"version\":0,\"stamp\":\"{STAMP_LA}\"}}").into_bytes()
    );
    assert!(!b.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert!(!b.ends_with(b"\n"));
}

// ---- The zone's check ------------------------------------------------------------------------

#[test]
fn zones_that_pass_are_kept() {
    for z in [
        "America/Los_Angeles",
        "UTC",
        "Etc/GMT+5",
        "Etc/GMT-5",
        "America/Argentina/Buenos_Aires",
        "EST5EDT",
        "America/Port-au-Prince",
        "+5",
        "_x",
        "a/-b",
        // Matched case-sensitively, so a lowercase legacy name is not renamed, and still passes.
        "asia/calcutta",
    ] {
        assert_eq!(check_zone(z), z, "{z:?}");
    }
}

#[test]
fn zones_that_fail_become_empty() {
    for z in [
        "",
        "-Foo",
        "America/",
        "/America",
        "a//b",
        "a b",
        "UTC\n",
        "\nUTC",
        "UTC\r",
        "Am\u{e9}rica/x",
        "a\"b",
        "a;b",
        "a'b",
        "a$b",
        "a`b",
        "a.b",
        "..",
        "../etc",
        "a\\b",
        "a%b",
        "-",
        " UTC",
    ] {
        assert_eq!(check_zone(z), "", "{z:?}");
    }
}

// The seven legacy renames are `agentspace-zone`'s, and the push holds no copy of the table
// (architect, 2026-11-19, for E0.06-T01-S04). So these two tests go through `zone_as_sent`: the
// zone as the operator works it out, then the push's guard. Their expected values are unchanged.

#[test]
fn the_seven_legacy_names_are_renamed() {
    for (old, new) in [
        ("Asia/Calcutta", "Asia/Kolkata"),
        ("Asia/Katmandu", "Asia/Kathmandu"),
        ("Asia/Rangoon", "Asia/Yangon"),
        ("Europe/Kiev", "Europe/Kyiv"),
        ("America/Buenos_Aires", "America/Argentina/Buenos_Aires"),
        ("America/Godthab", "America/Nuuk"),
        ("America/Indianapolis", "America/Indiana/Indianapolis"),
    ] {
        assert_eq!(zone_as_sent(old), new);
        assert_eq!(zone_as_sent(new), new, "a current name is kept");
        // The guard alone renames nothing: a legacy name that reaches it is kept as it is.
        assert_eq!(check_zone(old), old, "The push holds no legacy table");
    }
}

#[test]
fn legacy_names_are_matched_case_sensitively() {
    assert_eq!(zone_as_sent("ASIA/CALCUTTA"), "ASIA/CALCUTTA");
    assert_eq!(zone_as_sent("Asia/calcutta"), "Asia/calcutta");
}
