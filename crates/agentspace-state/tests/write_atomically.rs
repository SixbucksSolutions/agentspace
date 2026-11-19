//! `write_atomically` (docs/state.md, Replacing a file): the exact bytes in place of a file, in one
//! rename from a temporary file in the same folder, with nothing left behind.
//!
//! Runs on Linux and on Windows. Only throwaway folders under the temp folder are touched.

use std::fs;
use std::path::{Path, PathBuf};

use agentspace_state::write_atomically;

fn sandbox() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("agentspace-write-atomically-")
        .tempdir()
        .expect("A throwaway folder")
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[track_caller]
fn only(dir: &Path, name: &str) {
    assert_eq!(
        names_in(dir),
        [name.to_string()],
        "The folder holds the file and nothing else"
    );
}

fn target(t: &tempfile::TempDir) -> PathBuf {
    t.path().join("board-holder.json")
}

#[test]
fn a_new_file_gets_exactly_the_bytes() {
    let t = sandbox();
    let p = target(&t);
    write_atomically(&p, b"{\"pid\":1}\n").expect("The write succeeds");
    assert_eq!(fs::read(&p).unwrap(), b"{\"pid\":1}\n");
    only(t.path(), "board-holder.json");
}

#[test]
fn bytes_are_written_as_they_are_with_no_translation() {
    let t = sandbox();
    let p = target(&t);
    let bytes: Vec<u8> = vec![0xEF, 0xBB, 0xBF, 0, b'\r', b'\n', 0xFF, 0x1A, b'\n', 0, 0];
    write_atomically(&p, &bytes).unwrap();
    assert_eq!(fs::read(&p).unwrap(), bytes);
}

#[test]
fn every_byte_value_survives() {
    let t = sandbox();
    let p = target(&t);
    let bytes: Vec<u8> = (0..=255u8).cycle().take(70_000).collect();
    write_atomically(&p, &bytes).unwrap();
    assert_eq!(fs::read(&p).unwrap(), bytes);
}

#[test]
fn an_empty_write_makes_an_empty_file() {
    let t = sandbox();
    let p = target(&t);
    write_atomically(&p, b"").unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"");
    only(t.path(), "board-holder.json");
}

#[test]
fn an_existing_file_is_replaced_by_the_new_bytes() {
    let t = sandbox();
    let p = target(&t);
    fs::write(&p, b"old").unwrap();
    write_atomically(&p, b"new content, longer than the old").unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"new content, longer than the old");
    only(t.path(), "board-holder.json");
}

#[test]
fn a_shorter_write_leaves_none_of_the_longer_old_file() {
    let t = sandbox();
    let p = target(&t);
    fs::write(&p, vec![b'x'; 10_000]).unwrap();
    write_atomically(&p, b"short").unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"short");
}

#[test]
fn an_empty_write_replaces_a_full_file() {
    let t = sandbox();
    let p = target(&t);
    fs::write(&p, b"old").unwrap();
    write_atomically(&p, b"").unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"");
}

#[test]
fn repeated_replacement_leaves_no_temporary_file() {
    let t = sandbox();
    let p = target(&t);
    for i in 0..50 {
        write_atomically(&p, format!("{i}\n").as_bytes()).unwrap();
    }
    assert_eq!(fs::read(&p).unwrap(), b"49\n");
    only(t.path(), "board-holder.json");
}

#[test]
fn a_file_name_with_dots_and_a_hex_like_part_is_written_whole() {
    let t = sandbox();
    let p = t.path().join("board-holder-launch.0a1b2c3d.result.json");
    write_atomically(&p, b"{}\n").unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"{}\n");
    only(t.path(), "board-holder-launch.0a1b2c3d.result.json");
}

#[test]
fn a_neighbour_is_left_alone() {
    let t = sandbox();
    let p = target(&t);
    let other = t.path().join("leases.json");
    fs::write(&other, b"neighbour").unwrap();
    write_atomically(&p, b"mine").unwrap();
    assert_eq!(fs::read(&other).unwrap(), b"neighbour");
    assert_eq!(
        names_in(t.path()),
        ["board-holder.json".to_string(), "leases.json".to_string()]
    );
}

#[test]
fn a_missing_folder_is_an_error_and_is_not_made() {
    // Architect's ruling, 2026-11-19 (state.md, Replacing a file): a replace never creates a
    // folder. The caller makes it first; a missing one fails and leaves nothing behind.
    let t = sandbox();
    let dir = t.path().join("nope");
    let p = dir.join("board-holder.json");
    assert!(write_atomically(&p, b"x").is_err());
    assert!(!dir.exists(), "No folder was made");
    assert!(names_in(t.path()).is_empty(), "Nothing was left behind");
}

#[test]
fn a_folder_at_the_target_is_an_error_and_leaves_it_and_no_temporary_file() {
    let t = sandbox();
    let p = target(&t);
    fs::create_dir(&p).unwrap();
    fs::write(p.join("inside"), b"kept").unwrap();
    assert!(write_atomically(&p, b"x").is_err());
    assert!(p.is_dir(), "The folder is still a folder");
    assert_eq!(fs::read(p.join("inside")).unwrap(), b"kept");
    only(t.path(), "board-holder.json");
}

#[cfg(unix)]
#[test]
fn a_write_that_fails_leaves_the_old_file_whole_and_no_temporary_file() {
    use std::os::unix::fs::PermissionsExt;

    let t = sandbox();
    let p = target(&t);
    fs::write(&p, b"old").unwrap();
    // A folder that cannot be written to refuses the temporary file, before any rename.
    fs::set_permissions(t.path(), fs::Permissions::from_mode(0o555)).unwrap();
    let probe = t.path().join("probe");
    let can_write_anyway = fs::write(&probe, b"x").is_ok();
    let result = write_atomically(&p, b"new");
    fs::set_permissions(t.path(), fs::Permissions::from_mode(0o755)).unwrap();
    if can_write_anyway {
        // Running as a user the mode does not bind (root): nothing was provoked.
        return;
    }
    assert!(result.is_err(), "A write the folder refuses is an error");
    assert_eq!(fs::read(&p).unwrap(), b"old", "The old file is whole");
    only(t.path(), "board-holder.json");
}

#[test]
fn concurrent_writers_leave_one_whole_value_and_no_temporary_file() {
    let t = sandbox();
    let p = target(&t);
    let values: Vec<Vec<u8>> = (0..8u8).map(|i| vec![b'a' + i; 4096]).collect();
    std::thread::scope(|s| {
        for v in &values {
            let p = &p;
            s.spawn(move || {
                for _ in 0..20 {
                    // On Windows a rename can lose to another writer's open handle; a lost race is
                    // an error, never a torn file.
                    let _ = write_atomically(p, v);
                }
            });
        }
    });
    let got = fs::read(&p).unwrap();
    assert!(
        values.contains(&got),
        "The file is exactly one writer's bytes"
    );
    only(t.path(), "board-holder.json");
}

#[cfg(windows)]
#[test]
fn a_file_a_reader_holds_open_sharing_everything_is_still_replaced() {
    use std::os::windows::fs::OpenOptionsExt;

    let t = sandbox();
    let p = target(&t);
    fs::write(&p, b"old").unwrap();
    // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE.
    let _reader = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .open(&p)
        .unwrap();
    write_atomically(&p, b"new").unwrap();
    drop(_reader);
    assert_eq!(fs::read(&p).unwrap(), b"new");
    only(t.path(), "board-holder.json");
}
