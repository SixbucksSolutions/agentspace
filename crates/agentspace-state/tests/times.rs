//! Times in the contract's form (lease-api.md, Times).

use agentspace_state::{format_time, now, parse_time};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn format_known_instants() {
    assert_eq!(format_time(0), "1970-01-01T00:00:00Z");
    assert_eq!(format_time(1_700_000_000), "2023-11-14T22:13:20Z");
    assert_eq!(format_time(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(format_time(1_709_208_000), "2024-02-29T12:00:00Z");
    assert_eq!(format_time(1_795_086_000), "2026-11-19T11:00:00Z");
    assert_eq!(format_time(2_147_483_648), "2038-01-19T03:14:08Z");
    assert_eq!(format_time(-1), "1969-12-31T23:59:59Z");
}

#[test]
fn format_has_the_contract_shape() {
    for t in [0, 59, 86_399, 1_700_000_000, 4_102_444_799] {
        let s = format_time(t);
        assert_eq!(s.len(), 20, "{s}");
        let b = s.as_bytes();
        assert_eq!(b[4], b'-');
        assert_eq!(b[10], b'T');
        assert_eq!(b[13], b':');
        assert_eq!(b[19], b'Z');
        assert!(!s.contains('.') && !s.contains('+'));
    }
}

#[test]
fn parse_known_instants() {
    assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(parse_time("2023-11-14T22:13:20Z"), Some(1_700_000_000));
    assert_eq!(parse_time("2000-02-29T00:00:00Z"), Some(951_782_400));
    assert_eq!(parse_time("2026-11-19T11:00:00Z"), Some(1_795_086_000));
    assert_eq!(parse_time("1969-12-31T23:59:59Z"), Some(-1));
    assert_eq!(parse_time("9999-12-31T23:59:59Z"), Some(253_402_300_799));
}

#[test]
fn format_and_parse_round_trip() {
    for t in [
        0,
        1,
        59,
        60,
        3599,
        86_399,
        86_400,
        1_700_000_000,
        1_795_086_000,
        4_000_000_000,
    ] {
        assert_eq!(parse_time(&format_time(t)), Some(t));
    }
}

#[test]
fn parse_refuses_any_other_form() {
    for s in [
        "",
        "2026-11-19T11:00:00",
        "2026-11-19T11:00:00z",
        "2026-11-19t11:00:00Z",
        "2026-11-19 11:00:00Z",
        "2026-11-19T11:00:00.5Z",
        "2026-11-19T11:00:00+00:00",
        "2026-11-19T11:00:00-05:00",
        "2026-11-19T11:00:00Z\n",
        " 2026-11-19T11:00:00Z",
        "2026-11-19T11:00Z",
        "2026-1-19T11:00:00Z",
        "20261119T110000Z",
        "2026-13-19T11:00:00Z",
        "2026-00-19T11:00:00Z",
        "2026-11-00T11:00:00Z",
        "2026-11-32T11:00:00Z",
    ] {
        assert_eq!(parse_time(s), None, "{s:?}");
    }
}

#[test]
fn parse_refuses_impossible_instants() {
    for s in [
        "2026-02-30T00:00:00Z",
        "2026-02-29T00:00:00Z",
        "2100-02-29T00:00:00Z",
        "2026-04-31T00:00:00Z",
        "2026-11-31T00:00:00Z",
        "2026-11-19T24:00:00Z",
        "2026-11-19T23:60:00Z",
        "2026-11-19T23:59:60Z",
    ] {
        assert_eq!(parse_time(s), None, "{s:?}");
    }
}

#[test]
fn parse_accepts_leap_days() {
    assert_eq!(parse_time("2024-02-29T00:00:00Z"), Some(1_709_164_800));
    assert_eq!(parse_time("2000-02-29T00:00:00Z"), Some(951_782_400));
}

#[test]
fn now_is_utc_whole_seconds_truncated() {
    let secs = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    };
    let before = secs();
    let n = now();
    let after = secs();
    // Truncation, never rounding: now() never runs ahead of the whole second the clock shows.
    assert!(n >= before && n <= after, "{before} {n} {after}");
}
