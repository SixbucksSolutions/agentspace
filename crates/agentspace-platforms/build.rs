//! Embeds the repository's `box\system` in the seam's Linux sync row at build time (docs/operator.md,
//! The platform seam, the sync row; D37's addendum, D101).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! It writes `box_system.rs` into `OUT_DIR`: one `(relative path, include_bytes!)` pair per file
//! under `box\system`, whole and recursively, sorted by path, with `/` between a path's parts, as
//! `agentspace-sync`'s build script does for today's sync. The files are carried byte for byte as
//! they are in the repository; this script MUST NOT change, filter or read them as anything but
//! bytes.

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let system = manifest.join("..").join("..").join("box").join("system");
    // Cargo scans a folder named here for any change in it, so a new, changed or removed file
    // rebuilds the crate.
    println!("cargo:rerun-if-changed={}", system.display());
    println!("cargo:rerun-if-changed=build.rs");

    let mut files = Vec::new();
    walk(&system, "", &mut files);
    files.sort();
    assert!(!files.is_empty(), "box/system holds no file");

    let mut out = String::from(
        "/// `box\\system`'s files, as `(path relative to box\\system with '/', bytes)`.\n\
         pub(crate) const BOX_SYSTEM: &[(&str, &[u8])] = &[\n",
    );
    for (relative, full) in &files {
        println!("cargo:rerun-if-changed={}", full.display());
        let full = full
            .to_str()
            .expect("a box/system path that is not Unicode");
        out.push_str(&format!("    ({relative:?}, include_bytes!({full:?})),\n"));
    }
    out.push_str("];\n");

    let dest = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR")).join("box_system.rs");
    fs::write(dest, out).expect("write box_system.rs");
}

/// Every file under `dir`, as its path relative to `box\system` and its full path.
fn walk(dir: &Path, prefix: &str, files: &mut Vec<(String, PathBuf)>) {
    let entries = fs::read_dir(dir).expect("read box/system");
    for entry in entries {
        let entry = entry.expect("read box/system entry");
        let name = entry
            .file_name()
            .into_string()
            .expect("a box/system name that is not Unicode");
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let path = entry.path();
        let kind = entry.file_type().expect("box/system entry type");
        if kind.is_dir() {
            walk(&path, &relative, files);
        } else {
            files.push((relative, path));
        }
    }
}
