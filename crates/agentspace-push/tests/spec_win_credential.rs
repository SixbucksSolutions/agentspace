//! The Bot credential and the receipt, read on Windows with DPAPI (docs/operator.md, What the
//! laptop reads; The receipt; The Bot credential).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Each credential is a dummy, written for the current user with `CryptProtectData` into a
//! throwaway `LOCALAPPDATA`, as operator.md says a test writes one. Nothing here reads the real
//! `%LOCALAPPDATA%\localswim-v2`.
#![cfg(windows)]

mod win_support;

use win_support::*;

fn assert_refused(rig: &Rig, what: &str) {
    let say = rig.up();
    assert_eq!(rig.calls(), 0, "{what}: an ssh was started");
    assert_warnings(&say, &[NOT_READ], what);
    assert!(rig.stamp_bytes().is_none(), "{what}: a stamp was written");
    assert!(
        !say.any_text_contains(&rig.token),
        "{what}: a message holds the token"
    );
}

#[test]
fn a_credential_written_by_powershell_7_utf8_no_mark_is_read() {
    let rig = Rig::new();
    let say = rig.up();
    assert_eq!(rig.calls(), 1);
    assert_warnings(&say, &[], "A UTF-8 credential");
    assert!(bytes_contain(&rig.stdin(1), rig.token.as_bytes()));
}

#[test]
fn a_credential_as_powershell_7_and_windows_powershell_5_indent_it_is_read() {
    // Both write the same four members, UTF-8 with no mark, and differ in indentation and line
    // ends (windows-operations.md).
    for write in [ps7_json as fn(&str) -> String, ps51_json] {
        let rig = Rig::new();
        let hex = to_hex(&protect(&utf16le(&rig.token)));
        rig.write_bot_bytes(write(&hex).as_bytes());
        let say = rig.up();
        assert_eq!(rig.calls(), 1, "An indented credential was refused");
        assert_warnings(&say, &[], "An indented credential");
        assert!(bytes_contain(&rig.stdin(1), rig.token.as_bytes()));
    }
}

#[test]
fn a_credential_in_utf8_with_a_mark_is_read() {
    let rig = Rig::new();
    let hex = to_hex(&protect(&utf16le(&rig.token)));
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend(credential_json(&hex).into_bytes());
    rig.write_bot_bytes(&bytes);
    let say = rig.up();
    assert_eq!(rig.calls(), 1, "A UTF-8 credential with a mark was refused");
    assert_warnings(&say, &[], "A UTF-8 credential with a mark");
}

#[test]
fn a_credential_in_utf16le_with_a_mark_is_refused() {
    // `bot.json` is UTF-8: the file Windows PowerShell 5.1 writes by default is not one.
    let rig = Rig::new();
    let hex = to_hex(&protect(&utf16le(&rig.token)));
    rig.write_bot_bytes(&utf16le_with_mark(&credential_json(&hex)));
    assert_refused(&rig, "A UTF-16LE credential");
}

#[test]
fn a_credential_that_breaks_a_rule_is_refused_though_its_blob_decrypts() {
    // The blob is a real one for this user, so only the document's own rule refuses each.
    let rig_hex = |rig: &Rig| to_hex(&protect(&utf16le(&rig.token)));
    type Make = fn(&str) -> String;
    let cases: [(&str, Make); 8] = [
        ("An extra member", |h| {
            let mut m = members(h);
            m.push(("extra".to_string(), "1".to_string()));
            object(&m)
        }),
        ("A missing member", |h| {
            let mut m = members(h);
            m.remove(1);
            object(&m)
        }),
        ("A role of human", |h| {
            let mut m = members(h);
            m[2].1 = "\"human\"".to_string();
            object(&m)
        }),
        ("A format_version of 1.0", |h| {
            let mut m = members(h);
            m[1].1 = "1.0".to_string();
            object(&m)
        }),
        ("A format_version of 2", |h| {
            let mut m = members(h);
            m[1].1 = "2".to_string();
            object(&m)
        }),
        ("A format of another name", |h| {
            let mut m = members(h);
            m[0].1 = "\"other-credential\"".to_string();
            object(&m)
        }),
        ("A role given twice", |h| {
            let mut m = members(h);
            m[2].1 = "\"human\"".to_string();
            m.push(("role".to_string(), "\"bot\"".to_string()));
            object(&m)
        }),
        ("A member name in another case", |h| {
            let mut m = members(h);
            m[2].0 = "Role".to_string();
            object(&m)
        }),
    ];
    for (what, make) in cases {
        let rig = Rig::new();
        rig.write_bot_bytes(make(&rig_hex(&rig)).as_bytes());
        assert_refused(&rig, what);
    }
}

#[test]
fn the_decrypted_token_is_sent_not_its_hex_or_its_blob() {
    let rig = Rig::new();
    rig.up();
    let sent = rig.stdin(1);
    assert!(bytes_contain(
        &sent,
        format!("LOCALSWIM_TOKEN={}\n", rig.token).as_bytes()
    ));
}

#[test]
fn a_blob_that_is_not_dpapi_is_refused_with_the_fixed_warning() {
    let rig = Rig::new();
    rig.write_bot_bytes(credential_json("0102030405060708090a0b0c0d0e0f10").as_bytes());
    assert_refused(&rig, "A blob that is not DPAPI");
}

#[test]
fn localswims_example_header_followed_by_zeros_is_refused_by_the_decryption() {
    // Localswim's examples hold a DPAPI header and bytes of value zero, "which
    // CryptUnprotectData refuses": the document is good, and the blob is not.
    let rig = Rig::new();
    let mut blob = vec![
        0x01, 0x00, 0x00, 0x00, 0xD0, 0x8C, 0x9D, 0xDF, 0x01, 0x15, 0xD1, 0x11, 0x8C, 0x7A, 0x00,
        0xC0, 0x4F, 0xC2, 0x97, 0xEB, 0x01, 0x00, 0x00, 0x00,
    ];
    blob.extend([0x11; 16]);
    blob.extend([0x00; 20]);
    rig.write_bot_bytes(credential_json(&to_hex(&blob)).as_bytes());
    assert_refused(&rig, "Localswim's example blob");
}

#[test]
fn a_corrupted_blob_is_refused_with_the_fixed_warning() {
    let rig = Rig::new();
    let mut blob = protect(&utf16le(&rig.token));
    let last = blob.len() - 1;
    blob[last] ^= 0xFF;
    let mid = blob.len() / 2;
    blob[mid] ^= 0xFF;
    rig.write_bot_bytes(credential_json(&to_hex(&blob)).as_bytes());
    assert_refused(&rig, "A corrupted blob");
}

#[test]
fn a_blob_with_a_wrong_start_is_refused() {
    let rig = Rig::new();
    let mut blob = protect(&utf16le(&rig.token));
    blob[0] = 0x02;
    rig.write_bot_bytes(credential_json(&to_hex(&blob)).as_bytes());
    assert_refused(&rig, "A blob whose first four bytes differ");
}

#[test]
fn a_plaintext_of_an_odd_byte_count_is_refused() {
    let rig = Rig::new();
    let mut plain = utf16le(&rig.token);
    plain.push(b'x');
    rig.write_credential_plain(&plain);
    assert_refused(&rig, "A token of an odd byte count");
}

#[test]
fn a_token_with_an_unpaired_surrogate_is_refused() {
    let rig = Rig::new();
    let mut plain = utf16le("abc");
    plain.extend([0x00, 0xD8]);
    rig.write_credential_plain(&plain);
    assert_refused(&rig, "A token with an unpaired surrogate");
}

#[test]
fn an_uppercase_hex_is_read() {
    // The hex is read tolerantly: an even count of hex digits in either case, and the header is
    // not checked by the reader.
    let rig = Rig::new();
    let hex = to_hex(&protect(&utf16le(&rig.token))).to_ascii_uppercase();
    rig.write_bot_bytes(credential_json(&hex).as_bytes());
    let say = rig.up();
    assert_eq!(rig.calls(), 1, "An uppercase hex was refused");
    assert_warnings(&say, &[], "An uppercase hex");
}

#[test]
fn a_token_that_is_not_43_characters_of_base64url_is_refused() {
    // A made-up token of the right shape is read; each of these differs from it by one fault.
    // A rig holds a lock for its life, so the good token is made without one.
    let good = dummy_token();
    assert_eq!(good.len(), 43);
    let cases = [
        ("42 characters", good[..42].to_string()),
        ("44 characters", format!("{good}x")),
        ("a plus", format!("{}+{}", &good[..20], &good[21..])),
        ("a slash", format!("{}/{}", &good[..20], &good[21..])),
        ("a padding sign", format!("{}=", &good[..42])),
        ("a space", format!("{} {}", &good[..20], &good[21..])),
        (
            "a non-ASCII letter",
            format!("{}\u{e9}{}", &good[..20], &good[21..]),
        ),
        (
            "an emoji",
            format!("{}\u{1f600}{}", &good[..20], &good[22..]),
        ),
        ("empty", String::new()),
    ];
    for (what, token) in cases {
        let rig = Rig::new();
        rig.write_credential_for(&token);
        assert_refused(&rig, what);
    }
}

#[test]
fn an_empty_token_is_refused() {
    let rig = Rig::new();
    // An empty plaintext may not protect at all; the check only applies when it does.
    if let Some(blob) = try_protect(&[]) {
        rig.write_bot_bytes(credential_json(&to_hex(&blob)).as_bytes());
        assert_refused(&rig, "An empty token");
    }
}

#[test]
fn a_token_with_an_lf_a_cr_or_a_nul_is_refused() {
    for bad in ["abc\ndef", "abc\r", "abc\0def"] {
        let rig = Rig::new();
        rig.write_credential_for(bad);
        assert_refused(&rig, "A token with a line end or a NUL");
    }
}

#[test]
fn a_base_url_with_a_cr_an_lf_or_a_nul_is_refused() {
    for bad in [
        "http://127.0.0.1:1\r",
        "http://127.0.0.1:1\nx",
        "http://127.0.0.1:1\0x",
    ] {
        let mut rig = Rig::new();
        rig.base = bad.to_string();
        rig.write_receipt();
        assert_refused(&rig, "A base URL with a line end or a NUL");
    }
}

#[test]
fn an_empty_base_url_is_refused() {
    let mut rig = Rig::new();
    rig.base = String::new();
    rig.write_receipt();
    assert_refused(&rig, "An empty base URL");
}

#[test]
fn a_missing_base_is_refused() {
    let rig = Rig::new();
    rig.write_receipt_text(
        &serde_json::json!({ "credential_directory": rig.credential_dir().to_string_lossy() })
            .to_string(),
    );
    assert_refused(&rig, "A receipt with no base");
}

#[test]
fn a_receipt_that_is_not_a_json_object_is_refused() {
    for text in ["", "not json", "[1,2]", "\"text\"", "{\"base\": \"x\",}"] {
        let rig = Rig::new();
        rig.write_receipt_text(text);
        assert_refused(&rig, "A receipt that is not a JSON object");
    }
}

#[test]
fn a_missing_empty_or_non_string_credential_directory_is_refused() {
    for text in [
        "{\"base\":\"http://127.0.0.1:1\"}",
        "{\"base\":\"http://127.0.0.1:1\",\"credential_directory\":\"\"}",
        "{\"base\":\"http://127.0.0.1:1\",\"credential_directory\":5}",
    ] {
        let rig = Rig::new();
        rig.write_receipt_text(text);
        assert_refused(&rig, "A receipt with a bad credential directory");
    }
}

#[test]
fn a_missing_bot_json_is_refused_with_the_fixed_warning() {
    let rig = Rig::new();
    std::fs::remove_file(rig.bot_path()).unwrap();
    assert_refused(&rig, "A missing bot.json");
}

#[test]
fn a_bot_json_that_is_a_folder_is_refused() {
    let rig = Rig::new();
    std::fs::remove_file(rig.bot_path()).unwrap();
    std::fs::create_dir_all(rig.bot_path()).unwrap();
    assert_refused(&rig, "A bot.json that is a folder");
}

#[test]
fn a_bot_json_that_is_not_a_credential_is_refused() {
    for text in ["", "not json", "{}", "[]", "{\"a\":1}", "\"text\""] {
        let rig = Rig::new();
        rig.write_bot_bytes(text.as_bytes());
        assert_refused(&rig, "A bot.json that is not a credential");
    }
}

#[test]
fn only_bot_json_is_read_and_a_good_credential_under_another_name_is_not() {
    // A reader MUST ignore any file but `bot.json`, and no fallback to another name is kept.
    let rig = Rig::new();
    let hex = to_hex(&protect(&utf16le(&rig.token)));
    std::fs::remove_file(rig.bot_path()).unwrap();
    for other in ["human.json", "bot.json.bak", "other.json"] {
        std::fs::write(rig.credential_dir().join(other), credential_json(&hex)).unwrap();
    }
    assert_refused(&rig, "A folder with no bot.json");
}

#[test]
fn a_plaintext_token_in_the_hex_member_is_refused() {
    let rig = Rig::new();
    rig.write_bot_bytes(credential_json(&rig.token).as_bytes());
    assert_refused(&rig, "A plaintext token");
}

#[test]
fn a_file_that_is_not_one_json_object_is_refused_though_its_blob_decrypts() {
    let rig = Rig::new();
    let hex = to_hex(&protect(&utf16le(&rig.token)));
    let good = credential_json(&hex);
    for doc in [
        format!("[{good}]"),
        format!("{good}{good}"),
        format!("{good}x"),
        format!("{good},"),
    ] {
        rig.write_bot_bytes(doc.as_bytes());
        assert_refused(&rig, "A file that is not one JSON object");
    }
}

#[test]
fn an_unset_local_app_data_skips_the_push_without_a_message() {
    let rig = Rig::new();
    let mut ctx = rig.context();
    ctx.local_app_data = None;
    let say = FakeSay::person();
    agentspace_push::push(&ctx, &say, INSTANCE, DEFAULT_ALIAS, false);
    assert_eq!(rig.calls(), 0);
    assert!(say.silent(), "A missing LOCALAPPDATA was reported");
}

#[test]
fn the_receipt_is_found_only_through_local_app_data() {
    let rig = Rig::new();
    // A receipt anywhere else, the home's own AppData included, is not looked for.
    let elsewhere = rig.home.join("AppData").join("Local").join("localswim-v2");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::copy(rig.receipt_path(), elsewhere.join("service.json")).unwrap();
    std::fs::remove_file(rig.receipt_path()).unwrap();
    let say = rig.up();
    assert_eq!(rig.calls(), 0, "A receipt outside LOCALAPPDATA was used");
    assert!(say.silent());
}

#[test]
fn a_relative_credential_directory_is_resolved_against_the_working_folder() {
    let rig = Rig::new();
    let relative = rig.work.join("rel-creds");
    std::fs::create_dir_all(&relative).unwrap();
    let hex = to_hex(&protect(&utf16le(&rig.token)));
    std::fs::write(relative.join("bot.json"), credential_json(&hex)).unwrap();
    rig.write_receipt_text(
        &serde_json::json!({ "base": rig.base, "credential_directory": "rel-creds" }).to_string(),
    );
    let say = rig.up();
    assert_eq!(
        rig.calls(),
        1,
        "A relative credential directory was not resolved"
    );
    assert_warnings(&say, &[], "A relative credential directory");
}

#[test]
fn a_credential_directory_may_end_in_a_slash_or_use_forward_slashes() {
    for variant in 0..2 {
        let rig = Rig::new();
        let dir = rig.credential_dir().to_string_lossy().into_owned();
        let written = if variant == 0 {
            format!("{dir}\\")
        } else {
            dir.replace('\\', "/")
        };
        rig.write_receipt_text(
            &serde_json::json!({ "base": rig.base, "credential_directory": written }).to_string(),
        );
        let say = rig.up();
        assert_eq!(rig.calls(), 1, "A credential directory form was refused");
        assert_warnings(&say, &[], "A credential directory form");
    }
}

#[test]
fn a_member_name_matches_in_any_ascii_case_and_other_members_are_ignored() {
    let rig = Rig::new();
    rig.write_receipt_text(
        &serde_json::json!({
            "BASE": rig.base,
            "Credential_Directory": rig.credential_dir().to_string_lossy(),
            "other": [1, 2, 3],
        })
        .to_string(),
    );
    let say = rig.up();
    assert_eq!(rig.calls(), 1);
    assert_warnings(&say, &[], "Members in other cases");
}

#[test]
fn a_member_given_twice_is_refused() {
    let rig = Rig::new();
    let dir = serde_json::to_string(&rig.credential_dir().to_string_lossy()).unwrap();
    rig.write_receipt_text(&format!(
        "{{\"base\":\"http://127.0.0.1:1\",\"BASE\":\"http://127.0.0.1:2\",\"credential_directory\":{dir}}}"
    ));
    assert_refused(&rig, "A base given twice");
}

#[test]
fn a_credential_that_cannot_be_read_warns_at_every_up_even_when_nothing_would_be_sent() {
    let rig = Rig::new();
    rig.up();
    assert_eq!(rig.calls(), 1);
    rig.write_bot_bytes(b"not a credential");
    let say = rig.up();
    assert_eq!(rig.calls(), 1, "An unreadable credential started an ssh");
    assert_warnings(
        &say,
        &[NOT_READ],
        "An unreadable credential after a matching stamp",
    );
}

#[test]
fn an_unreadable_credential_does_not_disturb_the_stored_stamp() {
    let rig = Rig::new();
    rig.up();
    let before = rig.stamp_bytes();
    rig.write_bot_bytes(b"not a credential");
    rig.up();
    assert!(rig.stamp_bytes() == before, "The stamp changed");
}
