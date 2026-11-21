//! No test of this crate changes its own process's environment (#0137).
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! **A test MUST NOT change the environment of the process that runs it.** The test harness runs
//! tests on several threads, and under edition 2024 a change to the environment is `unsafe`
//! because another thread's read of it can be undefined behaviour. A test that needs a process to
//! have a variable gives it to a process it starts, as a value of that child alone, or passes the
//! value to the code under test as an argument.
//!
//! **This scan reads the source of every file under `tests/`, this one included, and fails on the
//! name of either function that changes the environment, in any spelling:** a path to it, a
//! `use` of it, a call to it, or a renamed import of it, in code or in a comment. It looks for
//! the names, not for a full path, so no way of reaching them hides from it. The two names are
//! built from pieces below, so that this file does not hold them whole and the scan has no
//! exemption for itself.
//!
//! **The scan has a control**: it is run over a planted file in a throwaway folder under the
//! temp folder, written with each spelling, and it MUST find each one, or the scan has shown
//! nothing.
#![cfg(windows)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The two names, in pieces so that this file does not contain either whole.
fn names() -> [String; 2] {
    [
        format!("{}_{}", "set", "var"),
        format!("{}_{}", "remove", "var"),
    ]
}

/// Every file under `dir`, found by walking its folders.
fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Where each name appears in a file under `dir`: its path, its line number and which name it
/// is. A file that cannot be read is an offender too, as a line 0, since the scan did not see it.
fn offenders(dir: &Path) -> Vec<(PathBuf, usize, String)> {
    let mut files = Vec::new();
    files_under(dir, &mut files);
    files.sort();
    let mut found = Vec::new();
    for file in files {
        let Ok(bytes) = fs::read(&file) else {
            found.push((file, 0, "unreadable".to_string()));
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        for (i, line) in text.lines().enumerate() {
            for name in names() {
                if line.contains(&name) {
                    found.push((file.clone(), i + 1, name));
                }
            }
        }
    }
    found
}

#[test]
fn no_test_of_the_crate_changes_its_own_environment() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut files = Vec::new();
    files_under(&tests, &mut files);
    assert!(
        files.len() >= 10,
        "The scan reads the crate's tests, which are many files ({} found).",
        files.len()
    );
    assert!(
        files
            .iter()
            .any(|f| f.file_name().is_some_and(|n| n == "mod.rs")),
        "The scan reaches the files in the tests' folders."
    );
    let found = offenders(&tests);
    let report: Vec<String> = found
        .iter()
        .map(|(file, line, name)| {
            format!(
                "{} line {line}: {name}",
                file.strip_prefix(&tests).unwrap_or(file).display()
            )
        })
        .collect();
    assert!(
        found.is_empty(),
        "A test changes its own process's environment (#0137), which it MUST NOT: {}.",
        report.join("; ")
    );
}

#[test]
fn the_scan_finds_each_spelling_in_a_planted_file() {
    let [set, remove] = names();
    let dir = std::env::temp_dir().join(format!(
        "agentspace-launch-spec-scan-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("The clock is after the epoch.")
            .as_nanos()
    ));
    let nested = dir.join("nested");
    fs::create_dir_all(&nested).expect("The planted folder can be made.");

    let spellings = [
        format!("unsafe {{ std::env::{set}(\"A\", \"b\"); }}"),
        format!("unsafe {{ env::{set}(\"A\", \"b\"); }}"),
        format!("use std::env::{set};"),
        format!("use std::env::{{self, {remove} as gone}};"),
        format!("    {set} (\"A\", \"b\");"),
        format!("unsafe {{ std::env::{remove}(\"A\"); }}"),
        format!("// The old way was env::{set}."),
    ];
    for (i, spelling) in spellings.iter().enumerate() {
        // Some in the planted folder, some in a folder inside it.
        let folder = if i % 2 == 0 { &dir } else { &nested };
        fs::write(
            folder.join(format!("planted_{i}.rs")),
            format!("fn clean() {{}}\n{spelling}\nfn also_clean() {{}}\n"),
        )
        .expect("The planted file can be made.");
    }
    fs::write(dir.join("clean.rs"), "fn nothing_here() {}\n").expect("A clean file can be made.");

    let found = offenders(&dir);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        found.len(),
        spellings.len(),
        "The scan finds each planted spelling once, and not the clean file."
    );
    for i in 0..spellings.len() {
        let name = format!("planted_{i}.rs");
        assert!(
            found.iter().any(|(file, line, _)| file
                .file_name()
                .is_some_and(|n| n.to_str() == Some(name.as_str()))
                && *line == 2),
            "The scan finds the spelling planted in {name}, on its second line."
        );
    }
}
