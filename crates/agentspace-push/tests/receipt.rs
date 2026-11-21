//! What the laptop reads, first: localswim's service receipt (operator.md, The receipt).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;
use agentspace_operator_api::Mode;
use common::*;

fn refused(env: &Env, mut s: Scenario) {
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
        assert!(
            r.events
                .iter()
                .all(|e| !format!("{e:?}").contains("service.json")),
            "the warning names no path"
        );
    }
}

fn skipped(env: &Env, s: Scenario) {
    for mode in MODES {
        let mut s = s.clone();
        s.mode = mode;
        let r = env.run(&s);
        assert_eq!(
            r.printed,
            Vec::<String>::new(),
            "{mode:?}: skipped without a message"
        );
        assert!(!r.pushed());
        assert!(r.unprotect_calls.is_empty(), "no credential is read");
        assert!(r.stamp_after.is_none());
    }
}

fn accepted_with(env: &Env, receipt: Vec<u8>, expect_content: &[u8]) {
    let mut s = env.scenario();
    s.receipt = Slot::Bytes(receipt);
    let r = env.run(&s);
    assert_eq!(r.ssh_calls.len(), 1, "{r:?}");
    assert_eq!(r.ssh_calls[0].stdin, expect_content);
}

fn with_receipt(env: &Env, text: Vec<u8>) -> Scenario {
    let mut s = env.scenario();
    s.receipt = Slot::Bytes(text);
    s
}

// ---- Where it is, and when there is none ------------------------------------------------------

#[test]
fn good_receipt_pushes_with_its_base() {
    let env = Env::new();
    let r = env.run(&env.scenario());
    assert_eq!(r.ssh_calls.len(), 1);
    assert_eq!(r.ssh_calls[0].stdin, CONTENT.as_bytes());
}

#[test]
fn unset_empty_or_blank_localappdata_skips_without_a_message() {
    let env = Env::new();
    for lad in [Lad::Unset, Lad::Empty, Lad::Blank] {
        let mut s = env.scenario();
        s.lad = lad;
        skipped(&env, s);
    }
}

#[test]
fn a_missing_receipt_skips_without_a_message() {
    let env = Env::new();
    let mut s = env.scenario();
    s.receipt = Slot::Absent;
    skipped(&env, s);
}

#[test]
fn a_folder_where_the_receipt_belongs_warns() {
    let env = Env::new();
    let mut s = env.scenario();
    s.receipt = Slot::Folder;
    refused(&env, s);
}

#[test]
fn the_receipt_is_found_only_under_localappdata() {
    // The old place, runtime\service.json beside a checkout, is never read: with no receipt
    // under LOCALAPPDATA, a receipt elsewhere changes nothing.
    let env = Env::new();
    let mut s = env.scenario();
    s.receipt = Slot::Absent;
    std::fs::create_dir_all(env.home().join("runtime")).unwrap();
    std::fs::write(
        env.home().join("runtime").join("service.json"),
        env.good_receipt(),
    )
    .unwrap();
    s.keep_home = true;
    skipped(&env, s);
}

// ---- Encodings --------------------------------------------------------------------------------

#[test]
fn receipt_encodings_are_chosen_by_the_byte_order_mark() {
    let env = Env::new();
    let text = env.receipt_text(&Env::json_str(BASE));
    for bytes in [
        enc_utf8(&text),
        enc_utf8_bom(&text),
        enc_utf16le_bom(&text),
        enc_utf16be_bom(&text),
    ] {
        accepted_with(&env, bytes, CONTENT.as_bytes());
    }
}

#[test]
fn receipt_whitespace_json_allows_is_accepted() {
    let env = Env::new();
    let text = format!(" \n\t\r\n{}\n\n ", env.receipt_text(&Env::json_str(BASE)));
    accepted_with(&env, enc_utf8(&text), CONTENT.as_bytes());
}

#[test]
fn receipt_bytes_invalid_in_the_encoding_are_refused() {
    let env = Env::new();
    let good = env.receipt_text(&Env::json_str(BASE));
    // A lone 0xFF byte inside a string, no mark: invalid UTF-8.
    let mut bad = good.clone().into_bytes();
    let at = good.find("fake.exe").unwrap();
    bad[at] = 0xFF;
    refused(&env, with_receipt(&env, bad));
    // UTF-16LE with an odd number of bytes.
    let mut odd = enc_utf16le_bom(&good);
    odd.push(0x41);
    refused(&env, with_receipt(&env, odd));
    // UTF-16LE with an unpaired surrogate.
    let mut lone = vec![0xFF, 0xFE];
    lone.extend(utf16le("{\"x\":\""));
    lone.extend([0x00, 0xD8]);
    lone.extend(utf16le("\"}"));
    refused(&env, with_receipt(&env, lone));
}

// ---- JSON -------------------------------------------------------------------------------------

#[test]
fn receipt_that_is_not_one_json_object_is_refused() {
    let env = Env::new();
    let good = env.receipt_text(&Env::json_str(BASE));
    let cred = Env::json_str(&env.cred_dir_text());
    for text in [
        String::new(),
        "   ".to_string(),
        "[]".to_string(),
        "\"text\"".to_string(),
        "12".to_string(),
        "null".to_string(),
        "{".to_string(),
        "not json".to_string(),
        // Trailing comma, comment, single quotes, unquoted names: not JSON's own forms.
        format!("{{\"base\":\"{BASE}\",\"credential_directory\":{cred},}}"),
        format!("{{\"base\":\"{BASE}\",/* c */\"credential_directory\":{cred}}}"),
        format!("{{\"base\":\"{BASE}\",\"credential_directory\":{cred}}} // c"),
        format!("{{'base':'{BASE}','credential_directory':{cred}}}"),
        format!("{{base:\"{BASE}\",credential_directory:{cred}}}"),
        // A second value after the first.
        format!("{good} {good}"),
        format!("{good}x"),
    ] {
        refused(&env, with_receipt(&env, enc_utf8(&text)));
    }
}

#[test]
fn receipt_members_are_matched_by_name_in_any_ascii_case() {
    let env = Env::new();
    let cred = Env::json_str(&env.cred_dir_text());
    for text in [
        format!("{{\"BASE\":\"{BASE}\",\"Credential_Directory\":{cred}}}"),
        format!("{{\"bAsE\":\"{BASE}\",\"CREDENTIAL_DIRECTORY\":{cred}}}"),
    ] {
        accepted_with(&env, enc_utf8(&text), CONTENT.as_bytes());
    }
}

#[test]
fn receipt_base_is_sent_after_json_unescaping_and_otherwise_as_it_is() {
    let env = Env::new();
    accepted_with(
        &env,
        enc_utf8(&env.receipt_text("\"http:\\/\\/127.0.0.1:8811\"")),
        CONTENT.as_bytes(),
    );
    accepted_with(
        &env,
        enc_utf8(&env.receipt_text("\"\\u0068ttp://127.0.0.1:8811\"")),
        CONTENT.as_bytes(),
    );
}

#[test]
fn base_must_be_exactly_http_127_0_0_1_and_a_port() {
    // Architect's ruling (2026-11-20, for v0.07-T02): `http://127.0.0.1:PORT`, PORT 1 to 65535 in
    // decimal with no leading zero, and no trailing slash. Anything else is refused.
    let env = Env::new();
    for port in ["1", "80", "8811", "65535", "10000", "9"] {
        let base = format!("http://127.0.0.1:{port}");
        accepted_with(
            &env,
            enc_utf8(&env.receipt_text(&Env::json_str(&base))),
            &content(&base, TOKEN),
        );
    }
    for base in [
        "http://127.0.0.1:8811/",
        "http://127.0.0.1:8811/path",
        "http://127.0.0.1:8811?x=1",
        "http://127.0.0.1:8811#frag",
        "http://127.0.0.1:8811 ",
        " http://127.0.0.1:8811",
        "https://127.0.0.1:8811",
        "HTTP://127.0.0.1:8811",
        "http://localhost:8811",
        "http://127.0.0.2:8811",
        "http://127.0.0.1.:8811",
        "http://0.0.0.0:8811",
        "http://[::1]:8811",
        "http://user@127.0.0.1:8811",
        "http://127.0.0.1",
        "http://127.0.0.1:",
        "http://127.0.0.1:0",
        "http://127.0.0.1:00",
        "http://127.0.0.1:08811",
        "http://127.0.0.1:65536",
        "http://127.0.0.1:99999",
        "http://127.0.0.1:123456",
        "http://127.0.0.1:-1",
        "http://127.0.0.1:+80",
        "http://127.0.0.1:80a",
        "http://127.0.0.1:8811:8811",
        "http://x",
        "127.0.0.1:8811",
        "//127.0.0.1:8811",
    ] {
        refused(
            &env,
            with_receipt(&env, enc_utf8(&env.receipt_text(&Env::json_str(base)))),
        );
    }
}

#[test]
fn every_other_receipt_member_is_ignored_whatever_it_holds() {
    let env = Env::new();
    let cred = Env::json_str(&env.cred_dir_text());
    let text = format!(
        "{{\"base\":\"{BASE}\",\"x\":null,\"x\":[1,{{}}],\"credential_directory\":{cred},\"n\":-1.5e3}}"
    );
    accepted_with(&env, enc_utf8(&text), CONTENT.as_bytes());
}

#[test]
fn credential_directory_missing_not_a_string_or_empty_is_refused() {
    let env = Env::new();
    for text in [
        format!("{{\"base\":\"{BASE}\"}}"),
        format!("{{\"base\":\"{BASE}\",\"credential_directory\":null}}"),
        format!("{{\"base\":\"{BASE}\",\"credential_directory\":5}}"),
        format!("{{\"base\":\"{BASE}\",\"credential_directory\":[\"x\"]}}"),
        format!("{{\"base\":\"{BASE}\",\"credential_directory\":\"\"}}"),
    ] {
        refused(&env, with_receipt(&env, enc_utf8(&text)));
    }
}

#[test]
fn base_missing_empty_null_or_not_a_string_is_refused() {
    let env = Env::new();
    let cred = Env::json_str(&env.cred_dir_text());
    refused(
        &env,
        with_receipt(
            &env,
            enc_utf8(&format!("{{\"credential_directory\":{cred}}}")),
        ),
    );
    for base in ["\"\"", "null", "5", "true", "[\"http://x\"]", "{}"] {
        refused(&env, with_receipt(&env, enc_utf8(&env.receipt_text(base))));
    }
}

#[test]
fn base_holding_a_cr_an_lf_or_a_nul_is_refused() {
    let env = Env::new();
    for base in [
        "\"http://127.0.0.1:8811\\r\"",
        "\"http://127.0.0.1:8811\\n\"",
        "\"http://127.0.0.1:8811\\u0000\"",
        "\"http://127.0.0.1\\nLOCALSWIM_TOKEN=x:8811\"",
        "\"\\rhttp://127.0.0.1:8811\"",
    ] {
        refused(&env, with_receipt(&env, enc_utf8(&env.receipt_text(base))));
    }
}

#[test]
fn a_member_given_twice_in_any_case_is_refused() {
    let env = Env::new();
    let cred = Env::json_str(&env.cred_dir_text());
    for text in [
        format!("{{\"base\":\"{BASE}\",\"base\":\"{BASE}\",\"credential_directory\":{cred}}}"),
        format!(
            "{{\"base\":\"{BASE}\",\"BASE\":\"http://127.0.0.1:8812\",\"credential_directory\":{cred}}}"
        ),
        format!(
            "{{\"base\":\"{BASE}\",\"credential_directory\":{cred},\"Credential_Directory\":{cred}}}"
        ),
        format!(
            "{{\"base\":\"{BASE}\",\"credential_directory\":{cred},\"credential_directory\":\"x\"}}"
        ),
    ] {
        refused(&env, with_receipt(&env, enc_utf8(&text)));
    }
}

#[test]
fn a_refusal_never_changes_what_a_caller_sees_beyond_the_pinned_rewrite() {
    // The Could-not-read text holds no `the instance`, so both modes print it alike.
    let env = Env::new();
    let mut s = env.scenario();
    s.receipt = Slot::Bytes(enc_utf8("[]"));
    for mode in [Mode::Person, Mode::Caller] {
        s.mode = mode;
        assert_eq!(env.run(&s).printed, vec![W_COULD_NOT_READ.to_string()]);
    }
}

#[test]
fn receipts_are_read_afresh_each_time() {
    let env = Env::new();
    let mut first = env.scenario();
    first.unprotect = UnprotectBehavior::Returns(utf16le(TOKEN));
    first.ssh = SshBehavior::Answer {
        code: 1,
        stdout: vec![],
    };
    let r1 = env.run(&first);
    assert_eq!(r1.ssh_calls[0].stdin, CONTENT.as_bytes());

    let mut second = env.scenario();
    second.unprotect = UnprotectBehavior::Returns(utf16le(TOKEN_B));
    second.receipt = Slot::Bytes(enc_utf8(&env.receipt_text("\"http://127.0.0.1:9999\"")));
    second.ssh = SshBehavior::Answer {
        code: 1,
        stdout: vec![],
    };
    let r2 = env.run(&second);
    assert_eq!(
        r2.ssh_calls[0].stdin,
        content("http://127.0.0.1:9999", TOKEN_B)
    );
}

#[test]
#[cfg(windows)]
fn the_credential_folder_is_joined_with_a_backslash_unless_it_ends_in_a_separator() {
    // Windows only: on Linux a backslash is not a separator.
    let env = Env::new();
    for dir_text in [
        env.cred_dir().display().to_string(),
        format!("{}\\", env.cred_dir().display()),
        format!("{}/", env.cred_dir().display()),
        env.cred_dir().display().to_string().replace('\\', "/"),
    ] {
        let cred = Env::json_str(&dir_text);
        let text = format!("{{\"base\":\"{BASE}\",\"credential_directory\":{cred}}}");
        accepted_with(&env, enc_utf8(&text), CONTENT.as_bytes());
    }
}
