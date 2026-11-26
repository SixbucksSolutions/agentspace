//! A host's operator alias for a host of another platform (docs/state.md, A host's operator alias
//! and A host's platform and names; D91, D95; T01's pins P12 and P17; E0.12-T02-S01, card #0250):
//! the owner mark, the `Host` name and the `HostKeyAlias` carry `<host name>`, which is
//! `<project>.<os>-<arch>` for a platform other than `linux/arm64`, and `<user>` is `agent` on a
//! `windows/amd64` host and `ubuntu` on either Linux platform.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. This file is apart from `host_alias.rs`
//! on purpose: the story's card names that file's two lease-alias reds alone, and whether the
//! operator alias of a host of another platform belongs to this story or to the operator's is the
//! architect's to confirm. These tests pass the host name in `HostAliasFile::project`, the one
//! field that names the host today (N-list, the test author's report).

use agentspace_alias::*;

const PROJECT: &str = "examplehost";
const COMMENT: &str = "Written by agentspace. Do not edit.";
const PROXY_HOST: &str = "\"C:\\agentspace\\agentspace-ssh-proxy.exe\" agentspace-host.examplehost";
const KNOWN_HOSTS: &str = "C:/Users/someone/.agentspace/known_hosts";
const HOST_KEY: &str = "C:/Users/someone/.agentspace/hosts/examplehost/id_ed25519";
const HOST_CERT: &str = "C:/Users/someone/.agentspace/hosts/examplehost/id_ed25519-cert.pub";
const WINDOWS_HOST_NAME: &str = "examplehost.windows-amd64";

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

fn host_default() -> HostAliasFile<'static> {
    HostAliasFile {
        project: PROJECT,
        comment: COMMENT,
        key_path: HOST_KEY,
        cert_path: HOST_CERT,
        known_hosts: KNOWN_HOSTS,
        proxy_command: PROXY_HOST,
    }
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap()
}

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

fn host_named(name: &str) -> HostAliasFile<'_> {
    HostAliasFile {
        project: name,
        ..host_default()
    }
}

#[test]
fn the_operator_alias_of_a_windows_host_names_it_by_its_host_name() {
    // state.md: the owner mark, the `Host` name and the `HostKeyAlias` carry `<host name>`.
    let t = text(host_alias_file_bytes(&host_named(WINDOWS_HOST_NAME)).unwrap());
    let lines: Vec<&str> = t.lines().collect();
    assert_eq!(
        lines[0],
        "# agentspace host examplehost.windows-amd64; Written by agentspace. Do not edit."
    );
    assert_eq!(lines[1], "Host agentspace-host.examplehost.windows-amd64");
    assert!(
        t.contains("  HostKeyAlias agentspace-host.examplehost.windows-amd64\n"),
        "{t}"
    );
    assert!(
        t.contains(&format!("  UserKnownHostsFile \"{KNOWN_HOSTS}\"\n")),
        "{t}"
    );
    assert!(t.contains("  StrictHostKeyChecking accept-new\n"), "{t}");
    assert_eq!(lines.len(), 12);
}

#[test]
fn the_operator_alias_of_a_windows_host_is_the_arm64_block_with_its_name_changed_alone() {
    // state.md's `<user>` for a host's operator alias is `agent` on a `windows/amd64` host, from
    // the host's platform and never stored (D95, pin P17); the host's name carries the platform.
    let t = text(host_alias_file_bytes(&host_named(WINDOWS_HOST_NAME)).unwrap());
    let want = HOST_BLOCK
        .replace("  User ubuntu\n", "  User agent\n")
        .replace(
            "# agentspace host examplehost;",
            "# agentspace host examplehost.windows-amd64;",
        )
        .replace(
            "Host agentspace-host.examplehost\n",
            "Host agentspace-host.examplehost.windows-amd64\n",
        )
        .replace(
            "HostKeyAlias agentspace-host.examplehost\n",
            "HostKeyAlias agentspace-host.examplehost.windows-amd64\n",
        );
    assert_ne!(want, HOST_BLOCK);
    assert_eq!(t, want);
}

#[test]
fn state_md_host_block_with_a_windows_host_name_is_that_block() {
    let template = doc_block(
        "### A host's operator alias",
        "# agentspace host <host name>; <comment>\n",
    );
    let want = template
        .replace("<host name>", WINDOWS_HOST_NAME)
        .replace("<user>", "agent")
        .replace("<comment>", COMMENT)
        .replace("<key path>", HOST_KEY)
        .replace("<certificate path>", HOST_CERT)
        .replace("<known_hosts path>", KNOWN_HOSTS)
        .replace("<proxy command>", PROXY_HOST);
    let t = text(host_alias_file_bytes(&host_named(WINDOWS_HOST_NAME)).unwrap());
    assert_eq!(t, want);
}

#[test]
fn the_operator_alias_of_a_linux_amd64_host_keeps_ubuntu_and_names_its_platform() {
    let t = text(host_alias_file_bytes(&host_named("examplehost.linux-amd64")).unwrap());
    let want = HOST_BLOCK
        .replace(
            "# agentspace host examplehost;",
            "# agentspace host examplehost.linux-amd64;",
        )
        .replace(
            "Host agentspace-host.examplehost\n",
            "Host agentspace-host.examplehost.linux-amd64\n",
        )
        .replace(
            "HostKeyAlias agentspace-host.examplehost\n",
            "HostKeyAlias agentspace-host.examplehost.linux-amd64\n",
        );
    assert_ne!(want, HOST_BLOCK);
    assert_eq!(t, want);
    assert!(t.contains("  User ubuntu\n"), "{t}");
}

#[test]
fn a_windows_host_operator_alias_is_never_a_lease_owner_mark() {
    let bytes = host_alias_file_bytes(&host_named(WINDOWS_HOST_NAME)).unwrap();
    assert_eq!(owner_mark(&bytes), None);
}

#[test]
fn a_host_name_that_breaks_the_alias_rule_is_still_refused() {
    for bad in [
        "examplehost.windows amd64",
        "examplehost.windows/amd64",
        "examplehost;windows-amd64",
        "examplehost.windows-amd64\n",
        "examplehost.windows-amd64%",
    ] {
        assert!(
            host_alias_file_bytes(&host_named(bad)).is_err(),
            "host name {bad:?}"
        );
    }
}
