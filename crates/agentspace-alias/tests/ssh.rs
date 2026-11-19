//! OpenSSH reads a generated alias file (docs/state.md, "The alias file" and "The `Include` line").
//! The scratch folder is under the temp folder, and HOME points into it, so ssh never reads the
//! real `~/.ssh`. The scratch config sits in HOME's `.ssh` and holds the relative `Include` line, as
//! OpenSSH resolves a relative `Include` against the user's own `~/.ssh` (state.md, The `Include`
//! line).

use agentspace_alias::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const ID: &str = "3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90";

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    let p = std::env::temp_dir().join(format!(
        "agentspace-alias-test-{}-{tag}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    Scratch(p)
}

fn ssh_g(home: &Path, config: &Path, alias: &str) -> String {
    let out = Command::new("ssh")
        .env("HOME", home)
        .arg("-G")
        .arg("-F")
        .arg(config)
        .arg(alias)
        .output()
        .expect("ssh must be installed to run this test");
    assert!(
        out.status.success(),
        "ssh -G failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn lines(out: &str, key: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| l.strip_prefix(&format!("{key} ")))
        .map(str::to_string)
        .collect()
}

fn insert_bytes(c: &[u8]) -> Vec<u8> {
    match include_plan(c) {
        IncludePlan::Insert(v) => v,
        _ => panic!("expected Insert"),
    }
}

struct World {
    home: PathBuf,
    config: PathBuf,
    alias: String,
    key: String,
    cert: String,
}

fn write_world(s: &Scratch, proxy: &str) -> World {
    let home = s.0.clone();
    let sshdir = home.join(".ssh");
    fs::create_dir_all(sshdir.join("agentspace.d")).unwrap();
    let home_s = home.to_str().unwrap().to_string();
    let key = lease_file_text(&home_s, ID, "id_ed25519");
    let cert = lease_file_text(&home_s, ID, "id_ed25519-cert.pub");
    let alias = alias_name("swimcoach", ID);
    let bytes = alias_file_bytes(&AliasFile {
        id: ID,
        comment: "Written by agentspace. Do not edit.",
        alias: &alias,
        user: "ubuntu",
        key_path: &key,
        cert_path: &cert,
        proxy_command: proxy,
    })
    .unwrap();
    fs::write(sshdir.join("agentspace.d").join(&alias), bytes).unwrap();
    let config = sshdir.join("config");
    fs::write(&config, insert_bytes(b"")).unwrap();
    World {
        home,
        config,
        alias,
        key,
        cert,
    }
}

#[test]
fn openssh_reads_a_generated_alias_file() {
    let s = scratch("g");
    let proxy = "\"/opt/agentspace/agentspace-ssh-proxy\" agentspace-swimcoach-3f2a9c1e";
    let w = write_world(&s, proxy);
    let out = ssh_g(&w.home, &w.config, &w.alias);
    assert_eq!(lines(&out, "user"), ["ubuntu"], "{out}");
    assert_eq!(
        lines(&out, "identityfile"),
        std::slice::from_ref(&w.key),
        "{out}"
    );
    assert_eq!(
        lines(&out, "certificatefile"),
        std::slice::from_ref(&w.cert),
        "{out}"
    );
    assert_eq!(lines(&out, "proxycommand"), [proxy], "{out}");
    assert_eq!(lines(&out, "hostkeyalias"), ["agentspace"], "{out}");
    assert_eq!(lines(&out, "identitiesonly"), ["yes"], "{out}");
    assert_eq!(lines(&out, "stricthostkeychecking"), ["true"], "{out}");
    assert_eq!(lines(&out, "serveraliveinterval"), ["30"], "{out}");
    assert_eq!(lines(&out, "serveralivecountmax"), ["4"], "{out}");
    assert_eq!(
        lines(&out, "remoteforward"),
        [
            "[127.0.0.1]:8811 [127.0.0.1]:8811",
            "[127.0.0.1]:53147 [127.0.0.1]:53147"
        ],
        "{out}"
    );
}

#[test]
fn openssh_reads_a_proxy_command_with_spaces_as_one_value() {
    let s = scratch("sp");
    let proxy = "\"/opt/a b/p\" agentspace-swimcoach-3f2a9c1e";
    let w = write_world(&s, proxy);
    let out = ssh_g(&w.home, &w.config, &w.alias);
    assert_eq!(lines(&out, "proxycommand"), [proxy], "{out}");
}

#[test]
fn openssh_does_not_apply_the_block_to_another_alias() {
    let s = scratch("other");
    let w = write_world(&s, "\"/x/p\" a");
    let out = ssh_g(&w.home, &w.config, "agentspace-other-00000000");
    assert!(lines(&out, "proxycommand").is_empty(), "{out}");
    assert!(lines(&out, "remoteforward").is_empty(), "{out}");
}

#[test]
fn openssh_reads_an_inserted_include_line_in_a_config_with_a_host_block() {
    let s = scratch("ins");
    let w = write_world(&s, "\"/x/p\" a");
    let original = b"Host other\r\n  User someone\r\n";
    fs::write(&w.config, insert_bytes(original)).unwrap();
    let out = ssh_g(&w.home, &w.config, &w.alias);
    assert_eq!(lines(&out, "user"), ["ubuntu"], "{out}");
    let out = ssh_g(&w.home, &w.config, "other");
    assert_eq!(lines(&out, "user"), ["someone"], "{out}");
}
