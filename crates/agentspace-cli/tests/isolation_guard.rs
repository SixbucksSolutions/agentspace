//! The isolation guard (architect, 2026-11-19, after E0.05-T03-S02's tests ran T01's in-process
//! command lines against the real home).
//!
//! RFC 2119 keywords; the capitals are load-bearing.
//!
//! - **Every test that runs a command in process MUST do it through `execute_in`, with a World
//!   whose folders are under a fresh temp folder and whose `var` returns nothing from the real
//!   environment.**
//! - **This guard proves it for the whole crate:** it runs the crate's entire suite again, as a
//!   child `cargo test`, with `USERPROFILE` and `HOME` pointed at a folder that does not exist and
//!   no `AGENTSPACE_*` variable set, and then shows that nothing created that folder. A test that
//!   reached the process's own environment would resolve its home there, and the lease state lock
//!   alone would create it.
//!
//! - **When the home is created, the failure message MUST list what was created under it**
//!   (E0.09-T03-S03), so that the next failure names its cause. The listing is every path under
//!   the moved home, relative to it, sorted, written with `/` whatever the platform. It MUST NOT
//!   print the value of any environment variable, and MUST NOT print any path outside the
//!   throwaway folder. A path that cannot be read is listed as `<path> (unreadable)`, and the
//!   walk MUST NOT follow a link.
//!
//! The child builds into its own target folder, so it never waits on the outer run's, and skips
//! this test so that it does not recurse. The listing's own test is cheap, starts no cargo, and
//! so runs in the child too.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Set on the child run, so that the guard does not run inside itself.
const CHILD: &str = "AGENTSPACE_ISOLATION_GUARD_CHILD";

/// Where the toolchain lives, read before the child's profile is moved: `CARGO_HOME` and
/// `RUSTUP_HOME` when set, else their defaults under the real profile. Only the two folder names
/// are read here, never anything of agentspace's.
fn tool_home(var: &str, default_leaf: &str) -> Option<OsString> {
    if let Some(v) = std::env::var_os(var).filter(|v| !v.is_empty()) {
        return Some(v);
    }
    let profile = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(profile).join(default_leaf).into_os_string())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("The workspace root can be found")
}

/// Every path under `home`, relative to it and sorted, with `/` between the parts. A folder is
/// listed as well as what it holds. A link is listed and never followed. What cannot be read is
/// listed as `<path> (unreadable)`, and the walk goes on. A home that does not exist lists
/// nothing, and a home that cannot be opened is listed as `. (unreadable)`.
fn created_under(home: &Path) -> Vec<String> {
    let mut found = Vec::new();
    match std::fs::read_dir(home) {
        Ok(entries) => walk(home, entries, &mut found),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => found.push(String::from(". (unreadable)")),
    }
    found.sort();
    found
}

/// Adds each entry of one folder to `found`, and descends into each real folder.
fn walk(home: &Path, entries: std::fs::ReadDir, found: &mut Vec<String>) {
    for entry in entries {
        let Ok(entry) = entry else {
            // An entry that cannot be read has no name to report.
            found.push(String::from("? (unreadable)"));
            continue;
        };
        let path = entry.path();
        let rel = path
            .strip_prefix(home)
            .unwrap_or(&path)
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        // `DirEntry::file_type` does not follow a link, so a link is never a folder here.
        match entry.file_type() {
            Ok(t) if t.is_dir() => {
                found.push(rel.clone());
                match std::fs::read_dir(&path) {
                    Ok(inner) => walk(home, inner, found),
                    Err(_) => found.push(format!("{rel} (unreadable)")),
                }
            }
            Ok(_) => found.push(rel),
            Err(_) => found.push(format!("{rel} (unreadable)")),
        }
    }
}

/// The failure message for a home that was created: the cause, then what was created under it.
fn created_message(created: &[String]) -> String {
    let mut m = String::from(
        "A test reached the process's own environment: the home that does not exist was created",
    );
    if created.is_empty() {
        m.push_str(".\nNothing is under it.");
    } else {
        m.push_str(&format!(
            ".\nCreated under it, relative to it ({} paths):",
            created.len()
        ));
        for p in created {
            m.push_str("\n  ");
            m.push_str(p);
        }
    }
    m
}

#[test]
fn the_failure_message_names_what_was_created_under_the_moved_home_relative_to_it() {
    let scratch = tempfile::Builder::new()
        .prefix("agentspace-isolation-listing-")
        .tempdir()
        .expect("A throwaway folder can be made");
    let home = scratch.path().join("no-such-home");
    assert!(
        created_under(&home).is_empty(),
        "A home that does not exist MUST list nothing"
    );

    std::fs::create_dir_all(home.join("nested").join("deeper")).expect("The folders can be made");
    std::fs::write(home.join("top.txt"), b"x").expect("The file can be written");
    std::fs::write(home.join("nested").join("deeper").join("leaf.txt"), b"x")
        .expect("The nested file can be written");

    let created = created_under(&home);
    assert_eq!(
        created,
        [
            "nested",
            "nested/deeper",
            "nested/deeper/leaf.txt",
            "top.txt"
        ],
        "The listing MUST hold every planted path, relative, sorted, with one separator"
    );

    let message = created_message(&created);
    for p in &created {
        assert!(
            message.contains(p.as_str()),
            "The failure message MUST name {p}\n{message}"
        );
    }
    assert!(
        message.contains("(4 paths)"),
        "The failure message MUST count the paths\n{message}"
    );
    assert!(
        !message.contains(scratch.path().to_string_lossy().as_ref()),
        "The failure message MUST NOT name the throwaway folder's own path"
    );
    assert!(
        created_message(&[]).contains("Nothing is under it"),
        "An empty home MUST say so"
    );
}

#[test]
fn the_whole_suite_creates_nothing_in_a_home_that_does_not_exist() {
    if std::env::var_os(CHILD).is_some() {
        return;
    }
    let scratch = tempfile::Builder::new()
        .prefix("agentspace-isolation-guard-")
        .tempdir()
        .expect("A throwaway folder can be made");
    assert!(
        scratch.path().starts_with(std::env::temp_dir()),
        "The throwaway folder MUST be under the temp folder"
    );
    let home = scratch.path().join("no-such-home");
    assert!(!home.exists());

    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target"))
        .join("isolation-guard");

    let mut child = Command::new(env!("CARGO"));
    child
        .current_dir(workspace_root())
        .args(["test", "-p", "agentspace-cli", "--locked", "--offline"])
        .arg("--target-dir")
        .arg(&target)
        .env(CHILD, "1")
        .env("USERPROFILE", &home)
        .env("HOME", &home)
        .env_remove("AGENTSPACE_STATE_DIR")
        .env_remove("AGENTSPACE_SSH_DIR")
        .env_remove("AGENTSPACE_PROXY_COMMAND")
        .env_remove("CARGO_TARGET_DIR");
    for (var, leaf) in [("CARGO_HOME", ".cargo"), ("RUSTUP_HOME", ".rustup")] {
        if let Some(v) = tool_home(var, leaf) {
            child.env(var, v);
        }
    }
    let out = child.output().expect("The child cargo test can be started");
    let tail = |b: &[u8]| {
        let s = String::from_utf8_lossy(b);
        let lines: Vec<&str> = s.lines().collect();
        lines[lines.len().saturating_sub(40)..].join("\n")
    };
    assert!(
        out.status.success(),
        "The crate's suite MUST pass with the home moved away.\n{}\n{}",
        tail(&out.stdout),
        tail(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("test result: ok"),
        "The child MUST have run the suite.\n{}",
        tail(&out.stdout)
    );
    assert!(!home.exists(), "{}", created_message(&created_under(&home)));
}
