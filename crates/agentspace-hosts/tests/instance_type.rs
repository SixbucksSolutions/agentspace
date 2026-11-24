//! The setter of a host's `instance_type` (docs/state.md, "The instance type", the steps 1 to 4
//! under "Who writes it, and when"; "The host registry" and "The host registry lock";
//! E0.09-T02-S01, card #0200): it sets or removes one record's `instance_type` under `hosts.lock`,
//! re-reading the file under the lock, and changes nothing else.
//!
//! The setter is pin P38's `agentspace_hosts::set_instance_type(home: &Path, project: &str,
//! instance_type: Option<&str>) -> Result<(), HostsError>`, which takes and releases the host
//! registry lock itself (state.md: "which the writer takes, re-reading the file under it, and
//! releases after the rename"), `Some(type)` setting the member and `None` removing it. Every call
//! of it in this file goes through `set_type` below.

mod common;

use agentspace_hosts::{HostState, Hosts, HostsError, lock, read, try_lock, write};
use common::*;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

/// The one call of the setter.
fn set_type(home: &Path, project: &str, instance_type: Option<&str>) -> Result<(), HostsError> {
    agentspace_hosts::set_instance_type(home, project, instance_type)
}

const SEED_BYTES: &str = "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}]}\n";

/// Four records of every shape the setter MUST leave alone: the box with an ID and a time and no
/// type; a ready host with a type; a creating host with none; a failed host with a type.
const FILE: &str = "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":\"i-0123456789abcdef0\",\"last_attempt_at\":\"2026-11-24T18:30:00Z\"},{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_id\":\"i-0fedcba9876543210\",\"instance_type\":\"m9g.large\",\"last_attempt_at\":\"2026-11-24T19:05:42Z\"},{\"project\":\"example-two\",\"stack\":\"agentspace-host-example-two\",\"state\":\"creating\",\"last_attempt_at\":\"2026-11-24T19:06:00Z\"},{\"project\":\"failed-one\",\"stack\":\"agentspace-host-failed-one\",\"state\":\"failed\",\"instance_id\":\"i-0123456789abcdef1\",\"instance_type\":\"c7g.large\",\"last_attempt_at\":\"2026-11-24T19:10:00Z\"}]}\n";

fn put_file() -> TempHome {
    let t = TempHome::new();
    t.put_hosts(FILE);
    t
}

/// The names of the folder's entries are the registry and its lock alone: no temporary file.
fn assert_nothing_left_over(t: &TempHome) {
    assert_eq!(
        TempHome::names_in(&t.state()),
        ["hosts.json", "hosts.lock"],
        "Only the registry and its lock are in the folder"
    );
}

/// The lock is free again: a setter never leaves it held, after a success or after a refusal.
fn assert_lock_is_free(t: &TempHome) {
    assert!(
        free_within(FREE_WITHIN, || try_lock(t.path())
            .expect("The lock must be takeable")
            .is_some()),
        "The setter left the host registry lock held"
    );
}

// --- It changes that record's member and nothing else -----------------------------------------

#[test]
fn it_sets_a_type_on_a_record_that_has_none_and_every_other_byte_is_as_read() {
    // Steps 1 to 4 write `instance_type` and no other member of the record they change.
    let t = put_file();
    set_type(t.path(), "example-two", Some("m9g.2xlarge")).unwrap();
    let expected = FILE.replace(
        "\"state\":\"creating\",\"last_attempt_at\":\"2026-11-24T19:06:00Z\"",
        "\"state\":\"creating\",\"instance_type\":\"m9g.2xlarge\",\"last_attempt_at\":\"2026-11-24T19:06:00Z\"",
    );
    assert_ne!(expected, FILE, "The test's own replacement must hit");
    assert_eq!(String::from_utf8(t.hosts_bytes()).unwrap(), expected);
    assert_nothing_left_over(&t);
    assert_eq!(
        read(t.path()).unwrap().hosts[2].instance_type.as_deref(),
        Some("m9g.2xlarge")
    );
}

#[test]
fn it_sets_a_type_on_the_box_that_has_none_and_the_box_keeps_its_id_and_time() {
    let t = put_file();
    set_type(t.path(), "swimcoach", Some("m9g.xlarge")).unwrap();
    let expected = FILE.replacen(
        "\"instance_id\":\"i-0123456789abcdef0\",\"last_attempt_at\"",
        "\"instance_id\":\"i-0123456789abcdef0\",\"instance_type\":\"m9g.xlarge\",\"last_attempt_at\"",
        1,
    );
    assert_ne!(expected, FILE);
    assert_eq!(String::from_utf8(t.hosts_bytes()).unwrap(), expected);
}

#[test]
fn it_replaces_a_type_the_record_holds() {
    let t = put_file();
    set_type(t.path(), "agentspace", Some("m9g.4xlarge")).unwrap();
    let expected = FILE.replace("\"m9g.large\"", "\"m9g.4xlarge\"");
    assert_ne!(expected, FILE);
    assert_eq!(String::from_utf8(t.hosts_bytes()).unwrap(), expected);
}

#[test]
fn it_removes_a_type_and_leaves_the_record_as_one_written_before_v0_09() {
    let t = put_file();
    set_type(t.path(), "failed-one", None).unwrap();
    let expected = FILE.replace("\"instance_type\":\"c7g.large\",", "");
    assert_ne!(expected, FILE);
    assert_eq!(String::from_utf8(t.hosts_bytes()).unwrap(), expected);
    assert!(
        !String::from_utf8(t.hosts_bytes()).unwrap().contains("null"),
        "A removed member is left out, never written as null"
    );
    assert_eq!(read(t.path()).unwrap().hosts[3].instance_type, None);
    assert_nothing_left_over(&t);
}

#[test]
fn removing_a_type_that_is_not_there_leaves_the_file_as_it_was() {
    let t = put_file();
    set_type(t.path(), "example-two", None).unwrap();
    assert_eq!(t.hosts_bytes(), FILE.as_bytes());
}

#[test]
fn setting_the_type_a_record_already_holds_leaves_the_file_as_it_was() {
    let t = put_file();
    set_type(t.path(), "agentspace", Some("m9g.large")).unwrap();
    assert_eq!(t.hosts_bytes(), FILE.as_bytes());
}

#[test]
fn it_changes_no_other_record_and_no_other_member_of_the_one_it_changes() {
    // Read both sides as records, whatever the state: ready, creating and failed alike (pin P12).
    let t = put_file();
    let before = read(t.path()).unwrap();
    for (i, project) in ["swimcoach", "agentspace", "example-two", "failed-one"]
        .iter()
        .enumerate()
    {
        set_type(t.path(), project, Some("t4g.nano")).unwrap();
        let after = read(t.path()).unwrap();
        assert_eq!(after.hosts.len(), before.hosts.len());
        for (j, (a, b)) in after.hosts.iter().zip(&before.hosts).enumerate() {
            let mut expect = b.clone();
            if j <= i {
                expect.instance_type = Some("t4g.nano".to_string());
            }
            assert_eq!(a, &expect, "After setting {project}, record {j}");
        }
    }
}

#[test]
fn it_writes_a_loosely_written_file_back_compact_with_the_one_change() {
    // A byte order mark, whitespace and another member order are read (D20); the write is the
    // rules' bytes: compact, the tables' order, one LF.
    let t = TempHome::new();
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(
        b"{ \"version\" : 0 , \"hosts\" : [ {\"state\":\"ready\",\"stack\":\"agentbox\",\"project\":\"swimcoach\"},\r\n {\"last_attempt_at\":\"2026-11-24T19:05:42Z\",\"state\":\"ready\",\"stack\":\"agentspace-host-agentspace\",\"project\":\"agentspace\"} ] }\r\n\r\n",
    );
    t.put_hosts(bytes);
    set_type(t.path(), "agentspace", Some("m9g.large")).unwrap();
    assert_eq!(
        String::from_utf8(t.hosts_bytes()).unwrap(),
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"},{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_type\":\"m9g.large\",\"last_attempt_at\":\"2026-11-24T19:05:42Z\"}]}\n"
    );
}

#[test]
fn it_works_in_a_home_whose_folder_already_exists_without_the_file() {
    let t = TempHome::new();
    std::fs::create_dir_all(t.state()).unwrap();
    set_type(t.path(), "swimcoach", Some("m9g.2xlarge")).unwrap();
    assert_eq!(
        String::from_utf8(t.hosts_bytes()).unwrap(),
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_type\":\"m9g.2xlarge\"}]}\n"
    );
}

// --- A type that is not of the member's form is refused ---------------------------------------

#[test]
fn it_refuses_a_type_that_would_not_be_valid_and_writes_nothing() {
    // state.md: "A change that would not be valid is not written."
    let t = put_file();
    let over = format!("{}.{}", "a".repeat(32), "b".repeat(32));
    for ty in [
        "M9G.xlarge",
        "m9gxlarge",
        ".xlarge",
        "m9g.",
        "",
        "m9g.xlarge\n",
        "m9g.x.large",
        over.as_str(),
    ] {
        let e = set_type(t.path(), "agentspace", Some(ty))
            .expect_err("A type that is not valid must not be written");
        assert_message(&e, t.path());
        assert_eq!(
            t.hosts_bytes(),
            FILE.as_bytes(),
            "{ty:?}: the old file must stay"
        );
    }
    assert_nothing_left_over(&t);
    assert_lock_is_free(&t);
}

#[test]
fn it_refuses_a_bad_type_when_there_is_no_file_and_creates_no_registry() {
    let t = TempHome::new();
    let e = set_type(t.path(), "swimcoach", Some("NOT.valid"))
        .expect_err("A type that is not valid must not be written");
    assert_message(&e, t.path());
    assert!(!t.hosts_json().exists());
}

// --- A project with no record -----------------------------------------------------------------

#[test]
fn it_refuses_a_project_with_no_record_and_writes_nothing() {
    // state.md does not name the setter's refusal in so many words; it is read from "Who writes
    // it" (line 863: "one given -Host for a project with no record fails, writing nothing"),
    // from the setter's step list (983-985: "changing that record's instance_type and no other
    // member") and from "Its states" (line 936: the only record agentspace adds with the stack
    // agentbox is the seed's, and every record it adds comes from the driver or `provision`).
    // Pin P38 names it of both setters: `Unwritable(NoSuchRecord)`, nothing written.
    let t = put_file();
    for ty in [Some("m9g.large"), None] {
        let e = set_type(t.path(), "no-such-project", ty)
            .expect_err("A project with no record has no type to set");
        assert_message(&e, t.path());
        assert_eq!(
            t.hosts_bytes(),
            FILE.as_bytes(),
            "The file must stay as it was"
        );
    }
    assert_nothing_left_over(&t);
    assert_lock_is_free(&t);
}

#[test]
fn it_refuses_a_project_with_no_record_when_there_is_no_file_too_and_creates_none() {
    let t = TempHome::new();
    for ty in [Some("m9g.large"), None] {
        let e = set_type(t.path(), "agentspace", ty)
            .expect_err("A project with no record has no type to set");
        assert_message(&e, t.path());
        assert!(
            !t.hosts_json().exists(),
            "A refusal writes no file, not even the seed's"
        );
    }
    assert_lock_is_free(&t);
}

#[test]
fn it_never_adds_a_record() {
    let t = put_file();
    let _ = set_type(t.path(), "brand-new", Some("m9g.large"));
    assert_eq!(read(t.path()).unwrap().hosts.len(), 4);
    assert_eq!(t.hosts_bytes(), FILE.as_bytes());
}

#[test]
fn a_project_name_is_compared_as_the_normalized_string_stored() {
    // `Agentspace` is not `agentspace`: no record, nothing written.
    let t = put_file();
    assert!(set_type(t.path(), "Agentspace", Some("m9g.large")).is_err());
    assert!(set_type(t.path(), "", Some("m9g.large")).is_err());
    assert_eq!(t.hosts_bytes(), FILE.as_bytes());
}

// --- A file that is not valid, or something that is not a file --------------------------------

#[test]
fn it_refuses_a_file_that_is_not_valid_and_repairs_nothing() {
    for bytes in [
        "not json".to_string(),
        FILE.replace("\"m9g.large\"", "\"M9G.LARGE\""),
        FILE.replace("\"instance_type\":\"c7g.large\"", "\"instance_type\":null"),
        "{\"version\":0,\"hosts\":[]}".to_string(),
    ] {
        let t = TempHome::new();
        t.put_hosts(&bytes);
        let e = set_type(t.path(), "agentspace", Some("m9g.large"))
            .expect_err("A file that is not valid is an error, never the seed");
        assert!(is_file_problem(&e), "{e:?}");
        assert_message(&e, t.path());
        assert_eq!(
            t.hosts_bytes(),
            bytes.as_bytes(),
            "Nothing in agentspace repairs, rewrites or removes a file that is not valid"
        );
        assert_lock_is_free(&t);
    }
}

#[test]
fn it_refuses_a_folder_at_the_registry_and_changes_nothing() {
    let t = TempHome::new();
    std::fs::create_dir_all(t.hosts_json()).unwrap();
    let e = set_type(t.path(), "swimcoach", Some("m9g.large"))
        .expect_err("A folder is not a missing file");
    assert_message(&e, t.path());
    assert!(t.hosts_json().is_dir());
    assert!(TempHome::names_in(&t.hosts_json()).is_empty());
}

#[test]
fn a_folder_at_the_locks_path_fails_at_once_and_is_never_reported_busy() {
    let t = put_file();
    std::fs::create_dir_all(t.state().join("hosts.lock")).unwrap();
    let started = std::time::Instant::now();
    let e =
        set_type(t.path(), "agentspace", Some("m9g.4xlarge")).expect_err("A folder is not a lock");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "A failure other than the lock being held must not be waited for"
    );
    assert!(is_cannot_lock(&e), "{e:?}");
    assert_message(&e, t.path());
    assert_eq!(t.hosts_bytes(), FILE.as_bytes());
}

// --- No file: the seed's record ---------------------------------------------------------------

#[test]
fn with_no_file_a_removal_writes_the_seed_record_with_no_type() {
    // state.md, "The instance type", step 1: "With no file, it writes the file then: the box's
    // record that a missing file implies reads as holding m9g.xlarge (pin P2c), so the removal
    // writes the seed document's record, with no instance_type, as any first change writes it."
    let t = TempHome::new();
    assert!(!t.state().exists());
    set_type(t.path(), "swimcoach", None).unwrap();
    assert_eq!(String::from_utf8(t.hosts_bytes()).unwrap(), SEED_BYTES);
    assert_eq!(
        read(t.path()).unwrap().hosts[0].instance_type,
        None,
        "The implied m9g.xlarge is no longer read once the file is written"
    );
    assert_nothing_left_over(&t);
}

#[test]
fn with_no_file_a_removal_writes_the_file_in_a_folder_that_exists_too() {
    let t = TempHome::new();
    std::fs::create_dir_all(t.state()).unwrap();
    set_type(t.path(), "swimcoach", None).unwrap();
    assert_eq!(String::from_utf8(t.hosts_bytes()).unwrap(), SEED_BYTES);
}

#[test]
fn with_no_file_setting_a_type_for_the_box_writes_the_seed_record_with_that_type() {
    // The type written is the one given, never the implied default beside it.
    let t = TempHome::new();
    set_type(t.path(), "swimcoach", Some("m9g.2xlarge")).unwrap();
    assert_eq!(
        String::from_utf8(t.hosts_bytes()).unwrap(),
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_type\":\"m9g.2xlarge\"}]}\n"
    );
    assert_nothing_left_over(&t);
}

#[test]
fn with_no_file_setting_the_implied_default_itself_writes_it_because_it_was_asked() {
    // Only an explicit set writes m9g.xlarge: it is then a reading of a stack, as a deploy
    // records it. This pins that a SET is written whatever the implied default is.
    let t = TempHome::new();
    set_type(t.path(), "swimcoach", Some("m9g.xlarge")).unwrap();
    assert_eq!(
        String::from_utf8(t.hosts_bytes()).unwrap(),
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_type\":\"m9g.xlarge\"}]}\n"
    );
}

// --- Under hosts.lock -------------------------------------------------------------------------

#[test]
fn it_takes_the_lock_and_lets_it_go() {
    let t = put_file();
    set_type(t.path(), "agentspace", Some("m9g.4xlarge")).unwrap();
    assert!(t.state().join("hosts.lock").is_file());
    assert_eq!(
        std::fs::metadata(t.state().join("hosts.lock"))
            .unwrap()
            .len(),
        0,
        "The lock's content is empty and meaningless"
    );
    assert_lock_is_free(&t);
}

#[test]
fn it_waits_for_a_held_lock_and_writes_only_once_it_has_it() {
    // The registry lock's own test of a waiting taker (registry_lock.rs): a setter on another
    // thread must still be waiting a while after it started, must not have written, and must
    // finish once the holder lets go.
    let t = put_file();
    let held = lock(t.path()).unwrap();
    let home = t.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let setter = std::thread::spawn(move || {
        let r = set_type(&home, "agentspace", Some("m9g.4xlarge"));
        tx.send(r.is_ok()).unwrap();
    });
    std::thread::sleep(Duration::from_millis(1500));
    assert!(
        rx.try_recv().is_err(),
        "The setter finished while another holder had the lock: it did not take it"
    );
    assert_eq!(
        t.hosts_bytes(),
        FILE.as_bytes(),
        "The setter wrote before it held the lock"
    );
    drop(held);
    assert!(
        rx.recv_timeout(Duration::from_secs(20))
            .expect("The setter must finish once the lock is let go"),
        "The setter must succeed once it has the lock"
    );
    setter.join().unwrap();
    assert_eq!(
        read(t.path()).unwrap().hosts[1].instance_type.as_deref(),
        Some("m9g.4xlarge")
    );
}

#[test]
fn it_reads_the_file_again_under_the_lock_and_keeps_a_change_made_while_it_waited() {
    // state.md, step list: "which the writer takes, re-reading the file under it". A change that
    // the holder writes while the setter waits is in the setter's result, and the setter's change
    // is in it too: no lost update.
    let t = put_file();
    let held = lock(t.path()).unwrap();
    let home = t.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let setter = std::thread::spawn(move || {
        tx.send(set_type(&home, "agentspace", Some("m9g.4xlarge")).is_ok())
            .unwrap();
    });
    std::thread::sleep(Duration::from_millis(500));
    // The holder's change, as the driver makes one: a new record, and a time at a run's start.
    let mut h = read(t.path()).unwrap();
    h.hosts
        .push(host("added-while-waiting", HostState::Creating));
    h.hosts[0].last_attempt_at = Some("2026-11-24T21:00:00Z".to_string());
    write(t.path(), &h, &held).unwrap();
    assert!(rx.try_recv().is_err(), "The setter must still be waiting");
    drop(held);
    assert!(rx.recv_timeout(Duration::from_secs(20)).unwrap());
    setter.join().unwrap();
    let after = read(t.path()).unwrap();
    assert_eq!(after.hosts.len(), 5, "The record the holder added is kept");
    assert_eq!(after.hosts[4].project, "added-while-waiting");
    assert_eq!(
        after.hosts[0].last_attempt_at.as_deref(),
        Some("2026-11-24T21:00:00Z"),
        "The time the holder wrote is kept"
    );
    assert_eq!(after.hosts[1].instance_type.as_deref(), Some("m9g.4xlarge"));
}

#[test]
fn a_record_added_while_the_setter_waits_is_found_by_its_read_under_the_lock() {
    // The record's existence is decided by the file as it is under the lock, not as it was when
    // the setter was called: the file holds no `later-project` when the setter starts waiting, and
    // the holder adds it before letting go. A setter that read the file first would refuse it.
    let t = put_file();
    let held = lock(t.path()).unwrap();
    let home = t.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let setter = std::thread::spawn(move || {
        tx.send(set_type(&home, "later-project", Some("m9g.large")).is_ok())
            .unwrap();
    });
    std::thread::sleep(Duration::from_millis(500));
    let mut h = read(t.path()).unwrap();
    h.hosts.push(host("later-project", HostState::Creating));
    write(t.path(), &h, &held).unwrap();
    drop(held);
    // The record was added under the setter's wait: a re-read under the lock finds it.
    assert!(
        rx.recv_timeout(Duration::from_secs(20)).unwrap(),
        "The setter read the file before it held the lock, so it missed the record the holder added"
    );
    setter.join().unwrap();
    let after = read(t.path()).unwrap();
    assert_eq!(after.hosts[4].project, "later-project");
    assert_eq!(after.hosts[4].instance_type.as_deref(), Some("m9g.large"));
}

#[test]
fn setters_of_different_records_at_once_all_land() {
    let t = TempHome::new();
    let mut records = Vec::new();
    for i in 0..8 {
        records.push(host(&format!("project-{i}"), HostState::Ready));
    }
    {
        let guard = lock(t.path()).unwrap();
        write(t.path(), &with_box(records), &guard).unwrap();
    }
    let mut threads = Vec::new();
    for i in 0..8 {
        let home = t.path().to_path_buf();
        threads.push(std::thread::spawn(move || {
            set_type(
                &home,
                &format!("project-{i}"),
                Some(&format!("m9g.{i}xlarge")),
            )
            .unwrap();
        }));
    }
    for th in threads {
        th.join().unwrap();
    }
    let after: Hosts = read(t.path()).unwrap();
    assert_eq!(after.hosts.len(), 9);
    assert_eq!(after.hosts[0].instance_type, None);
    for i in 0..8 {
        assert_eq!(
            after.hosts[i + 1].instance_type.as_deref(),
            Some(format!("m9g.{i}xlarge").as_str()),
            "project-{i}: a concurrent setter lost its change"
        );
    }
    assert_nothing_left_over(&t);
    assert_lock_is_free(&t);
}

#[test]
fn a_reader_is_never_blocked_by_a_setter_and_sees_the_old_file_or_the_new_one() {
    let t = put_file();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let home = t.path().to_path_buf();
    let reader_done = done.clone();
    let reader = std::thread::spawn(move || {
        let mut seen = 0u32;
        while !reader_done.load(std::sync::atomic::Ordering::SeqCst) {
            let h = read(&home).expect("A reader must see the old file or the new, never part");
            let ty = h.hosts[1].instance_type.as_deref();
            assert!(
                matches!(ty, None | Some("m9g.large") | Some("m9g.4xlarge")),
                "A read saw a third type"
            );
            assert_eq!(h.hosts.len(), 4);
            seen += 1;
        }
        seen
    });
    for i in 0..60 {
        let ty = if i % 2 == 0 {
            Some("m9g.4xlarge")
        } else {
            Some("m9g.large")
        };
        set_type(t.path(), "agentspace", ty).unwrap();
    }
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(reader.join().unwrap() > 0);
}

// --- The instance ID and the type in one write (steps 2 and 4) --------------------------------
//
// state.md, "The instance type", steps 2 and 4: `deploy` removes the record's `instance_id`, and
// `instance_type` with it in the same write; and it writes the type it read "in the same write as
// the `instance_id`" (pin P23). The setter is pin P38's `set_instance(home, project, instance_id,
// instance_type)`, each `None` removing its member. `set_both` is its one call.

fn set_both(
    home: &Path,
    project: &str,
    instance_id: Option<&str>,
    instance_type: Option<&str>,
) -> Result<(), HostsError> {
    agentspace_hosts::set_instance(home, project, instance_id, instance_type)
}

#[test]
fn step_two_removes_the_id_and_the_type_in_one_write_and_nothing_else() {
    let t = put_file();
    set_both(t.path(), "agentspace", None, None).unwrap();
    let expected = FILE.replace(
        "\"state\":\"ready\",\"instance_id\":\"i-0fedcba9876543210\",\"instance_type\":\"m9g.large\",",
        "\"state\":\"ready\",",
    );
    assert_ne!(expected, FILE);
    assert_eq!(String::from_utf8(t.hosts_bytes()).unwrap(), expected);
    assert_nothing_left_over(&t);
}

#[test]
fn step_four_writes_the_id_and_the_type_together_in_the_tables_order() {
    let t = put_file();
    set_both(
        t.path(),
        "example-two",
        Some("i-0aaaaaaaaaaaaaaaa"),
        Some("m9g.2xlarge"),
    )
    .unwrap();
    let expected = FILE.replace(
        "\"state\":\"creating\",\"last_attempt_at\"",
        "\"state\":\"creating\",\"instance_id\":\"i-0aaaaaaaaaaaaaaaa\",\"instance_type\":\"m9g.2xlarge\",\"last_attempt_at\"",
    );
    assert_ne!(expected, FILE);
    assert_eq!(String::from_utf8(t.hosts_bytes()).unwrap(), expected);
}

#[test]
fn a_read_back_with_no_usable_type_leaves_the_id_and_no_type() {
    // A read-back whose `InstanceType` is missing or malformed: "leaves no type" (pin P30).
    let t = put_file();
    set_both(t.path(), "agentspace", Some("i-0fedcba9876543211"), None).unwrap();
    let expected = FILE.replace(
        "\"instance_id\":\"i-0fedcba9876543210\",\"instance_type\":\"m9g.large\",",
        "\"instance_id\":\"i-0fedcba9876543211\",",
    );
    assert_ne!(expected, FILE);
    assert_eq!(String::from_utf8(t.hosts_bytes()).unwrap(), expected);
}

#[test]
fn a_bad_value_among_the_two_writes_neither() {
    // One write: a type that would not be valid beside an ID that would be leaves the file alone,
    // so there is no state with the ID changed and the type not.
    let t = put_file();
    set_both(
        t.path(),
        "agentspace",
        Some("i-0aaaaaaaaaaaaaaaa"),
        Some("NOT.valid"),
    )
    .expect_err("A type that is not valid must not be written");
    set_both(t.path(), "agentspace", Some("not-an-id"), Some("m9g.large"))
        .expect_err("An ID that is not valid must not be written");
    assert_eq!(t.hosts_bytes(), FILE.as_bytes());
    assert_nothing_left_over(&t);
}

#[test]
fn the_pair_refuses_a_project_with_no_record_and_writes_nothing() {
    let t = put_file();
    set_both(
        t.path(),
        "no-such-project",
        Some("i-0aaaaaaaaaaaaaaaa"),
        Some("m9g.large"),
    )
    .expect_err("A project with no record has no ID or type to set");
    assert_eq!(t.hosts_bytes(), FILE.as_bytes());
    let t = TempHome::new();
    set_both(t.path(), "agentspace", None, None)
        .expect_err("A project with no record has no ID or type to set");
    assert!(!t.hosts_json().exists());
}

#[test]
fn the_pair_with_no_file_writes_the_seed_record_with_only_what_it_was_given() {
    let t = TempHome::new();
    set_both(t.path(), "swimcoach", None, None).unwrap();
    assert_eq!(String::from_utf8(t.hosts_bytes()).unwrap(), SEED_BYTES);
    let t = TempHome::new();
    set_both(
        t.path(),
        "swimcoach",
        Some("i-0123456789abcdef0"),
        Some("m9g.large"),
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(t.hosts_bytes()).unwrap(),
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":\"i-0123456789abcdef0\",\"instance_type\":\"m9g.large\"}]}\n"
    );
    let t = TempHome::new();
    set_both(t.path(), "swimcoach", Some("i-0123456789abcdef0"), None).unwrap();
    assert!(
        !String::from_utf8(t.hosts_bytes())
            .unwrap()
            .contains("instance_type"),
        "No type that it was not given, the implied one least"
    );
}

#[test]
fn the_pair_waits_for_a_held_lock_too() {
    let t = put_file();
    let held = lock(t.path()).unwrap();
    let home = t.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let setter = std::thread::spawn(move || {
        tx.send(set_both(&home, "agentspace", None, None).is_ok())
            .unwrap();
    });
    std::thread::sleep(Duration::from_millis(1500));
    assert!(rx.try_recv().is_err(), "The setter did not take the lock");
    assert_eq!(t.hosts_bytes(), FILE.as_bytes());
    drop(held);
    assert!(rx.recv_timeout(Duration::from_secs(20)).unwrap());
    setter.join().unwrap();
    assert_eq!(read(t.path()).unwrap().hosts[1].instance_type, None);
}
