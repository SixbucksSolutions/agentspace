//! The isolation guard (architect, 2026-11-19, after E0.05-T03-S02's tests ran T01's in-process
//! command lines against the real home).
//!
//! RFC 2119 keywords; the capitals are load-bearing.
//!
//! - **Every test that runs a command in process MUST do it through `execute_in`, with a World
//!   whose folders are under a fresh temp folder and whose `var` returns nothing from the real
//!   environment.**
//! - **This guard proves it for the whole crate:** it runs every one of the crate's test
//!   executables again, as children, with `USERPROFILE` and `HOME` pointed at a folder that does
//!   not exist and no `AGENTSPACE_*` variable set, and then shows that nothing created that
//!   folder. A test that reached the process's own environment would resolve its home there, and
//!   the lease state lock alone would create it.
//!
//! - **When the home is created, the failure message MUST list what was created under it**
//!   (E0.09-T03-S03), so that the next failure names its cause. The listing is every path under
//!   the moved home, relative to it, sorted, written with `/` whatever the platform. It MUST NOT
//!   print the value of any environment variable, and MUST NOT print any path outside the
//!   throwaway folder. A path that cannot be read is listed as `<path> (unreadable)`, and the
//!   walk MUST NOT follow a link.
//!
//! - **The toolchain MUST NOT run under the moved home** (#0209, E0.09-T03-S03). Cargo, rustup,
//!   rustc, rustdoc and the linker MUST be run with the real environment, and only the test
//!   executables they build MAY run with the moved one. Until #0209 the guard ran a child
//!   `cargo test` under the moved home. When that child had to link, the MSVC linker started its
//!   telemetry helper, which resolved the user's folders from the moved `USERPROFILE`, created
//!   `AppData` under the moved home, and held it open for minutes. The guard then failed with no
//!   test at fault.
//! - **So the guard builds first and runs second.** It builds the crate's test executables with
//!   `cargo test --no-run` and the real environment, reads their paths from cargo's JSON
//!   messages, and runs each one directly, from the crate's folder, as `cargo test` would. It
//!   MUST fail when the build fails or finds no test executable, and every executable MUST exit
//!   with success and report `test result: ok`.
//! - **`TMP` and `TEMP` MUST be set on each run to the guard's own values.** With neither set,
//!   Windows falls back to `USERPROFILE` for the temp folder, which is the moved home.
//! - **A test MUST NOT read a variable that only `cargo test` sets through `std::env` at run
//!   time**, since the executables are not run by cargo here. A compile-time `env!` is unaffected.
//! - **The throwaway folder is closed at the end.** A failure to delete it is added to the message
//!   of any assert that fails, and SHOULD NOT fail a run that otherwise passed: something outside
//!   the suite can hold a file in it open.
//!
//! The children build into their own target folder, so the build never waits on the outer run's,
//! and the guard's own executable skips this test in the child so that it does not recurse. The
//! listing's own test is cheap, starts nothing, and so runs in the child too.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Set on the child runs, so that the guard does not run inside itself.
const CHILD: &str = "AGENTSPACE_ISOLATION_GUARD_CHILD";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("The workspace root can be found")
}

fn crate_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .expect("The crate's folder can be found")
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

/// The last 40 lines of a child's output.
fn tail(b: &[u8]) -> String {
    let s = String::from_utf8_lossy(b);
    let lines: Vec<&str> = s.lines().collect();
    lines[lines.len().saturating_sub(40)..].join("\n")
}

/// The test executables of this crate in cargo's `--message-format=json` output: each
/// `compiler-artifact` built with the test profile, from this crate's manifest, that has an
/// executable. In the order cargo reported them, with no repeats.
fn test_executables(json_lines: &str, manifest: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for line in json_lines.lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if msg["reason"] != "compiler-artifact" || msg["profile"]["test"] != true {
            continue;
        }
        let ours = msg["manifest_path"]
            .as_str()
            .and_then(|m| Path::new(m).canonicalize().ok())
            .is_some_and(|m| m == manifest);
        if !ours {
            continue;
        }
        if let Some(exe) = msg["executable"].as_str() {
            let exe = PathBuf::from(exe);
            if !found.contains(&exe) {
                found.push(exe);
            }
        }
    }
    found
}

/// A file's own name, for a message: never the folder it is in.
fn short_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("?"))
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
fn the_test_executables_are_read_from_cargos_json_messages() {
    let manifest = crate_dir().join("Cargo.toml");
    let m = serde_json::to_string(&manifest.to_string_lossy()).expect("A path can be quoted");
    let other = serde_json::to_string(
        &workspace_root()
            .join("crates")
            .join("agentspace-state")
            .join("Cargo.toml")
            .to_string_lossy(),
    )
    .expect("A path can be quoted");
    let lines = [
        format!(
            r#"{{"reason":"compiler-artifact","manifest_path":{m},"profile":{{"test":true}},"executable":"C:/t/lib-1.exe"}}"#
        ),
        // The binary that `CARGO_BIN_EXE_*` names is built without the test profile.
        format!(
            r#"{{"reason":"compiler-artifact","manifest_path":{m},"profile":{{"test":false}},"executable":"C:/t/agentspace.exe"}}"#
        ),
        // Another crate's test executable is not this crate's.
        format!(
            r#"{{"reason":"compiler-artifact","manifest_path":{other},"profile":{{"test":true}},"executable":"C:/t/state-1.exe"}}"#
        ),
        format!(
            r#"{{"reason":"compiler-artifact","manifest_path":{m},"profile":{{"test":true}},"executable":null}}"#
        ),
        String::from("not json"),
        format!(
            r#"{{"reason":"compiler-artifact","manifest_path":{m},"profile":{{"test":true}},"executable":"C:/t/help-2.exe"}}"#
        ),
        String::from(r#"{"reason":"build-finished","success":true}"#),
    ]
    .join("\n");
    assert_eq!(
        test_executables(&lines, &manifest),
        [
            PathBuf::from("C:/t/lib-1.exe"),
            PathBuf::from("C:/t/help-2.exe")
        ],
        "Only this crate's test-profile executables MUST be found"
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

    // The build, with the real environment (#0209).
    let built = Command::new(env!("CARGO"))
        .current_dir(workspace_root())
        .args(["test", "-p", "agentspace-cli", "--locked", "--offline"])
        .args(["--no-run", "--message-format=json"])
        .arg("--target-dir")
        .arg(&target)
        .stdin(Stdio::null())
        .output()
        .expect("The build of the test executables can be started");
    let exes = if built.status.success() {
        test_executables(
            &String::from_utf8_lossy(&built.stdout),
            &crate_dir().join("Cargo.toml"),
        )
    } else {
        Vec::new()
    };

    // The runs, each with the home moved away and the temp folder kept.
    let temp_of = |var: &str| {
        std::env::var_os(var)
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| OsString::from(std::env::temp_dir()))
    };
    let (tmp, temp) = (temp_of("TMP"), temp_of("TEMP"));
    let mut failed = Vec::new();
    let mut unreported = Vec::new();
    for exe in &exes {
        let name = short_name(exe);
        let out = Command::new(exe)
            .current_dir(crate_dir())
            .env(CHILD, "1")
            .env("USERPROFILE", &home)
            .env("HOME", &home)
            .env("TMP", &tmp)
            .env("TEMP", &temp)
            .env_remove("AGENTSPACE_STATE_DIR")
            .env_remove("AGENTSPACE_SSH_DIR")
            .env_remove("AGENTSPACE_PROXY_COMMAND")
            .stdin(Stdio::null())
            .output();
        match out {
            Ok(out) if out.status.success() => {
                if !String::from_utf8_lossy(&out.stdout).contains("test result: ok") {
                    unreported.push(format!("{name}:\n{}", tail(&out.stdout)));
                }
            }
            Ok(out) => failed.push(format!(
                "{name}:\n{}\n{}",
                tail(&out.stdout),
                tail(&out.stderr)
            )),
            Err(e) => failed.push(format!("{name}: could not be started ({:?})", e.kind())),
        }
    }

    // The listing is taken before the throwaway folder is closed, since closing removes it.
    let home_created = home.exists();
    let created = created_under(&home);
    let note = match scratch.close() {
        Ok(()) => String::new(),
        // The kind only: the error's own text names the folder.
        Err(e) => format!(
            "\nThe throwaway folder could not be deleted afterwards ({:?}).",
            e.kind()
        ),
    };

    assert!(
        built.status.success(),
        "The crate's test executables MUST build with the real environment.\n{}{note}",
        tail(&built.stderr)
    );
    assert!(
        !exes.is_empty(),
        "The build MUST report at least one test executable of agentspace-cli.{note}"
    );
    assert!(
        failed.is_empty(),
        "The crate's suite MUST pass with the home moved away ({} of {} executables failed).\n{}{note}",
        failed.len(),
        exes.len(),
        failed.join("\n\n")
    );
    assert!(
        unreported.is_empty(),
        "Each child MUST have run its tests.\n{}{note}",
        unreported.join("\n\n")
    );
    assert!(!home_created, "{}{note}", created_message(&created));
}
