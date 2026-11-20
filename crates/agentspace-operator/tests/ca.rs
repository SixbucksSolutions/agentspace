//! `ca`, in a throwaway home (docs/operator.md: ca, The test seam; docs/state.md: The CA). The
//! operator calls Windows' own `ssh-keygen.exe` and sets access rules, so these runs use the real
//! `SystemRoot`, `USERNAME`, `USERDOMAIN` and `COMPUTERNAME` of the test process and nothing else
//! of its environment; everything `ca` writes is under the rig's throwaway `USERPROFILE`.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rig::*;

const SYNCED: &str = "the CA key MUST NOT live in a synced folder or in the repo.";

struct Paths {
    folder: PathBuf,
    key: PathBuf,
    public: PathBuf,
}

fn paths(rig: &Rig) -> Paths {
    let folder = rig.state_dir().join("ca");
    Paths {
        key: folder.join("agentspace_ca"),
        public: folder.join("agentspace_ca.pub"),
        folder,
    }
}

fn user() -> String {
    std::env::var("USERNAME").expect("USERNAME is set for the test process")
}

fn ssh_keygen() -> PathBuf {
    Path::new(&std::env::var_os("SystemRoot").expect("SystemRoot"))
        .join("System32")
        .join("OpenSSH")
        .join("ssh-keygen.exe")
}

fn icacls() -> PathBuf {
    Path::new(&std::env::var_os("SystemRoot").expect("SystemRoot"))
        .join("System32")
        .join("icacls.exe")
}

/// Runs `ca`. Returns `None`, saying why, when the test process is elevated: `ca` then refuses to
/// create a CA, and these tests have nothing to show.
fn make_ca(rig: &Rig, args: &[&str]) -> Option<Out> {
    let out = rig.op(args).real_windows().run();
    if out.err().contains("elevated shell") {
        eprintln!("skipped: the test process is elevated, so ca refuses to create a CA");
        return None;
    }
    Some(out)
}

fn make_ca_with(rig: &Rig, env: &[(&str, &str)]) -> Option<Out> {
    let mut op = rig.op(&["ca"]).real_windows();
    for (k, v) in env {
        op = op.env(k, v);
    }
    let out = op.run();
    if out.err().contains("elevated shell") {
        eprintln!("skipped: the test process is elevated, so ca refuses to create a CA");
        return None;
    }
    Some(out)
}

fn fingerprint_by_ssh_keygen(public: &Path) -> String {
    let out = Command::new(ssh_keygen())
        .arg("-l")
        .arg("-f")
        .arg(public)
        .stdin(Stdio::null())
        .output()
        .expect("ssh-keygen runs");
    assert!(out.status.success(), "ssh-keygen -l on the new public key");
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .nth(1)
        .expect("a fingerprint")
        .to_string()
}

fn result_lines(first: &str, p: &Paths, fp: &str) -> Vec<String> {
    vec![
        first.to_string(),
        format!("Private key  {}", p.key.display()),
        format!("Public key   {}", p.public.display()),
        format!("Fingerprint  {fp}"),
    ]
}

fn fp_of(out: &Out) -> String {
    out.err_lines()
        .iter()
        .find_map(|l| l.strip_prefix("Fingerprint  ").map(str::to_string))
        .unwrap_or_default()
}

// ---- Creating, and checking ----

#[test]
fn the_first_run_creates_the_ca_and_prints_four_lines() {
    let rig = Rig::new();
    let p = paths(&rig);
    let Some(out) = make_ca(&rig, &["ca"]) else {
        return;
    };
    assert_eq!(out.code, Some(0), "stderr {:?}", out.err());
    assert_eq!(out.out(), "", "ca writes nothing to stdout");
    let fp = fp_of(&out);
    assert!(
        fp.starts_with("SHA256:") && fp.len() == 7 + 43,
        "a SHA256 fingerprint, got {fp:?}"
    );
    let mut expected = vec![format!(
        "Creating the agentspace SSH CA in {}...",
        p.folder.display()
    )];
    expected.extend(result_lines("Created the agentspace SSH CA.", &p, &fp));
    assert_eq!(out.err_lines(), expected);
    assert_eq!(
        fp,
        fingerprint_by_ssh_keygen(&p.public),
        "the fingerprint is the public key's"
    );
    let public = std::fs::read_to_string(&p.public).expect("the public key");
    assert!(public.starts_with("ssh-ed25519 "), "an ed25519 key");
    assert_eq!(public.trim_end().lines().count(), 1, "one line");
    assert!(p.key.is_file());
}

#[test]
fn the_private_key_has_no_passphrase() {
    let rig = Rig::new();
    let p = paths(&rig);
    let Some(out) = make_ca(&rig, &["ca"]) else {
        return;
    };
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let derived = Command::new(ssh_keygen())
        .args(["-y", "-P", ""])
        .arg("-f")
        .arg(&p.key)
        .stdin(Stdio::null())
        .output()
        .expect("ssh-keygen runs");
    assert!(
        derived.status.success(),
        "ssh-keygen -y with an empty passphrase"
    );
    let derived = String::from_utf8_lossy(&derived.stdout).into_owned();
    let public = std::fs::read_to_string(&p.public).unwrap();
    let words = |t: &str| t.split_whitespace().take(2).collect::<Vec<_>>().join(" ");
    assert_eq!(
        words(&derived),
        words(&public),
        "the public key is the private key's own"
    );
}

#[test]
fn a_later_run_checks_prints_the_same_lines_and_changes_nothing() {
    let rig = Rig::new();
    let p = paths(&rig);
    let Some(first) = make_ca(&rig, &["ca"]) else {
        return;
    };
    assert_eq!(first.code, Some(0), "{:?}", first.err());
    let key = std::fs::read(&p.key).unwrap();
    let public = std::fs::read(&p.public).unwrap();
    let tree = rig.tree(&rig.profile);
    let second = make_ca(&rig, &["ca"]).unwrap();
    assert_eq!(second.code, Some(0), "{:?}", second.err());
    assert_eq!(second.out(), "");
    assert_eq!(
        second.err_lines(),
        result_lines(
            "The agentspace SSH CA already exists; nothing changed.",
            &p,
            &fp_of(&first)
        ),
        "no Creating line, and the existing CA's lines"
    );
    assert_eq!(
        std::fs::read(&p.key).unwrap(),
        key,
        "the private key is unchanged"
    );
    assert_eq!(
        std::fs::read(&p.public).unwrap(),
        public,
        "the public key is unchanged"
    );
    assert_eq!(rig.tree(&rig.profile), tree, "nothing was added");
}

#[test]
fn quiet_suppresses_all_four_lines_and_the_creating_line_but_still_creates() {
    let rig = Rig::new();
    let p = paths(&rig);
    let Some(out) = make_ca(&rig, &["ca", "-Quiet"]) else {
        return;
    };
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), "");
    assert_eq!(out.out(), "");
    assert!(
        p.key.is_file() && p.public.is_file(),
        "-Quiet still creates the CA"
    );
    let again = make_ca(&rig, &["-Quiet", "ca"]).unwrap();
    assert_eq!(again.code, Some(0));
    assert_eq!(again.err(), "");
}

#[test]
fn ca_starts_no_program_the_seam_names_and_writes_only_under_the_home() {
    let rig = Rig::new();
    let missing = rig
        .root()
        .join("no-such-program")
        .to_string_lossy()
        .into_owned();
    let Some(out) = make_ca_with(
        &rig,
        &[
            ("AGENTSPACE_AWS_CMD", &missing),
            ("AGENTSPACE_SSH_CMD", &missing),
            ("AGENTSPACE_SCP_CMD", &missing),
        ],
    ) else {
        return;
    };
    assert_eq!(
        out.code,
        Some(0),
        "a variable that no command reaches is never read: {:?}",
        out.err()
    );
    assert!(
        rig.calls().is_empty(),
        "ca makes no AWS call and touches no box"
    );
    for folder in [
        &rig.home,
        &rig.local_app_data,
        &rig.app_data,
        &rig.temp,
        &rig.work,
    ] {
        assert_eq!(
            rig.tree(folder),
            Vec::<String>::new(),
            "{}",
            folder.display()
        );
    }
}

// ---- Step 1: ssh-keygen.exe ----

#[test]
fn a_missing_ssh_keygen_fails_first_and_creates_nothing() {
    let rig = Rig::new();
    let out = rig.run(&["ca"]);
    assert_fails_with(&out, &expected_for(&rig), "no ssh-keygen");
    assert_eq!(out.err().lines().count(), 1);
    assert_eq!(
        rig.tree(&rig.profile),
        Vec::<String>::new(),
        "nothing is created"
    );

    // Anything at that path that is not a file counts as missing: a folder, for one.
    let rig = Rig::new();
    std::fs::create_dir_all(
        rig.system_root
            .join("System32")
            .join("OpenSSH")
            .join("ssh-keygen.exe"),
    )
    .unwrap();
    let out = rig.run(&["ca"]);
    assert_fails_with(&out, &expected_for(&rig), "a folder at ssh-keygen's path");
}

fn expected_for(rig: &Rig) -> String {
    format!(
        "agentspace: {} is missing; install the Windows OpenSSH client.",
        rig.system_root
            .join("System32")
            .join("OpenSSH")
            .join("ssh-keygen.exe")
            .display()
    )
}

#[test]
fn ssh_keygens_check_comes_before_the_folders() {
    // A CA folder inside a git working tree AND no ssh-keygen: the missing program is reported.
    let rig = Rig::new();
    std::fs::create_dir_all(rig.profile.join(".git")).unwrap();
    let out = rig.run(&["ca"]);
    assert_fails_with(&out, &expected_for(&rig), "step 1 before step 2");
}

// ---- Step 2: where the CA's folder is ----

#[test]
fn a_ca_folder_inside_a_onedrive_folder_is_refused_before_anything_is_made() {
    for variable in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        let rig = Rig::new();
        let p = paths(&rig);
        let synced = rig.profile.to_string_lossy().into_owned();
        let out = rig.op(&["ca"]).real_windows().env(variable, &synced).run();
        assert_fails_with(
            &out,
            &format!(
                "agentspace: {} is inside {synced}; {SYNCED}",
                p.folder.display()
            ),
            variable,
        );
        assert_eq!(
            rig.tree(&rig.profile),
            Vec::<String>::new(),
            "{variable}: nothing is created"
        );
    }
}

#[test]
fn the_onedrive_comparison_ignores_case_and_looks_at_whole_folders() {
    let rig = Rig::new();
    let upper = rig.profile.to_string_lossy().to_ascii_uppercase();
    let out = rig.op(&["ca"]).real_windows().env("OneDrive", &upper).run();
    assert_eq!(out.code, Some(1), "{:?}", out.err());
    let err = out.err();
    assert!(
        err.starts_with("agentspace: ")
            && err.contains(" is inside ")
            && err.trim_end().ends_with(SYNCED),
        "{err:?}"
    );
    assert_eq!(rig.tree(&rig.profile), Vec::<String>::new());

    // A folder whose name only starts like the home's is not the home, and an empty value is not
    // a folder at all: neither refuses.
    for kind in ["a sibling that starts alike", "an empty value"] {
        let rig = Rig::new();
        let value = if kind == "an empty value" {
            String::new()
        } else {
            rig.root().join("profil").to_string_lossy().into_owned()
        };
        let Some(out) = make_ca_with(&rig, &[("OneDrive", &value)]) else {
            return;
        };
        assert_eq!(out.code, Some(0), "{kind}: {:?}", out.err());
    }
}

#[test]
fn a_ca_folder_inside_a_git_working_tree_is_refused() {
    // The `.git` may be a folder, or the file a git worktree has; it may be at the home, or at
    // `.agentspace`; `<folder>` is the working tree's folder.
    for (at_state_folder, as_file) in [(false, false), (false, true), (true, false), (true, true)] {
        let rig = Rig::new();
        let p = paths(&rig);
        let tree = if at_state_folder {
            rig.state_dir()
        } else {
            rig.profile.clone()
        };
        std::fs::create_dir_all(&tree).unwrap();
        if as_file {
            std::fs::write(tree.join(".git"), "gitdir: elsewhere\n").unwrap();
        } else {
            std::fs::create_dir_all(tree.join(".git")).unwrap();
        }
        let before = rig.tree(&rig.profile);
        let out = rig.op(&["ca"]).real_windows().run();
        assert_fails_with(
            &out,
            &format!(
                "agentspace: {} is inside {}; {SYNCED}",
                p.folder.display(),
                tree.display()
            ),
            &format!("state folder {at_state_folder}, file {as_file}"),
        );
        assert_eq!(rig.tree(&rig.profile), before, "nothing is created");
    }
}

// ---- Step 3: a half-made CA ----

#[test]
fn a_private_key_without_its_public_key_is_a_half_made_ca_and_is_not_repaired() {
    let rig = Rig::new();
    let p = paths(&rig);
    std::fs::create_dir_all(&p.folder).unwrap();
    std::fs::write(&p.key, "not a key, but a file\n").unwrap();
    let out = rig.op(&["ca"]).real_windows().run();
    assert_fails_with(
        &out,
        &format!(
            "agentspace: {} exists but {} does not. ca does not repair a half-made CA: restore the public key, or write it again from the private key with ssh-keygen -y, then run ca again.",
            p.key.display(),
            p.public.display()
        ),
        "private key only",
    );
    assert!(!p.public.exists(), "ca never repairs");
    assert_eq!(
        std::fs::read_to_string(&p.key).unwrap(),
        "not a key, but a file\n"
    );
}

#[test]
fn a_public_key_without_its_private_key_is_a_lost_ca_and_is_not_replaced() {
    let rig = Rig::new();
    let p = paths(&rig);
    std::fs::create_dir_all(&p.folder).unwrap();
    std::fs::write(&p.public, "ssh-ed25519 AAAA somebody\n").unwrap();
    let out = rig.op(&["ca"]).real_windows().run();
    assert_fails_with(
        &out,
        &format!(
            "agentspace: {} exists but {} does not: the CA's private key is lost, and nothing more can be signed with it. ca does not replace it on its own. To make a new CA, delete {} and run ca again, then run sync for every host and re-issue every lease's certificate.",
            p.public.display(),
            p.key.display(),
            p.public.display()
        ),
        "public key only",
    );
    assert!(!p.key.exists(), "ca never replaces");
    assert_eq!(
        std::fs::read_to_string(&p.public).unwrap(),
        "ssh-ed25519 AAAA somebody\n"
    );
}

#[test]
fn a_failure_prints_even_under_quiet() {
    let rig = Rig::new();
    let p = paths(&rig);
    std::fs::create_dir_all(&p.folder).unwrap();
    std::fs::write(&p.key, "x").unwrap();
    let out = rig.op(&["ca", "-Quiet"]).real_windows().run();
    assert_eq!(out.code, Some(1));
    assert!(out.err().starts_with("agentspace: "), "{:?}", out.err());
    assert_eq!(out.err().lines().count(), 1);
    assert_eq!(out.out(), "");
}

// ---- Step 5: checking an existing CA ----

#[test]
fn a_public_key_that_is_not_the_private_keys_own_fails_and_is_not_repaired() {
    let rig = Rig::new();
    let p = paths(&rig);
    let Some(first) = make_ca(&rig, &["ca"]) else {
        return;
    };
    assert_eq!(first.code, Some(0), "{:?}", first.err());
    // Another CA's public key, over this one's.
    let other = Rig::new();
    let q = paths(&other);
    assert_eq!(make_ca(&other, &["ca"]).unwrap().code, Some(0));
    let theirs = std::fs::read(&q.public).unwrap();
    std::fs::write(&p.public, &theirs).unwrap();
    let out = rig.op(&["ca"]).real_windows().run();
    assert_fails_with(
        &out,
        &format!(
            "agentspace: {} is not the ed25519 public key of {}. ca does not repair a mismatched pair.",
            p.public.display(),
            p.key.display()
        ),
        "a mismatched pair",
    );
    assert_eq!(
        std::fs::read(&p.public).unwrap(),
        theirs,
        "ca changes nothing"
    );
}

fn loosen(path: &Path) {
    let granted = Command::new(icacls())
        .arg(path)
        .args(["/grant", "*S-1-1-0:R"])
        .stdin(Stdio::null())
        .output()
        .expect("icacls runs");
    assert!(
        granted.status.success(),
        "icacls could not loosen the throwaway file"
    );
}

#[test]
fn a_private_key_that_others_can_read_fails_with_the_access_rules_message() {
    let rig = Rig::new();
    let p = paths(&rig);
    let Some(first) = make_ca(&rig, &["ca"]) else {
        return;
    };
    assert_eq!(first.code, Some(0), "{:?}", first.err());
    let key = std::fs::read(&p.key).unwrap();
    loosen(&p.key);
    let out = rig.op(&["ca"]).real_windows().run();
    assert_eq!(out.code, Some(1), "{:?}", out.err());
    assert_eq!(out.out(), "");
    let err = out.err();
    let head = format!(
        "agentspace: {} MUST be {}'s alone, but it ",
        p.key.display(),
        user()
    );
    assert!(
        err.starts_with(&head),
        "the message names the key and the user"
    );
    assert!(
        err.contains("has a rule allowing "),
        "the problem: a rule that allows another"
    );
    assert!(
        err.contains(" ca does not change access rules on an existing CA. If anyone else may have read the key, replace the CA (D17)."),
        "ca's own advice"
    );
    assert!(err.trim_end().ends_with("and run ca again."));
    assert_eq!(err.lines().count(), 1);
    assert_eq!(
        std::fs::read(&p.key).unwrap(),
        key,
        "ca never repairs or overwrites a CA"
    );
}

#[test]
fn a_ca_folder_that_others_can_read_is_reported_before_the_key() {
    let rig = Rig::new();
    let p = paths(&rig);
    let Some(first) = make_ca(&rig, &["ca"]) else {
        return;
    };
    assert_eq!(first.code, Some(0), "{:?}", first.err());
    loosen(&p.folder);
    let out = rig.op(&["ca"]).real_windows().run();
    assert_eq!(out.code, Some(1), "{:?}", out.err());
    let head = format!(
        "agentspace: {} MUST be {}'s alone, but it ",
        p.folder.display(),
        user()
    );
    assert!(
        out.err().starts_with(&head),
        "the folder's check comes first: {:?}",
        out.err()
    );
}

#[test]
fn a_files_in_the_way_of_the_ca_folder_does_not_panic_and_fails_with_an_agentspace_line() {
    let rig = Rig::new();
    std::fs::create_dir_all(rig.state_dir()).unwrap();
    std::fs::write(
        rig.state_dir().join("ca"),
        "a file where the folder has to be",
    )
    .unwrap();
    let out = rig.op(&["ca"]).real_windows().run();
    assert_eq!(out.code, Some(1), "{:?}", out.err());
    // The whole line, with the system's own words alone: Rust's ` (os error <n>)` is dropped
    // (operator.md, Exit codes: architect, 2026-11-19). The throwaway path, which carries the
    // Windows user name, is masked before anything is compared or shown.
    let folder = rig.state_dir().join("ca").display().to_string();
    let last = out.err_lines().last().cloned().unwrap_or_default();
    let shown = last.replace(&folder, "<ca folder>");
    let rust = std::io::Error::from_raw_os_error(183).to_string();
    let system = rust
        .strip_suffix(" (os error 183)")
        .expect("Rust's suffix on ERROR_ALREADY_EXISTS")
        .trim();
    assert_eq!(
        shown,
        format!("agentspace: Could not create <ca folder>: {system}")
    );
    assert!(!out.err().contains("os error"));
    assert!(!out.err().contains("panicked"));
    assert_eq!(out.out(), "");
}
