// RFC 2119 keywords in these comments; the capitals are load-bearing.
//
// The state filter (docs/lease-api.md, Values, "State filter" and States) and the quoting of a
// value in a message (Exit codes and errors, Messages).

mod common;

use agentspace_values::{quote, states, ttl};
use common::*;

const ALL: [&str; 5] = ["pending", "ready", "expired", "released", "unsatisfiable"];

fn st(occurrences: &[&str]) -> Vec<&'static str> {
    let label = format!("{occurrences:?}");
    ok(states(occurrences), &label)
}

fn st_bad(occurrences: &[&str]) -> String {
    let label = format!("{occurrences:?}");
    invalid(states(occurrences), &label, "state")
}

// ---------------------------------------------------------------- states

#[test]
fn no_state_flag_at_all_is_no_filter() {
    // Architect, 2026-11-19: an empty list of occurrences means `--state` was not given.
    assert!(matches!(states(&[]), Ok(v) if v.is_empty()));
}

#[test]
fn a_state_name_is_quoted_trimmed() {
    assert_eq!(st_bad(&[" Foo "]), msg_state("'Foo'"));
}

#[test]
fn one_state_name_each() {
    for name in ALL {
        assert_eq!(st(&[name]), vec![name]);
    }
}

#[test]
fn names_are_ascii_case_insensitive_and_come_back_lowercase() {
    for name in ALL {
        for v in case_variants(name) {
            assert_eq!(st(&[&v]), vec![name], "{v}");
        }
    }
    assert_eq!(st(&["Ready"]), vec!["ready"]);
    assert_eq!(st(&["UNSATISFIABLE"]), vec!["unsatisfiable"]);
}

#[test]
fn surrounding_whitespace_is_trimmed() {
    assert_eq!(st(&[" ready "]), vec!["ready"]);
    assert_eq!(st(&["\tready\n"]), vec!["ready"]);
    assert_eq!(st(&["\u{3000}Ready\u{a0}"]), vec!["ready"]);
}

#[test]
fn a_comma_separated_list_is_split_and_each_name_trimmed() {
    // lease-api.md: `--state ready,pending`; "whitespace around each name is trimmed".
    assert_eq!(st(&["ready,pending"]), vec!["pending", "ready"]);
    assert_eq!(st(&[" ready , pending "]), vec!["pending", "ready"]);
    assert_eq!(st(&["ready,\tpending"]), vec!["pending", "ready"]);
    assert_eq!(st(&["READY,Pending"]), vec!["pending", "ready"]);
    assert_eq!(
        st(&["pending,ready,expired,released,unsatisfiable"]),
        ALL.to_vec()
    );
}

#[test]
fn the_result_is_in_the_order_of_the_states_table_whatever_the_order_given() {
    // The table: pending, ready, expired, released, unsatisfiable.
    assert_eq!(
        st(&["unsatisfiable,released,expired,ready,pending"]),
        ALL.to_vec()
    );
    assert_eq!(st(&["released,pending"]), vec!["pending", "released"]);
    assert_eq!(st(&["expired,ready"]), vec!["ready", "expired"]);
    assert_eq!(st(&["unsatisfiable,ready"]), vec!["ready", "unsatisfiable"]);
    assert_eq!(
        st(&["released", "expired", "pending"]),
        vec!["pending", "expired", "released"]
    );
    assert_eq!(
        st(&["unsatisfiable", "released", "expired", "ready", "pending"]),
        ALL.to_vec()
    );
}

#[test]
fn the_flag_may_be_repeated() {
    assert_eq!(st(&["ready", "pending"]), vec!["pending", "ready"]);
    assert_eq!(
        st(&["ready,pending", "expired"]),
        vec!["pending", "ready", "expired"]
    );
    assert_eq!(
        st(&["expired", "ready,pending"]),
        vec!["pending", "ready", "expired"]
    );
    assert_eq!(st(&[" Ready ", "\tPENDING\n"]), vec!["pending", "ready"]);
}

#[test]
fn duplicates_are_dropped() {
    assert_eq!(st(&["ready,ready"]), vec!["ready"]);
    assert_eq!(st(&["ready", "ready"]), vec!["ready"]);
    assert_eq!(st(&["ready", "READY"]), vec!["ready"]);
    assert_eq!(st(&["ready, Ready ,READY"]), vec!["ready"]);
    assert_eq!(
        st(&["ready,pending,ready", "pending,ready,expired,expired"]),
        vec!["pending", "ready", "expired"]
    );
    assert_eq!(st(&[&ALL.join(","), &ALL.join(",")]), ALL.to_vec());
}

#[test]
fn an_unknown_name_is_invalid_value_for_state_and_the_message_names_it() {
    assert_eq!(
        st_bad(&["bogus"]),
        "State 'bogus' is not a state. Use pending, ready, expired, released or unsatisfiable."
    );
    // From conformance/lease-cli/test-values.ps1.
    assert_eq!(st_bad(&["ready,bogus"]), msg_state("'bogus'"));
    assert_eq!(st_bad(&["ready", "bogus"]), msg_state("'bogus'"));
}

#[test]
fn the_name_quoted_is_the_one_refused_and_trimmed() {
    assert_eq!(st_bad(&[" bogus "]), msg_state("'bogus'"));
    assert_eq!(st_bad(&["ready,\tbogus\n,pending"]), msg_state("'bogus'"));
    assert_eq!(st_bad(&["READY,Bogus"]), msg_state("'Bogus'"));
    assert_eq!(st_bad(&["re ady"]), msg_state("'re ady'"));
}

#[test]
fn an_empty_name_is_invalid_and_quoted_as_two_single_quotes() {
    // lease-api.md: `ready,`, `ready,,pending` and `--state " "` each quote the empty name as ''.
    for occ in [
        vec!["ready,"],
        vec!["ready,,pending"],
        vec![","],
        vec![",ready"],
        vec![" "],
        vec!["\t"],
        vec!["\u{3000}"],
        vec!["ready, "],
        vec!["ready", ""],
        vec![""],
        vec!["ready", " "],
        vec!["ready, ,pending"],
    ] {
        assert_eq!(st_bad(&occ), msg_state("''"), "{occ:?}");
    }
}

#[test]
fn the_first_bad_name_in_order_is_the_one_reported() {
    assert_eq!(st_bad(&["bogus,zzz"]), msg_state("'bogus'"));
    assert_eq!(st_bad(&["zzz", "bogus"]), msg_state("'zzz'"));
    assert_eq!(st_bad(&["ready,", "bogus"]), msg_state("''"));
    assert_eq!(st_bad(&["bogus", "ready,"]), msg_state("'bogus'"));
    assert_eq!(st_bad(&["ready", "bogus", ""]), msg_state("'bogus'"));
    assert_eq!(st_bad(&["pending,ready,,bogus"]), msg_state("''"));
}

#[test]
fn one_bad_name_refuses_the_whole_filter() {
    // Not a partial result: the good names beside a bad one are not returned.
    for occ in [
        vec!["ready,bogus"],
        vec!["ready", "bogus"],
        vec!["bogus,ready"],
    ] {
        assert!(states(&occ).is_err(), "{occ:?}");
    }
}

#[test]
fn names_that_are_not_exactly_a_state_are_refused() {
    for v in [
        "read",
        "readyy",
        "pend",
        "pending ready",
        "ready pending",
        "ready;pending",
        "ready|pending",
        "ready/pending",
        "ready:pending",
        "ready.pending",
        "ready+pending",
        "ready-pending",
        "ready_pending",
        "all",
        "any",
        "*",
        "unsatisfied",
        "release",
        "expire",
        "expire d",
        "0",
        "ready\n\u{0}",
        "ready\nx",
        "r\u{e9}ady",
        "rea\u{3b4}y",
        "\u{ff52}eady",
        "releaſed",
        "ı",
        "pendıng",
        "\u{212a}ready",
        "unsatisfiabl\u{e9}",
    ] {
        invalid(states(&[v]), v, "state");
    }
}

#[test]
fn case_folding_is_ascii_only() {
    // U+017F LATIN SMALL LETTER LONG S uppercases to S, and U+0131 DOTLESS I to I.
    for v in [
        "relea\u{17f}ed",
        "unsati\u{17f}fiable",
        "pend\u{131}ng",
        "READ\u{178}",
    ] {
        invalid(states(&[v]), v, "state");
    }
}

#[test]
fn a_name_with_a_control_character_is_quoted_escaped() {
    assert_eq!(st_bad(&["bo\u{1}gus"]), msg_state("'bo\\u0001gus'"));
    assert_eq!(st_bad(&["ready,bo\u{7f}gus"]), msg_state("'bo\\u007fgus'"));
    assert_eq!(st_bad(&["bo\ngus"]), msg_state("'bo\\u000agus'"));
    assert_eq!(st_bad(&["bo\u{0}gus"]), msg_state("'bo\\u0000gus'"));
}

// ---------------------------------------------------------------- quote

#[test]
fn quote_wraps_in_single_quotes() {
    assert_eq!(quote("abc"), "'abc'");
    assert_eq!(quote("8h"), "'8h'");
    assert_eq!(quote("--help=yes"), "'--help=yes'");
    assert_eq!(quote("linux/arm64"), "'linux/arm64'");
}

#[test]
fn quote_of_empty_is_two_single_quotes() {
    assert_eq!(quote(""), "''");
}

#[test]
fn quote_is_as_given_untrimmed() {
    assert_eq!(quote(" a "), "' a '");
    assert_eq!(quote("  "), "'  '");
    assert_eq!(quote("\u{a0}x\u{3000}"), "'\u{a0}x\u{3000}'");
    assert_eq!(quote("\u{2003}"), "'\u{2003}'");
}

#[test]
fn quote_does_not_change_case() {
    assert_eq!(quote("AbC"), "'AbC'");
    assert_eq!(quote("Agent-Linux@03"), "'Agent-Linux@03'");
}

#[test]
fn quote_writes_each_control_character_as_u_and_four_lowercase_hex_digits() {
    assert_eq!(quote("a\nb"), "'a\\u000ab'");
    assert_eq!(quote("\t"), "'\\u0009'");
    assert_eq!(quote("\r"), "'\\u000d'");
    assert_eq!(quote("\0"), "'\\u0000'");
    assert_eq!(quote("\u{1a}"), "'\\u001a'");
    assert_eq!(quote("\u{1b}"), "'\\u001b'");
    assert_eq!(quote("\u{1f}"), "'\\u001f'");
    assert_eq!(quote("\u{7f}"), "'\\u007f'");
    assert_eq!(quote("\u{b}\u{c}"), "'\\u000b\\u000c'");
}

#[test]
fn quote_escapes_every_control_character_from_u0000_to_u001f_and_u007f() {
    for n in (0u32..0x20).chain([0x7f]) {
        let c = char::from_u32(n).unwrap_or('\0');
        let want = format!("'x\\u{n:04x}y'");
        assert_eq!(quote(&format!("x{c}y")), want, "U+{n:04X}");
        assert!(want.chars().all(|ch| !ch.is_ascii_control()));
    }
}

#[test]
fn quote_never_leaves_a_control_character_in() {
    for n in (0u32..0x20).chain([0x7f]) {
        let c = char::from_u32(n).unwrap_or('\0');
        for v in [
            format!("{c}"),
            format!("{c}{c}"),
            format!("a{c}"),
            format!("{c}a{c}b{c}"),
        ] {
            let q = quote(&v);
            assert!(!q.chars().any(|ch| ch.is_ascii_control()), "{v:?} -> {q:?}");
            assert!(q.starts_with('\'') && q.ends_with('\''), "{q:?}");
        }
    }
}

#[test]
fn quote_keeps_other_characters_as_they_are() {
    for v in [
        "\u{e9}",
        "\u{65e5}\u{672c}",
        "\u{feff}",
        "a\u{feff}b",
        "\u{1f600}",
        "\u{2028}",
    ] {
        assert_eq!(quote(v), format!("'{v}'"));
    }
}

#[test]
fn quote_of_a_very_long_value() {
    let v = "9".repeat(100_000);
    let q = quote(&v);
    assert_eq!(q.len(), v.len() + 2);
    assert_eq!(&q[1..=v.len()], v.as_str());
}

// ---------------------------------------------------------------- messages escape control characters

#[test]
fn every_message_escapes_a_control_character_in_the_value() {
    for n in (0u32..0x20).chain([0x7f]) {
        let c = char::from_u32(n).unwrap_or('\0');
        let esc = format!("\\u{n:04x}");
        let v = format!("a{c}b");
        let quoted = format!("'a{esc}b'");
        for (field, f, _) in string_values() {
            let m = invalid(f(&v), &v, field);
            assert_eq!(m, msg_for(field, &quoted), "{field} U+{n:04X}");
        }
        assert_eq!(ttl_invalid(&v), msg_not_duration(&quoted), "ttl U+{n:04X}");
        let v48 = format!("48{c}x");
        let q48 = format!("'48{esc}x'");
        assert_eq!(ttl_invalid(&v48), msg_not_duration(&q48), "ttl U+{n:04X}");
        assert_eq!(
            invalid(states(&[v.as_str()]), &v, "state"),
            msg_state(&quoted),
            "state U+{n:04X}"
        );
    }
}

#[test]
fn an_out_of_range_message_escapes_a_control_character_that_is_not_trimmed_away() {
    // U+000B is whitespace between a number and its unit, so `48<VT>h` is well formed.
    let v = "48\u{b}h";
    let m = out_of_range(ttl(v), v, "ttl");
    assert_eq!(m, msg_range("'48\\u000bh'"));
}

#[test]
fn messages_start_with_a_capital_letter_and_never_with_a_space() {
    for (field, f, _) in string_values() {
        let m = invalid(f(" no pe_ "), " no pe_ ", field);
        assert!(
            m.chars().next().is_some_and(|c| c.is_ascii_uppercase()),
            "{m}"
        );
    }
    assert!(ttl_invalid("1.5h").starts_with("TTL "));
    assert!(ttl_range("48h").starts_with("TTL "));
    assert!(st_bad(&["bogus"]).starts_with("State "));
}
