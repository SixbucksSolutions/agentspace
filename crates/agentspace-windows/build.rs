//! Embeds the repository's `box/windows` in the crate at build time (docs/operator.md, sync on a
//! Windows host; D96, D101).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! It writes `box_windows.rs` into `OUT_DIR`: one `(relative path, include_bytes!)` pair per file
//! under `box/windows`, whole and recursively, sorted by path, with `/` between a path's parts,
//! but for the top-level `tests` folder, which `sync` MUST NOT carry (operator.md, The host
//! agent's files: the tests are not installed). The files are carried byte for byte as they are in
//! the checkout; this script MUST NOT change, filter or read them as anything but bytes. Which
//! files exist is read here at each build, so the host agent's files, which E0.12-T03-S07 writes,
//! arrive by merge with no change to this crate.

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let windows = manifest.join("..").join("..").join("box").join("windows");
    // Cargo scans a folder named here for any change in it, so a new, changed or removed file
    // rebuilds the crate.
    println!("cargo:rerun-if-changed={}", windows.display());
    println!("cargo:rerun-if-changed=build.rs");

    let mut files = Vec::new();
    walk(&windows, "", &mut files);
    files.sort();
    assert!(!files.is_empty(), "box/windows holds no file");

    let mut out = String::from(
        "/// `box/windows`'s files but its `tests` folder, as `(path relative to box/windows with \
         '/', bytes)`, sorted by path.\n\
         pub(crate) const BOX_WINDOWS: &[(&str, &[u8])] = &[\n",
    );
    for (relative, full) in &files {
        println!("cargo:rerun-if-changed={}", full.display());
        let full = full
            .to_str()
            .expect("a box/windows path that is not Unicode");
        out.push_str(&format!("    ({relative:?}, include_bytes!({full:?})),\n"));
    }
    out.push_str("];\n");

    let dest = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR")).join("box_windows.rs");
    fs::write(dest, out).expect("write box_windows.rs");
}

/// Every file under `dir`, as its path relative to `box/windows` and its full path, the top-level
/// `tests` folder left out.
fn walk(dir: &Path, prefix: &str, files: &mut Vec<(String, PathBuf)>) {
    let entries = fs::read_dir(dir).expect("read box/windows");
    for entry in entries {
        let entry = entry.expect("read box/windows entry");
        let name = entry
            .file_name()
            .into_string()
            .expect("a box/windows name that is not Unicode");
        let kind = entry.file_type().expect("box/windows entry type");
        if prefix.is_empty() && name == "tests" && kind.is_dir() {
            continue;
        }
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let path = entry.path();
        if kind.is_dir() {
            walk(&path, &relative, files);
        } else {
            files.push((relative, path));
        }
    }
}
