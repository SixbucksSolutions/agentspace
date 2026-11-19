// RFC 2119 keywords in these comments; the capitals are load-bearing.
//
// Project, platform, profile and size: docs/lease-api.md, Values. Each is accepted in the forms
// listed, normalized to one form, and refused with `invalid_value` otherwise.

mod common;

use agentspace_values::{platform, profile, project, size};
use common::*;

// ---------------------------------------------------------------- project

#[test]
fn project_is_lowercased_and_trimmed() {
    for (given, want) in [
        ("x", "x"),
        ("X", "x"),
        ("My-Proj", "my-proj"),
        ("  Alpha-9 ", "alpha-9"),
        ("0", "0"),
        ("9a", "9a"),
        ("a--b", "a--b"),
        ("A-B-C-D", "a-b-c-d"),
        ("\tproj\n", "proj"),
    ] {
        assert_eq!(ok(project(given), given), want);
    }
}

#[test]
fn project_of_1_to_32_characters_is_accepted() {
    let c32 = "a".repeat(32);
    assert_eq!(ok(project(&c32), &c32), c32);
    let mid = format!("a{}z", "-".repeat(30));
    assert_eq!(mid.len(), 32);
    assert_eq!(ok(project(&mid), &mid), mid);
    let upper32 = "Q".repeat(32);
    assert_eq!(ok(project(&upper32), &upper32), "q".repeat(32));
}

#[test]
fn project_of_33_characters_is_refused() {
    // From conformance/lease-cli/test-values.ps1.
    let c33 = "a".repeat(33);
    invalid(project(&c33), &c33, "project");
    let c64 = "a".repeat(64);
    invalid(project(&c64), &c64, "project");
}

#[test]
fn project_rules_for_hyphen_placement() {
    for v in ["-x", "x-", "-x-", "-", "--", "a-b-", "-a-b"] {
        invalid(project(v), v, "project");
    }
}

#[test]
fn project_refuses_characters_outside_letters_digits_and_hyphen() {
    for v in [
        "a_b",
        "a.b",
        "a b",
        "a/b",
        "a\\b",
        "a@b",
        "a:b",
        "a+b",
        "a*b",
        "a\u{e9}",
        "\u{e9}a",
        "caf\u{e9}",
        "a\u{ff0d}b",
        "\u{ff21}",
        "\u{661}",
        "a\u{2010}b",
        "a\u{2212}b",
    ] {
        invalid(project(v), v, "project");
    }
}

#[test]
fn project_refuses_empty_and_whitespace_only() {
    for v in ["", " ", "\t", "\n", "\u{3000}", "  \r\n  "] {
        invalid(project(v), v, "project");
    }
}

#[test]
fn project_refuses_whitespace_inside() {
    for v in ["a b", "a\tb", "a\nb", "a\u{a0}b", "a\u{3000}b", "a  b"] {
        invalid(project(v), v, "project");
    }
}

#[test]
fn project_letters_are_ascii_only_even_when_unicode_case_maps_to_ascii() {
    // U+212A KELVIN SIGN lowercases to 'k', and U+017F LATIN SMALL LETTER LONG S uppercases to 'S'.
    // Letters and digits are ASCII only, and "any case" is ASCII case (Input tolerance).
    for v in [
        "\u{212a}",
        "\u{17f}",
        "a\u{212a}",
        "\u{130}",
        "\u{131}",
        "\u{212b}",
    ] {
        invalid(project(v), v, "project");
    }
}

#[test]
fn project_conformance_cases() {
    // From conformance/lease-cli/test-values.ps1.
    for v in ["a_b", " ", "\t", "-x-", "a\tb", "bad_"] {
        invalid(project(v), v, "project");
    }
}

#[test]
fn project_message_quotes_the_value_untrimmed() {
    let v = " a_b ";
    assert_eq!(
        invalid(project(v), v, "project"),
        msg_for("project", "' a_b '")
    );
    let v = "";
    assert_eq!(invalid(project(v), v, "project"), msg_for("project", "''"));
    assert_eq!(
        invalid(project("-x-"), "-x-", "project"),
        "Project '-x-' is not a project name. Use 1 to 32 letters, digits and '-', starting and ending with a letter or digit."
    );
}

// ---------------------------------------------------------------- platform

#[test]
fn platform_accepts_os_arch() {
    for (given, want) in [
        ("linux/arm64", "linux/arm64"),
        ("Linux/ARM64", "linux/arm64"),
        ("linux/amd64", "linux/amd64"),
        ("windows/amd64", "windows/amd64"),
        (" linux/arm64 ", "linux/arm64"),
        ("\tlinux/arm64\n", "linux/arm64"),
        ("darwin/arm64", "darwin/arm64"),
        ("linux/386", "linux/386"),
        ("linux/arm", "linux/arm"),
        ("a/b", "a/b"),
        ("A1/B2", "a1/b2"),
    ] {
        assert_eq!(ok(platform(given), given), want);
    }
}

#[test]
fn platform_aarch64_is_arm64() {
    for (given, want) in [
        ("linux/aarch64", "linux/arm64"),
        ("linux/AARCH64", "linux/arm64"),
        ("Linux/Aarch64", "linux/arm64"),
        ("windows/aarch64", "windows/arm64"),
    ] {
        assert_eq!(ok(platform(given), given), want);
    }
}

#[test]
fn platform_x86_64_and_x86_dash_64_are_amd64() {
    for (given, want) in [
        ("linux/x86_64", "linux/amd64"),
        ("linux/x86-64", "linux/amd64"),
        ("linux/X86_64", "linux/amd64"),
        ("LINUX/X86-64", "linux/amd64"),
        ("windows/x86_64", "windows/amd64"),
        (" linux/x86_64 ", "linux/amd64"),
    ] {
        assert_eq!(ok(platform(given), given), want);
    }
}

#[test]
fn platform_v8_of_arm64_is_dropped() {
    // lease-api.md: `linux/aarch64/v8` becomes `linux/arm64`.
    for (given, want) in [
        ("linux/aarch64/v8", "linux/arm64"),
        ("linux/arm64/v8", "linux/arm64"),
        ("LINUX/AARCH64/V8", "linux/arm64"),
        ("Linux/Arm64/V8", "linux/arm64"),
        (" linux/arm64/v8 ", "linux/arm64"),
    ] {
        assert_eq!(ok(platform(given), given), want);
    }
}

#[test]
fn platform_other_variants_are_kept() {
    // lease-api.md: `linux/arm64/v9` and `linux/amd64/v3` stay as they are.
    for (given, want) in [
        ("linux/arm64/v9", "linux/arm64/v9"),
        ("linux/amd64/v3", "linux/amd64/v3"),
        ("linux/aarch64/v9", "linux/arm64/v9"),
        ("linux/x86_64/v3", "linux/amd64/v3"),
        ("linux/x86-64/v2", "linux/amd64/v2"),
        ("Linux/AMD64/V3", "linux/amd64/v3"),
        ("linux/arm/v7", "linux/arm/v7"),
        ("linux/arm/v8", "linux/arm/v8"),
        ("linux/amd64/v8", "linux/amd64/v8"),
        ("linux/arm64/v7", "linux/arm64/v7"),
        ("linux/arm64/other", "linux/arm64/other"),
    ] {
        assert_eq!(ok(platform(given), given), want);
    }
}

#[test]
fn platform_conformance_cases_are_refused() {
    // From conformance/lease-cli/test-values.ps1.
    for v in ["linux", "linux/arm64/v8/x"] {
        invalid(platform(v), v, "platform");
    }
}

#[test]
fn platform_refuses_wrong_shapes() {
    for v in [
        "",
        " ",
        "/",
        "//",
        "linux/",
        "/arm64",
        "linux//arm64",
        "linux/arm64/",
        "linux/arm64//v8",
        "/linux/arm64",
        "linux/arm64/v8/",
        "linux/arm64/v8/x/y",
        "linux arm64",
        "linux / arm64",
        "linux/ arm64",
        "linux /arm64",
        "linux\\arm64",
        "linux:arm64",
    ] {
        invalid(platform(v), v, "platform");
    }
}

#[test]
fn platform_refuses_underscore_and_hyphen_except_in_the_architecture_spellings() {
    for v in [
        "linux/arm_64",
        "linux/arm-64",
        "li-nux/arm64",
        "li_nux/arm64",
        "x86_64/arm64",
        "x86-64/arm64",
        "linux/amd64/x86_64",
        "linux/arm64/v-8",
        "linux/arm64/v_8",
        "linux/amd64/x86-64",
        "linux/x86_64_",
        "linux/_x86_64",
        "linux/x86__64",
        "linux/x86_-64",
        "linux/x86_",
        "linux/x86-",
    ] {
        invalid(platform(v), v, "platform");
    }
}

#[test]
fn platform_refuses_non_ascii_letters_and_digits() {
    for v in [
        "linux/\u{e4}rm64",
        "l\u{131}nux/arm64",
        "linux/arm\u{ff16}\u{ff14}",
        "\u{212a}inux/arm64",
        "linux/arm64/\u{ff56}8",
        "linux/\u{661}",
    ] {
        invalid(platform(v), v, "platform");
    }
}

#[test]
fn platform_conformance_linux_alone_is_invalid() {
    let v = "linux";
    assert_eq!(
        invalid(platform(v), v, "platform"),
        "Platform 'linux' is not an os/arch pair. Give one as in 'linux/arm64'."
    );
}

#[test]
fn platform_message_quotes_the_value_untrimmed_and_escaped() {
    let v = " linux/arm64/v8/x ";
    assert_eq!(
        invalid(platform(v), v, "platform"),
        msg_for("platform", "' linux/arm64/v8/x '")
    );
    let v = "linux/\narm64";
    assert_eq!(
        invalid(platform(v), v, "platform"),
        msg_for("platform", "'linux/\\u000aarm64'")
    );
}

// ---------------------------------------------------------------- profile

#[test]
fn profile_is_lowercased_trimmed_with_leading_zeros_dropped() {
    for (given, want) in [
        ("agent-linux", "agent-linux"),
        ("Agent-Linux", "agent-linux"),
        ("Agent-Linux@03", "agent-linux@3"),
        ("agent-linux@1", "agent-linux@1"),
        ("agent-linux@10", "agent-linux@10"),
        ("agent-linux@007", "agent-linux@7"),
        ("agent-linux@0100", "agent-linux@100"),
        ("agent-linux@000010", "agent-linux@10"),
        ("  agent-linux@2  ", "agent-linux@2"),
        ("\tAGENT-LINUX@02\n", "agent-linux@2"),
        ("a", "a"),
        ("a@1", "a@1"),
        ("7@7", "7@7"),
        ("a--b", "a--b"),
        ("x@123456789", "x@123456789"),
    ] {
        assert_eq!(ok(profile(given), given), want);
    }
}

#[test]
fn profile_without_a_version_stays_without_one() {
    let n = ok(profile("Agent-Linux"), "Agent-Linux");
    assert!(!n.contains('@'));
}

#[test]
fn profile_name_of_1_to_64_characters() {
    let c64 = "a".repeat(64);
    assert_eq!(ok(profile(&c64), &c64), c64);
    let c64v = format!("{}@5", "b".repeat(64));
    assert_eq!(ok(profile(&c64v), &c64v), c64v);
    let c65 = "a".repeat(65);
    invalid(profile(&c65), &c65, "profile");
    let c65v = format!("{}@5", "a".repeat(65));
    invalid(profile(&c65v), &c65v, "profile");
}

#[test]
fn profile_version_of_only_zeros_is_refused() {
    // From conformance: `agent-linux@0` is invalid_value for profile.
    for v in [
        "agent-linux@0",
        "agent-linux@00",
        "agent-linux@0000000000",
        " agent-linux@0 ",
    ] {
        invalid(profile(v), v, "profile");
    }
}

#[test]
fn profile_refuses_malformed_versions() {
    for v in [
        "agent-linux@",
        "agent-linux@@1",
        "agent-linux@1@2",
        "agent-linux@v1",
        "agent-linux@1.2",
        "agent-linux@1.0",
        "agent-linux@-1",
        "agent-linux@+1",
        "agent-linux@1a",
        "agent-linux@ 1",
        "agent-linux @1",
        "agent-linux@1 2",
        "agent-linux@\u{661}",
        "agent-linux@\u{ff11}",
        "agent-linux@1\u{ff11}",
    ] {
        invalid(profile(v), v, "profile");
    }
}

#[test]
fn profile_refuses_malformed_names() {
    for v in [
        "",
        " ",
        "@1",
        "@",
        "-a",
        "a-",
        "-a@1",
        "a-@1",
        "agent linux",
        "agent_linux",
        "agent.linux",
        "agent/linux",
        "agent-linux@0@1",
        "agent\u{e9}",
        "\u{212a}",
        "agent-lin\u{17f}",
        "agent\u{2010}linux",
    ] {
        invalid(profile(v), v, "profile");
    }
}

#[test]
fn profile_accepts_a_very_long_version_without_panicking() {
    // lease-api.md gives the version no upper bound ("a version number of 1 or more").
    let digits = "9".repeat(300);
    let v = format!("a@{digits}");
    assert_eq!(ok(profile(&v), &v), v);
    let v = format!("a@000{digits}");
    assert_eq!(ok(profile(&v), &v), format!("a@{digits}"));
}

#[test]
fn profile_message_quotes_the_value() {
    let v = "agent-linux@0";
    assert_eq!(
        invalid(profile(v), v, "profile"),
        "Profile 'agent-linux@0' is not a profile name. Give name or name@version, as in 'agent-linux'; a version is 1 or more."
    );
    let v = " agent linux ";
    assert_eq!(
        invalid(profile(v), v, "profile"),
        msg_for("profile", "' agent linux '")
    );
}

// ---------------------------------------------------------------- size

#[test]
fn size_is_lowercased_and_trimmed() {
    for (given, want) in [
        ("m", "m"),
        ("M", "m"),
        ("  Large ", "large"),
        ("XL2", "xl2"),
        ("12345678", "12345678"),
        ("abcdefgh", "abcdefgh"),
        ("ABCDEFGH", "abcdefgh"),
        ("\tm\n", "m"),
        ("1", "1"),
    ] {
        assert_eq!(ok(size(given), given), want);
    }
}

#[test]
fn size_of_9_characters_is_refused() {
    // From conformance/lease-cli/test-values.ps1.
    for v in ["toolarge9", "abcdefghi", "123456789"] {
        invalid(size(v), v, "size");
    }
}

#[test]
fn size_refuses_other_characters_and_empty() {
    for v in [
        "", " ", "\t", "\n", "a-b", "a_b", "a b", "a.b", "a/b", "-", "\u{e9}", "m\u{e9}",
        "\u{ff4d}", "\u{212a}", "\u{17f}", "\u{661}",
    ] {
        invalid(size(v), v, "size");
    }
}

#[test]
fn size_message_quotes_the_value_untrimmed() {
    let v = " toolarge9 ";
    assert_eq!(
        invalid(size(v), v, "size"),
        msg_for("size", "' toolarge9 '")
    );
    let v = " ";
    assert_eq!(
        invalid(size(v), v, "size"),
        "Size ' ' is not a size. Use 1 to 8 letters and digits, as in 'm'."
    );
}

// ---------------------------------------------------------------- a trailing line feed (#0029)

#[test]
fn a_trailing_line_feed_is_trimmed_for_every_name_and_never_kept() {
    for (field, f, sample) in string_values() {
        let v = format!("{sample}\n");
        let n = ok(f(&v), &v);
        assert_eq!(n, sample.to_ascii_lowercase(), "{field}");
        assert!(!n.contains('\n'), "{field}");
        let v = format!("{sample}\r\n\n");
        assert!(!ok(f(&v), &v).contains(['\n', '\r']), "{field}");
    }
}

#[test]
fn a_line_feed_anywhere_but_the_ends_refuses_every_name() {
    for (field, f, sample) in string_values() {
        let (a, b) = sample.split_at(sample.len() / 2);
        let v = format!("{a}\n{b}");
        invalid(f(&v), &v, field);
        // A line feed that a trim does not reach, because a character that is not whitespace
        // follows it, is not let through by a check that stops before the end of the line.
        let v = format!("{sample}\n\u{0}");
        invalid(f(&v), &v, field);
        let v = format!("{sample}\nx");
        invalid(f(&v), &v, field);
    }
}
