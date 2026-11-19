//! `quote_os`, and arguments that are not valid Unicode (lease-api.md, Input tolerance and Messages,
//! gap FA-09): a value is quoted as `'<value>'`, as given, with each control character and each
//! unpaired surrogate written as `\u` and four lowercase hex digits, and never with a replacement
//! character.
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. `quote_os` returns the
//! quotes as well as what is inside them (architect, 2026-11-19). On Unix, a byte that is not
//! UTF-8 is written as `\udc` and the byte's two lowercase hex digits, so 0xff is `\udcff`
//! (architect, 2026-11-19), and these tests assert that form exactly.

mod common;

use agentspace_cli::quote_os;
use common::*;
use std::ffi::{OsStr, OsString};

fn q(text: &str) -> String {
    quote_os(OsStr::new(text))
}

#[test]
fn a_plain_value_is_quoted_as_given() {
    assert_eq!(q("abc"), "'abc'");
    assert_eq!(q(""), "''");
    assert_eq!(q(" a  b "), "' a  b '");
    assert_eq!(q("--help=yes"), "'--help=yes'");
    assert_eq!(q("linux/arm64"), "'linux/arm64'");
}

#[test]
fn a_quote_and_a_backslash_in_the_value_are_kept_as_given() {
    assert_eq!(q("it's"), "'it's'");
    assert_eq!(q("a\\b"), "'a\\b'");
    assert_eq!(q("\"x\""), "'\"x\"'");
}

#[test]
fn every_control_character_is_written_as_backslash_u_and_four_lowercase_hex_digits() {
    for c in (0u32..=0x1f).chain([0x7f]) {
        let ch = char::from_u32(c).expect("A character");
        let want = format!("'a\\u{c:04x}b'");
        assert_eq!(q(&format!("a{ch}b")), want, "U+{c:04X}");
    }
    assert_eq!(q("\n"), "'\\u000a'");
    assert_eq!(q("\r\n"), "'\\u000d\\u000a'");
    assert_eq!(q("\t"), "'\\u0009'");
    assert_eq!(q("\u{1b}[0m"), "'\\u001b[0m'");
    assert_eq!(q("\u{1f}"), "'\\u001f'");
    assert_eq!(q("\u{7f}"), "'\\u007f'");
    assert_eq!(q("\0"), "'\\u0000'");
}

#[test]
fn the_hex_digits_are_lowercase() {
    for c in 0x0au32..=0x0f {
        let ch = char::from_u32(c).expect("A character");
        let out = q(&ch.to_string());
        assert!(!out.chars().any(|x| x.is_ascii_uppercase()), "{out}");
    }
    assert_eq!(q("\u{1d}"), "'\\u001d'");
    assert_eq!(q("\u{1e}"), "'\\u001e'");
}

#[test]
fn what_is_not_a_control_character_is_left_as_given() {
    // The spec's control characters are U+0000 to U+001F and U+007F, and no others.
    for text in [
        "\u{80}",
        "\u{85}",
        "\u{9f}",
        "\u{a0}",
        "\u{e9}",
        "\u{2028}",
        "\u{2029}",
        "\u{3000}",
        "\u{feff}",
        "\u{fffd}",
        "\u{1f600}",
        "caf\u{e9}",
        "\u{7e}",
        "\u{20}",
    ] {
        assert_eq!(q(text), format!("'{text}'"), "{text:?}");
    }
}

#[test]
fn the_quoted_value_holds_no_control_character_whatever_it_was() {
    let all: String = (0u32..=0x7f).filter_map(char::from_u32).collect();
    let out = q(&all);
    assert!(out.starts_with('\'') && out.ends_with('\''));
    assert!(!out.chars().any(|c| (c as u32) <= 0x1f || c == '\u{7f}'));
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    fn bytes(b: &[u8]) -> OsString {
        OsString::from_vec(b.to_vec())
    }

    #[test]
    fn an_invalid_utf8_value_never_gets_a_replacement_character() {
        for b in [
            &b"fr\xffob"[..],
            b"\xff",
            b"\xfe\xff",
            b"a\x80b",
            b"\xc3",
            b"\xed\xa0\x80",
            b"\xed\xbf\xbf",
            b"\xf4\x90\x80\x80",
            b"\xe2\x82",
            b"ok\xc3\x28",
        ] {
            let out = quote_os(&bytes(b));
            assert!(!out.contains('\u{fffd}'), "{b:?} gave {out:?}");
            assert!(
                out.starts_with('\'') && out.ends_with('\''),
                "{b:?} gave {out:?}"
            );
            assert!(
                !out.chars().any(|c| (c as u32) <= 0x1f || c == '\u{7f}'),
                "{b:?} gave {out:?}"
            );
        }
    }

    #[test]
    fn a_byte_that_is_not_utf8_is_written_as_backslash_udc_and_its_two_hex_digits() {
        assert_eq!(quote_os(&bytes(b"\xff")), "'\\udcff'");
        assert_eq!(quote_os(&bytes(b"fr\xffob")), "'fr\\udcffob'");
        assert_eq!(quote_os(&bytes(b"a\x80b")), "'a\\udc80b'");
        assert_eq!(quote_os(&bytes(b"\xc3")), "'\\udcc3'");
        // Each byte of a cut-off character is its own unit.
        assert_eq!(quote_os(&bytes(b"\xe2\x82")), "'\\udce2\\udc82'");
        // A surrogate encoded in UTF-8 is three bad bytes, not one unit.
        assert_eq!(quote_os(&bytes(b"\xed\xa0\x80")), "'\\udced\\udca0\\udc80'");
        assert_eq!(quote_os(&bytes(b"a\nb\xff")), "'a\\u000ab\\udcff'");
        assert_eq!(quote_os(&bytes(b"ok\xc3\x28")), "'ok\\udcc3('");
    }

    #[test]
    fn the_valid_part_of_an_invalid_value_is_kept() {
        let out = quote_os(&bytes(b"fr\xffob"));
        assert!(out.starts_with("'fr") && out.ends_with("ob'"), "{out:?}");
        let out = quote_os(&bytes(b"a\nb\xff"));
        assert!(out.starts_with("'a\\u000ab"), "{out:?}");
    }

    fn check(line: &[OsString], code: &str, field: Option<&str>, prefix: &str, suffix: &str) {
        let run = run_bin(line);
        let p = failure_problems(&run, code, field, None);
        assert!(p.is_empty(), "{}: {}", show(line), p.join("; "));
        let doc: serde_json::Value = serde_json::from_slice(&run.stderr).expect("JSON");
        let message = doc["message"].as_str().expect("A message");
        assert!(message.starts_with(prefix), "{}: {message:?}", show(line));
        assert!(message.ends_with(suffix), "{}: {message:?}", show(line));
        assert!(!message.contains('\u{fffd}'), "{}: {message:?}", show(line));
        assert_eq!(
            in_process(line),
            exit_for(code),
            "{} in process",
            show(line)
        );
    }

    #[test]
    fn a_non_utf8_command_or_subcommand_or_option_is_the_usage_error_any_unknown_word_is() {
        let bad = bytes(b"fr\xffob");
        check(
            std::slice::from_ref(&bad),
            "usage",
            None,
            "Unknown command 'fr",
            "ob'. Run 'agentspace --help' for the commands.",
        );
        check(
            &[os("lease"), bad.clone()],
            "usage",
            None,
            "Unknown subcommand 'fr",
            "ob'. Run 'agentspace lease --help' for the subcommands.",
        );
        check(
            &[bytes(b"--bo\xffgus")],
            "usage",
            None,
            "Unknown option '--bo",
            "gus'. Run 'agentspace --help' for its options.",
        );
        check(
            &[os("lease"), bytes(b"--bo\xffgus")],
            "usage",
            None,
            "Unknown option '--bo",
            "gus'. Run 'agentspace lease --help' for its options.",
        );
        check(
            &[os("lease"), os("list"), bytes(b"--bo\xffgus")],
            "usage",
            None,
            "Unknown option '--bo",
            "gus' for agentspace lease list. Run 'agentspace lease list --help'.",
        );
        check(
            &[os("lease"), os("list"), bytes(b"-\xff")],
            "usage",
            None,
            "Unknown option '-",
            "' for agentspace lease list. Run 'agentspace lease list --help'.",
        );
        check(
            &[os("lease"), os("list"), bad],
            "usage",
            None,
            "Unexpected argument 'fr",
            "ob' for agentspace lease list. Run 'agentspace lease list --help'.",
        );
    }

    #[test]
    fn a_flag_name_that_is_not_ascii_letters_is_an_unknown_option_even_when_invalid() {
        check(
            &[
                os("lease"),
                os("create"),
                os("--project"),
                os("x"),
                bytes(b"--tt\xffl"),
                os("8h"),
            ],
            "usage",
            None,
            "Unknown option '--tt",
            "l' for agentspace lease create. Run 'agentspace lease create --help'.",
        );
    }

    #[test]
    fn a_non_utf8_value_or_lease_id_is_read_as_written_and_the_line_goes_on_to_the_commands() {
        // This story's commands end `internal`; the value's own check, invalid_value for that
        // field, is E0.05-T03-S02's. What MUST NOT happen is a usage error or a panic.
        let lines: Vec<Vec<OsString>> = vec![
            vec![os("lease"), os("create"), os("--project"), bytes(b"x\xff")],
            vec![os("lease"), os("create"), bytes(b"--project=x\xff")],
            vec![
                os("lease"),
                os("create"),
                os("--project"),
                os("x"),
                os("--ttl"),
                bytes(b"\xff"),
            ],
            vec![
                os("lease"),
                os("create"),
                os("--project"),
                os("x"),
                bytes(b"--ttl=\xed\xa0\x80"),
            ],
            vec![os("lease"), os("get"), bytes(b"\xff")],
            vec![
                os("lease"),
                os("renew"),
                bytes(b"\xff"),
                os("--ttl"),
                os("8h"),
            ],
            vec![os("lease"), os("release"), bytes(b"3f2a\x80")],
            vec![os("lease"), os("list"), os("--state"), bytes(b"re\xffady")],
        ];
        for line in lines {
            let sub = line[1].to_string_lossy().into_owned();
            let run = run_bin(&line);
            let p = failure_problems(&run, "internal", None, Some(&msg_stub(&sub)));
            assert!(p.is_empty(), "{}: {}", show(&line), p.join("; "));
            assert_eq!(in_process(&line), 1, "{} in process", show(&line));
        }
    }

    #[test]
    fn a_non_utf8_value_next_to_help_is_still_help() {
        let line = [
            os("lease"),
            os("create"),
            os("--ttl"),
            bytes(b"\xff"),
            os("--help"),
        ];
        // The value `\xff` is read as the value, and then `--help` asks for help.
        let run = run_bin(&line);
        let p = help_problems(&run, "agentspace lease create");
        assert!(p.is_empty(), "{}", p.join("; "));
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::ffi::OsStringExt;

    fn wide(units: &[u16]) -> OsString {
        OsString::from_wide(units)
    }

    #[test]
    fn each_unpaired_surrogate_is_written_as_backslash_u_and_four_lowercase_hex_digits() {
        assert_eq!(quote_os(&wide(&[0xd800])), "'\\ud800'");
        assert_eq!(quote_os(&wide(&[0xdc00])), "'\\udc00'");
        assert_eq!(quote_os(&wide(&[0xdbff])), "'\\udbff'");
        assert_eq!(quote_os(&wide(&[0xdfff])), "'\\udfff'");
        assert_eq!(quote_os(&wide(&[0x61, 0xd800, 0x62])), "'a\\ud800b'");
        assert_eq!(quote_os(&wide(&[0xdc00, 0xd800])), "'\\udc00\\ud800'");
        assert_eq!(quote_os(&wide(&[0x0a, 0xd83d])), "'\\u000a\\ud83d'");
        // A pair is one character, not two unpaired surrogates.
        assert_eq!(quote_os(&wide(&[0xd83d, 0xde00])), "'\u{1f600}'");
    }

    #[test]
    fn an_unpaired_surrogate_in_a_command_or_option_is_a_usage_error_with_valid_utf8() {
        let run = run_bin(&[wide(&[0x66, 0xd800, 0x6f])]);
        let p = failure_problems(&run, "usage", None, Some(&msg_unknown_command("f\\ud800o")));
        assert!(p.is_empty(), "{}", p.join("; "));
        let run = run_bin(&[os("lease"), os("get"), wide(&[0x2d, 0x2d, 0xdc00])]);
        let p = failure_problems(
            &run,
            "usage",
            None,
            Some(&msg_unknown_option_sub("--\\udc00", "get")),
        );
        assert!(p.is_empty(), "{}", p.join("; "));
    }

    #[test]
    fn an_unpaired_surrogate_as_a_value_or_lease_id_goes_on_to_the_commands() {
        let run = run_bin(&[os("lease"), os("get"), wide(&[0x61, 0xd800])]);
        let p = failure_problems(&run, "internal", None, Some(&msg_stub("get")));
        assert!(p.is_empty(), "{}", p.join("; "));
        let run = run_bin(&[
            os("lease"),
            os("create"),
            os("--project"),
            os("x"),
            os("--ttl"),
            wide(&[0xdfff]),
        ]);
        let p = failure_problems(&run, "internal", None, Some(&msg_stub("create")));
        assert!(p.is_empty(), "{}", p.join("; "));
    }
}
