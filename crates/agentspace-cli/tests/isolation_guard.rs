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
//! The child builds into its own target folder, so it never waits on the outer run's, and skips
//! this test so that it does not recurse.

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
    assert!(
        !home.exists(),
        "A test reached the process's own environment: the home that does not exist was created"
    );
}
