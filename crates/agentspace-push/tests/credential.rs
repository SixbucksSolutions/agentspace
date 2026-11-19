//! What the laptop reads, second: the Bot credential's CLIXML, down to the `SS` element's hex,
//! the decryption behind `Unprotect`, and the token's own checks (operator.md, The Bot
//! credential).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;
use common::*;

fn accepted(env: &Env, mut s: Scenario, bytes: Vec<u8>) -> Run {
    s.credential = Slot::Bytes(bytes);
    let r = env.run(&s);
    assert_eq!(r.ssh_calls.len(), 1, "should be accepted: {r:?}");
    assert_eq!(r.ssh_calls[0].stdin, CONTENT.as_bytes());
    r
}

fn refused_bytes(env: &Env, bytes: Vec<u8>) {
    let mut s = env.scenario();
    s.credential = Slot::Bytes(bytes);
    refused_scenario(env, s);
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

fn doc(version: Option<&str>, types: &[&str], props_text: &str) -> Vec<u8> {
    enc_utf8(&clixml(version, types, props_text))
}

fn hex() -> String {
    hex_lower(&blob())
}

// ---- Where it is ------------------------------------------------------------------------------

#[test]
fn the_good_credential_gives_the_token_and_the_blob_goes_to_unprotect_unchanged() {
    let env = Env::new();
    let r = env.run(&env.scenario());
    assert_eq!(r.unprotect_calls, vec![blob()]);
    assert_eq!(r.ssh_calls[0].stdin, CONTENT.as_bytes());
}

#[test]
fn a_missing_bot_xml_or_a_folder_in_its_place_is_refused() {
    let env = Env::new();
    for slot in [Slot::Absent, Slot::Folder] {
        let mut s = env.scenario();
        s.credential = slot;
        refused_scenario(&env, s);
    }
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

// ---- Encodings of the document ----------------------------------------------------------------

#[test]
fn credential_encodings_utf8_utf8_bom_and_utf16le_are_accepted() {
    let env = Env::new();
    let text = clixml_ok(&hex());
    let with_decl = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n{text}");
    let with_decl16 = format!("<?xml version=\"1.0\" encoding=\"utf-16\"?>\r\n{text}");
    for bytes in [
        enc_utf8(&text),
        enc_utf8_bom(&text),
        enc_utf16le_bom(&text),
        enc_utf8(&with_decl),
        enc_utf8_bom(&with_decl),
        enc_utf16le_bom(&with_decl16),
    ] {
        accepted(&env, env.scenario(), bytes);
    }
}

#[test]
fn credential_line_ends_and_whitespace_between_elements_mean_nothing() {
    let env = Env::new();
    let text = format!(
        "<Objs Version=\"1.1.0.1\" xmlns=\"{PS_NS}\">\r\n  <Obj RefId=\"0\">\r\n    <TN RefId=\"0\">\r\n      \
         <T>{PSCRED}</T>\r\n      <T>System.Object</T>\r\n    </TN>\r\n    <ToString>{PSCRED}</ToString>\r\n    \
         <Props>\r\n      <S N=\"UserName\">bot</S>\r\n      <SS N=\"Password\">{}</SS>\r\n    </Props>\r\n  </Obj>\r\n</Objs>\r\n",
        hex()
    );
    accepted(&env, env.scenario(), enc_utf8(&text));
}

#[test]
fn comments_processing_instructions_cdata_and_character_references_mean_what_xml_says() {
    let env = Env::new();
    let h = hex();
    let (a, b) = h.split_at(10);
    let (b1, b2) = b.split_at(5);
    let variants = [
        format!("{a}<!-- a comment -->{b}"),
        format!("{a}<?pi data?>{b}"),
        format!("{a}<![CDATA[{b1}]]>{b2}"),
        format!("&#x{:x};{}", h.as_bytes()[0], &h[1..]),
    ];
    for v in variants {
        let text = clixml(
            Some("1.1.0.1"),
            &[PSCRED],
            &format!("<S N=\"UserName\">bot</S><SS N=\"Password\">{v}</SS>"),
        );
        accepted(&env, env.scenario(), enc_utf8(&text));
    }
    // A comment and a processing instruction elsewhere in the document.
    let text = format!("<!-- lead --><?pi x?>{}<!-- trail -->", clixml_ok(&h));
    accepted(&env, env.scenario(), enc_utf8(&text));
}

#[test]
fn a_document_type_declaration_is_refused() {
    let env = Env::new();
    let text = clixml_ok(&hex());
    refused_bytes(&env, enc_utf8(&format!("<!DOCTYPE Objs>{text}")));
    refused_bytes(
        &env,
        enc_utf8(&format!("<!DOCTYPE Objs [<!ENTITY e \"x\">]>{text}")),
    );
    refused_bytes(
        &env,
        enc_utf16le_bom(&format!("<?xml version=\"1.0\"?><!DOCTYPE Objs>{text}")),
    );
}

#[test]
fn text_that_is_not_xml_is_refused() {
    let env = Env::new();
    let good = clixml_ok(&hex());
    for b in [
        Vec::new(),
        enc_utf8("   "),
        enc_utf8("not xml"),
        enc_utf8("<Objs>"),
        enc_utf8(&good.replace("</Objs>", "")),
        enc_utf8(&format!("{good}{good}")),
        vec![0xFF, 0xFE, 0x41],
        vec![0xC3, 0x28, 0xA0],
    ] {
        refused_bytes(&env, b);
    }
}

// ---- Structure --------------------------------------------------------------------------------

#[test]
fn objs_version_is_two_to_four_numbers_starting_with_1() {
    let env = Env::new();
    let p = props(&hex());
    for v in [
        Some("1.1.0.1"),
        Some("1.0"),
        Some("1.2.3"),
        Some(" 1.1.0.1 "),
        Some("\n1.1\t"),
        None,
    ] {
        accepted(&env, env.scenario(), doc(v, &[PSCRED], &p));
    }
    for v in [
        "",
        "1",
        "2.0",
        "0.1",
        "1.1.0.1.1",
        "1.x",
        "1..1",
        "1.",
        ".1",
        "abc",
        "1.1 1",
        "-1.1",
    ] {
        refused_bytes(&env, doc(Some(v), &[PSCRED], &p));
    }
}

#[test]
fn the_root_must_be_objs_in_the_powershell_namespace() {
    let env = Env::new();
    let good = clixml_ok(&hex());
    refused_bytes(
        &env,
        enc_utf8(
            &good
                .replace("<Objs", "<Other")
                .replace("</Objs>", "</Other>"),
        ),
    );
    // No namespace at all.
    refused_bytes(
        &env,
        enc_utf8(&good.replace(&format!(" xmlns=\"{PS_NS}\""), "")),
    );
    // Another namespace.
    refused_bytes(
        &env,
        enc_utf8(&good.replace(PS_NS, "http://example.invalid/ns")),
    );
    // The namespace bound to a prefix is the same namespace.
    let prefixed = format!(
        "<ps:Objs Version=\"1.1.0.1\" xmlns:ps=\"{PS_NS}\"><ps:Obj RefId=\"0\"><ps:TN RefId=\"0\">\
         <ps:T>{PSCRED}</ps:T></ps:TN><ps:Props><ps:S N=\"UserName\">bot</ps:S>\
         <ps:SS N=\"Password\">{}</ps:SS></ps:Props></ps:Obj></ps:Objs>",
        hex()
    );
    accepted(&env, env.scenario(), enc_utf8(&prefixed));
    // A child that is not in the namespace does not count.
    let mixed = good.replace("<Props>", "<Props xmlns=\"\">");
    refused_bytes(&env, enc_utf8(&mixed));
}

#[test]
fn objs_has_exactly_one_obj() {
    let env = Env::new();
    let one = format!(
        "<Obj RefId=\"0\"><TN RefId=\"0\"><T>{PSCRED}</T></TN><Props>{}</Props></Obj>",
        props(&hex())
    );
    let none = format!("<Objs xmlns=\"{PS_NS}\"></Objs>");
    let two = format!("<Objs xmlns=\"{PS_NS}\">{one}{one}</Objs>");
    let other = format!("<Objs xmlns=\"{PS_NS}\">{one}<Obj RefId=\"1\"/></Objs>");
    refused_bytes(&env, enc_utf8(&none));
    refused_bytes(&env, enc_utf8(&two));
    refused_bytes(&env, enc_utf8(&other));
    accepted(
        &env,
        env.scenario(),
        enc_utf8(&format!("<Objs xmlns=\"{PS_NS}\">{one}</Objs>")),
    );
}

#[test]
fn obj_must_be_a_pscredential_in_any_position_among_the_t_children() {
    let env = Env::new();
    let p = props(&hex());
    for types in [
        vec![PSCRED],
        vec!["System.Object", PSCRED],
        vec!["A", "B", PSCRED, "C"],
    ] {
        accepted(&env, env.scenario(), doc(Some("1.1.0.1"), &types, &p));
    }
    for types in [
        vec![],
        vec!["System.Object"],
        vec!["System.Management.Automation.PSCredentialX"],
        vec!["system.management.automation.pscredential"],
        vec![" System.Management.Automation.PSCredential"],
        vec!["System.Management.Automation.PSCredential "],
    ] {
        refused_bytes(&env, doc(Some("1.1.0.1"), &types, &p));
    }
    // No TN at all.
    let no_tn = format!("<Objs xmlns=\"{PS_NS}\"><Obj RefId=\"0\"><Props>{p}</Props></Obj></Objs>");
    refused_bytes(&env, enc_utf8(&no_tn));
}

#[test]
fn props_needs_one_username_s_and_one_password_ss() {
    let env = Env::new();
    let h = hex();
    // Present, in any ASCII case of N, with other members ignored.
    for p in [
        format!("<S N=\"username\">bot</S><SS N=\"password\">{h}</SS>"),
        format!("<S N=\"USERNAME\">bot</S><SS N=\"PASSWORD\">{h}</SS>"),
        format!("<SS N=\"Password\">{h}</SS><S N=\"UserName\">bot</S>"),
        format!(
            "<S N=\"Other\">x</S><S N=\"UserName\">bot</S><I32 N=\"Count\">3</I32><SS N=\"Password\">{h}</SS>"
        ),
        format!(
            "<S N=\"UserName\">bot</S><SS N=\"Password\">{h}</SS><SS N=\"Other\">not hex at all</SS>"
        ),
    ] {
        accepted(&env, env.scenario(), doc(Some("1.1.0.1"), &[PSCRED], &p));
    }
    for p in [
        // No UserName, no Password.
        format!("<SS N=\"Password\">{h}</SS>"),
        "<S N=\"UserName\">bot</S>".to_string(),
        String::new(),
        // A second of either, in any case, could mean two things.
        format!("<S N=\"UserName\">bot</S><S N=\"username\">bot2</S><SS N=\"Password\">{h}</SS>"),
        format!("<S N=\"UserName\">bot</S><SS N=\"Password\">{h}</SS><SS N=\"PASSWORD\">{h}</SS>"),
        format!("<S N=\"UserName\">bot</S><SS N=\"Password\">{h}</SS><SS N=\"Password\">{h}</SS>"),
        // A password in an S element is plain text, which is refused.
        format!("<S N=\"UserName\">bot</S><S N=\"Password\">{TOKEN}</S>"),
        format!("<S N=\"UserName\">bot</S><S N=\"Password\">{h}</S>"),
        // An SS named otherwise, or an S that is not the user name.
        format!("<S N=\"UserName\">bot</S><SS N=\"Pass\">{h}</SS>"),
        format!("<S N=\"User\">bot</S><SS N=\"Password\">{h}</SS>"),
    ] {
        refused_bytes(&env, doc(Some("1.1.0.1"), &[PSCRED], &p));
    }
}

#[test]
fn the_n_attribute_is_in_no_namespace() {
    let env = Env::new();
    let h = hex();
    let p = format!("<S xmlns:x=\"{PS_NS}\" x:N=\"UserName\">bot</S><SS N=\"Password\">{h}</SS>");
    refused_bytes(&env, doc(Some("1.1.0.1"), &[PSCRED], &p));
}

// ---- The SS element's hex ---------------------------------------------------------------------

#[test]
fn hex_digits_in_either_case_become_the_blob() {
    let env = Env::new();
    for h in [hex(), hex().to_uppercase()] {
        let r = accepted(&env, env.scenario(), enc_utf8(&clixml_ok(&h)));
        assert_eq!(
            r.unprotect_calls,
            vec![blob()],
            "the first digit is the high four bits"
        );
    }
}

#[test]
fn hex_that_is_empty_odd_or_not_only_hex_is_refused() {
    let env = Env::new();
    let h = hex();
    let bad = [
        String::new(),
        "a".to_string(),
        format!("{h}0"),
        format!(" {h}"),
        format!("{h} "),
        format!("{h}\n"),
        format!("\n  {h}\n"),
        format!("{} {}", &h[..10], &h[10..]),
        format!("{}\n{}", &h[..10], &h[10..]),
        format!("0x{h}"),
        format!("{}g{}", &h[..10], &h[12..]),
        format!("{}\u{e9}{}", &h[..10], &h[12..]),
        "zz".to_string(),
    ];
    for b in bad {
        refused_bytes(&env, enc_utf8(&clixml_ok(&b)));
    }
}

#[test]
fn a_failed_decryption_is_refused() {
    let env = Env::new();
    let mut s = env.scenario();
    s.unprotect = UnprotectBehavior::Fails("The data is invalid".to_string());
    refused_scenario(&env, s);
}

// ---- The plaintext ----------------------------------------------------------------------------

fn with_plaintext(env: &Env, plain: Vec<u8>) -> Scenario {
    let mut s = env.scenario();
    s.unprotect = UnprotectBehavior::Returns(plain);
    s
}

#[test]
fn the_plaintext_is_utf16le_with_no_mark_and_the_content_carries_utf8() {
    let env = Env::new();
    for token in [
        "tok",
        "t\u{e9}k",
        "tok\u{1F600}en",
        "\u{4e2d}\u{6587}-token",
    ] {
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
    let deep_xml: String = "<a>".repeat(20_000);
    let mut big = clixml_ok(&hex());
    big.push_str(&" ".repeat(1 << 20));
    for b in [
        enc_utf8(&deep_xml),
        enc_utf8(&format!("<Objs xmlns=\"{PS_NS}\">{deep_xml}")),
        vec![0u8; 4096],
        vec![0xFF, 0xFE],
        vec![0xFE, 0xFF],
        vec![0xEF, 0xBB, 0xBF],
        (0..=255u8).collect(),
        enc_utf8(&"&#x0;".repeat(10)),
        enc_utf8("<Objs xmlns=\"x\" xmlns=\"y\"/>"),
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
