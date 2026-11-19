//! Windows OpenSSH reads what `ensure_include` and `write_alias` wrote (docs/state.md, "The alias
//! file" and "The `Include` line").
//!
//! `ssh -G` only prints the configuration; nothing connects. Every run names its config with `-F`
//! and the child's profile variables point into the scratch folder, so the real `~/.ssh` is never
//! read. state.md says a relative `Include` in a `-F` config resolves against the user's real
//! `~/.ssh`, so the test's own top-level config, passed with `-F`, holds an ABSOLUTE `Include` of
//! the throwaway `agentspace.d`. `ensure_include`'s own config is checked by its bytes and not
//! given to ssh. The home folder's path holds a space and a `%`.
#![cfg(windows)]

mod common;

use agentspace_alias::*;
use common::*;
use std::fs;
use std::path::Path;
use std::process::Command;

fn windows_ssh() -> std::path::PathBuf {
    let root = std::env::var("SystemRoot").expect("SystemRoot is set on Windows");
    let p = Path::new(&root)
        .join("System32")
        .join("OpenSSH")
        .join("ssh.exe");
    assert!(p.is_file(), "Windows OpenSSH is not installed");
    p
}

fn ssh_g(home: &Path, config: &Path, alias: &str) -> String {
    let out = Command::new(windows_ssh())
        .env("USERPROFILE", home)
        .env("HOME", home)
        .arg("-G")
        .arg("-F")
        .arg(config)
        .arg(alias)
        .output()
        .expect("ssh.exe must run");
    assert!(
        out.status.success(),
        "ssh -G failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn values(out: &str, key: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| l.strip_prefix(&format!("{key} ")))
        .map(str::to_string)
        .collect()
}

/// A path the way ssh prints it back: forward slashes, `%` single (ssh reads `%%` as `%`).
fn slashed(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

#[test]
fn openssh_reads_the_written_alias_for_a_home_with_a_space_and_a_percent() {
    let s = scratch("ssh");
    let home = s.0.join("home dir %7 x");
    make_openssh_safe(&home);
    let dir = home.join(".ssh");
    assert!(home.display().to_string().contains(' '));
    assert!(home.display().to_string().contains('%'));

    // The program is found beside the running executable, with a space and a `%` in its path.
    let bin = home.join("bin dir %7");
    fs::create_dir_all(&bin).unwrap();
    let own = bin.join("agentspace.exe");
    fs::write(&own, b"").unwrap();
    fs::write(bin.join("agentspace-ssh-proxy.exe"), b"").unwrap();
    let program = find_proxy_program(&own).expect("the proxy program");
    let program_s = program.display().to_string();
    let proxy = default_proxy_command(
        &program_s,
        ALIAS,
        std::env::var("SystemRoot").ok().as_deref(),
    )
    .unwrap();

    ensure_include(&dir, NOW).unwrap();
    let bytes = alias_bytes(&home, ID, ALIAS, &proxy);
    write_alias(&dir, ALIAS, ID, &bytes).unwrap();

    // The config agentspace wrote holds the relative line, first.
    assert_eq!(
        fs::read(dir.join("config")).unwrap(),
        b"Include agentspace.d/*\n"
    );

    // The test's own top-level config reaches the same agentspace.d by an absolute Include.
    let top = home.join("top-config");
    let d = slashed(&dir.join("agentspace.d"));
    fs::write(&top, format!("Include \"{d}/*\"\n")).unwrap();

    let out = ssh_g(&home, &top, ALIAS);
    assert_eq!(values(&out, "user"), ["ubuntu"], "{out}");
    // `ssh -G` prints values as the file holds them, each `%` still doubled (ssh reads `%%` as one
    // `%` only when it connects), so the file's own text is what is compared, exactly.
    let home_s = home.display().to_string();
    assert_eq!(
        values(&out, "identityfile"),
        [lease_file_text(&home_s, ID, "id_ed25519")],
        "{out}"
    );
    assert!(lease_file_text(&home_s, ID, "id_ed25519").contains("%%7"));
    assert_eq!(
        values(&out, "certificatefile"),
        [lease_file_text(&home_s, ID, "id_ed25519-cert.pub")],
        "{out}"
    );
    assert_eq!(values(&out, "hostkeyalias"), ["agentspace"], "{out}");
    // The ProxyCommand is the quoted program, backslashes kept, each `%` doubled, then the alias
    // written literally (never `%n`).
    assert!(proxy.contains('%') && !proxy.contains("%%"));
    assert_eq!(
        values(&out, "proxycommand"),
        [proxy.replace('%', "%%")],
        "{out}"
    );
    assert!(proxy.ends_with(&format!(" {ALIAS}")));
    assert_eq!(values(&out, "identitiesonly"), ["yes"], "{out}");
    assert_eq!(values(&out, "stricthostkeychecking"), ["true"], "{out}");
}

#[test]
fn openssh_reads_the_alias_through_the_cmd_form_too() {
    let s = scratch("ssh2");
    let home = s.0.join("home dir %7 x");
    make_openssh_safe(&home);
    let dir = home.join(".ssh");
    let bin = home.join("bin dir %7");
    fs::create_dir_all(&bin).unwrap();
    let own = bin.join("agentspace.exe");
    fs::write(&own, b"").unwrap();
    fs::write(bin.join("agentspace-ssh-proxy.cmd"), b"").unwrap();
    let program = find_proxy_program(&own).unwrap();
    let proxy = default_proxy_command(
        &program.display().to_string(),
        ALIAS,
        std::env::var("SystemRoot").ok().as_deref(),
    )
    .unwrap();
    ensure_include(&dir, NOW).unwrap();
    write_alias(&dir, ALIAS, ID, &alias_bytes(&home, ID, ALIAS, &proxy)).unwrap();
    let top = home.join("top-config");
    fs::write(
        &top,
        format!("Include \"{}/*\"\n", slashed(&dir.join("agentspace.d"))),
    )
    .unwrap();
    let out = ssh_g(&home, &top, ALIAS);
    let got = values(&out, "proxycommand");
    assert_eq!(got.len(), 1, "{out}");
    assert!(got[0].contains("cmd.exe /d /c"), "{}", got[0]);
    assert_eq!(got[0], proxy.replace('%', "%%"));
    assert!(got[0].ends_with(&format!(" {ALIAS}")), "{}", got[0]);
    assert_eq!(values(&out, "user"), ["ubuntu"], "{out}");
}
