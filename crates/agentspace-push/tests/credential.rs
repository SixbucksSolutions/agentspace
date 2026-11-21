//! What the laptop reads, second: the Bot credential, `bot.json`, down to the `token_dpapi_hex`
//! member's hex, the decryption behind `Unprotect`, and the token's own checks (operator.md, The
//! Bot credential; localswim's windows-operations.md, "The credential files").
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Each refused document differs from the good one by exactly one fault, so a test fails when the
//! rule it names is missing. Every refusal is checked twice: the pinned `parse_credential_blob`
//! gives none, and the push takes its one "Could not read" path with nothing sent or recorded.

mod common;
use common::*;

fn accepted(env: &Env, mut s: Scenario, bytes: Vec<u8>) -> Run {
    s.credential = Slot::Bytes(bytes);
    let r = env.run(&s);
    assert_eq!(r.ssh_calls.len(), 1, "should be accepted: {r:?}");
    assert_eq!(r.ssh_calls[0].stdin, CONTENT.as_bytes());
    r
}

fn refused_scenario(env: &Env, mut s: Scenario) {
    for mode in MODES {
        s.mode = mode;
        let r = env.run(&s);
        assert_eq!(
            r.printed,
            vec![W_COULD_NOT_READ.to_string()],
            "{mode:?} {r:?}"
        );
        assert!(!r.pushed(), "nothing is sent");
        assert!(r.stamp_after.is_none(), "nothing is recorded");
    }
}

/// Refuses every document of `cases`, naming the case that fails.
fn refused_all(env: &Env, cases: Vec<(String, Vec<u8>)>) {
    for (name, bytes) in cases {
        assert!(
            parse_blob(&bytes).is_none(),
            "the parse should refuse: {name}"
        );
        let mut s = env.scenario();
        s.credential = Slot::Bytes(bytes);
        for mode in MODES {
            s.mode = mode;
            let r = env.run(&s);
            assert_eq!(
                r.printed,
                vec![W_COULD_NOT_READ.to_string()],
                "{name} {mode:?}"
            );
            assert!(!r.pushed(), "{name}: nothing is sent");
            assert!(r.stamp_after.is_none(), "{name}: nothing is recorded");
        }
    }
}

fn hex() -> String {
    hex_lower(&blob())
}

fn other_hex() -> String {
    hex_lower(&example_blob())
}

/// The good document with one member's value text replaced.
fn with(name: &str, value: &str) -> Vec<u8> {
    enc_utf8(&object(&members_with(&hex(), name, value)))
}

/// The good document without the member `name`.
fn without(name: &str) -> Vec<u8> {
    let kept: Vec<_> = members(&hex())
        .into_iter()
        .filter(|(n, _)| n != name)
        .collect();
    enc_utf8(&object(&kept))
}

/// The good document with the member `name` written twice, with these value texts, in this order.
fn twice(name: &str, first: &str, second: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for (n, v) in members(&hex()) {
        if n == name {
            out.push((n.clone(), first.to_string()));
            out.push((n, second.to_string()));
        } else {
            out.push((n, v));
        }
    }
    enc_utf8(&object(&out))
}

const NAMES: [&str; 4] = ["format", "format_version", "role", "token_dpapi_hex"];

// ---- Where it is ------------------------------------------------------------------------------

#[test]
fn the_good_credential_gives_the_token_and_the_blob_goes_to_unprotect_unchanged() {
    let env = Env::new();
    let r = env.run(&env.scenario());
    assert_eq!(r.unprotect_calls, vec![blob()]);
    assert_eq!(r.ssh_calls[0].stdin, CONTENT.as_bytes());
}

#[test]
fn a_missing_bot_json_or_a_folder_in_its_place_is_refused() {
    let env = Env::new();
    for slot in [Slot::Absent, Slot::Folder] {
        let mut s = env.scenario();
        s.credential = slot;
        refused_scenario(&env, s);
    }
}

#[test]
fn only_bot_json_is_read_and_a_good_credential_under_another_name_is_no_credential() {
    // Localswim's folder holds human.json beside bot.json, and a reader MUST ignore any other
    // file.
    let env = Env::new();
    for other in [
        "human.json",
        "bot.txt",
        "bot.json.bak",
        "Bot.json.txt",
        "bot",
    ] {
        let mut s = env.scenario();
        s.credential = Slot::Absent;
        s.other_files = vec![(other.to_string(), good_credential())];
        refused_scenario(&env, s);
    }
}

#[test]
fn another_file_in_the_folder_does_not_change_what_bot_json_gives() {
    let env = Env::new();
    let mut s = env.scenario();
    s.other_files = vec![
        (
            "human.json".to_string(),
            enc_utf8(&object(&members_with(&other_hex(), "role", &js("human")))),
        ),
        ("bot.json.bak".to_string(), b"not a credential".to_vec()),
    ];
    let r = env.run(&s);
    assert_eq!(r.unprotect_calls, vec![blob()]);
    assert_eq!(r.ssh_calls[0].stdin, CONTENT.as_bytes());
}

#[test]
fn the_token_is_read_even_when_the_push_is_not_due() {
    // The stamp covers the token, so it is read before due-ness is known (operator.md).
    let env = Env::new();
    let mut s = env.scenario();
    s.stored_stamp = Slot::Bytes(stamp_file_bytes(STAMP_LA));
    let r = env.run(&s);
    assert!(!r.pushed());
    assert_eq!(r.unprotect_calls.len(), 1);
    assert_eq!(r.printed, Vec::<String>::new());

    // And a credential that cannot be read warns at every up, even when nothing would be sent.
    s.credential = Slot::Absent;
    let r = env.run(&s);
    assert_eq!(r.printed, vec![W_COULD_NOT_READ.to_string()]);
}

// ---- A valid bot.json is read -----------------------------------------------------------------

#[test]
fn a_valid_bot_json_gives_the_blob_through_the_pinned_parse() {
    assert_eq!(parse_blob(&good_credential()), Some(blob()));
}

#[test]
fn hex_digits_become_the_blob_with_the_first_digit_as_the_high_four_bits() {
    let env = Env::new();
    let mut s = env.scenario();
    s.credential = Slot::Bytes(enc_utf8(&credential_json(&hex())));
    let r = env.run(&s);
    assert_eq!(r.unprotect_calls, vec![blob()]);
    // The payload's first byte is 0x12: a reader that took the nibbles the other way round would
    // hand 0x21 on.
    assert_eq!(r.unprotect_calls[0][44], 0x12);
}

#[test]
fn members_in_any_order_are_accepted() {
    let env = Env::new();
    let all = members(&hex());
    let mut reversed = all.clone();
    reversed.reverse();
    let mut rotated = all.clone();
    rotated.rotate_left(1);
    let mut swapped = all;
    swapped.swap(0, 3);
    for m in [reversed, rotated, swapped] {
        let bytes = enc_utf8(&object(&m));
        assert_eq!(parse_blob(&bytes), Some(blob()));
        accepted(&env, env.scenario(), bytes);
    }
}

#[test]
fn whitespace_line_ends_and_indentation_mean_nothing() {
    // PowerShell 7 and Windows PowerShell 5.1 write the same members with different indentation
    // (windows-operations.md).
    let env = Env::new();
    let h = hex();
    let ps7 = format!(
        "{{\n  \"format\": \"localswim-credential\",\n  \"format_version\": 1,\n  \"role\": \"bot\",\n  \"token_dpapi_hex\": \"{h}\"\n}}"
    );
    let ps51 = format!(
        "{{\r\n    \"format\":  \"localswim-credential\",\r\n    \"format_version\":  1,\r\n    \"role\":  \"bot\",\r\n    \"token_dpapi_hex\":  \"{h}\"\r\n}}"
    );
    for text in [
        ps7.clone(),
        ps51.clone(),
        format!("{ps7}\n"),
        format!("\r\n \t{ps51}\r\n\r\n"),
        format!(
            "{{ \"format\" : \"localswim-credential\" , \"format_version\" : 1 , \"role\" : \"bot\" , \"token_dpapi_hex\" : \"{h}\" }}"
        ),
    ] {
        let bytes = enc_utf8(&text);
        assert_eq!(parse_blob(&bytes), Some(blob()));
        accepted(&env, env.scenario(), bytes);
    }
}

#[test]
fn a_utf8_byte_order_mark_is_accepted() {
    // `bot.json` is written with no mark, and a reader SHOULD also accept one.
    let env = Env::new();
    let text = credential_json(&hex());
    for bytes in [
        enc_utf8_bom(&text),
        enc_utf8_bom(&format!("\r\n  {text}\r\n")),
    ] {
        assert_eq!(parse_blob(&bytes), Some(blob()));
        accepted(&env, env.scenario(), bytes);
    }
}

#[test]
fn a_mark_that_is_not_the_one_utf8_mark_is_refused() {
    let env = Env::new();
    let text = credential_json(&hex());
    let mut twice_marked = vec![0xEF, 0xBB, 0xBF];
    twice_marked.extend(enc_utf8_bom(&text));
    refused_all(
        &env,
        vec![
            ("two UTF-8 marks".into(), twice_marked),
            ("a UTF-16LE mark".into(), enc_utf16le_bom(&text)),
            ("a UTF-16BE mark".into(), enc_utf16be_bom(&text)),
            ("a UTF-16LE file with no mark".into(), utf16le(&text)),
            ("a mark and nothing else".into(), vec![0xEF, 0xBB, 0xBF]),
            (
                "a UTF-8 mark inside the text".into(),
                enc_utf8(&format!("{{\u{feff}{}", &text[1..])),
            ),
        ],
    );
}

// ---- Exactly the four members -----------------------------------------------------------------

#[test]
fn each_of_the_four_members_missing_is_refused() {
    let env = Env::new();
    let cases = NAMES
        .iter()
        .map(|n| (format!("without {n}"), without(n)))
        .collect();
    refused_all(&env, cases);
}

#[test]
fn an_object_with_no_members_is_refused() {
    let env = Env::new();
    refused_all(
        &env,
        vec![
            ("empty object".into(), enc_utf8("{}")),
            ("empty object, spaced".into(), enc_utf8(" { } ")),
        ],
    );
}

#[test]
fn an_extra_member_is_refused() {
    let env = Env::new();
    let h = hex();
    let mut cases = Vec::new();
    for (name, value) in [
        ("extra", "1".to_string()),
        ("extra", js("x")),
        ("extra", "null".to_string()),
        ("extra", "{}".to_string()),
        ("extra", "[1,2]".to_string()),
        ("", "1".to_string()),
        ("token", js(&h)),
        ("user", js("bot")),
        ("format_version2", "1".to_string()),
        ("role ", js("bot")),
        (" role", js("bot")),
    ] {
        let mut m = members(&h);
        m.push((name.to_string(), value.clone()));
        cases.push((format!("extra member {name:?}"), enc_utf8(&object(&m))));
        // An extra member first, too, so a reader that stops at four members does not hide it.
        let mut m = members(&h);
        m.insert(0, (name.to_string(), value));
        cases.push((
            format!("extra first member {name:?}"),
            enc_utf8(&object(&m)),
        ));
    }
    refused_all(&env, cases);
}

#[test]
fn a_member_name_that_differs_only_in_ascii_case_is_refused() {
    let env = Env::new();
    let h = hex();
    let mut cases = Vec::new();
    for name in NAMES {
        let variants = [
            name.to_ascii_uppercase(),
            format!("{}{}", name[..1].to_ascii_uppercase(), &name[1..]),
        ];
        for variant in variants {
            // Alongside the real member: one name differs from another only in ASCII case.
            let mut m = members(&h);
            m.push((variant.clone(), good_value(&h, name)));
            cases.push((format!("{variant} beside {name}"), enc_utf8(&object(&m))));
            // In its place: the name is not the member's name.
            let renamed: Vec<(String, String)> = members(&h)
                .into_iter()
                .map(|(n, v)| {
                    if n == name {
                        (variant.clone(), v)
                    } else {
                        (n, v)
                    }
                })
                .collect();
            cases.push((
                format!("{variant} in place of {name}"),
                enc_utf8(&object(&renamed)),
            ));
        }
    }
    // The exact example from the card.
    assert!(cases.iter().any(|(n, _)| n == "Role beside role"));
    refused_all(&env, cases);
}

// ---- A duplicate member -----------------------------------------------------------------------

#[test]
fn a_duplicate_member_with_the_same_value_is_refused() {
    let env = Env::new();
    let h = hex();
    let good = |n: &str| good_value(&h, n);
    let cases = NAMES
        .iter()
        .map(|n| (format!("{n} twice"), twice(n, &good(n), &good(n))))
        .collect();
    refused_all(&env, cases);
}

#[test]
fn a_duplicate_member_with_different_values_is_refused_whichever_comes_last() {
    // A parser that keeps the last value (serde_json alone does) accepts the second order of each
    // pair, whose last value is the right one.
    let env = Env::new();
    let h = hex();
    let good = |n: &str| good_value(&h, n);
    let other = |n: &str| match n {
        "format" => js("other-format"),
        "format_version" => "2".to_string(),
        "role" => js("human"),
        _ => js(&other_hex()),
    };
    let mut cases = Vec::new();
    for n in NAMES {
        cases.push((
            format!("{n}: good, then another"),
            twice(n, &good(n), &other(n)),
        ));
        cases.push((
            format!("{n}: another, then good"),
            twice(n, &other(n), &good(n)),
        ));
    }
    refused_all(&env, cases);
}

#[test]
fn a_duplicate_through_an_escape_is_refused() {
    // `\u006f` is `o`: the two names are the same name.
    let env = Env::new();
    let h = hex();
    let escaped_last = format!(
        "{{\"format\":\"localswim-credential\",\"format_version\":1,\"role\":\"human\",\"token_dpapi_hex\":\"{h}\",\"r\\u006fle\":\"bot\"}}"
    );
    let escaped_first = format!(
        "{{\"r\\u006fle\":\"bot\",\"format\":\"localswim-credential\",\"format_version\":1,\"role\":\"bot\",\"token_dpapi_hex\":\"{h}\"}}"
    );
    refused_all(
        &env,
        vec![
            ("role, then r\\u006fle".into(), enc_utf8(&escaped_last)),
            ("r\\u006fle, then role".into(), enc_utf8(&escaped_first)),
        ],
    );
}

// ---- format, format_version and role ----------------------------------------------------------

#[test]
fn a_wrong_format_is_refused_and_it_is_compared_exactly() {
    let env = Env::new();
    let cases = [
        js("Localswim-Credential"),
        js("LOCALSWIM-CREDENTIAL"),
        js("localswim-credential "),
        js(" localswim-credential"),
        js("localswim-credential\n"),
        js("localswim-credentials"),
        js("localswim"),
        js("localswim_credential"),
        js("credential"),
        js(""),
        js("human"),
        "1".to_string(),
        "null".to_string(),
        "true".to_string(),
        "[\"localswim-credential\"]".to_string(),
        "{}".to_string(),
    ]
    .into_iter()
    .map(|v| (format!("format {v}"), with("format", &v)))
    .collect();
    refused_all(&env, cases);
}

#[test]
fn a_format_version_other_than_the_integer_1_is_refused() {
    // 1.0 and the exponent forms are the same number and not the integer 1 as written (localswim:
    // "MUST be written as the integer 1, with no fraction or exponent"); a string, and any other
    // value, are refused too.
    let env = Env::new();
    let cases = [
        "1.0",
        "1.00",
        "1.0e0",
        "1e0",
        "1E0",
        "1e+0",
        "10e-1",
        "0.1e1",
        "1.5",
        "2",
        "3",
        "0",
        "-1",
        "-1.0",
        "10",
        "11",
        "18446744073709551616",
        "\"1\"",
        "\"1.0\"",
        "\" 1\"",
        "\"\"",
        "\"one\"",
        "true",
        "false",
        "null",
        "[1]",
        "{\"v\":1}",
    ]
    .into_iter()
    .map(|v| (format!("format_version {v}"), with("format_version", v)))
    .collect();
    refused_all(&env, cases);
}

#[test]
fn a_wrong_role_is_refused() {
    let env = Env::new();
    let cases = [
        js("human"),
        js("Bot"),
        js("BOT"),
        js("bot "),
        js(" bot"),
        js("bot\n"),
        js("bots"),
        js("bo"),
        js("admin"),
        js(""),
        "null".to_string(),
        "1".to_string(),
        "true".to_string(),
        "[\"bot\"]".to_string(),
        "{}".to_string(),
    ]
    .into_iter()
    .map(|v| (format!("role {v}"), with("role", &v)))
    .collect();
    refused_all(&env, cases);
}

// ---- token_dpapi_hex --------------------------------------------------------------------------

#[test]
fn hex_that_is_empty_odd_or_not_only_hex_is_refused() {
    let env = Env::new();
    let h = hex();
    let bad = [
        String::new(),
        "a".to_string(),
        format!("{h}0"),
        h[..h.len() - 1].to_string(),
        format!(" {h}"),
        format!("{h} "),
        format!("{h}\n"),
        format!("\n  {h}\n"),
        format!("{} {}", &h[..10], &h[10..]),
        format!("{}\n{}", &h[..10], &h[10..]),
        format!("{}:{}", &h[..10], &h[10..]),
        format!("{}-{}", &h[..10], &h[10..]),
        format!("0x{h}"),
        format!("{}g{}", &h[..10], &h[12..]),
        format!("{}\u{e9}{}", &h[..10], &h[12..]),
        format!("{}\0{}", &h[..10], &h[12..]),
        "zz".to_string(),
        "not hex".to_string(),
        TOKEN.to_string(),
    ];
    let mut cases: Vec<(String, Vec<u8>)> = bad
        .iter()
        .map(|b| (format!("hex {b:?}"), with("token_dpapi_hex", &js(b))))
        .collect();
    // Not a string at all.
    for v in ["12", "null", "true", "[]", "{}", "[\"00\"]", "1.5"] {
        cases.push((format!("hex as {v}"), with("token_dpapi_hex", v)));
    }
    refused_all(&env, cases);
}

#[test]
fn hex_that_is_even_and_only_hex_reaches_the_decryption_whatever_the_blob_holds() {
    // The reader does not judge the blob's head: the decryption does. Localswim's example head
    // with zeros is a blob that decrypts to nothing, and a file that holds it still parses.
    let env = Env::new();
    let example = hex_lower(&example_blob());
    let bytes = enc_utf8(&credential_json(&example));
    assert_eq!(parse_blob(&bytes), Some(example_blob()));
    let mut s = env.scenario();
    s.credential = Slot::Bytes(bytes);
    let r = env.run(&s);
    assert_eq!(r.unprotect_calls, vec![example_blob()]);
}

#[test]
fn a_failed_decryption_is_refused() {
    let env = Env::new();
    let mut s = env.scenario();
    s.unprotect = UnprotectBehavior::Fails("The data is invalid".to_string());
    refused_scenario(&env, s);
}

// ---- A file that is not one JSON object -------------------------------------------------------

#[test]
fn a_file_that_is_not_one_json_object_is_refused() {
    let env = Env::new();
    let good = credential_json(&hex());
    let no_close = good[..good.len() - 1].to_string();
    let trailing_comma = format!("{},}}", &good[..good.len() - 1]);
    let cases: Vec<(&str, String)> = vec![
        ("empty", String::new()),
        ("blanks", "   \r\n\t".to_string()),
        ("not json", "not json".to_string()),
        ("an empty array", "[]".to_string()),
        ("an array holding the object", format!("[{good}]")),
        ("two objects in an array", format!("[{good},{good}]")),
        ("a string", "\"localswim-credential\"".to_string()),
        ("a string holding the document", js(&good)),
        ("a number", "1".to_string()),
        ("null", "null".to_string()),
        ("true", "true".to_string()),
        ("two objects", format!("{good}{good}")),
        ("two objects, spaced", format!("{good}\n{good}")),
        ("trailing garbage", format!("{good}x")),
        ("trailing garbage after blanks", format!("{good}\n garbage")),
        ("a trailing comma after the object", format!("{good},")),
        ("a second closing brace", format!("{good}}}")),
        ("a trailing number", format!("{good} 1")),
        ("a trailing NUL", format!("{good}\0")),
        ("leading garbage", format!("x{good}")),
        ("a leading comma", format!(",{good}")),
        ("a comment before", format!("/* a */{good}")),
        ("a comment after", format!("{good} // a")),
        ("no closing brace", no_close),
        ("a trailing comma in the object", trailing_comma),
        ("single quotes", good.replace('"', "'")),
        ("unquoted names", good.replace("\"format\"", "format")),
        (
            "an unterminated string",
            format!("{{\"format\":\"{}", "localswim-credential"),
        ),
        ("the member list cut at a colon", "{\"format\":".to_string()),
    ];
    let cases = cases
        .into_iter()
        .map(|(n, t)| (n.to_string(), enc_utf8(&t)))
        .collect();
    refused_all(&env, cases);
}

#[test]
fn a_file_that_is_not_valid_utf8_is_refused() {
    let env = Env::new();
    let good = credential_json(&hex());
    let mut bad_tail = enc_utf8(&good);
    bad_tail.extend([0xC3, 0x28]);
    // Between two members, where a reader that decodes loosely still finds the four.
    let mut bad_inside = enc_utf8(&good);
    let at = bad_inside
        .windows(7)
        .position(|w| w == b",\"role\"")
        .unwrap()
        + 1;
    bad_inside.insert(at, 0xFF);
    let mut overlong = enc_utf8(&good);
    overlong.extend([0xC0, 0xAF]);
    let mut surrogate = enc_utf8(&good);
    surrogate.extend([0xED, 0xA0, 0x80]);
    refused_all(
        &env,
        vec![
            ("a bad tail".into(), bad_tail),
            ("a stray 0xFF between members".into(), bad_inside),
            ("an overlong sequence".into(), overlong),
            ("an encoded surrogate".into(), surrogate),
            ("a lone 0xFF".into(), vec![0xFF]),
            ("0xFF 0xFE 0x41".into(), vec![0xFF, 0xFE, 0x41]),
            ("0xC3 0x28 0xA0".into(), vec![0xC3, 0x28, 0xA0]),
            ("all byte values".into(), (0..=255u8).collect()),
        ],
    );
}

// ---- A refusal never prints a value ------------------------------------------------------------

#[test]
fn a_refusal_prints_no_value_from_the_file_and_names_no_member() {
    let env = Env::new();
    let docs: Vec<(&str, String)> = vec![
        (
            "format",
            object(&members_with(&hex(), "format", &js("QUOTE-ME-FORMAT"))),
        ),
        (
            "format_version",
            object(&members_with(
                &hex(),
                "format_version",
                &js("QUOTE-ME-VERSION"),
            )),
        ),
        (
            "role",
            object(&members_with(&hex(), "role", &js("QUOTE-ME-ROLE"))),
        ),
        (
            "hex",
            object(&members_with(
                &hex(),
                "token_dpapi_hex",
                &js("QUOTE-ME-HEX"),
            )),
        ),
        ("not json", "QUOTE-ME-GARBAGE {".to_string()),
        ("extra member", {
            let mut m = members(&hex());
            m.push(("QUOTE-ME-NAME".to_string(), js("QUOTE-ME-VALUE")));
            object(&m)
        }),
        ("duplicate", {
            let mut m = members(&hex());
            m.push(("role".to_string(), js("QUOTE-ME-DUPLICATE")));
            object(&m)
        }),
    ];
    for (name, text) in docs {
        let mut s = env.scenario();
        s.credential = Slot::Bytes(enc_utf8(&text));
        for mode in MODES {
            s.mode = mode;
            let r = env.run(&s);
            assert_eq!(r.printed, vec![W_COULD_NOT_READ.to_string()], "{name}");
            let all = format!("{:?} {:?} {}", r.printed, r.events, r.debug);
            assert!(!all.contains("QUOTE-ME"), "{name}: a value is quoted");
            for word in [
                "bot.json",
                "token_dpapi_hex",
                "format_version",
                "localswim-credential",
                "role",
            ] {
                assert!(
                    !all.contains(word),
                    "{name}: the warning or the outcome names {word}"
                );
            }
        }
    }
}

// ---- The plaintext ----------------------------------------------------------------------------

fn with_plaintext(env: &Env, plain: Vec<u8>) -> Scenario {
    let mut s = env.scenario();
    s.unprotect = UnprotectBehavior::Returns(plain);
    s
}

#[test]
fn the_plaintext_is_utf16le_with_no_mark_and_the_content_carries_the_ascii_token() {
    let env = Env::new();
    let all_classes = "AZaz09-_AZaz09-_AZaz09-_AZaz09-_AZaz09-_AZa".to_string();
    assert_eq!(all_classes.len(), 43);
    for token in [TOKEN, TOKEN_B, all_classes.as_str()] {
        let mut s = with_plaintext(&env, utf16le(token));
        s.ssh = SshBehavior::Answer {
            code: 1,
            stdout: vec![],
        };
        let r = env.run(&s);
        assert_eq!(r.ssh_calls[0].stdin, content(BASE, token), "{token:?}");
    }
}

#[test]
fn a_utf16le_byte_order_mark_in_the_plaintext_is_refused() {
    // The plaintext has no byte order mark: one makes the token 44 code units and not base64url.
    let env = Env::new();
    let mut p = vec![0xFF, 0xFE];
    p.extend(utf16le(TOKEN));
    refused_scenario(&env, with_plaintext(&env, p));
    let mut p = vec![0xFE, 0xFF];
    p.extend(utf16le(TOKEN));
    refused_scenario(&env, with_plaintext(&env, p));
}

#[test]
fn a_token_that_is_not_exactly_43_characters_of_base64url_is_refused() {
    // Architect's ruling (2026-11-20, for v0.07-T02): after the plaintext's own checks, the token
    // MUST be exactly 43 characters of A-Z, a-z, 0-9, '-' and '_'. Each of these differs from a
    // good token by one fault, and the good one is read.
    let env = Env::new();
    assert_eq!(TOKEN.len(), 43);
    let r = env.run(&with_plaintext(&env, utf16le(TOKEN)));
    assert!(r.pushed(), "The good token is read");
    let mut bad: Vec<(String, String)> = vec![
        ("42 characters".into(), TOKEN[..42].to_string()),
        ("44 characters".into(), format!("{TOKEN}x")),
        ("41 characters".into(), TOKEN[..41].to_string()),
        ("86 characters".into(), format!("{TOKEN}{TOKEN}")),
        ("1 character".into(), "a".to_string()),
        ("a padding sign".into(), format!("{}=", &TOKEN[..42])),
        ("a padding sign beyond 43".into(), format!("{TOKEN}=")),
        ("two padding signs".into(), format!("{}==", &TOKEN[..41])),
        (
            "a space".into(),
            format!("{} {}", &TOKEN[..20], &TOKEN[21..]),
        ),
        ("a dot".into(), format!("{}.{}", &TOKEN[..20], &TOKEN[21..])),
        (
            "a colon".into(),
            format!("{}:{}", &TOKEN[..20], &TOKEN[21..]),
        ),
        (
            "a tab".into(),
            format!("{}\t{}", &TOKEN[..20], &TOKEN[21..]),
        ),
        (
            "a Latin letter with an accent".into(),
            format!("{}\u{e9}{}", &TOKEN[..20], &TOKEN[21..]),
        ),
        (
            "a CJK character".into(),
            format!("{}\u{4e2d}{}", &TOKEN[..20], &TOKEN[21..]),
        ),
        (
            "an emoji, two code units".into(),
            format!("{}\u{1F600}{}", &TOKEN[..20], &TOKEN[22..]),
        ),
        (
            "an emoji, 43 code points".into(),
            format!("{}\u{1F600}{}", &TOKEN[..20], &TOKEN[21..]),
        ),
        (
            "a full-width letter".into(),
            format!("{}\u{ff21}{}", &TOKEN[..20], &TOKEN[21..]),
        ),
    ];
    for (name, c) in [("a plus", '+'), ("a slash", '/')] {
        // In the middle, first and last: each alphabet fault is tried at three places.
        bad.push((
            format!("{name} in the middle"),
            format!("{}{c}{}", &TOKEN[..20], &TOKEN[21..]),
        ));
        bad.push((format!("{name} first"), format!("{c}{}", &TOKEN[1..])));
        bad.push((format!("{name} last"), format!("{}{c}", &TOKEN[..42])));
    }
    // Where the 43 characters stand in standard base64 (a '+', a '/') but not in base64url.
    bad.push((
        "a standard base64 token".into(),
        "abcdefghijklmnopqrstuvwxyz0123456789+/ABCDE".to_string(),
    ));
    for (what, token) in bad {
        let mut s = with_plaintext(&env, utf16le(&token));
        s.credential = Slot::Bytes(good_credential());
        for mode in MODES {
            s.mode = mode;
            let r = env.run(&s);
            assert_eq!(
                r.printed,
                vec![W_COULD_NOT_READ.to_string()],
                "{what} {mode:?}"
            );
            assert!(!r.pushed(), "{what}: nothing is sent");
            assert!(r.stamp_after.is_none(), "{what}: nothing is recorded");
            // The value is never printed, whole or in part.
            let all = format!("{:?} {:?} {}", r.printed, r.events, r.debug);
            if token.len() >= 20 {
                assert!(!all.contains(&token[..20]), "{what}: the value is printed");
            }
        }
    }
}

#[test]
fn an_uppercase_or_mixed_case_hex_is_accepted_and_gives_the_same_blob() {
    // The hex is read tolerantly (architect, 2026-11-20): an even count of at least two hex
    // digits, either case, and the blob's header is not checked.
    let env = Env::new();
    let h = hex();
    let mixed: String = h
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i % 2 == 0 {
                c.to_ascii_uppercase()
            } else {
                c
            }
        })
        .collect();
    for text in [h.to_uppercase(), mixed] {
        let bytes = enc_utf8(&credential_json(&text));
        assert_eq!(parse_blob(&bytes), Some(blob()), "{text}");
        let r = accepted(&env, env.scenario(), bytes);
        assert_eq!(r.unprotect_calls, vec![blob()]);
    }
}

#[test]
fn hex_of_two_digits_is_enough_and_the_header_is_not_checked() {
    let env = Env::new();
    for (text, expected) in [
        ("ab", vec![0xAB]),
        ("00", vec![0x00]),
        (
            "0102030405060708090a0b0c0d0e0f10",
            (1..=16).collect::<Vec<u8>>(),
        ),
        ("DEADbeef", vec![0xDE, 0xAD, 0xBE, 0xEF]),
    ] {
        let bytes = enc_utf8(&credential_json(text));
        assert_eq!(parse_blob(&bytes), Some(expected.clone()), "{text}");
        let r = accepted(&env, env.scenario(), bytes);
        assert_eq!(r.unprotect_calls, vec![expected]);
    }
}

#[test]
fn a_plaintext_with_an_odd_byte_count_is_refused() {
    let env = Env::new();
    let mut p = utf16le(TOKEN);
    p.push(0x41);
    refused_scenario(&env, with_plaintext(&env, p));
    refused_scenario(&env, with_plaintext(&env, vec![0x41]));
}

#[test]
fn a_plaintext_with_an_unpaired_surrogate_is_refused() {
    let env = Env::new();
    // A lone high surrogate, a lone low surrogate, a low before a high.
    for units in [
        vec![0x0061u16, 0xD800, 0x0062],
        vec![0x0061, 0xDC00, 0x0062],
        vec![0x0061, 0xDC00, 0xD800],
        vec![0x0061, 0xD800],
    ] {
        let p: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
        refused_scenario(&env, with_plaintext(&env, p));
    }
}

#[test]
fn an_empty_token_is_refused() {
    let env = Env::new();
    refused_scenario(&env, with_plaintext(&env, Vec::new()));
}

#[test]
fn a_token_holding_a_cr_an_lf_or_a_nul_is_refused() {
    let env = Env::new();
    for token in [
        format!("{TOKEN}\r"),
        format!("{TOKEN}\n"),
        format!("{TOKEN}\0"),
        format!("{TOKEN}\r\nLOCALSWIM_URL=http://elsewhere.invalid"),
        format!("ab\ncd{TOKEN}"),
        "\0".to_string(),
        "\n".to_string(),
    ] {
        refused_scenario(&env, with_plaintext(&env, utf16le(&token)));
    }
}

// ---- Bad input never panics --------------------------------------------------------------------

#[test]
fn hostile_credential_files_are_refused_and_never_panic() {
    let env = Env::new();
    let deep_json = "[".repeat(50_000);
    let deep_obj = "{\"a\":".repeat(50_000);
    let deep_member = format!("{{\"format\":{}", "[".repeat(50_000));
    let mut big = credential_json(&hex());
    big.push_str(&" ".repeat(1 << 20));
    let mut huge_member = good_credential();
    huge_member.truncate(huge_member.len() - 1);
    huge_member.extend(format!(",\"extra\":\"{}\"}}", "x".repeat(1 << 20)).into_bytes());
    for b in [
        enc_utf8(&deep_json),
        enc_utf8(&deep_obj),
        enc_utf8(&deep_member),
        vec![0u8; 4096],
        vec![0xFF, 0xFE],
        vec![0xFE, 0xFF],
        vec![0xEF, 0xBB, 0xBF],
        (0..=255u8).collect(),
        enc_utf8(&"\\u0000".repeat(10)),
        enc_utf8("{\"format\":\"\\ud800\"}"),
        enc_utf8("{\"format\":\"\\uZZZZ\"}"),
        enc_utf8("{\"format\":1e999999999}"),
        enc_utf8("{\"format_version\":1e999999999}"),
        huge_member,
        enc_utf8(&big),
    ] {
        let mut s = env.scenario();
        s.credential = Slot::Bytes(b.clone());
        let r = env.run(&s);
        if r.pushed() {
            // Only the padded-but-valid document is allowed through.
            assert_eq!(b, enc_utf8(&big));
        } else {
            assert_eq!(r.printed, vec![W_COULD_NOT_READ.to_string()]);
        }
    }
}

#[test]
fn hostile_receipts_are_refused_and_never_panic() {
    let env = Env::new();
    let deep_json = "[".repeat(50_000);
    let deep_obj = "{\"a\":".repeat(50_000);
    for b in [
        enc_utf8(&deep_json),
        enc_utf8(&deep_obj),
        vec![0u8; 4096],
        (0..=255u8).collect::<Vec<u8>>(),
        vec![0xFF, 0xFE, 0x00],
        vec![0xFE, 0xFF],
        vec![0xEF, 0xBB, 0xBF],
        enc_utf8("{\"base\":\"\\ud800\",\"credential_directory\":\"x\"}"),
        enc_utf8("{\"base\":\"\\uZZZZ\"}"),
    ] {
        let mut s = env.scenario();
        s.receipt = Slot::Bytes(b);
        let r = env.run(&s);
        assert!(!r.pushed());
        assert_eq!(r.printed, vec![W_COULD_NOT_READ.to_string()]);
    }
}
