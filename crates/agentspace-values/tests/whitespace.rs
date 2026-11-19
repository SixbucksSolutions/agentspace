// RFC 2119 keywords in these comments; the capitals are load-bearing.
//
// Input tolerance (docs/lease-api.md): surrounding whitespace is any character with the Unicode
// White_Space property, and no others; it is trimmed before the value is checked. U+FEFF is not
// whitespace, is never trimmed, and is refused wherever it is. Every check matches the whole value
// (#0029).

mod common;

use agentspace_values::{lease_id, platform, profile, project, size, states, ttl};
use common::*;

/// One test per whitespace character: each of the seven values is accepted with that character
/// before, after and on both sides, and gives the same normal form as without it. Inside a value
/// the character refuses (a project name, a platform and the rest have no whitespace in them).
macro_rules! trims {
    ($name:ident, $ch:expr) => {
        #[test]
        fn $name() {
            let ws: char = $ch;
            for (field, f, sample) in string_values() {
                let want = f(sample).unwrap_or_else(|_| panic!("sample for {field} is valid"));
                for v in [
                    format!("{ws}{sample}"),
                    format!("{sample}{ws}"),
                    format!("{ws}{sample}{ws}"),
                    format!("{ws}{ws}{sample}{ws}{ws}"),
                ] {
                    assert_eq!(ok(f(&v), &v), want, "{field} with {ws:?} around it");
                }
                let upper = sample.to_ascii_uppercase();
                let v = format!("{ws}{upper}{ws}");
                assert_eq!(
                    ok(f(&v), &v),
                    want,
                    "{field} upper case with {ws:?} around it"
                );
                // Inside the value it is not trimmed, so the value is refused.
                let (a, b) = sample.split_at(sample.len() / 2);
                let v = format!("{a}{ws}{b}");
                invalid(f(&v), &v, field);
                // A value of only that character is empty once trimmed, and refused.
                let v = format!("{ws}{ws}");
                invalid(f(&v), &v, field);
            }
            // The TTL.
            for (given, want) in [
                (format!("{ws}8h"), 28800),
                (format!("8h{ws}"), 28800),
                (format!("{ws}8h{ws}"), 28800),
                (format!("{ws}PT8H{ws}"), 28800),
                (format!("{ws}1h30m{ws}"), 5400),
                // Whitespace is optional between a number and its unit, and between parts.
                (format!("1{ws}h"), 3600),
                (format!("1h{ws}30m"), 5400),
                (format!("1{ws}h{ws}30{ws}m"), 5400),
            ] {
                assert_eq!(ttl_ok(&given), want, "ttl {given:?}");
            }
            // No whitespace inside the ISO form.
            for v in [
                format!("P1D{ws}T2H"),
                format!("PT{ws}8H"),
                format!("PT8{ws}H"),
            ] {
                ttl_invalid(&v);
            }
            // A TTL of only that character is a value, and not a duration.
            let v = format!("{ws}");
            ttl_invalid(&v);
            // A state name.
            for (given, want) in [
                (format!("{ws}ready"), vec!["ready"]),
                (format!("ready{ws}"), vec!["ready"]),
                (format!("{ws}Ready{ws}"), vec!["ready"]),
                (format!("pending,{ws}ready"), vec!["pending", "ready"]),
                (format!("pending{ws},ready{ws}"), vec!["pending", "ready"]),
            ] {
                let got = ok(states(&[given.as_str()]), &given);
                assert_eq!(got, want, "states {given:?}");
            }
            let v = format!("{ws}");
            let m = invalid(states(&[v.as_str()]), &v, "state");
            assert_eq!(m, msg_state("''"));
            let v = format!("re{ws}ady");
            invalid(states(&[v.as_str()]), &v, "state");
        }
    };
}

trims!(trims_u0009_tab, '\u{9}');
trims!(trims_u000a_line_feed, '\u{a}');
trims!(trims_u000b_line_tabulation, '\u{b}');
trims!(trims_u000c_form_feed, '\u{c}');
trims!(trims_u000d_carriage_return, '\u{d}');
trims!(trims_u0020_space, '\u{20}');
trims!(trims_u0085_next_line, '\u{85}');
trims!(trims_u00a0_no_break_space, '\u{a0}');
trims!(trims_u1680_ogham_space_mark, '\u{1680}');
trims!(trims_u2000_en_quad, '\u{2000}');
trims!(trims_u2001_em_quad, '\u{2001}');
trims!(trims_u2002_en_space, '\u{2002}');
trims!(trims_u2003_em_space, '\u{2003}');
trims!(trims_u2004_three_per_em_space, '\u{2004}');
trims!(trims_u2005_four_per_em_space, '\u{2005}');
trims!(trims_u2006_six_per_em_space, '\u{2006}');
trims!(trims_u2007_figure_space, '\u{2007}');
trims!(trims_u2008_punctuation_space, '\u{2008}');
trims!(trims_u2009_thin_space, '\u{2009}');
trims!(trims_u200a_hair_space, '\u{200a}');
trims!(trims_u2028_line_separator, '\u{2028}');
trims!(trims_u2029_paragraph_separator, '\u{2029}');
trims!(trims_u202f_narrow_no_break_space, '\u{202f}');
trims!(trims_u205f_medium_mathematical_space, '\u{205f}');
trims!(trims_u3000_ideographic_space, '\u{3000}');

#[test]
fn the_set_in_these_tests_is_the_set_of_the_doc() {
    // The macro list above is the White_Space property, and the helper lists the same 25.
    assert_eq!(whitespace_set().len(), 25);
    for c in whitespace_set() {
        assert!(c.is_whitespace(), "{c:?}");
    }
}

#[test]
fn every_whitespace_character_mixed_around_a_value_is_trimmed() {
    let all: String = whitespace_set().into_iter().collect();
    for (field, f, sample) in string_values() {
        let want = ok(f(sample), sample);
        let v = format!("{all}{sample}{all}");
        assert_eq!(ok(f(&v), &v), want, "{field}");
    }
    let v = format!("{all}8h{all}");
    assert_eq!(ttl_ok(&v), 28800);
    let v = format!("{all}ready{all}");
    assert_eq!(ok(states(&[v.as_str()]), &v), vec!["ready"]);
}

/// A character that looks like whitespace and is not in the set is never trimmed, so a value that
/// starts or ends with it is refused.
macro_rules! not_trimmed {
    ($name:ident, $ch:expr) => {
        #[test]
        fn $name() {
            let c: char = $ch;
            for (field, f, sample) in string_values() {
                for v in [
                    format!("{c}{sample}"),
                    format!("{sample}{c}"),
                    format!("{c}{sample}{c}"),
                    format!(" {c} {sample}"),
                    format!("{sample} {c} "),
                ] {
                    invalid(f(&v), &v, field);
                }
            }
            for v in [
                format!("{c}8h"),
                format!("8h{c}"),
                format!("PT8H{c}"),
                format!("8{c}h"),
            ] {
                ttl_invalid(&v);
            }
            for v in [
                format!("{c}ready"),
                format!("ready{c}"),
                format!("pending,{c}ready"),
            ] {
                invalid(states(&[v.as_str()]), &v, "state");
            }
        }
    };
}

not_trimmed!(u0000_nul_is_not_whitespace, '\u{0}');
not_trimmed!(u001c_file_separator_is_not_whitespace, '\u{1c}');
not_trimmed!(u001d_group_separator_is_not_whitespace, '\u{1d}');
not_trimmed!(u001e_record_separator_is_not_whitespace, '\u{1e}');
not_trimmed!(u001f_unit_separator_is_not_whitespace, '\u{1f}');
not_trimmed!(u007f_delete_is_not_whitespace, '\u{7f}');
not_trimmed!(u00ad_soft_hyphen_is_not_whitespace, '\u{ad}');
not_trimmed!(
    u180e_mongolian_vowel_separator_is_not_whitespace,
    '\u{180e}'
);
not_trimmed!(u200b_zero_width_space_is_not_whitespace, '\u{200b}');
not_trimmed!(u200c_zero_width_non_joiner_is_not_whitespace, '\u{200c}');
not_trimmed!(u200d_zero_width_joiner_is_not_whitespace, '\u{200d}');
not_trimmed!(u2060_word_joiner_is_not_whitespace, '\u{2060}');
not_trimmed!(u3164_hangul_filler_is_not_whitespace, '\u{3164}');
not_trimmed!(ufeff_byte_order_mark_is_not_whitespace, '\u{feff}');

// ---------------------------------------------------------------- U+FEFF

#[test]
fn feff_is_refused_at_the_start_in_the_middle_and_at_the_end_of_every_string_value() {
    for (field, f, sample) in string_values() {
        let (a, b) = sample.split_at(sample.len() / 2);
        for v in [
            format!("\u{feff}{sample}"),
            format!("{a}\u{feff}{b}"),
            format!("{sample}\u{feff}"),
            format!("\u{feff}{sample}\u{feff}"),
            format!(" \u{feff}{sample}"),
            format!("{sample}\u{feff} "),
            "\u{feff}".to_string(),
            " \u{feff} ".to_string(),
        ] {
            let m = invalid(f(&v), &v, field);
            assert!(
                m.contains('\u{feff}'),
                "the message quotes {v:?} as given: {m:?}"
            );
        }
    }
}

#[test]
fn feff_is_refused_at_the_start_in_the_middle_and_at_the_end_of_a_ttl() {
    for v in [
        "\u{feff}8h",
        "8\u{feff}h",
        "8h\u{feff}",
        "\u{feff}PT8H",
        "PT\u{feff}8H",
        "PT8H\u{feff}",
        "1h\u{feff}30m",
        "\u{feff}",
        "8\u{feff}",
        "\u{feff}8",
        " \u{feff} ",
        "8h \u{feff}",
    ] {
        ttl_invalid(v);
    }
}

#[test]
fn feff_is_refused_at_the_start_in_the_middle_and_at_the_end_of_a_state() {
    for v in [
        "\u{feff}ready",
        "rea\u{feff}dy",
        "ready\u{feff}",
        "ready,\u{feff}pending",
        "ready,pending\u{feff}",
        "\u{feff}",
    ] {
        let m = invalid(states(&[v]), v, "state");
        assert!(
            m.contains('\u{feff}'),
            "the refused name is quoted as it is: {m:?}"
        );
    }
    // The state that is quoted is the name refused, so a name alone with U+FEFF is quoted as it is.
    let m = invalid(
        states(&["ready,\u{feff}pending"]),
        "ready,\u{feff}pending",
        "state",
    );
    assert_eq!(m, msg_state("'\u{feff}pending'"));
}

#[test]
fn feff_message_quotes_the_value_as_given() {
    let m = invalid(project("a\u{feff}b"), "a\u{feff}b", "project");
    assert_eq!(m, msg_for("project", "'a\u{feff}b'"));
    let m = invalid(lease_id("\u{feff}"), "\u{feff}", "id");
    assert_eq!(m, msg_for("id", "'\u{feff}'"));
    let m = invalid(size("m\u{feff}"), "m\u{feff}", "size");
    assert_eq!(m, msg_for("size", "'m\u{feff}'"));
}

// ---------------------------------------------------------------- the whole value (#0029)

#[test]
fn a_trailing_line_feed_never_gets_through_a_check_for_any_value() {
    // A check that matches only up to a trailing line feed, as a regular expression's `$` does,
    // accepts the value below. Each is refused: a line feed that survives trimming is a defect.
    for (field, f, sample) in string_values() {
        for tail in ["\n\u{feff}", "\n\u{0}", "\nx", "\n.", "\n/", "\n\u{200b}"] {
            let v = format!("{sample}{tail}");
            invalid(f(&v), &v, field);
        }
    }
    for v in [
        "8h\n\u{feff}",
        "8h\n\u{0}",
        "8h\nx",
        "PT8H\n\u{200b}",
        "8\n\u{0}",
        "8\nx",
    ] {
        ttl_invalid(v);
    }
    for v in [
        "ready\n\u{0}",
        "ready\nx",
        "ready\n\u{feff}",
        "ready\n\u{200b}",
    ] {
        invalid(states(&[v]), v, "state");
    }
}

#[test]
fn a_trailing_line_feed_is_trimmed_off_a_ttl_and_a_state() {
    assert_eq!(ttl_ok("8h\n"), 28800);
    assert_eq!(ttl_ok("8h\r\n"), 28800);
    assert_eq!(ttl_ok("PT8H\n"), 28800);
    assert_eq!(ttl_ok("\n8h\n"), 28800);
    // Without a unit, the line feed is trimmed and the value is a bare number.
    let m = ttl_invalid("8\n");
    assert_eq!(m, msg_no_unit("'8\\u000a'"));
    assert_eq!(ok(states(&["ready\n"]), "ready\\n"), vec!["ready"]);
    assert_eq!(
        ok(states(&["ready\n,pending\n"]), "ready\\n,pending\\n"),
        vec!["pending", "ready"]
    );
}

#[test]
fn a_line_feed_inside_a_value_is_not_trimmed() {
    // The ends are trimmed away only where they are ends.
    assert_eq!(ok(project("a\n"), "a\\n"), "a");
    for v in ["a\nb", "\na\nb", "a\nb\n"] {
        invalid(project(v), v, "project");
    }
    for v in [
        "lin\nux/arm64",
        "linux/arm\n64",
        "linux\n/arm64",
        "linux/\narm64",
    ] {
        invalid(platform(v), v, "platform");
    }
    for v in [
        "agent\n-linux",
        "agent-linux\n@1",
        "agent-linux@\n1",
        "agent-linux@1\n2",
    ] {
        invalid(profile(v), v, "profile");
    }
    invalid(size("m\nm"), "m\nm", "size");
}

// ---------------------------------------------------------------- never panics

#[test]
fn odd_input_never_panics() {
    let mut inputs: Vec<String> = vec![
        String::new(),
        "\u{0}".to_string(),
        "\u{10ffff}".to_string(),
        "\u{fffd}".to_string(),
        "\u{e9}".repeat(100),
        "\u{65e5}\u{672c}\u{8a9e}".to_string(),
        "1\u{e9}".to_string(),
        "8h\u{65e5}".to_string(),
        "P\u{65e5}".to_string(),
        "PT8\u{1f600}".to_string(),
        "\u{1f600}".to_string(),
        "9".repeat(10_000),
        format!("{}h", "9".repeat(10_000)),
        format!("P{}D", "9".repeat(10_000)),
        format!("{}1m", "0".repeat(10_000)),
        "a".repeat(100_000),
        "/".repeat(1000),
        "@".repeat(1000),
        ",".repeat(1000),
    ];
    for c in whitespace_set() {
        inputs.push(c.to_string());
    }
    for v in &inputs {
        for (_, f, _) in string_values() {
            let _ = f(v);
        }
        let _ = ttl(v);
        let _ = states(&[v.as_str()]);
        let _ = agentspace_values::quote(v);
    }
}
