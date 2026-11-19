//! Reading and replacing `leases.json` (docs/state.md, Replacing a file; The lease state file).
#![cfg(windows)]

mod common;
mod disk_support;

use std::time::{Duration, Instant};

use agentspace_state::{StoreError, lock, read, write};
use common::*;
use disk_support::{Deny, hold, open_sharing_everything, sandbox, state_of, temp_files_in};

fn describe<T>(r: &Result<T, StoreError>) -> String {
    match r {
        Ok(_) => "Ok".to_string(),
        Err(e) => format!("{e:?}"),
    }
}

// ---- read ----

#[test]
fn a_missing_file_is_no_leases() {
    let t = sandbox();
    let f = read(t.path()).expect("A missing file is no leases");
    assert!(f.leases.is_empty());
}

#[test]
fn a_missing_state_folder_is_no_leases_and_is_not_created() {
    let t = sandbox();
    let dir = t.path().join("nope");
    let f = read(&dir).expect("A missing folder is no leases");
    assert!(f.leases.is_empty());
    assert!(!dir.exists());
}

#[test]
fn a_folder_at_the_file_is_unreadable() {
    let t = sandbox();
    std::fs::create_dir(t.path().join("leases.json")).unwrap();
    let r = read(t.path());
    assert!(
        matches!(r, Err(StoreError::Unreadable(_))),
        "Got {}",
        describe(&r)
    );
}

#[test]
fn an_unreadable_message_names_no_path() {
    let t = sandbox();
    std::fs::create_dir(t.path().join("leases.json")).unwrap();
    match read(t.path()) {
        Err(StoreError::Unreadable(why)) => {
            assert!(!why.contains(t.path().to_str().unwrap()), "Was: {why}");
            assert!(!why.is_empty());
        }
        _ => panic!("Expected Unreadable"),
    }
}

#[test]
fn a_file_that_does_not_validate_is_not_valid() {
    let t = sandbox();
    std::fs::write(t.path().join("leases.json"), b"{\"version\":0}").unwrap();
    let r = read(t.path());
    assert!(
        matches!(r, Err(StoreError::NotValid(_))),
        "Got {}",
        describe(&r)
    );
}

#[test]
fn a_file_on_disk_reads_back() {
    let t = sandbox();
    std::fs::write(
        t.path().join("leases.json"),
        bytes_of(vec![ready(ID_A, "demo"), pending(ID_B, "other")]),
    )
    .unwrap();
    let f = read(t.path()).unwrap();
    assert_eq!(f.leases.len(), 2);
    assert_eq!(f.leases[0].id, ID_A);
}

#[test]
fn read_shares_read_write_and_delete_with_another_holder() {
    let t = sandbox();
    let path = t.path().join("leases.json");
    std::fs::write(&path, bytes_of(vec![ready(ID_A, "demo")])).unwrap();
    // Another process holds the file open for writing, sharing nothing it could be asked to
    // share but read, write and delete: the reader must still get in.
    let _child = hold(&path, "Open", "ReadWrite", "ReadWrite, Delete", 60);
    let f =
        read(t.path()).expect("A reader must not be blocked by a holder that shares everything");
    assert_eq!(f.leases.len(), 1);
}

#[test]
fn read_opens_with_every_share_so_a_writer_is_never_blocked_by_it() {
    // A held read handle is not observable after the call returns, so this checks the visible
    // effect: reading leaves the file replaceable and deletable at once.
    let t = sandbox();
    let path = t.path().join("leases.json");
    std::fs::write(&path, bytes_of(vec![ready(ID_A, "demo")])).unwrap();
    read(t.path()).unwrap();
    let g = lock(t.path()).unwrap();
    write(t.path(), &state_of(vec![]), &g).expect("A replace must follow a read");
}

#[test]
fn a_file_that_cannot_be_opened_is_unreadable_and_is_not_no_leases() {
    let t = sandbox();
    let path = t.path().join("leases.json");
    std::fs::write(&path, bytes_of(vec![ready(ID_A, "demo")])).unwrap();
    let _deny = Deny::on(&path);
    let r = read(t.path());
    assert_eq!(
        r.as_ref().err(),
        Some(&StoreError::Unreadable(
            "the state file could not be opened".to_string()
        )),
        "Got {}",
        describe(&r)
    );
}

#[test]
fn a_file_where_the_state_folder_must_be_is_unreadable_with_its_why() {
    let t = sandbox();
    let f = t.path().join("a-file");
    std::fs::write(&f, b"x").unwrap();
    assert_eq!(
        read(&f).unwrap_err(),
        StoreError::Unreadable("the state folder is a file".to_string())
    );
}

// ---- write ----

#[test]
fn a_file_where_the_state_folder_must_be_is_unwritable_with_its_why() {
    let t = sandbox();
    let g = lock(t.path()).unwrap();
    let f = t.path().join("a-file");
    std::fs::write(&f, b"x").unwrap();
    assert_eq!(
        write(&f, &state_of(vec![]), &g).unwrap_err(),
        StoreError::Unwritable("the state folder is a file".to_string())
    );
    assert_eq!(std::fs::read(&f).unwrap(), b"x");
}

#[test]
fn a_reader_that_opened_the_old_file_before_the_write_still_reads_it_whole_after_the_rename() {
    use std::io::Read;
    let t = sandbox();
    let path = t.path().join("leases.json");
    let old = bytes_of(vec![ready(ID_A, "demo")]);
    std::fs::write(&path, &old).unwrap();
    // A reader sharing read, write and delete, opened before the write and kept open across it.
    let mut reader = open_sharing_everything(&path);
    let g = lock(t.path()).unwrap();
    write(t.path(), &state_of(vec![pending(ID_B, "other")]), &g)
        .expect("A reader sharing delete must not stop the rename");
    // A fresh read gets the new bytes.
    let fresh = read(t.path()).unwrap();
    assert_eq!(fresh.leases.len(), 1);
    assert_eq!(fresh.leases[0].id, ID_B);
    // The reader that held the old file open still reads the old bytes, whole.
    let mut seen = Vec::new();
    reader.read_to_end(&mut seen).unwrap();
    assert_eq!(seen, old);
    assert!(temp_files_in(t.path()).is_empty());
}

#[test]
fn write_then_read_round_trips() {
    let t = sandbox();
    let g = lock(t.path()).unwrap();
    let file = state_of(vec![ready(ID_A, "demo"), released_from_ready(ID_C, "x")]);
    write(t.path(), &file, &g).expect("The write must succeed");
    let back = read(t.path()).unwrap();
    assert_eq!(back.leases.len(), 2);
    assert_eq!(back.leases[0].id, ID_A);
    assert_eq!(back.leases[1].id, ID_C);
}

#[test]
fn write_replaces_an_existing_file_and_leaves_no_temporary_file() {
    let t = sandbox();
    std::fs::write(
        t.path().join("leases.json"),
        bytes_of(vec![ready(ID_A, "demo")]),
    )
    .unwrap();
    let g = lock(t.path()).unwrap();
    write(t.path(), &state_of(vec![pending(ID_B, "other")]), &g).unwrap();
    let back = read(t.path()).unwrap();
    assert_eq!(back.leases.len(), 1);
    assert_eq!(back.leases[0].id, ID_B);
    assert!(temp_files_in(t.path()).is_empty());
}

#[test]
fn the_written_bytes_are_the_state_files_serialization() {
    let t = sandbox();
    let g = lock(t.path()).unwrap();
    let file = state_of(vec![ready(ID_A, "demo")]);
    let expected = agentspace_state::serialize(&file).unwrap();
    write(t.path(), &file, &g).unwrap();
    assert_eq!(
        std::fs::read(t.path().join("leases.json")).unwrap(),
        expected
    );
}

#[test]
fn a_reader_sharing_read_write_and_delete_does_not_stop_write() {
    let t = sandbox();
    let path = t.path().join("leases.json");
    std::fs::write(&path, bytes_of(vec![ready(ID_A, "demo")])).unwrap();
    let _child = hold(&path, "Open", "Read", "ReadWrite, Delete", 60);
    let g = lock(t.path()).unwrap();
    let started = Instant::now();
    let r = write(t.path(), &state_of(vec![pending(ID_B, "other")]), &g);
    assert!(r.is_ok(), "Got {}", describe(&r));
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(read(t.path()).unwrap().leases[0].id, ID_B);
    assert!(temp_files_in(t.path()).is_empty());
}

#[test]
fn a_holder_sharing_read_only_makes_write_retry_then_fail_unwritable_with_no_temporary_file() {
    let t = sandbox();
    let path = t.path().join("leases.json");
    let original = bytes_of(vec![ready(ID_A, "demo")]);
    std::fs::write(&path, &original).unwrap();
    let _child = hold(&path, "Open", "Read", "Read", 120);
    let g = lock(t.path()).unwrap();
    let started = Instant::now();
    let r = write(t.path(), &state_of(vec![pending(ID_B, "other")]), &g);
    let waited = started.elapsed();
    assert!(
        matches!(r, Err(StoreError::Unwritable(_))),
        "Got {}",
        describe(&r)
    );
    assert!(
        waited >= Duration::from_secs(9) && waited <= Duration::from_secs(11),
        "Failed after {waited:?}, expected 10 s plus or minus 1"
    );
    assert!(
        temp_files_in(t.path()).is_empty(),
        "A temporary file was left behind"
    );
    assert_eq!(std::fs::read(&path).unwrap(), original, "The file changed");
}

#[test]
fn a_holder_that_lets_go_during_the_retry_lets_write_succeed() {
    let t = sandbox();
    let path = t.path().join("leases.json");
    std::fs::write(&path, bytes_of(vec![ready(ID_A, "demo")])).unwrap();
    let _child = hold(&path, "Open", "Read", "Read", 4);
    let g = lock(t.path()).unwrap();
    let started = Instant::now();
    let r = write(t.path(), &state_of(vec![pending(ID_B, "other")]), &g);
    let waited = started.elapsed();
    assert!(r.is_ok(), "Got {}", describe(&r));
    assert!(
        waited >= Duration::from_secs(1),
        "Must have retried, waited {waited:?}"
    );
    assert_eq!(read(t.path()).unwrap().leases[0].id, ID_B);
    assert!(temp_files_in(t.path()).is_empty());
}

/// Sets the read-only attribute on a file and clears it again on drop, so the throwaway folder can
/// be deleted.
struct ReadOnly(std::path::PathBuf);

impl ReadOnly {
    fn on(path: &std::path::Path) -> ReadOnly {
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(path, perms).unwrap();
        ReadOnly(path.to_path_buf())
    }
}

impl Drop for ReadOnly {
    // This file builds on Windows only, where clearing the attribute opens no file to others.
    #[allow(clippy::permissions_set_readonly_false)]
    fn drop(&mut self) {
        if let Ok(m) = std::fs::metadata(&self.0) {
            let mut perms = m.permissions();
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(&self.0, perms);
        }
    }
}

#[test]
fn a_read_only_file_fails_write_at_once_as_unwritable_with_no_temporary_file() {
    // The POSIX rename reports a held target as a sharing violation, never as access denied, so
    // access denied is a real refusal that waiting does not change: it is not tried again.
    let t = sandbox();
    let path = t.path().join("leases.json");
    let original = bytes_of(vec![ready(ID_A, "demo")]);
    std::fs::write(&path, &original).unwrap();
    let _read_only = ReadOnly::on(&path);
    let g = lock(t.path()).unwrap();
    let started = Instant::now();
    let r = write(t.path(), &state_of(vec![pending(ID_B, "other")]), &g);
    let waited = started.elapsed();
    assert!(
        matches!(r, Err(StoreError::Unwritable(_))),
        "Got {}",
        describe(&r)
    );
    assert!(
        waited < Duration::from_secs(2),
        "Failed only after {waited:?}, expected at once"
    );
    assert!(
        temp_files_in(t.path()).is_empty(),
        "A temporary file was left behind"
    );
    assert_eq!(std::fs::read(&path).unwrap(), original, "The file changed");
}

#[test]
fn a_folder_at_the_file_fails_write_as_unwritable_with_no_temporary_file() {
    let t = sandbox();
    std::fs::create_dir(t.path().join("leases.json")).unwrap();
    let g = lock(t.path()).unwrap();
    let r = write(t.path(), &state_of(vec![]), &g);
    assert!(
        matches!(r, Err(StoreError::Unwritable(_))),
        "Got {}",
        describe(&r)
    );
    assert!(temp_files_in(t.path()).is_empty());
    assert!(t.path().join("leases.json").is_dir());
}

#[test]
fn an_unwritable_message_names_no_path() {
    let t = sandbox();
    std::fs::create_dir(t.path().join("leases.json")).unwrap();
    let g = lock(t.path()).unwrap();
    match write(t.path(), &state_of(vec![]), &g) {
        Err(StoreError::Unwritable(why)) => {
            assert!(!why.contains(t.path().to_str().unwrap()), "Was: {why}");
            assert!(!why.is_empty());
        }
        _ => panic!("Expected Unwritable"),
    }
}

#[test]
fn the_temporary_file_is_named_leases_json_32_hex_digits_tmp_and_a_reader_never_mistakes_it() {
    // A leftover from a killed writer must not be taken for the file: reading ignores it.
    let t = sandbox();
    std::fs::write(
        t.path()
            .join("leases.json.0123456789abcdef0123456789abcdef.tmp"),
        b"not json",
    )
    .unwrap();
    let f = read(t.path()).expect("A leftover temporary file is not the state file");
    assert!(f.leases.is_empty());
}
