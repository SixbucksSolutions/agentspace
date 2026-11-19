// RFC 2119 keywords in these comments; the capitals are load-bearing.
//
// TTL: docs/lease-api.md, Values, "TTL", with the TTL's three `invalid_value` templates and the
// `out_of_range` template of Messages (architect's rulings on gap FA-08 included). The result is a
// whole number of seconds.

mod common;

use agentspace_values::ttl;
use common::*;

// ---------------------------------------------------------------- accepted

#[test]
fn the_examples_of_the_doc_are_accepted_in_seconds() {
    for (given, want) in [
        ("8h", 28_800),
        ("90m", 5_400),
        ("1h30m", 5_400),
        ("1h 30m", 5_400),
        ("2 hours", 7_200),
        ("45 min", 2_700),
        ("1d", 86_400),
        ("23h59m60s", 86_400),
        ("PT8H", 28_800),
        ("pt90m", 5_400),
        ("P1D", 86_400),
    ] {
        assert_eq!(ttl_ok(given), want, "{given}");
    }
}

#[test]
fn the_iso_example_p1dt2h_is_well_formed_and_out_of_range() {
    // 26 hours: the limits are checked after normalization.
    let m = ttl_range("P1DT2H");
    assert_eq!(m, msg_range("'P1DT2H'"));
    assert_eq!(ttl_range("p1dt2h"), msg_range("'p1dt2h'"));
}

#[test]
fn the_limits() {
    // 59s and 24h1s are out, 60s, 1440m, P1D and 86400s are in (lease-api.md, TTL).
    assert_eq!(ttl_range("59s"), msg_range("'59s'"));
    assert_eq!(ttl_ok("60s"), 60);
    assert_eq!(ttl_ok("1m"), 60);
    assert_eq!(ttl_ok("1440m"), 86_400);
    assert_eq!(ttl_ok("P1D"), 86_400);
    assert_eq!(ttl_ok("86400s"), 86_400);
    assert_eq!(ttl_range("86401s"), msg_range("'86401s'"));
    assert_eq!(ttl_range("24h1s"), msg_range("'24h1s'"));
    assert_eq!(ttl_ok("24h"), 86_400);
    assert_eq!(ttl_ok("24h0s"), 86_400);
    assert_eq!(ttl_range("1441m"), msg_range("'1441m'"));
    assert_eq!(ttl_ok("1439m"), 86_340);
    assert_eq!(ttl_ok("23h59m59s"), 86_399);
    assert_eq!(ttl_ok("0d23h59m60s"), 86_400);
    assert_eq!(ttl_range("1d0h0m1s"), msg_range("'1d0h0m1s'"));
    assert_eq!(ttl_ok("1d0h0m0s"), 86_400);
}

#[test]
fn below_the_minimum_is_out_of_range() {
    for v in [
        "59s", "1s", "0s", "30s", "0m", "0h", "0d", "0h0m", "0d0h0m0s", "000m", "PT59S", "PT1S",
    ] {
        assert_eq!(ttl_range(v), msg_range(&format!("'{v}'")));
    }
}

#[test]
fn zero_with_a_unit_is_out_of_range_and_never_invalid() {
    // Short form and ISO form (lease-api.md: "including a zero with a unit, such as 0m or PT0S").
    for v in [
        "0m", "PT0S", "0s", "0h", "0d", "PT0M", "PT0H", "P0D", "PT0H0M0S", "P0DT0H", "pt0s",
    ] {
        ttl_range(v);
    }
}

#[test]
fn above_the_maximum_is_out_of_range() {
    for v in [
        "48h",
        "P2D",
        "2d",
        "25h",
        "86401s",
        "1441m",
        "1d1s",
        "1d1m",
        "1d1h",
        "PT25H",
        "PT1441M",
        "PT86401S",
        "P1DT1S",
        "P1DT1M",
        "P1DT1H",
        "P1DT0H0M1S",
        "100000s",
        "999d",
    ] {
        assert_eq!(ttl_range(v), msg_range(&format!("'{v}'")));
    }
}

#[test]
fn the_normal_form_is_seconds() {
    for (given, want) in [
        ("1h30m15s", 5_415),
        ("PT1H30M15S", 5_415),
        ("PT60S", 60),
        ("P0DT1H", 3_600),
        ("P1DT0H", 86_400),
        ("PT24H", 86_400),
        ("PT1440M", 86_400),
        ("P1DT0S", 86_400),
        ("p1dt0s", 86_400),
        ("1d0h0m0s", 86_400),
        ("0d0h1m", 60),
        ("007m", 420),
        ("0000008h", 28_800),
        ("08h", 28_800),
    ] {
        assert_eq!(ttl_ok(given), want, "{given}");
    }
}

#[test]
fn leading_zeros_are_allowed_without_limit() {
    assert_eq!(ttl_ok("08h"), 28_800);
    let long = format!("{}8h", "0".repeat(500));
    assert_eq!(ttl_ok(&long), 28_800);
    let long = format!("{}1m", "0".repeat(10_000));
    assert_eq!(ttl_ok(&long), 60);
    let iso = format!("PT{}8H", "0".repeat(500));
    assert_eq!(ttl_ok(&iso), 28_800);
}

#[test]
fn parts_with_whitespace_between_number_and_unit_and_between_parts() {
    for (given, want) in [
        ("1h 30m", 5_400),
        ("1 h 30 m", 5_400),
        ("1   h   30   m", 5_400),
        ("1h\t30m", 5_400),
        ("1\th\t30\tm", 5_400),
        ("  1h30m  ", 5_400),
        ("\n1h30m\n", 5_400),
        ("1 hour 30 minutes", 5_400),
        ("1 hour 30 minutes 15 seconds", 5_415),
    ] {
        assert_eq!(ttl_ok(given), want, "{given:?}");
    }
    // 1d and 1m is over the maximum.
    ttl_range("1d 0h 1m 0s");
}

// ---------------------------------------------------------------- unit spellings

#[test]
fn every_day_spelling_in_any_case() {
    for spelling in ["d", "day", "days"] {
        for u in case_variants(spelling) {
            for v in [format!("1{u}"), format!("1 {u}"), format!("  1  {u}  ")] {
                assert_eq!(ttl_ok(&v), 86_400, "{v:?}");
            }
        }
    }
}

#[test]
fn every_hour_spelling_in_any_case() {
    for spelling in ["h", "hr", "hrs", "hour", "hours"] {
        for u in case_variants(spelling) {
            for v in [format!("2{u}"), format!("2 {u}"), format!("  2  {u}  ")] {
                assert_eq!(ttl_ok(&v), 7_200, "{v:?}");
            }
        }
    }
}

#[test]
fn every_minute_spelling_in_any_case_and_lowercase_m() {
    for spelling in ["min", "mins", "minute", "minutes"] {
        for u in case_variants(spelling) {
            for v in [format!("2{u}"), format!("2 {u}"), format!("  2  {u}  ")] {
                assert_eq!(ttl_ok(&v), 120, "{v:?}");
            }
        }
    }
    for v in ["2m", "2 m", "  2  m  "] {
        assert_eq!(ttl_ok(v), 120, "{v:?}");
    }
}

#[test]
fn every_second_spelling_in_any_case() {
    for spelling in ["s", "sec", "secs", "second", "seconds"] {
        for u in case_variants(spelling) {
            for v in [format!("90{u}"), format!("90 {u}"), format!("  90  {u}  ")] {
                assert_eq!(ttl_ok(&v), 90, "{v:?}");
            }
        }
    }
}

#[test]
fn uppercase_m_alone_is_never_minutes() {
    for v in ["2M", "2 M", "2\tM", "1h2M", "1h 2 M", "  2M  ", "2M\n"] {
        let m = ttl_invalid(v);
        assert_eq!(m, msg_m(&format!("'{}'", escape(v))), "{v:?}");
    }
}

#[test]
fn mixed_spellings_in_one_value() {
    assert_eq!(ttl_ok("1 DAY"), 86_400);
    assert_eq!(ttl_ok("0 d 23 Hrs 59 MINS 60 Secs"), 86_400);
    assert_eq!(ttl_ok("23HOURS59Minutes60SECONDS"), 86_400);
    assert_eq!(ttl_ok("1hr30min"), 5_400);
    assert_eq!(ttl_ok("1h30m"), 5_400);
    assert_eq!(ttl_ok("1H30MIN"), 5_400);
}

#[test]
fn spellings_that_are_not_units_are_refused() {
    for v in [
        "1 hh",
        "1 hourss",
        "1 dayss",
        "1 hrz",
        "1 minn",
        "1 secc",
        "1 seco",
        "1 mn",
        "1 hou",
        "1 da",
        "1 dy",
        "1 ms",
        "1 us",
        "1 ns",
        "1 sc",
        "1 ss",
        "1 hs",
        "1 dd",
        "1 mm",
        "1 hm",
        "1 minut",
        "1 secon",
        "1 mins.",
        "1 h.",
        "1 d.",
        "1hx",
        "1 hours!",
        "1 minutes,",
    ] {
        ttl_invalid(v);
    }
}

#[test]
fn a_unit_is_not_matched_by_prefix() {
    // `1hour30` and the like: a trailing number with no unit.
    for v in ["1hour30", "1h30", "1d1h30", "1m30", "30s1"] {
        ttl_invalid(v);
    }
}

// ---------------------------------------------------------------- ISO 8601

#[test]
fn iso_in_any_case() {
    for v in ["PT8H", "pt8h", "Pt8h", "pT8H", "PT8h", "pt8H"] {
        assert_eq!(ttl_ok(v), 28_800, "{v}");
    }
    for v in ["P1D", "p1d", "P1d", "p1D"] {
        assert_eq!(ttl_ok(v), 86_400, "{v}");
    }
    for v in ["PT90M", "pt90m", "PT90m", "pt90M"] {
        assert_eq!(ttl_ok(v), 5_400, "{v}");
    }
    for v in ["PT90S", "pt90s", "PT90s", "pt90S"] {
        assert_eq!(ttl_ok(v), 90, "{v}");
    }
}

#[test]
fn iso_m_after_t_is_minutes_in_either_case() {
    assert_eq!(ttl_ok("PT8M"), 480);
    assert_eq!(ttl_ok("PT8m"), 480);
    assert_eq!(ttl_ok("pt8m"), 480);
    assert_eq!(ttl_ok("P0DT0H1M"), 60);
}

#[test]
fn iso_combinations() {
    assert_eq!(ttl_ok("PT1H30M"), 5_400);
    assert_eq!(ttl_ok("PT1H30M15S"), 5_415);
    assert_eq!(ttl_ok("PT1M30S"), 90);
    assert_eq!(ttl_ok("PT1H15S"), 3_615);
    assert_eq!(ttl_ok("PT1H0M15S"), 3_615);
    assert_eq!(ttl_ok("P1DT0H"), 86_400);
    assert_eq!(ttl_ok("P0DT1H"), 3_600);
    assert_eq!(ttl_ok("P0DT23H59M60S"), 86_400);
    // P0D is zero with a unit, so it is out_of_range.
    ttl_range("P0D");
}

#[test]
fn iso_refused_forms() {
    for v in [
        "P",
        "PT",
        "P1DT",
        "T1H",
        "PT1H1H",
        "PT1M1H",
        "PT1S1M",
        "PT1S1H",
        "P1H",
        "P1S",
        "PT1D",
        "P1DT1D",
        "P1D1D",
        "P1D2H",
        "P1D2M",
        "P1D2S",
        "P1W",
        "PT1W",
        "P1Y",
        "P1Y2M",
        "PT1.5H",
        "PT1,5H",
        "P1.5D",
        "PT1.5S",
        "-PT1H",
        "+PT1H",
        "PT-1H",
        "PT+1H",
        "P-1D",
        "P+1D",
        "PTH",
        "PTM",
        "PTS",
        "PD",
        "P1DTH",
        "PT1H30",
        "PT1",
        "P1",
        "PT1H T",
        "PT 1H",
        "PT8 H",
        "P1D T2H",
        "P 1D",
        "P1 D",
        "PT1H 30M",
        "PT1H\t30M",
        "PT1H\n30M",
        "PT\u{ff11}H",
        "PT\u{661}H",
        "PT1\u{ff28}",
        "PTT1H",
        "PP1D",
        "P1DTT1H",
        "PT1H30MX",
        "XPT1H",
        "PT1HM",
        "PT1H30M30",
        "1hPT1M",
        "PT1M1h",
        "R1/PT1H",
        "PT1H/",
        "PT1H,",
        "PT1H.",
        "PT1H;",
    ] {
        ttl_invalid(v);
    }
}

#[test]
fn iso_months_and_years_before_the_t_use_the_m_template() {
    // Gap FA-08: an ISO form whose M comes before any T means months, and takes the "uses 'M'"
    // template, since its trimmed value ends in a digit and an uppercase M.
    for v in ["P8M", "P1M", "P12M", "P0M"] {
        let m = ttl_invalid(v);
        assert_eq!(m, msg_m(&format!("'{v}'")));
    }
    let m = ttl_invalid(" P8M ");
    assert_eq!(m, msg_m("' P8M '"));
}

#[test]
fn iso_lowercase_months_before_the_t_are_refused_as_not_a_duration() {
    // The template's trigger is an uppercase M.
    for v in ["p8m", "P8m", "p1m"] {
        let m = ttl_invalid(v);
        assert_eq!(m, msg_not_duration(&format!("'{v}'")));
    }
}

#[test]
fn iso_weeks_and_years_are_not_a_duration() {
    for v in ["P1W", "P1Y", "PT1W", "P1Y2M3D", "P2W"] {
        ttl_invalid(v);
    }
    assert_eq!(ttl_invalid("P1W"), msg_not_duration("'P1W'"));
    assert_eq!(ttl_invalid("P1Y"), msg_not_duration("'P1Y'"));
    assert_eq!(ttl_invalid("PT"), msg_not_duration("'PT'"));
    assert_eq!(ttl_invalid("P"), msg_not_duration("'P'"));
}

// ---------------------------------------------------------------- refused as invalid_value

#[test]
fn a_bare_number_has_no_unit() {
    for v in [
        "8",
        "28800",
        "0",
        "00",
        "007",
        "1",
        "60",
        "86400",
        " 8 ",
        "\t28800\n",
    ] {
        let m = ttl_invalid(v);
        assert_eq!(m, msg_no_unit(&format!("'{}'", escape(v))), "{v:?}");
    }
}

#[test]
fn a_bare_number_too_large_to_compute_is_still_a_bare_number() {
    // Not out_of_range: its unit is unknown, so it is not well formed.
    for v in [
        "99999999999999999999".to_string(),
        "9".repeat(10_000),
        "18446744073709551616".to_string(),
        "4294967296".to_string(),
    ] {
        let m = ttl_invalid(&v);
        assert_eq!(m, msg_no_unit(&format!("'{v}'")));
    }
}

#[test]
fn a_digit_only_value_of_non_ascii_digits_is_not_a_bare_number() {
    for v in ["\u{661}", "\u{ff18}", "8\u{661}"] {
        let m = ttl_invalid(v);
        assert_eq!(m, msg_not_duration(&format!("'{v}'")));
    }
}

#[test]
fn m_templates_for_an_uppercase_m_after_a_digit() {
    for v in [
        "8M", "8 M", "8M ", " 8M", "1h30M", "8M30s", "8M 30s", "  8 M  ", "1M30s",
    ] {
        let m = ttl_invalid(v);
        assert_eq!(m, msg_m(&format!("'{}'", escape(v))), "{v:?}");
    }
}

#[test]
fn m_followed_by_a_letter_is_not_the_m_template() {
    // `8Mo` and `8 Months` are months, in a form that is not a duration at all.
    for v in ["8Mo", "8 Mo", "8Month", "8 Months", "8Mx", "8Mn"] {
        let m = ttl_invalid(v);
        assert_eq!(m, msg_not_duration(&format!("'{v}'")), "{v:?}");
    }
}

#[test]
fn m_in_minutes_spellings_is_a_duration() {
    // `8Min` and the like are any-case minutes and not the ambiguous `M`.
    assert_eq!(ttl_ok("8Min"), 480);
    assert_eq!(ttl_ok("8MIN"), 480);
    assert_eq!(ttl_ok("8 MINUTES"), 480);
    assert_eq!(ttl_ok("8Mins"), 480);
}

#[test]
fn weeks_months_and_years_are_refused() {
    for v in [
        "1w", "1W", "1 w", "1 week", "1 weeks", "1weeks", "1 Week", "1wk", "2wks", "1mo", "1 mo",
        "1 month", "1 months", "1 Months", "1y", "1Y", "1 yr", "1 year", "1 years", "2yrs",
    ] {
        let m = ttl_invalid(v);
        assert_eq!(m, msg_not_duration(&format!("'{v}'")), "{v:?}");
    }
}

#[test]
fn a_fraction_is_refused() {
    for v in [
        "1.5h",
        "1.5 h",
        "0.5h",
        ".5h",
        "1.h",
        "1,5h",
        "1.5",
        "0.5",
        "1.0h",
        "1h30.5m",
        "1h 0.5m",
        "1/2h",
        "1\u{bd}h",
        "1.5\u{ff48}",
    ] {
        ttl_invalid(v);
    }
    assert_eq!(ttl_invalid("1.5h"), msg_not_duration("'1.5h'"));
}

#[test]
fn a_sign_is_refused() {
    for v in [
        "-5m",
        "+5m",
        "- 5m",
        "+ 5m",
        "-0m",
        "+0m",
        "5m-",
        "5m+",
        "1h-30m",
        "1h+30m",
        "-1h30m",
        "\u{2212}5m",
        "\u{ff0b}5m",
        "\u{ff0d}5m",
        "--5m",
        "-",
        "+",
    ] {
        ttl_invalid(v);
    }
    assert_eq!(ttl_invalid("-5m"), msg_not_duration("'-5m'"));
}

#[test]
fn a_unit_given_twice_is_refused() {
    for v in [
        "1h1h",
        "1h 1h",
        "1m1m",
        "1s1s",
        "1d1d",
        "1h30m1h",
        "1d1h1d",
        "1h1m1m",
        "1m1s1s",
        "1hour1hr",
        "2 hours 3 hrs",
        "1 min 2 mins",
        "1 sec 1 s",
        "1 day 1 d",
    ] {
        ttl_invalid(v);
    }
    assert_eq!(ttl_invalid("1h1h"), msg_not_duration("'1h1h'"));
}

#[test]
fn units_out_of_order_are_refused() {
    for v in [
        "30m1h",
        "1m1d",
        "1s1m",
        "1s1h",
        "1s1d",
        "1h1d",
        "1m1h",
        "30s1m",
        "1s 1m 1h 1d",
        "1m 1h",
        "1h1d",
        "1d1s1m",
        "1d1m1h",
        "1d1h1s1m",
    ] {
        ttl_invalid(v);
    }
    assert_eq!(ttl_invalid("30m1h"), msg_not_duration("'30m1h'"));
}

#[test]
fn units_out_of_order_are_refused_even_when_the_sum_is_out_of_range() {
    // Not well formed, so never out_of_range.
    for v in [
        "99999999999999999999s1m",
        "9999999d9999999999999999999999d",
        "1s99999999h",
    ] {
        ttl_invalid(v);
    }
}

#[test]
fn empty_and_whitespace_only_are_refused() {
    assert_eq!(ttl_invalid(""), msg_not_duration("''"));
    assert_eq!(ttl_invalid(" "), msg_not_duration("' '"));
    assert_eq!(ttl_invalid("\t"), msg_not_duration("'\\u0009'"));
    assert_eq!(
        ttl_invalid(" \n\t "),
        msg_not_duration("' \\u000a\\u0009 '")
    );
    assert_eq!(ttl_invalid("\u{3000}"), msg_not_duration("'\u{3000}'"));
    assert_eq!(
        ttl_invalid("\u{a0}\u{2003}"),
        msg_not_duration("'\u{a0}\u{2003}'")
    );
}

#[test]
fn other_garbage_is_not_a_duration() {
    for v in [
        "forever",
        "soon",
        "h",
        "m",
        "s",
        "d",
        "min",
        "hours",
        "1:30",
        "1:30:00",
        "01:30",
        "1h:30m",
        "1h,30m",
        "1h and 30m",
        "1h+30m",
        "1h&30m",
        "1 2h",
        "1 2 h",
        "1_0s",
        "1e3s",
        "0x10s",
        "8 h m",
        "8hm",
        "h8",
        "m8h",
        "8h8",
        "8 8h",
        "8h 8",
        "(8h)",
        "\"8h\"",
        "'8h'",
        "8h;",
        "8h.",
        "\u{ff11}h",
        "1\u{ff48}",
        "\u{661}h",
        "8\u{e9}",
        "\u{65e5}",
        "8h\u{65e5}",
        "1\u{3000}\u{3000}h\u{200b}",
        "NaN",
        "inf",
        "1e9",
        "8hh",
        "88hh",
    ] {
        ttl_invalid(v);
    }
    assert_eq!(ttl_invalid("forever"), msg_not_duration("'forever'"));
}

#[test]
fn conformance_cases_of_test_values() {
    // From conformance/lease-cli/test-values.ps1: invalid_value for ttl.
    for v in [
        "8", "28800", "0", "8M", "8 M", "1.5h", "1w", "P1M", "-5m", "1h1h", "30m1h", "PT", " ",
        "1:30",
    ] {
        ttl_invalid(v);
    }
    // And out_of_range for ttl.
    for v in [
        "48h",
        "24h1s",
        "1441m",
        "59s",
        "0m",
        "PT0S",
        "P2D",
        "99999999999999999999h",
    ] {
        ttl_range(v);
    }
}

// ---------------------------------------------------------------- too large to compute

#[test]
fn a_number_too_large_to_compute_is_out_of_range_and_never_a_panic() {
    // lease-api.md: `99999999999999999999h` is well formed and out_of_range, and MUST NOT be internal.
    for v in [
        "99999999999999999999h".to_string(),
        "99999999999999999999H".to_string(),
        "99999999999999999999 hours".to_string(),
        "99999999999999999999d".to_string(),
        "99999999999999999999m".to_string(),
        "99999999999999999999s".to_string(),
        "18446744073709551616s".to_string(),
        "18446744073709551615s".to_string(),
        "4294967296s".to_string(),
        "4294967295s".to_string(),
        "4294967296m".to_string(),
        "2147483648h".to_string(),
        "9223372036854775808s".to_string(),
        "9223372036854775807h".to_string(),
        "99999999999999999999d99999999999999999999h".to_string(),
        "99999999999999999999d99999999999999999999h99999999999999999999m99999999999999999999s"
            .to_string(),
        "P99999999999999999999D".to_string(),
        "PT99999999999999999999H".to_string(),
        "PT99999999999999999999M".to_string(),
        "PT99999999999999999999S".to_string(),
        "P99999999999999999999DT99999999999999999999H99999999999999999999M99999999999999999999S"
            .to_string(),
        format!("{}h", "9".repeat(10_000)),
        format!("{}d", "9".repeat(10_000)),
        format!("PT{}S", "9".repeat(10_000)),
        format!("1d{}h", "9".repeat(1_000)),
    ] {
        let m = ttl_range(&v);
        assert_eq!(m, msg_range(&format!("'{v}'")));
    }
}

#[test]
fn a_huge_number_that_is_not_well_formed_is_invalid_value_not_out_of_range() {
    // A unit twice is not well formed, however big the numbers are.
    for v in [
        "99999999999999999999h99999999999999999999h",
        "99999999999999999999h 1h",
        "99999999999999999999m99999999999999999999h",
        "99999999999999999999",
        "99999999999999999999.5h",
        "-99999999999999999999h",
        "99999999999999999999w",
        "PT99999999999999999999H99999999999999999999H",
        "P99999999999999999999M",
    ] {
        ttl_invalid(v);
    }
}

#[test]
fn a_number_that_overflows_a_u32_of_seconds_is_never_wrapped_into_range() {
    // 4294967356 seconds wraps to 60 in 32 bits, and 4294967296 + 3600 wraps to 3600.
    for v in [
        "4294967356s",
        "4294963696s",
        "4294967296s",
        "71582789m",
        "71582789m 28s",
    ] {
        ttl_range(v);
    }
    // 2^32 / 3600 = 1193046.47: a count of hours that wraps on a multiply by 3600.
    for v in [
        "1193047h",
        "1193046h",
        "1193046h 28m 16s",
        "1193047h 4s",
        "49710d",
        "49711d",
    ] {
        ttl_range(v);
    }
    // 2^64 / 86400 days is about 2.1e14: a count of days that wraps a u64 on a multiply.
    for v in ["213503982334601d", "213503982334602d", "213503982334603d"] {
        ttl_range(v);
    }
}

// ---------------------------------------------------------------- messages

#[test]
fn the_out_of_range_message_quotes_the_value_untrimmed() {
    assert_eq!(
        ttl_range(" 59s "),
        "TTL ' 59s ' is outside the allowed range. Give from 1m to 24h; it is never clamped."
    );
    assert_eq!(
        ttl_range("48h"),
        "TTL '48h' is outside the allowed range. Give from 1m to 24h; it is never clamped."
    );
    assert_eq!(
        ttl_range("0m"),
        "TTL '0m' is outside the allowed range. Give from 1m to 24h; it is never clamped."
    );
}

#[test]
fn the_out_of_range_message_escapes_control_characters_of_the_value() {
    assert_eq!(ttl_range("48h\n"), msg_range("'48h\\u000a'"));
    assert_eq!(ttl_range("\t59s"), msg_range("'\\u000959s'"));
    assert_eq!(ttl_range("48\u{b}h"), msg_range("'48\\u000bh'"));
}

#[test]
fn the_no_unit_message() {
    assert_eq!(
        ttl_invalid("8"),
        "TTL '8' has no unit. Give one, as in '8h'."
    );
    assert_eq!(
        ttl_invalid(" 28800 "),
        "TTL ' 28800 ' has no unit. Give one, as in '8h'."
    );
}

#[test]
fn the_m_message() {
    assert_eq!(
        ttl_invalid("8M"),
        "TTL '8M' uses 'M', which could mean months or minutes. Write minutes as 'm' or 'min'."
    );
    assert_eq!(
        ttl_invalid("8 M"),
        "TTL '8 M' uses 'M', which could mean months or minutes. Write minutes as 'm' or 'min'."
    );
}

#[test]
fn the_not_a_duration_message() {
    assert_eq!(
        ttl_invalid("1.5h"),
        "TTL '1.5h' is not a duration. Give one as in '8h', '90m', '1h30m', '2 hours' or 'PT8H'."
    );
    assert_eq!(
        ttl_invalid("1:30"),
        "TTL '1:30' is not a duration. Give one as in '8h', '90m', '1h30m', '2 hours' or 'PT8H'."
    );
}

#[test]
fn a_message_for_a_ttl_with_a_control_character_has_none() {
    for c in (0u32..0x20).chain([0x7f]).filter_map(char::from_u32) {
        for v in [
            format!("8{c}"),
            format!("{c}8h{c}x"),
            format!("1.5{c}h"),
            format!("8h{c}8h"),
        ] {
            let m = ttl_invalid(&v);
            assert!(!m.chars().any(|ch| ch.is_ascii_control()), "{v:?}: {m:?}");
        }
        let v = format!("4.8{c}h");
        let m = ttl_invalid(&v);
        assert!(m.contains(&format!("\\u{:04x}", c as u32)), "{v:?}: {m:?}");
    }
}

#[test]
fn fields_are_ttl() {
    // `field` is `ttl` in each of the three refusals.
    let e = refused(ttl("8"), "8");
    assert_eq!(e.field, "ttl");
    let e = refused(ttl("48h"), "48h");
    assert_eq!(e.field, "ttl");
}

/// The value as a message quotes it: each control character as `\u` and four lowercase hex digits.
fn escape(v: &str) -> String {
    let mut out = String::new();
    for c in v.chars() {
        if c.is_ascii_control() {
            out.push_str(&format!("\\u{:04x}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}
