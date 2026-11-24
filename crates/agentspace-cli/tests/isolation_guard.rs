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
//! - **Every test that the crate's `cargo test` runs MUST be run, and counted** (P48,
//!   E0.09-T03-S03), so that a skipped executable or test cannot pass silently. The executables
//!   run and counted MUST be exactly the build's list, the test executables in cargo's JSON. Each
//!   executable is first run with `--list`, in the same moved environment as its run, so the
//!   count is of the same program in the same environment; a listing runs no test, and anything
//!   it created would be caught like the run's. Its run MUST then report `passed` plus `ignored`
//!   equal to the tests the listing names. A failed count is reported by the executable's file
//!   name only.
//! - **The guard does not run doctests**, which `cargo test` runs for the library through
//!   rustdoc. The library holds none (2026-11-23), so `cargo test` runs none for it.
//! - **`TMP` and `TEMP` MUST be set on each run to the guard's own values.** With neither set,
//!   Windows falls back to `USERPROFILE` for the temp folder, which is the moved home.
//! - **A test MUST NOT read a variable that only `cargo test` sets through `std::env` at run
//!   time**, since the executables are not run by cargo here. Each one inherits the guard's own
//!   environment, which holds cargo's run-time variables for the guard's target, not its own. No
//!   test reads one (2026-11-23). A compile-time `env!` is unaffected.
//! - **The throwaway folder is closed at the end.** A failure to delete it is added to the message
//!   of any assert that fails, naming the folder by its own name in the temp folder (P49), with
//!   the error's kind and never its text, which holds the full path. It SHOULD NOT fail a run that
//!   otherwise passed: something outside the suite can hold a file in it open.
//!
//! The children build into their own target folder, so the build never waits on the outer run's,
//! and the guard's own executable skips this test in the child so that it does not recurse. Its
//! other tests, of the listing, the JSON reading, the count check and the note, are cheap, start
//! nothing, and so run in the child too.

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

/// The number of tests a test executable's `--list` output names: each line ending in `: test`.
fn listed_tests(listing: &str) -> usize {
    listing.lines().filter(|l| l.ends_with(": test")).count()
}

/// The passed and ignored counts of a run's last `test result: ok.` line, or `None` when its last
/// `test result:` line is not `ok` or does not carry both counts.
fn result_counts(stdout: &str) -> Option<(usize, usize)> {
    let line = stdout
        .lines()
        .rev()
        .find(|l| l.starts_with("test result:"))?;
    if !line.starts_with("test result: ok.") {
        return None;
    }
    let (mut passed, mut ignored) = (None, None);
    for part in line.split(';') {
        let words: Vec<&str> = part.split_whitespace().collect();
        if let [.., n, kind] = words.as_slice() {
            match *kind {
                "passed" => passed = n.parse().ok(),
                "ignored" => ignored = n.parse().ok(),
                _ => {}
            }
        }
    }
    Some((passed?, ignored?))
}

/// The count check (P48): the run of the executable `name` MUST report passed plus ignored equal
/// to the tests its listing names. The message names the executable by its file name only.
fn count_check(name: &str, listing: &str, stdout: &str) -> Result<(), String> {
    let listed = listed_tests(listing);
    match result_counts(stdout) {
        Some((passed, ignored)) if passed + ignored == listed => Ok(()),
        Some((passed, ignored)) => Err(format!(
            "{name}: its listing names {listed} tests, and its run reports {passed} passed and \
             {ignored} ignored"
        )),
        None => Err(format!(
            "{name}: its listing names {listed} tests, and its run reports no `test result: ok.` \
             line with passed and ignored counts"
        )),
    }
}

/// The note for a throwaway folder that could not be deleted (P49): its own name in the temp
/// folder and the error's kind, never the error's text, which holds the full path.
fn undeleted_note(name: &str, kind: std::io::ErrorKind) -> String {
    format!(
        "\nThe throwaway folder {name}, in the temp folder, could not be deleted afterwards \
         ({kind:?})."
    )
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
fn the_count_check_fails_when_a_run_reports_fewer_tests_than_its_listing_names() {
    let listing = "a::one: test\na::two: test\nb::three: test\n\n3 tests, 0 benchmarks\n";
    let run = |summary: &str| format!("\nrunning 3 tests\n...\n\n{summary}\n\n");

    let two = run(
        "test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; \
         finished in 0.01s",
    );
    let err = count_check("lib-1.exe", listing, &two)
        .expect_err("A listing of 3 tests against a run of 2 passed MUST fail");
    assert!(
        err.contains("lib-1.exe") && err.contains("names 3 tests") && err.contains("2 passed"),
        "The failure MUST name the executable and both counts\n{err}"
    );

    let filtered = run(
        "test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; \
         1 filtered out; finished in 0.01s",
    );
    assert!(
        count_check("lib-1.exe", listing, &filtered).is_err(),
        "A run that filtered a test out MUST fail"
    );
    assert!(
        count_check("lib-1.exe", listing, "running 3 tests\n").is_err(),
        "A run with no summary MUST fail"
    );
    let not_ok = run(
        "test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; \
         0 filtered out; finished in 0.01s",
    );
    assert!(
        count_check("lib-1.exe", listing, &not_ok).is_err(),
        "A run whose summary is not ok MUST fail"
    );

    let all = run(
        "test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; \
         finished in 0.01s",
    );
    assert_eq!(
        count_check("lib-1.exe", listing, &all),
        Ok(()),
        "Passed plus ignored equal to the listing MUST pass"
    );
    assert_eq!(
        count_check(
            "empty-1.exe",
            "\n0 tests, 0 benchmarks\n",
            &run(
                "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; \
                 finished in 0.00s"
            )
        ),
        Ok(()),
        "An executable with no tests MUST pass with a run of none"
    );
}

#[test]
fn the_undeleted_folder_is_named_by_its_own_name_and_the_errors_kind() {
    let note = undeleted_note(
        "agentspace-isolation-guard-AbC123",
        std::io::ErrorKind::PermissionDenied,
    );
    assert!(
        note.contains("agentspace-isolation-guard-AbC123") && note.contains("PermissionDenied"),
        "The note MUST name the folder and the error's kind\n{note}"
    );
    assert!(
        !note.contains(std::env::temp_dir().to_string_lossy().as_ref()),
        "The note MUST NOT name the temp folder's own path"
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
    // Every `AGENTSPACE_*` variable of the guard's own environment, by name only, to be removed
    // from each run; Windows matches a variable's name without regard to case.
    let agentspace_vars: Vec<OsString> = std::env::vars_os()
        .map(|(name, _)| name)
        .filter(|name| {
            name.to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("AGENTSPACE_")
        })
        .collect();
    // The listing and the run of one executable, both in the moved environment.
    let child = |exe: &Path, list: bool| {
        let mut c = Command::new(exe);
        if list {
            c.arg("--list");
        }
        for name in &agentspace_vars {
            c.env_remove(name);
        }
        c.current_dir(crate_dir())
            .env(CHILD, "1")
            .env("USERPROFILE", &home)
            .env("HOME", &home)
            .env("TMP", &tmp)
            .env("TEMP", &temp)
            .stdin(Stdio::null())
            .output()
    };
    let mut failed = Vec::new();
    let mut miscounted = Vec::new();
    let mut counted = Vec::new();
    for exe in &exes {
        let name = short_name(exe);
        let listing = match child(exe, true) {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).into_owned(),
            Ok(out) => {
                failed.push(format!(
                    "{name} --list:\n{}\n{}",
                    tail(&out.stdout),
                    tail(&out.stderr)
                ));
                continue;
            }
            Err(e) => {
                failed.push(format!("{name}: could not be started ({:?})", e.kind()));
                continue;
            }
        };
        match child(exe, false) {
            Ok(out) if out.status.success() => {
                match count_check(&name, &listing, &String::from_utf8_lossy(&out.stdout)) {
                    Ok(()) => counted.push(exe.clone()),
                    Err(m) => miscounted.push(m),
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
    let uncounted: Vec<String> = exes
        .iter()
        .filter(|e| !counted.contains(e))
        .map(|e| short_name(e))
        .collect();

    // The listing is taken before the throwaway folder is closed, since closing removes it.
    let home_created = home.exists();
    let created = created_under(&home);
    let scratch_name = short_name(scratch.path());
    let note = match scratch.close() {
        Ok(()) => String::new(),
        Err(e) => undeleted_note(&scratch_name, e.kind()),
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
        miscounted.is_empty(),
        "Each executable MUST run every test its listing names.\n{}{note}",
        miscounted.join("\n")
    );
    assert!(
        counted.len() == exes.len() && uncounted.is_empty(),
        "Every executable of the build's list MUST be run and counted ({} of {}); not: {}.{note}",
        counted.len(),
        exes.len(),
        uncounted.join(", ")
    );
    assert!(!home_created, "{}{note}", created_message(&created));
}
