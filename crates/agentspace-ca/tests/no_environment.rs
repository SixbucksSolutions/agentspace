//! agentspace-ca reads no environment variable (operator.md, ca, Rulings for the rewrite's ca:
//! architect, 2026-11-19, for E0.06-T02-S01): `USERNAME`, `COMPUTERNAME` and the OneDrive values
//! reach it in a `CaEnv`.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. This test scans the library's source,
//! every `src/*.rs`, for a read of the environment, and MUST fail when one is there. A scan that
//! finds nothing proves nothing until it has found something, so the pattern is first run against
//! a planted line of each form.
//!
//! **One file is left out: `src/tests.rs`**, the unit tests' module, compiled only under
//! `cfg(test)`. It reads `SystemRoot` to find Windows OpenSSH's `ssh-keygen.exe` for its oracle
//! test, and this machine's names only to hide them in a failure (`masked`). It never reads them
//! for the library: every `CaEnv` there is built from made-up names.

use std::fs;
use std::path::Path;

/// The forms of an environment read the scan looks for.
const READS: [&str; 5] = ["std::env::var", "env::var", "var_os", "vars(", "vars_os"];

/// The unit tests' module, which is not the library (see the module's comment).
const LEFT_OUT: &str = "tests.rs";

/// The 1-based numbers of the lines of `text` that read the environment.
fn reads_in(text: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| READS.iter().any(|form| line.contains(form)))
        .map(|(at, _)| at + 1)
        .collect()
}

#[test]
fn the_pattern_finds_a_planted_read_of_each_form() {
    let planted = [
        "let a = std::env::var(\"X\");",
        "let b = env::var(\"X\");",
        "let c = std::env::var_os(\"X\");",
        "use std::env::var_os;",
        "for (k, v) in std::env::vars() {}",
        "for (k, v) in std::env::vars_os() {}",
    ];
    for line in planted {
        let text = format!("fn f() {{}}\n{line}\nfn g() {{}}\n");
        assert_eq!(reads_in(&text), vec![2], "The scan missed a planted read");
    }
    // A temp folder, a path and a struct named for the environment are not reads.
    let clean = "let t = std::env::temp_dir();\nlet e = CaEnv::default();\nlet p = env.user();\n";
    assert!(
        reads_in(clean).is_empty(),
        "The scan found a read that is not one"
    );
}

#[test]
fn the_library_source_reads_no_environment_variable() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut scanned = 0;
    let mut found = Vec::new();
    for entry in fs::read_dir(&src).expect("The crate's src folder could not be listed") {
        let path = entry.expect("An entry of src could not be read").path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if path.extension().is_none_or(|e| e != "rs") || name == LEFT_OUT {
            continue;
        }
        let text = fs::read_to_string(&path).expect("A source file could not be read");
        scanned += 1;
        for line in reads_in(&text) {
            found.push(format!("src/{name}:{line}"));
        }
    }
    // The library's own files: lib.rs, access.rs, keys.rs, place.rs, win.rs and other.rs.
    assert!(scanned >= 6, "The scan read only {scanned} source files");
    assert!(
        found.is_empty(),
        "The library reads the environment at {}",
        found.join(", ")
    );
}
