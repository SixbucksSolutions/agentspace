//! No test of this crate changes its own process's environment (#0137): a source scan.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! **No test in this crate MUST change its own process's environment.** Rust's test harness runs
//! tests on several threads, and a concurrent read of the environment is undefined behaviour once
//! another thread writes it. A test passes the throwaway home to the code under test as a value, or
//! to a child process it starts through `Command::env`. The scan reads every file under `tests` and
//! under `src`, whatever its extension, and fails on a line that names either function, in any
//! spelling: a call, a path, a `use`. It builds its own needles so that it does not find itself.

use std::fs;
use std::path::{Path, PathBuf};

/// The two functions' names, joined here so that this file holds neither.
fn needles() -> [String; 2] {
    [
        concat!("set", "_var").to_string(),
        concat!("remove", "_var").to_string(),
    ]
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Every line under `dir` that names either function, as the file and its 1-based line number.
fn offenders(dir: &Path) -> Vec<(PathBuf, usize)> {
    let needles = needles();
    let mut files = Vec::new();
    files_under(dir, &mut files);
    let mut found = Vec::new();
    for file in files {
        let Ok(bytes) = fs::read(&file) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        for (index, line) in text.lines().enumerate() {
            if needles.iter().any(|n| line.contains(n.as_str())) {
                found.push((file.clone(), index + 1));
            }
        }
    }
    found
}

fn crate_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(name)
}

#[test]
fn no_file_of_the_crate_changes_the_process_environment() {
    for name in ["tests", "src"] {
        let dir = crate_dir(name);
        let mut files = Vec::new();
        files_under(&dir, &mut files);
        assert!(!files.is_empty(), "The scan found no file under {name}");
        let found = offenders(&dir);
        // The first offender as a path relative to the crate's folder and its line number: an
        // absolute path holds the user's name, which a failure message MUST NOT print.
        let first = found.first().map(|(path, line)| {
            let relative = path
                .strip_prefix(env!("CARGO_MANIFEST_DIR"))
                .unwrap_or(path.as_path());
            format!("{}:{line}", relative.display())
        });
        assert!(
            found.is_empty(),
            "{} line(s) under {name} change the process environment, first at {}",
            found.len(),
            first.unwrap_or_default()
        );
    }
}

#[test]
fn the_scan_finds_every_spelling_in_a_planted_file_and_nothing_in_a_clean_one() {
    let [set, remove] = needles();
    let dir = tempfile::tempdir().expect("A throwaway folder");
    let nested = dir.path().join("nested");
    fs::create_dir_all(&nested).expect("A throwaway subfolder");
    fs::write(
        dir.path().join("planted.rs"),
        format!(
            "fn a() {{ unsafe {{ std::env::{set}(\"X\", \"1\") }} }}\n\
             fn b() {{ unsafe {{ env::{remove}(\"X\") }} }}\n\
             use std::env::{{{set} as renamed}};\n\
             fn c() {{}}\n"
        ),
    )
    .expect("The planted file");
    fs::write(
        nested.join("planted.txt"),
        format!("x\n  {remove}(\"Y\")\n"),
    )
    .expect("A planted file");
    fs::write(
        dir.path().join("clean.rs"),
        "fn d() { std::process::Command::new(\"x\").env(\"A\", \"b\"); }\n",
    )
    .expect("The clean file");

    let mut planted: Vec<(String, usize)> = offenders(dir.path())
        .iter()
        .map(|(p, n)| (p.file_name().unwrap().to_string_lossy().into_owned(), *n))
        .collect();
    planted.sort();
    assert_eq!(
        planted,
        vec![
            ("planted.rs".to_string(), 1),
            ("planted.rs".to_string(), 2),
            ("planted.rs".to_string(), 3),
            ("planted.txt".to_string(), 2),
        ]
    );
    let clean = tempfile::tempdir().expect("A throwaway folder");
    fs::write(clean.path().join("clean.rs"), "fn d() {}\n").expect("The clean file");
    assert!(offenders(clean.path()).is_empty());
}
