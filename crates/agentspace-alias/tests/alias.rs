//! Tests for the alias file, the alias rule, the ProxyCommand line, the owner mark and the
//! `Include` plan, from docs/state.md ("The alias file", "The `Include` line").

use agentspace_alias::*;

const ID: &str = "3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90";

fn sample<'a>(proxy: &'a str, key: &'a str, cert: &'a str) -> AliasFile<'a> {
    AliasFile {
        id: ID,
        comment: "Written by agentspace. Do not edit.",
        alias: "agentspace-swimcoach-3f2a9c1e",
        user: "ubuntu",
        key_path: key,
        cert_path: cert,
        proxy_command: proxy,
    }
}

// ---------- alias name and rule ----------

#[test]
fn alias_name_is_project_and_first_eight_hex() {
    assert_eq!(alias_name("swimcoach", ID), "agentspace-swimcoach-3f2a9c1e");
}

#[test]
fn alias_rule_accepts_allowed_characters() {
    assert!(is_valid_alias("agentspace-swimcoach-3f2a9c1e"));
    assert!(is_valid_alias("Ab.c_d-9"));
    assert!(is_valid_alias("a"));
}

#[test]
fn alias_rule_is_case_sensitive_and_accepts_both_cases() {
    assert!(is_valid_alias("ABC"));
    assert!(is_valid_alias("abc"));
}

#[test]
fn alias_rule_refuses_leading_dash() {
    assert!(!is_valid_alias("-abc"));
    assert!(!is_valid_alias("-"));
}

#[test]
fn alias_rule_refuses_empty() {
    assert!(!is_valid_alias(""));
}

#[test]
fn alias_rule_refuses_trailing_lf() {
    assert!(!is_valid_alias("abc\n"));
}

#[test]
fn alias_rule_refuses_other_characters() {
    for bad in [
        "a b", "a/b", "a\\b", "a%b", "a\"b", "a$b", "a;b", "a\tb", "a\rb", "a\nb", "é", "a*",
    ] {
        assert!(!is_valid_alias(bad), "{bad:?} must be refused");
    }
}

// ---------- lease_file_text ----------

#[test]
fn lease_file_text_windows_home() {
    assert_eq!(
        lease_file_text("C:\\Users\\someone", ID, "id_ed25519"),
        format!("C:/Users/someone/.agentspace/leases/{ID}/id_ed25519")
    );
}

#[test]
fn lease_file_text_drops_trailing_separator() {
    assert_eq!(
        lease_file_text("C:\\Users\\someone\\", ID, "id_ed25519"),
        format!("C:/Users/someone/.agentspace/leases/{ID}/id_ed25519")
    );
    assert_eq!(
        lease_file_text("C:\\Users\\someone/", ID, "id_ed25519"),
        format!("C:/Users/someone/.agentspace/leases/{ID}/id_ed25519")
    );
}

#[test]
fn lease_file_text_keeps_drive_root_separator() {
    assert_eq!(
        lease_file_text("C:\\", ID, "id_ed25519"),
        format!("C:/.agentspace/leases/{ID}/id_ed25519")
    );
}

#[test]
fn lease_file_text_slash_in_home_is_kept() {
    assert_eq!(
        lease_file_text("C:/Users/someone", ID, "id_ed25519-cert.pub"),
        format!("C:/Users/someone/.agentspace/leases/{ID}/id_ed25519-cert.pub")
    );
}

#[test]
fn lease_file_text_doubles_percent() {
    assert_eq!(
        lease_file_text("C:\\Users\\100%", ID, "id_ed25519"),
        format!("C:/Users/100%%/.agentspace/leases/{ID}/id_ed25519")
    );
}

#[test]
fn lease_file_text_keeps_spaces_and_case() {
    assert_eq!(
        lease_file_text("D:\\Some Name\\Home", ID, "k"),
        format!("D:/Some Name/Home/.agentspace/leases/{ID}/k")
    );
}

// ---------- the alias file's bytes ----------

const BLOCK: &str = "\
# agentspace lease 3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90; Written by agentspace. Do not edit.
Host agentspace-swimcoach-3f2a9c1e
  User ubuntu
  IdentityFile \"C:/Users/someone/.agentspace/leases/3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90/id_ed25519\"
  CertificateFile \"C:/Users/someone/.agentspace/leases/3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90/id_ed25519-cert.pub\"
  IdentitiesOnly yes
  HostKeyAlias agentspace
  StrictHostKeyChecking yes
  ProxyCommand \"C:\\agentspace\\agentspace-ssh-proxy.exe\" agentspace-swimcoach-3f2a9c1e
  ServerAliveInterval 30
  ServerAliveCountMax 4
  RemoteForward 127.0.0.1:8811 127.0.0.1:8811
  RemoteForward 127.0.0.1:53147 127.0.0.1:53147
";

#[test]
fn alias_file_bytes_match_state_md_block() {
    let key = lease_file_text("C:\\Users\\someone", ID, "id_ed25519");
    let cert = lease_file_text("C:\\Users\\someone", ID, "id_ed25519-cert.pub");
    let proxy = "\"C:\\agentspace\\agentspace-ssh-proxy.exe\" agentspace-swimcoach-3f2a9c1e";
    let bytes = alias_file_bytes(&sample(proxy, &key, &cert)).unwrap();
    assert_eq!(String::from_utf8(bytes).unwrap(), BLOCK);
}

#[test]
fn alias_file_has_no_bom_and_ends_in_one_lf() {
    let bytes = alias_file_bytes(&sample("p", "/k", "/c")).unwrap();
    assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert!(bytes.ends_with(b"\n"));
    assert!(!bytes.ends_with(b"\n\n"));
    assert!(!bytes.contains(&b'\r'));
}

#[test]
fn alias_file_lines_after_second_are_indented_two_spaces() {
    let bytes = alias_file_bytes(&sample("p", "/k", "/c")).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 13);
    assert!(lines[0].starts_with("# agentspace lease "));
    assert!(lines[1].starts_with("Host "));
    for l in &lines[2..] {
        assert!(l.starts_with("  ") && !l.starts_with("   "), "{l:?}");
    }
}

#[test]
fn alias_file_proxy_command_percent_is_doubled() {
    let f = sample("\"C:\\a%b\\p.exe\" x", "/k", "/c");
    let text = String::from_utf8(alias_file_bytes(&f).unwrap()).unwrap();
    assert!(
        text.contains("  ProxyCommand \"C:\\a%%b\\p.exe\" x\n"),
        "{text}"
    );
}

#[test]
fn alias_file_proxy_command_backslashes_stay() {
    let f = sample(
        "C:\\Windows\\System32\\cmd.exe /d /c \"C:\\x\\p.cmd\" a",
        "/k",
        "/c",
    );
    let text = String::from_utf8(alias_file_bytes(&f).unwrap()).unwrap();
    assert!(
        text.contains("  ProxyCommand C:\\Windows\\System32\\cmd.exe /d /c \"C:\\x\\p.cmd\" a\n")
    );
}

#[test]
fn alias_file_writes_the_paths_as_given_without_escaping_again() {
    // The paths are already in the alias file's form, as lease_file_text gives them (the card's
    // pinned interface), so a `%%` stays `%%` and is not doubled again.
    let key = lease_file_text("C:\\Users\\a%b", ID, "k");
    assert!(key.contains("a%%b"));
    let f = sample("p", &key, &key);
    let text = String::from_utf8(alias_file_bytes(&f).unwrap()).unwrap();
    assert!(
        text.contains(&format!("  IdentityFile \"{key}\"\n")),
        "{text}"
    );
    assert!(
        text.contains(&format!("  CertificateFile \"{key}\"\n")),
        "{text}"
    );
}

#[test]
fn alias_file_refuses_cr_or_lf_in_paths_and_proxy() {
    for bad in ["a\nb", "a\rb", "a\r\nb", "\n", "x\n"] {
        assert!(
            alias_file_bytes(&sample("p", bad, "/c")).is_err(),
            "key {bad:?}"
        );
        assert!(
            alias_file_bytes(&sample("p", "/k", bad)).is_err(),
            "cert {bad:?}"
        );
        assert!(
            alias_file_bytes(&sample(bad, "/k", "/c")).is_err(),
            "proxy {bad:?}"
        );
    }
}

#[test]
fn alias_error_message_names_no_path() {
    let e = alias_file_bytes(&sample("p", "C:\\Users\\someone\nx", "/c")).unwrap_err();
    assert!(!e.0.is_empty());
    assert!(!e.0.contains("someone"), "{}", e.0);
    assert!(!e.0.contains('\n'));
}

// ---------- default ProxyCommand ----------

#[test]
fn proxy_exe_form() {
    assert_eq!(
        default_proxy_command("C:\\agentspace\\agentspace-ssh-proxy.exe", "al", None).unwrap(),
        "\"C:\\agentspace\\agentspace-ssh-proxy.exe\" al"
    );
}

#[test]
fn proxy_exe_form_needs_no_system_root_even_if_empty() {
    assert_eq!(
        default_proxy_command("C:\\a\\p.exe", "al", Some("")).unwrap(),
        "\"C:\\a\\p.exe\" al"
    );
}

#[test]
fn proxy_cmd_and_bat_any_case_use_interpreter() {
    for ext in ["cmd", "CMD", "Cmd", "cMd", "bat", "BAT", "Bat"] {
        let prog = format!("C:\\a b\\p.{ext}");
        assert_eq!(
            default_proxy_command(&prog, "al", Some("C:\\Windows")).unwrap(),
            format!("C:\\Windows\\System32\\cmd.exe /d /c \"{prog}\" al"),
            "{ext}"
        );
    }
}

#[test]
fn proxy_other_extensions_use_program_form() {
    for prog in [
        "C:\\a\\p.exe",
        "C:\\a\\p.EXE",
        "C:\\a\\p.cmd.exe",
        "C:\\a\\p.batx",
        "C:\\a\\pcmd",
        "C:\\a.cmd\\p",
    ] {
        assert_eq!(
            default_proxy_command(prog, "al", Some("C:\\Windows")).unwrap(),
            format!("\"{prog}\" al"),
            "{prog}"
        );
    }
}

#[test]
fn proxy_interpreter_form_refuses_missing_or_empty_system_root() {
    assert!(default_proxy_command("C:\\a\\p.cmd", "al", None).is_err());
    assert!(default_proxy_command("C:\\a\\p.bat", "al", Some("")).is_err());
}

#[test]
fn proxy_system_root_written_out_never_as_variable() {
    let s = default_proxy_command("C:\\a\\p.cmd", "al", Some("D:\\Win")).unwrap();
    assert_eq!(s, "D:\\Win\\System32\\cmd.exe /d /c \"C:\\a\\p.cmd\" al");
    assert!(!s.contains("%SystemRoot%"));
}

#[test]
fn proxy_error_message_names_no_path() {
    let e = default_proxy_command("C:\\secret\\p.cmd", "al", None).unwrap_err();
    assert!(!e.0.is_empty());
    assert!(!e.0.contains("secret"), "{}", e.0);
}

// ---------- owner mark ----------

#[test]
fn owner_mark_exact_form() {
    let b = format!("# agentspace lease {ID}; some comment\nHost x\n");
    assert_eq!(owner_mark(b.as_bytes()).as_deref(), Some(ID));
}

#[test]
fn owner_mark_of_generated_file() {
    let key = lease_file_text("C:\\Users\\someone", ID, "id_ed25519");
    let bytes = alias_file_bytes(&sample("p", &key, &key)).unwrap();
    assert_eq!(owner_mark(&bytes).as_deref(), Some(ID));
}

#[test]
fn owner_mark_normalizes_id_to_lowercase() {
    let up = ID.to_uppercase();
    let b = format!("# agentspace lease {up}; c\n");
    assert_eq!(owner_mark(b.as_bytes()).as_deref(), Some(ID));
}

#[test]
fn owner_mark_with_empty_comment() {
    let b = format!("# agentspace lease {ID}; \n");
    assert_eq!(owner_mark(b.as_bytes()).as_deref(), Some(ID));
}

#[test]
fn owner_mark_is_not_found_without_the_mark() {
    assert_eq!(owner_mark(b""), None);
    assert_eq!(owner_mark(b"Host x\n"), None);
    assert_eq!(owner_mark(b"# something else\n"), None);
    assert_eq!(owner_mark(b"# agentspace lease \n"), None);
    assert_eq!(
        owner_mark(format!("# agentspace lease {ID}\n").as_bytes()),
        None
    );
}

#[test]
fn owner_mark_is_only_the_first_line() {
    let b = format!("Host x\n# agentspace lease {ID}; c\n");
    assert_eq!(owner_mark(b.as_bytes()), None);
    let b = format!("\n# agentspace lease {ID}; c\n");
    assert_eq!(owner_mark(b.as_bytes()), None);
}

#[test]
fn owner_mark_must_not_span_a_line_feed() {
    // The ID and its `;` have to be on the first line, up to the first LF.
    let b = format!("# agentspace lease \n{ID}; c\n");
    assert_eq!(owner_mark(b.as_bytes()), None);
    let b = format!("# agentspace lease {ID}\n; c\n");
    assert_eq!(owner_mark(b.as_bytes()), None);
}

#[test]
fn owner_mark_returned_id_holds_no_lf() {
    let b = format!("# agentspace lease {ID}; c\n");
    assert!(!owner_mark(b.as_bytes()).unwrap().contains('\n'));
}

#[test]
fn owner_mark_does_not_panic_on_binary() {
    let _ = owner_mark(&[0xFF, 0xFE, 0x00, b'#', 0xC3]);
    let _ = owner_mark(b"# agentspace lease \xFF\xFE; c\n");
}

// ---------- Include plan ----------

fn insert(plan: IncludePlan) -> Vec<u8> {
    match plan {
        IncludePlan::Insert(v) => v,
        IncludePlan::Found => panic!("expected Insert, got Found"),
        IncludePlan::MustMove => panic!("expected Insert, got MustMove"),
    }
}

fn is_found(c: &[u8]) -> bool {
    matches!(include_plan(c), IncludePlan::Found)
}

fn is_must_move(c: &[u8]) -> bool {
    matches!(include_plan(c), IncludePlan::MustMove)
}

#[test]
fn include_found_at_top() {
    assert!(is_found(b"Include agentspace.d/*\nHost a\n  User x\n"));
}

#[test]
fn include_found_with_nothing_after() {
    assert!(is_found(b"Include agentspace.d/*"));
    assert!(is_found(b"Include agentspace.d/*\n"));
}

#[test]
fn include_found_after_comments_and_other_lines_above_first_block() {
    assert!(is_found(
        b"# my config\n\nServerAliveInterval 5\nInclude agentspace.d/*\nHost a\n"
    ));
}

#[test]
fn include_found_with_bom() {
    assert!(is_found(b"\xEF\xBB\xBFInclude agentspace.d/*\nHost a\n"));
}

#[test]
fn include_found_with_crlf() {
    assert!(is_found(b"Include agentspace.d/*\r\nHost a\r\n"));
}

#[test]
fn include_found_with_other_spacing_and_case() {
    assert!(is_found(b"  INCLUDE \t  AGENTSPACE.D/*  \nHost a\n"));
    assert!(is_found(b"include agentspace.d/*\n"));
    assert!(is_found(b"\tInclude\t\tagentspace.d/*\t\r\n"));
}

#[test]
fn include_with_trailing_comment_is_a_different_line() {
    let c = b"Include agentspace.d/* # aliases\nHost a\n";
    let new = insert(include_plan(c));
    let mut want = b"Include agentspace.d/*\n".to_vec();
    want.extend_from_slice(c);
    assert_eq!(new, want);
}

#[test]
fn include_with_second_pattern_is_a_different_line() {
    let c = b"Include agentspace.d/* other/*\nHost a\n";
    assert!(matches!(include_plan(c), IncludePlan::Insert(_)));
}

#[test]
fn include_words_must_be_separated_by_space_or_tab() {
    assert!(matches!(
        include_plan(b"Includeagentspace.d/*\n"),
        IncludePlan::Insert(_)
    ));
    assert!(matches!(
        include_plan(b"Include=agentspace.d/*\n"),
        IncludePlan::Insert(_)
    ));
}

#[test]
fn include_commented_line_does_not_count() {
    let c = b"# Include agentspace.d/*\nHost a\n";
    let new = insert(include_plan(c));
    let mut want = b"Include agentspace.d/*\n".to_vec();
    want.extend_from_slice(c);
    assert_eq!(new, want);
}

#[test]
fn include_only_below_host_must_move() {
    assert!(is_must_move(b"Host a\n  User x\nInclude agentspace.d/*\n"));
}

#[test]
fn include_only_below_match_must_move() {
    assert!(is_must_move(b"Match all\nInclude agentspace.d/*\n"));
}

#[test]
fn include_below_block_spelled_with_equals_or_tab_must_move() {
    assert!(is_must_move(b"Host=a\nInclude agentspace.d/*\n"));
    assert!(is_must_move(b"Host\ta\nInclude agentspace.d/*\n"));
    assert!(is_must_move(b"Match=all\nInclude agentspace.d/*\n"));
    assert!(is_must_move(b"Match\tall\nInclude agentspace.d/*\n"));
    assert!(is_must_move(b"HOST a\nInclude agentspace.d/*\n"));
    assert!(is_must_move(b"mAtCh all\nInclude agentspace.d/*\n"));
    assert!(is_must_move(b"  \tHost a\nInclude agentspace.d/*\n"));
}

#[test]
fn include_below_bare_host_or_match_line_must_move() {
    assert!(is_must_move(b"Host\nInclude agentspace.d/*\n"));
    assert!(is_must_move(b"Match\r\nInclude agentspace.d/*\r\n"));
}

#[test]
fn include_must_move_with_bom_and_crlf() {
    assert!(is_must_move(
        b"\xEF\xBB\xBFHost a\r\nInclude agentspace.d/*\r\n"
    ));
}

#[test]
fn hostname_is_not_a_host_line() {
    // `HostName` and `Matches` have a different first word, so the Include below is still above
    // any block.
    assert!(is_found(b"HostName x\nInclude agentspace.d/*\nHost a\n"));
    assert!(is_found(b"Matches\nInclude agentspace.d/*\n"));
}

#[test]
fn commented_host_line_is_not_a_block() {
    assert!(is_found(b"# Host a\nInclude agentspace.d/*\nHost b\n"));
}

#[test]
fn include_found_above_and_again_below_is_found() {
    assert!(is_found(
        b"Include agentspace.d/*\nHost a\nInclude agentspace.d/*\n"
    ));
}

#[test]
fn include_below_a_block_with_comments_around_must_move() {
    assert!(is_must_move(
        b"# c\nHost a\n# c\nINCLUDE  agentspace.d/*\r\n"
    ));
}

#[test]
fn insert_into_empty_config() {
    assert_eq!(insert(include_plan(b"")), b"Include agentspace.d/*\n");
}

#[test]
fn insert_into_bom_only_config_keeps_bom() {
    assert_eq!(
        insert(include_plan(b"\xEF\xBB\xBF")),
        b"\xEF\xBB\xBFInclude agentspace.d/*\n"
    );
}

#[test]
fn insert_keeps_bom_then_line_then_original() {
    let c = b"\xEF\xBB\xBFHost a\n  User x\n";
    assert_eq!(
        insert(include_plan(c)),
        b"\xEF\xBB\xBFInclude agentspace.d/*\nHost a\n  User x\n".to_vec()
    );
}

#[test]
fn insert_uses_crlf_when_config_has_crlf_anywhere() {
    let c = b"# c\nHost a\r\n  User x\n";
    assert_eq!(
        insert(include_plan(c)),
        b"Include agentspace.d/*\r\n# c\nHost a\r\n  User x\n".to_vec()
    );
}

#[test]
fn insert_uses_crlf_with_crlf_only_far_below() {
    let c = b"Host a\nUser x\nUser y\nUser z\r\n";
    let new = insert(include_plan(c));
    assert!(new.starts_with(b"Include agentspace.d/*\r\nHost a\n"));
}

#[test]
fn insert_uses_lf_when_only_lone_cr_present() {
    let c = b"# a\rb\nHost a\n";
    assert_eq!(
        insert(include_plan(c)),
        b"Include agentspace.d/*\n# a\rb\nHost a\n".to_vec()
    );
}

#[test]
fn insert_with_bom_and_crlf() {
    let c = b"\xEF\xBB\xBFHost a\r\n";
    assert_eq!(
        insert(include_plan(c)),
        b"\xEF\xBB\xBFInclude agentspace.d/*\r\nHost a\r\n".to_vec()
    );
}

#[test]
fn insert_keeps_missing_final_line_ending() {
    let c = b"Host a\n  User x";
    let new = insert(include_plan(c));
    assert_eq!(new, b"Include agentspace.d/*\nHost a\n  User x".to_vec());
    assert!(!new.ends_with(b"\n"));
}

#[test]
fn insert_into_single_line_without_ending() {
    assert_eq!(
        insert(include_plan(b"ServerAliveInterval 5")),
        b"Include agentspace.d/*\nServerAliveInterval 5".to_vec()
    );
}

#[test]
fn insert_changes_no_other_byte_over_awkward_content() {
    let mut c: Vec<u8> = Vec::new();
    c.extend_from_slice(b"\xEF\xBB\xBF");
    c.extend_from_slice(
        b"  # weird \t spacing \r\n\r\n\n\xFF\xFE binary\r\nHost\ta  \r\n User\tx\r\n\r\n",
    );
    let new = insert(include_plan(&c));
    let mut want = b"\xEF\xBB\xBFInclude agentspace.d/*\r\n".to_vec();
    want.extend_from_slice(&c[3..]);
    assert_eq!(new, want);
}

#[test]
fn insert_result_is_then_found() {
    for c in [
        &b""[..],
        b"\xEF\xBB\xBF",
        b"Host a\n",
        b"Host a\r\n  User x",
        b"# only a comment",
        b"Include agentspace.d/* # x\nHost a\n",
    ] {
        let new = insert(include_plan(c));
        assert!(is_found(&new), "{:?}", String::from_utf8_lossy(c));
    }
}

#[test]
fn insert_when_line_is_only_in_a_comment_and_no_blocks() {
    let c = b"# Include agentspace.d/*";
    assert_eq!(
        insert(include_plan(c)),
        b"Include agentspace.d/*\n# Include agentspace.d/*".to_vec()
    );
}

// ---------- the doc's block, read at test time ----------

#[test]
fn state_md_block_is_what_the_test_transcribes() {
    let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/state.md"))
        .unwrap();
    let start = doc.find("# agentspace lease <id>; <comment>\n").unwrap();
    let end = start + doc[start..].find("```").unwrap();
    let template = &doc[start..end];
    let want = template
        .replace("<id>", ID)
        .replace("<comment>", "Written by agentspace. Do not edit.")
        .replace("<alias>", "agentspace-swimcoach-3f2a9c1e")
        .replace("<user>", "ubuntu")
        .replace(
            "<key path>",
            &format!("C:/Users/someone/.agentspace/leases/{ID}/id_ed25519"),
        )
        .replace(
            "<certificate path>",
            &format!("C:/Users/someone/.agentspace/leases/{ID}/id_ed25519-cert.pub"),
        )
        .replace(
            "<proxy command>",
            "\"C:\\agentspace\\agentspace-ssh-proxy.exe\" agentspace-swimcoach-3f2a9c1e",
        );
    assert_eq!(want, BLOCK);
}
