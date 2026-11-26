//! Another host's lease alias and a host's operator alias, byte for byte (docs/state.md, "A lease
//! alias for a host that is not the box" and "A host's operator alias", E0.07-T02-S03, #0148).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The box's own lease alias keeps v0.06's
//! bytes, which `alias.rs` pins with the two new fields set to `agentspace` and `None`.

use agentspace_alias::*;

const ID: &str = "3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90";
const PROJECT: &str = "examplehost";
const COMMENT: &str = "Written by agentspace. Do not edit.";
const HOME: &str = "C:\\Users\\someone";
const PROXY_LEASE: &str =
    "\"C:\\agentspace\\agentspace-ssh-proxy.exe\" agentspace-examplehost-3f2a9c1e";
const PROXY_HOST: &str = "\"C:\\agentspace\\agentspace-ssh-proxy.exe\" agentspace-host.examplehost";
const KNOWN_HOSTS: &str = "C:/Users/someone/.agentspace/known_hosts";
const HOST_KEY: &str = "C:/Users/someone/.agentspace/hosts/examplehost/id_ed25519";
const HOST_CERT: &str = "C:/Users/someone/.agentspace/hosts/examplehost/id_ed25519-cert.pub";

const BAD_VALUES: [&str; 5] = ["a\nb", "a\rb", "a\r\nb", "\n", "x\n"];

/// A lease on a host that is not the box, as `create` and `place_pending` build its alias.
fn other_host_lease<'a>(
    proxy: &'a str,
    key: &'a str,
    cert: &'a str,
    known_hosts: &'a str,
) -> AliasFile<'a> {
    AliasFile {
        id: ID,
        comment: COMMENT,
        alias: "agentspace-examplehost-3f2a9c1e",
        user: "ubuntu",
        key_path: key,
        cert_path: cert,
        host_key_alias: "agentspace-host.examplehost",
        known_hosts: Some(known_hosts),
        proxy_command: proxy,
    }
}

fn host_file<'a>(
    proxy: &'a str,
    key: &'a str,
    cert: &'a str,
    known_hosts: &'a str,
) -> HostAliasFile<'a> {
    HostAliasFile {
        project: PROJECT,
        comment: COMMENT,
        key_path: key,
        cert_path: cert,
        known_hosts,
        proxy_command: proxy,
    }
}

fn host_default() -> HostAliasFile<'static> {
    host_file(PROXY_HOST, HOST_KEY, HOST_CERT, KNOWN_HOSTS)
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap()
}

// ---------- Another host's lease alias ----------

fn lease_key(file: &str) -> String {
    lease_file_text(HOME, ID, file)
}

const OTHER_HOST_LEASE_BLOCK: &str = "\
# agentspace lease 3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90; Written by agentspace. Do not edit.
Host agentspace-examplehost-3f2a9c1e
  User ubuntu
  IdentityFile \"C:/Users/someone/.agentspace/leases/3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90/id_ed25519\"
  CertificateFile \"C:/Users/someone/.agentspace/leases/3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90/id_ed25519-cert.pub\"
  IdentitiesOnly yes
  HostKeyAlias agentspace-host.examplehost
  UserKnownHostsFile \"C:/Users/someone/.agentspace/known_hosts\"
  StrictHostKeyChecking yes
  ProxyCommand \"C:\\agentspace\\agentspace-ssh-proxy.exe\" agentspace-examplehost-3f2a9c1e
  ServerAliveInterval 30
  ServerAliveCountMax 4
  RemoteForward 127.0.0.1:8811 127.0.0.1:8811
  RemoteForward 127.0.0.1:53147 127.0.0.1:53147
";

#[test]
fn another_hosts_lease_alias_is_the_state_md_block_byte_for_byte() {
    let key = lease_key("id_ed25519");
    let cert = lease_key("id_ed25519-cert.pub");
    let bytes = alias_file_bytes(&other_host_lease(PROXY_LEASE, &key, &cert, KNOWN_HOSTS)).unwrap();
    assert_eq!(text(bytes), OTHER_HOST_LEASE_BLOCK);
}

#[test]
fn another_hosts_lease_alias_is_the_boxs_block_with_two_changed_lines() {
    // The one change from the box's alias is `HostKeyAlias` and the line straight after it.
    let key = lease_key("id_ed25519");
    let cert = lease_key("id_ed25519-cert.pub");
    let other =
        text(alias_file_bytes(&other_host_lease(PROXY_LEASE, &key, &cert, KNOWN_HOSTS)).unwrap());
    let mut boxs = other_host_lease(PROXY_LEASE, &key, &cert, KNOWN_HOSTS);
    boxs.host_key_alias = "agentspace";
    boxs.known_hosts = None;
    let boxs = text(alias_file_bytes(&boxs).unwrap());
    let other_lines: Vec<&str> = other.lines().collect();
    let box_lines: Vec<&str> = boxs.lines().collect();
    assert_eq!(box_lines.len(), 13);
    assert_eq!(other_lines.len(), 14);
    assert!(!boxs.contains("UserKnownHostsFile"), "{boxs}");
    // The box's alias, with its HostKeyAlias line replaced by the two, is the other host's.
    let mut want: Vec<String> = Vec::new();
    for l in &box_lines {
        if *l == "  HostKeyAlias agentspace" {
            want.push("  HostKeyAlias agentspace-host.examplehost".to_string());
            want.push(format!("  UserKnownHostsFile \"{KNOWN_HOSTS}\""));
        } else {
            want.push((*l).to_string());
        }
    }
    assert_eq!(other_lines, want);
}

#[test]
fn another_hosts_lease_alias_keeps_the_form_rules() {
    let key = lease_key("id_ed25519");
    let cert = lease_key("id_ed25519-cert.pub");
    let bytes = alias_file_bytes(&other_host_lease(PROXY_LEASE, &key, &cert, KNOWN_HOSTS)).unwrap();
    assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert!(bytes.ends_with(b"\n") && !bytes.ends_with(b"\n\n"));
    assert!(!bytes.contains(&b'\r'));
    let t = text(bytes);
    let lines: Vec<&str> = t.lines().collect();
    assert!(lines[0].starts_with("# agentspace lease "));
    assert!(lines[1].starts_with("Host "));
    for l in &lines[2..] {
        assert!(l.starts_with("  ") && !l.starts_with("   "), "{l:?}");
    }
    // The lease's owner mark is the same as the box's.
    assert_eq!(owner_mark(t.as_bytes()).as_deref(), Some(ID));
}

#[test]
fn another_hosts_known_hosts_path_is_written_as_given_with_a_percent_doubled_once() {
    // The path is in the alias file's form already, as the key's is: a `%%` stays `%%`.
    let key = lease_key("id_ed25519");
    let cert = lease_key("id_ed25519-cert.pub");
    let kh = "C:/Users/100%%/.agentspace/known_hosts";
    let t = text(alias_file_bytes(&other_host_lease(PROXY_LEASE, &key, &cert, kh)).unwrap());
    assert!(
        t.contains("  UserKnownHostsFile \"C:/Users/100%%/.agentspace/known_hosts\"\n"),
        "{t}"
    );
    assert!(!t.contains("%%%%"), "{t}");
}

#[test]
fn another_hosts_known_hosts_line_comes_straight_after_host_key_alias_in_double_quotes() {
    let key = lease_key("id_ed25519");
    let cert = lease_key("id_ed25519-cert.pub");
    let t = text(
        alias_file_bytes(&other_host_lease(
            PROXY_LEASE,
            &key,
            &cert,
            "C:/a b/known_hosts",
        ))
        .unwrap(),
    );
    let lines: Vec<&str> = t.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.starts_with("  HostKeyAlias "))
        .unwrap();
    assert_eq!(lines[at], "  HostKeyAlias agentspace-host.examplehost");
    assert_eq!(lines[at + 1], "  UserKnownHostsFile \"C:/a b/known_hosts\"");
    assert_eq!(lines[at + 2], "  StrictHostKeyChecking yes");
}

#[test]
fn another_hosts_lease_alias_refuses_cr_or_lf_in_the_host_key_alias_and_the_known_hosts_path() {
    let key = lease_key("id_ed25519");
    let cert = lease_key("id_ed25519-cert.pub");
    for bad in BAD_VALUES {
        let mut f = other_host_lease(PROXY_LEASE, &key, &cert, KNOWN_HOSTS);
        f.host_key_alias = bad;
        assert!(alias_file_bytes(&f).is_err(), "host_key_alias {bad:?}");
        let f = other_host_lease(PROXY_LEASE, &key, &cert, bad);
        assert!(alias_file_bytes(&f).is_err(), "known_hosts {bad:?}");
    }
}

#[test]
fn another_hosts_lease_alias_still_refuses_cr_or_lf_in_every_other_value() {
    let key = lease_key("id_ed25519");
    let cert = lease_key("id_ed25519-cert.pub");
    for bad in BAD_VALUES {
        assert!(
            alias_file_bytes(&other_host_lease(bad, &key, &cert, KNOWN_HOSTS)).is_err(),
            "proxy {bad:?}"
        );
        assert!(
            alias_file_bytes(&other_host_lease(PROXY_LEASE, bad, &cert, KNOWN_HOSTS)).is_err(),
            "key {bad:?}"
        );
        assert!(
            alias_file_bytes(&other_host_lease(PROXY_LEASE, &key, bad, KNOWN_HOSTS)).is_err(),
            "cert {bad:?}"
        );
    }
}

#[test]
fn a_refused_value_names_no_path() {
    let key = lease_key("id_ed25519");
    let cert = lease_key("id_ed25519-cert.pub");
    let e = alias_file_bytes(&other_host_lease(
        PROXY_LEASE,
        &key,
        &cert,
        "C:/Users/someone/secret\nx",
    ))
    .unwrap_err();
    assert!(!e.0.is_empty());
    assert!(
        !e.0.contains("someone") && !e.0.contains("secret"),
        "{}",
        e.0
    );
    assert!(!e.0.contains('\n'));
}

// ---------- A host's operator alias ----------

const HOST_BLOCK: &str = "\
# agentspace host examplehost; Written by agentspace. Do not edit.
Host agentspace-host.examplehost
  User ubuntu
  IdentityFile \"C:/Users/someone/.agentspace/hosts/examplehost/id_ed25519\"
  CertificateFile \"C:/Users/someone/.agentspace/hosts/examplehost/id_ed25519-cert.pub\"
  IdentitiesOnly yes
  HostKeyAlias agentspace-host.examplehost
  UserKnownHostsFile \"C:/Users/someone/.agentspace/known_hosts\"
  StrictHostKeyChecking accept-new
  ProxyCommand \"C:\\agentspace\\agentspace-ssh-proxy.exe\" agentspace-host.examplehost
  ServerAliveInterval 30
  ServerAliveCountMax 4
";

#[test]
fn the_host_alias_is_the_state_md_block_byte_for_byte() {
    let bytes = host_alias_file_bytes(&host_default()).unwrap();
    assert_eq!(text(bytes), HOST_BLOCK);
}

#[test]
fn the_host_alias_has_no_bom_ends_in_one_lf_and_indents_every_line_after_the_second() {
    let bytes = host_alias_file_bytes(&host_default()).unwrap();
    assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert!(bytes.ends_with(b"\n") && !bytes.ends_with(b"\n\n"));
    assert!(!bytes.contains(&b'\r'));
    let t = text(bytes);
    let lines: Vec<&str> = t.lines().collect();
    assert_eq!(lines.len(), 12);
    assert_eq!(
        lines[0],
        "# agentspace host examplehost; Written by agentspace. Do not edit."
    );
    assert_eq!(lines[1], "Host agentspace-host.examplehost");
    for l in &lines[2..] {
        assert!(l.starts_with("  ") && !l.starts_with("   "), "{l:?}");
    }
}

#[test]
fn the_host_alias_has_no_remote_forward_and_accepts_a_new_key() {
    // D47: the operator holds no board, and the first connection adds the host's key.
    let t = text(host_alias_file_bytes(&host_default()).unwrap());
    assert!(!t.contains("RemoteForward"), "{t}");
    assert!(t.contains("  StrictHostKeyChecking accept-new\n"), "{t}");
    assert!(!t.contains("StrictHostKeyChecking yes"), "{t}");
    assert!(t.contains("  User ubuntu\n"), "{t}");
    assert!(t.contains("  IdentitiesOnly yes\n"), "{t}");
}

#[test]
fn the_host_alias_names_the_host_key_and_the_known_hosts_file() {
    let t = text(host_alias_file_bytes(&host_default()).unwrap());
    assert!(
        t.contains("  HostKeyAlias agentspace-host.examplehost\n"),
        "{t}"
    );
    assert!(
        t.contains(&format!("  UserKnownHostsFile \"{KNOWN_HOSTS}\"\n")),
        "{t}"
    );
    // The paths are always in double quotes, written as given.
    assert!(
        t.contains(&format!("  IdentityFile \"{HOST_KEY}\"\n")),
        "{t}"
    );
    assert!(
        t.contains(&format!("  CertificateFile \"{HOST_CERT}\"\n")),
        "{t}"
    );
}

#[test]
fn the_host_alias_owner_mark_is_never_a_lease_owner_mark() {
    let bytes = host_alias_file_bytes(&host_default()).unwrap();
    assert_eq!(owner_mark(&bytes), None, "A lease's owner check matched");
    // Whatever case rule the lease's reader uses.
    for mark in [
        "# agentspace host examplehost; c\n",
        "# Agentspace Host examplehost; c\n",
        "# AGENTSPACE HOST EXAMPLEHOST; c\n",
        "# agentspace host 3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90; c\n",
    ] {
        assert_eq!(owner_mark(mark.as_bytes()), None, "{mark:?}");
    }
}

#[test]
fn the_host_alias_percent_in_the_proxy_command_is_doubled_once() {
    let f = host_file("a%b", HOST_KEY, HOST_CERT, KNOWN_HOSTS);
    let t = text(host_alias_file_bytes(&f).unwrap());
    assert!(t.contains("  ProxyCommand a%%b\n"), "{t}");
    assert!(!t.contains("%%%%"), "{t}");
}

#[test]
fn the_host_alias_writes_its_paths_as_given_without_escaping_again() {
    let key = "C:/Users/100%%/.agentspace/hosts/examplehost/id_ed25519";
    let cert = "C:/Users/100%%/.agentspace/hosts/examplehost/id_ed25519-cert.pub";
    let kh = "C:/Users/100%%/.agentspace/known_hosts";
    let t = text(host_alias_file_bytes(&host_file(PROXY_HOST, key, cert, kh)).unwrap());
    assert!(t.contains(&format!("  IdentityFile \"{key}\"\n")), "{t}");
    assert!(
        t.contains(&format!("  CertificateFile \"{cert}\"\n")),
        "{t}"
    );
    assert!(
        t.contains(&format!("  UserKnownHostsFile \"{kh}\"\n")),
        "{t}"
    );
    assert!(!t.contains("%%%%"), "{t}");
}

#[test]
fn the_host_alias_proxy_command_keeps_its_backslashes() {
    let proxy = "C:\\Windows\\System32\\cmd.exe /d /c \"C:\\x\\p.cmd\" agentspace-host.examplehost";
    let t =
        text(host_alias_file_bytes(&host_file(proxy, HOST_KEY, HOST_CERT, KNOWN_HOSTS)).unwrap());
    assert!(t.contains(&format!("  ProxyCommand {proxy}\n")), "{t}");
}

#[test]
fn the_host_alias_refuses_cr_or_lf_in_every_value() {
    for bad in BAD_VALUES {
        let mut f = host_default();
        f.project = bad;
        assert!(host_alias_file_bytes(&f).is_err(), "project {bad:?}");
        let mut f = host_default();
        f.comment = bad;
        assert!(host_alias_file_bytes(&f).is_err(), "comment {bad:?}");
        let mut f = host_default();
        f.key_path = bad;
        assert!(host_alias_file_bytes(&f).is_err(), "key_path {bad:?}");
        let mut f = host_default();
        f.cert_path = bad;
        assert!(host_alias_file_bytes(&f).is_err(), "cert_path {bad:?}");
        let mut f = host_default();
        f.known_hosts = bad;
        assert!(host_alias_file_bytes(&f).is_err(), "known_hosts {bad:?}");
        let mut f = host_default();
        f.proxy_command = bad;
        assert!(host_alias_file_bytes(&f).is_err(), "proxy_command {bad:?}");
    }
}

#[test]
fn the_host_alias_accepts_a_comment_without_a_line_break() {
    let mut f = host_default();
    f.comment = "Any text, even; with % and \\.";
    assert!(host_alias_file_bytes(&f).is_ok());
}

#[test]
fn the_host_alias_refuses_a_project_that_breaks_the_alias_rule() {
    // State.md, The alias file: the name `agentspace-host.<project>` passes the alias rule, and the
    // ProxyCommand puts it on a command line.
    for bad in ["a b", "a;b", "a/b", "a\\b", "a\"b", "a%b", "é"] {
        let mut f = host_default();
        f.project = bad;
        assert!(host_alias_file_bytes(&f).is_err(), "project {bad:?}");
    }
}

#[test]
fn the_host_alias_for_another_project_changes_its_name_and_mark_alone() {
    let mut f = host_default();
    f.project = "other-1";
    let t = text(host_alias_file_bytes(&f).unwrap());
    let want = HOST_BLOCK
        .replace(
            "# agentspace host examplehost;",
            "# agentspace host other-1;",
        )
        .replace(
            "Host agentspace-host.examplehost",
            "Host agentspace-host.other-1",
        )
        .replace(
            "HostKeyAlias agentspace-host.examplehost",
            "HostKeyAlias agentspace-host.other-1",
        );
    assert_eq!(t, want);
}

#[test]
fn a_host_alias_error_message_names_no_path() {
    let e = host_alias_file_bytes(&host_file(
        PROXY_HOST,
        "C:/Users/someone/secret\nx",
        HOST_CERT,
        KNOWN_HOSTS,
    ))
    .unwrap_err();
    assert!(!e.0.is_empty());
    assert!(
        !e.0.contains("someone") && !e.0.contains("secret"),
        "{}",
        e.0
    );
    assert!(!e.0.contains('\n'));
}

// ---------- The doc's blocks, read at test time ----------

/// The first fenced block after `heading` that starts with `first_line`, as the doc writes it.
fn doc_block(heading: &str, first_line: &str) -> String {
    let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/state.md"))
        .unwrap();
    let from = doc
        .find(heading)
        .unwrap_or_else(|| panic!("state.md has no {heading:?}"));
    let start = from
        + doc[from..]
            .find(first_line)
            .unwrap_or_else(|| panic!("no block after {heading:?}"));
    let end = start + doc[start..].find("```").unwrap();
    doc[start..end].to_string()
}

#[test]
fn state_md_other_host_lease_block_is_what_the_test_transcribes() {
    let template = doc_block(
        "### A lease alias for a host that is not the box",
        "# agentspace lease <id>; <comment>\n",
    );
    let want = template
        .replace("<id>", ID)
        .replace("<comment>", COMMENT)
        .replace("<alias>", "agentspace-examplehost-3f2a9c1e")
        .replace("<user>", "ubuntu")
        .replace("<key path>", &lease_key("id_ed25519"))
        .replace("<certificate path>", &lease_key("id_ed25519-cert.pub"))
        .replace("<host name>", PROJECT)
        .replace("<known_hosts path>", KNOWN_HOSTS)
        .replace("<proxy command>", PROXY_LEASE);
    assert_eq!(want, OTHER_HOST_LEASE_BLOCK);
}

#[test]
fn state_md_host_block_is_what_the_test_transcribes() {
    // From v0.12 the block names the host by `<host name>` (D91; state.md, A host's platform and
    // names, pin P12), and its `<user>` is `ubuntu` on a Linux host. For a `linux/arm64` host
    // `<host name>` is the project, so the bytes are v0.11's.
    let template = doc_block(
        "### A host's operator alias",
        "# agentspace host <host name>; <comment>\n",
    );
    let want = template
        .replace("<host name>", PROJECT)
        .replace("<user>", "ubuntu")
        .replace("<comment>", COMMENT)
        .replace("<key path>", HOST_KEY)
        .replace("<certificate path>", HOST_CERT)
        .replace("<known_hosts path>", KNOWN_HOSTS)
        .replace("<proxy command>", PROXY_HOST);
    assert_eq!(want, HOST_BLOCK);
}

// ---------- A host of another platform (v0.12, D91; state.md pins P7 and P12) ----------
//
// state.md, A lease alias for a host that is not the box: `HostKeyAlias agentspace-host.<host
// name>`, where `<host name>` is `<project>.<os>-<arch>` for a host of a platform other than
// `linux/arm64`, and `<user>` is the lease's `ssh.user` (`agent` on `windows/amd64`). The lease's
// alias, its `Host` line and its file's name stay `agentspace-<project>-<first 8 hex digits>`.

const WINDOWS_HOST_NAME: &str = "examplehost.windows-amd64";
const WINDOWS_HOST_KEY_ALIAS: &str = "agentspace-host.examplehost.windows-amd64";
const LINUX_AMD64_HOST_KEY_ALIAS: &str = "agentspace-host.examplehost.linux-amd64";

const WINDOWS_LEASE_BLOCK: &str = "\
# agentspace lease 3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90; Written by agentspace. Do not edit.
Host agentspace-examplehost-3f2a9c1e
  User agent
  IdentityFile \"C:/Users/someone/.agentspace/leases/3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90/id_ed25519\"
  CertificateFile \"C:/Users/someone/.agentspace/leases/3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90/id_ed25519-cert.pub\"
  IdentitiesOnly yes
  HostKeyAlias agentspace-host.examplehost.windows-amd64
  UserKnownHostsFile \"C:/Users/someone/.agentspace/known_hosts\"
  StrictHostKeyChecking yes
  ProxyCommand \"C:\\agentspace\\agentspace-ssh-proxy.exe\" agentspace-examplehost-3f2a9c1e
  ServerAliveInterval 30
  ServerAliveCountMax 4
  RemoteForward 127.0.0.1:8811 127.0.0.1:8811
  RemoteForward 127.0.0.1:53147 127.0.0.1:53147
";

fn lease_on_platform<'a>(
    user: &'a str,
    host_key_alias: &'a str,
    key: &'a str,
    cert: &'a str,
) -> AliasFile<'a> {
    AliasFile {
        user,
        host_key_alias,
        ..other_host_lease(PROXY_LEASE, key, cert, KNOWN_HOSTS)
    }
}

#[test]
fn a_lease_on_a_windows_host_names_the_hosts_key_by_its_host_name_byte_for_byte() {
    let key = lease_key("id_ed25519");
    let cert = lease_key("id_ed25519-cert.pub");
    let f = lease_on_platform("agent", WINDOWS_HOST_KEY_ALIAS, &key, &cert);
    assert_eq!(text(alias_file_bytes(&f).unwrap()), WINDOWS_LEASE_BLOCK);
}

#[test]
fn state_md_other_host_lease_block_with_a_windows_host_name_is_that_block() {
    let template = doc_block(
        "### A lease alias for a host that is not the box",
        "# agentspace lease <id>; <comment>\n",
    );
    let want = template
        .replace("<id>", ID)
        .replace("<comment>", COMMENT)
        .replace("<alias>", "agentspace-examplehost-3f2a9c1e")
        .replace("<user>", "agent")
        .replace("<key path>", &lease_key("id_ed25519"))
        .replace("<certificate path>", &lease_key("id_ed25519-cert.pub"))
        .replace("<host name>", WINDOWS_HOST_NAME)
        .replace("<known_hosts path>", KNOWN_HOSTS)
        .replace("<proxy command>", PROXY_LEASE);
    assert_eq!(want, WINDOWS_LEASE_BLOCK);
}

#[test]
fn a_lease_on_a_windows_host_differs_from_one_on_an_arm64_host_in_the_user_and_the_key_name_alone()
{
    let key = lease_key("id_ed25519");
    let cert = lease_key("id_ed25519-cert.pub");
    let arm64 =
        text(alias_file_bytes(&other_host_lease(PROXY_LEASE, &key, &cert, KNOWN_HOSTS)).unwrap());
    let windows = text(
        alias_file_bytes(&lease_on_platform(
            "agent",
            WINDOWS_HOST_KEY_ALIAS,
            &key,
            &cert,
        ))
        .unwrap(),
    );
    let want = arm64.replace("  User ubuntu\n", "  User agent\n").replace(
        "  HostKeyAlias agentspace-host.examplehost\n",
        &format!("  HostKeyAlias {WINDOWS_HOST_KEY_ALIAS}\n"),
    );
    assert_ne!(want, arm64, "The test's own replacements must hit");
    assert_eq!(windows, want);
}

#[test]
fn a_lease_on_a_linux_amd64_host_keeps_ubuntu_and_names_its_platform_in_the_key() {
    let key = lease_key("id_ed25519");
    let cert = lease_key("id_ed25519-cert.pub");
    let t = text(
        alias_file_bytes(&lease_on_platform(
            "ubuntu",
            LINUX_AMD64_HOST_KEY_ALIAS,
            &key,
            &cert,
        ))
        .unwrap(),
    );
    assert!(t.contains("  User ubuntu\n"), "{t}");
    assert!(
        t.contains("  HostKeyAlias agentspace-host.examplehost.linux-amd64\n"),
        "{t}"
    );
    assert!(
        t.contains(&format!("  UserKnownHostsFile \"{KNOWN_HOSTS}\"\n")),
        "{t}"
    );
}

#[test]
fn the_lease_alias_and_its_host_line_name_no_platform() {
    // state.md pin P7: a lease's alias is the same on every platform.
    let key = lease_key("id_ed25519");
    let cert = lease_key("id_ed25519-cert.pub");
    let t = text(
        alias_file_bytes(&lease_on_platform(
            "agent",
            WINDOWS_HOST_KEY_ALIAS,
            &key,
            &cert,
        ))
        .unwrap(),
    );
    assert!(
        t.contains("\nHost agentspace-examplehost-3f2a9c1e\n"),
        "{t}"
    );
    assert!(
        t.starts_with("# agentspace lease 3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90; "),
        "{t}"
    );
}

#[test]
fn a_windows_hosts_key_alias_still_refuses_cr_or_lf() {
    let key = lease_key("id_ed25519");
    let cert = lease_key("id_ed25519-cert.pub");
    for bad in BAD_VALUES {
        let alias = format!("agentspace-host.examplehost.windows-amd64{bad}");
        let f = lease_on_platform("agent", &alias, &key, &cert);
        assert!(alias_file_bytes(&f).is_err(), "key alias {bad:?}");
    }
}
