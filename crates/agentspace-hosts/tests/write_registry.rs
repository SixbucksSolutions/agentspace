//! Writing the host registry (docs/state.md, The host registry, Replacing a file): the bytes,
//! validation before every write, the atomic replace, and every refusal.

mod common;

use agentspace_hosts::{Host, HostState, Hosts, lock, read, write};
use common::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn write_new(t: &TempHome, h: &Hosts) -> Result<(), agentspace_hosts::HostsError> {
    let guard = lock(t.path())?;
    write(t.path(), h, &guard)
}

fn assert_refused_write(label: &str, t: &TempHome, h: &Hosts) {
    let before = std::fs::read(t.hosts_json()).ok();
    let state_names = TempHome::names_in(&t.state());
    match write_new(t, h) {
        Ok(()) => panic!("{label}: a change that is not valid was written"),
        Err(e) => {
            assert!(is_write_refusal(&e), "{label}: unexpected error {e:?}");
            assert_message(&e, t.path());
        }
    }
    assert_eq!(
        std::fs::read(t.hosts_json()).ok(),
        before,
        "{label}: a refused write must leave the old file"
    );
    let after = TempHome::names_in(&t.state());
    // The lock file may have been made by the taking of the lock; nothing else may appear.
    let extra: Vec<&String> = after
        .iter()
        .filter(|n| !state_names.contains(n) && n.as_str() != "hosts.lock")
        .collect();
    assert!(extra.is_empty(), "{label}: left {extra:?} behind");
}

fn record(project: &str, state: HostState) -> Host {
    host(project, state)
}

// --- The bytes --------------------------------------------------------------------------------

#[test]
fn the_first_write_makes_the_agentspace_folder_and_the_file() {
    let t = TempHome::new();
    assert!(!t.state().exists());
    write_new(&t, &seed()).unwrap();
    assert!(t.hosts_json().is_file());
}

#[test]
fn the_seed_is_written_as_compact_json_and_one_line_feed() {
    let t = TempHome::new();
    write_new(&t, &seed()).unwrap();
    assert_eq!(
        t.hosts_bytes(),
        b"{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}]}\n"
    );
}

#[test]
fn no_byte_order_mark_and_exactly_one_trailing_line_feed() {
    let t = TempHome::new();
    write_new(
        &t,
        &with_box(vec![record("agentspace", HostState::Creating)]),
    )
    .unwrap();
    let bytes = t.hosts_bytes();
    assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert_eq!(bytes.iter().filter(|b| **b == b'\n').count(), 1);
    assert_eq!(*bytes.last().unwrap(), b'\n');
    assert!(std::str::from_utf8(&bytes).is_ok());
}

#[test]
fn a_records_members_are_written_in_the_tables_order() {
    let t = TempHome::new();
    let h = with_box(vec![full_host(
        "agentspace",
        HostState::Failed,
        "i-1234567890abcdef0",
        "2026-11-20T19:10:00Z",
    )]);
    write_new(&t, &h).unwrap();
    let expected = "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"},{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"failed\",\"instance_id\":\"i-1234567890abcdef0\",\"last_attempt_at\":\"2026-11-20T19:10:00Z\"}]}\n";
    assert_eq!(String::from_utf8(t.hosts_bytes()).unwrap(), expected);
}

#[test]
fn an_absent_optional_member_is_left_out_never_written_as_null() {
    let t = TempHome::new();
    write_new(
        &t,
        &with_box(vec![record("agentspace", HostState::Creating)]),
    )
    .unwrap();
    let text = String::from_utf8(t.hosts_bytes()).unwrap();
    assert!(!text.contains("null"));
    assert!(!text.contains("instance_id"));
    assert!(!text.contains("last_attempt_at"));
}

#[test]
fn each_optional_member_alone_is_written_alone() {
    let t = TempHome::new();
    let mut with_id = record("agentspace", HostState::Ready);
    with_id.instance_id = Some("i-1234567890abcdef0".to_string());
    let mut with_at = record("example-two", HostState::Creating);
    with_at.last_attempt_at = Some("2026-11-20T19:05:42Z".to_string());
    write_new(&t, &with_box(vec![with_id, with_at])).unwrap();
    let text = String::from_utf8(t.hosts_bytes()).unwrap();
    assert!(text.contains(
        "{\"project\":\"agentspace\",\"stack\":\"agentspace-host-agentspace\",\"state\":\"ready\",\"instance_id\":\"i-1234567890abcdef0\"}"
    ));
    assert!(text.contains(
        "{\"project\":\"example-two\",\"stack\":\"agentspace-host-example-two\",\"state\":\"creating\",\"last_attempt_at\":\"2026-11-20T19:05:42Z\"}"
    ));
}

#[test]
fn every_state_is_written_in_lower_case() {
    let t = TempHome::new();
    let h = with_box(vec![
        record("a", HostState::Creating),
        record("b", HostState::Ready),
        record("c", HostState::Failed),
    ]);
    write_new(&t, &h).unwrap();
    let text = String::from_utf8(t.hosts_bytes()).unwrap();
    assert!(text.contains("\"state\":\"creating\""));
    assert!(text.contains("\"state\":\"failed\""));
    assert_eq!(text.matches("\"state\":\"ready\"").count(), 2);
}

#[test]
fn what_is_written_reads_back_the_same() {
    let t = TempHome::new();
    let h = with_box(vec![
        record("agentspace", HostState::Creating),
        full_host(
            "example-two",
            HostState::Failed,
            "i-fedcba9876543210f",
            "2026-11-20T19:10:00Z",
        ),
        full_host("a", HostState::Ready, "i-1234abcd", "2026-01-01T00:00:00Z"),
    ]);
    write_new(&t, &h).unwrap();
    assert_eq!(read(t.path()).unwrap(), h);
}

#[test]
fn records_are_kept_in_the_order_given() {
    let t = TempHome::new();
    let h = with_box(vec![
        record("zeta", HostState::Ready),
        record("alpha", HostState::Ready),
        record("mid", HostState::Ready),
    ]);
    write_new(&t, &h).unwrap();
    let names: Vec<String> = read(t.path())
        .unwrap()
        .hosts
        .into_iter()
        .map(|r| r.project)
        .collect();
    assert_eq!(names, ["swimcoach", "zeta", "alpha", "mid"]);
}

#[test]
fn a_record_is_written_as_given_and_no_other_record_is_changed() {
    // The writer changes only the record or records its change is about.
    let t = TempHome::new();
    let first = with_box(vec![
        full_host("a", HostState::Ready, "i-1234abcd", "2026-01-01T00:00:00Z"),
        record("b", HostState::Creating),
    ]);
    write_new(&t, &first).unwrap();
    let mut read_back = read(t.path()).unwrap();
    read_back.hosts[2].state = HostState::Failed;
    read_back.hosts[2].last_attempt_at = Some("2026-11-20T19:10:00Z".to_string());
    write_new(&t, &read_back).unwrap();
    let again = read(t.path()).unwrap();
    assert_eq!(again.hosts[0], first.hosts[0]);
    assert_eq!(again.hosts[1], first.hosts[1]);
    assert_eq!(again.hosts[2].state, HostState::Failed);
}

#[test]
fn a_version_written_as_zero_point_zero_is_written_back_as_zero() {
    let t = TempHome::new();
    t.put_hosts(format!("{{\"version\":0.0,\"hosts\":[{BOX_RECORD}]}}"));
    let h = read(t.path()).unwrap();
    write_new(&t, &h).unwrap();
    assert_eq!(
        t.hosts_bytes(),
        b"{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}]}\n"
    );
}

#[test]
fn a_file_read_with_a_byte_order_mark_and_whitespace_is_written_back_plain() {
    let t = TempHome::new();
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(
        b"{ \"version\": 0,\r\n \"hosts\": [ {\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"} ] }\r\n",
    );
    t.put_hosts(bytes);
    let h = read(t.path()).unwrap();
    write_new(&t, &h).unwrap();
    assert_eq!(
        t.hosts_bytes(),
        b"{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}]}\n"
    );
}

// --- Atomic: a temporary file, a rename, and nothing left -------------------------------------

#[test]
fn a_write_replaces_the_old_file_whole() {
    let t = TempHome::new();
    write_new(
        &t,
        &with_box(vec![record("agentspace", HostState::Creating)]),
    )
    .unwrap();
    let long = t.hosts_bytes().len();
    write_new(&t, &seed()).unwrap();
    assert!(
        t.hosts_bytes().len() < long,
        "No tail of the old file may remain"
    );
    assert_eq!(read(t.path()).unwrap(), seed());
}

#[test]
fn a_write_leaves_no_temporary_file_beside_the_registry() {
    let t = TempHome::new();
    for i in 0..5 {
        let mut r = record("agentspace", HostState::Creating);
        r.last_attempt_at = Some(format!("2026-11-20T19:0{i}:00Z"));
        write_new(&t, &with_box(vec![r])).unwrap();
    }
    assert_eq!(
        TempHome::names_in(&t.state()),
        ["hosts.json", "hosts.lock"],
        "Only the registry and its lock are in the folder"
    );
}

#[test]
fn a_reader_never_sees_part_of_a_file_while_another_writes() {
    let t = TempHome::new();
    let small = seed();
    let big = with_box(
        (0..40)
            .map(|i| {
                full_host(
                    &format!("project-{i}"),
                    HostState::Ready,
                    "i-1234567890abcdef0",
                    "2026-11-20T19:05:42Z",
                )
            })
            .collect(),
    );
    write_new(&t, &small).unwrap();
    let done = Arc::new(AtomicBool::new(false));
    let home = t.path().to_path_buf();
    let reader_done = done.clone();
    let (small_r, big_r) = (small.clone(), big.clone());
    let reader = std::thread::spawn(move || {
        let mut reads = 0u32;
        while !reader_done.load(Ordering::SeqCst) {
            let h = read(&home).expect("A reader must see the old file or the new, never part");
            assert!(h == small_r || h == big_r, "A read saw a third document");
            reads += 1;
        }
        reads
    });
    for i in 0..150 {
        let h = if i % 2 == 0 { &big } else { &small };
        write_new(&t, h).unwrap();
    }
    done.store(true, Ordering::SeqCst);
    let reads = reader.join().unwrap();
    assert!(reads > 0);
}

// --- Validated before every write -------------------------------------------------------------

#[test]
fn a_refused_write_leaves_the_old_file_untouched_and_nothing_beside_it() {
    let t = TempHome::new();
    write_new(&t, &with_box(vec![record("agentspace", HostState::Ready)])).unwrap();
    let bad = with_box(vec![
        record("agentspace", HostState::Ready),
        record("agentspace", HostState::Creating),
    ]);
    assert_refused_write("duplicate project", &t, &bad);
    assert_eq!(
        read(t.path()).unwrap(),
        with_box(vec![record("agentspace", HostState::Ready)])
    );
}

#[test]
fn two_records_with_one_project_are_refused() {
    let t = TempHome::new();
    write_new(&t, &seed()).unwrap();
    let bad = with_box(vec![
        record("agentspace", HostState::Ready),
        record("agentspace", HostState::Failed),
    ]);
    assert_refused_write("duplicate project", &t, &bad);
}

#[test]
fn a_stack_named_for_another_project_is_refused() {
    let t = TempHome::new();
    write_new(&t, &seed()).unwrap();
    let mut r = record("agentspace", HostState::Ready);
    r.stack = "agentspace-host-swimcoach".to_string();
    assert_refused_write("stack of another project", &t, &with_box(vec![r]));
}

#[test]
fn a_registry_without_the_box_is_refused() {
    let t = TempHome::new();
    write_new(&t, &seed()).unwrap();
    assert_refused_write("empty", &t, &hosts(vec![]));
    assert_refused_write(
        "only another host",
        &t,
        &hosts(vec![record("agentspace", HostState::Ready)]),
    );
}

#[test]
fn a_registry_without_the_box_is_refused_when_no_file_exists_too() {
    let t = TempHome::new();
    assert_refused_write(
        "no file yet",
        &t,
        &hosts(vec![record("agentspace", HostState::Creating)]),
    );
    assert!(!t.hosts_json().exists());
}

#[test]
fn two_agentbox_records_are_refused() {
    let t = TempHome::new();
    write_new(&t, &seed()).unwrap();
    let mut second = box_host();
    second.project = "agentspace".to_string();
    assert_refused_write("a second agentbox", &t, &with_box(vec![second]));
    assert_refused_write("the box twice", &t, &with_box(vec![box_host()]));
}

#[test]
fn an_agentbox_record_of_another_project_is_refused() {
    let t = TempHome::new();
    let mut b = box_host();
    b.project = "agentspace".to_string();
    assert_refused_write("agentbox for agentspace", &t, &hosts(vec![b]));
}

#[test]
fn a_bad_instance_id_is_refused() {
    let t = TempHome::new();
    write_new(&t, &seed()).unwrap();
    for id in [
        "i-0123",
        "I-1234abcd",
        "i-1234567890ABCDEF0",
        "",
        "i-1234abcd\n",
        "x",
    ] {
        let mut r = record("agentspace", HostState::Ready);
        r.instance_id = Some(id.to_string());
        assert_refused_write(&format!("instance id {id:?}"), &t, &with_box(vec![r]));
    }
}

#[test]
fn a_bad_time_is_refused() {
    let t = TempHome::new();
    write_new(&t, &seed()).unwrap();
    for at in [
        "2026-02-30T00:00:00Z",
        "2026-11-20T23:59:60Z",
        "2026-11-20T19:05:42+00:00",
        "2026-11-20",
        "",
        "2026-11-20T19:05:42Z\n",
    ] {
        let mut r = record("agentspace", HostState::Creating);
        r.last_attempt_at = Some(at.to_string());
        assert_refused_write(&format!("time {at:?}"), &t, &with_box(vec![r]));
    }
}

#[test]
fn a_bad_project_or_stack_is_refused() {
    let t = TempHome::new();
    write_new(&t, &seed()).unwrap();
    let too_long = "a".repeat(33);
    for project in [
        "",
        "Agentspace",
        "-a",
        "a-",
        "a b",
        "a.b",
        too_long.as_str(),
    ] {
        let mut r = record("agentspace", HostState::Ready);
        r.project = project.to_string();
        r.stack = format!("agentspace-host-{project}");
        assert_refused_write(&format!("project {project:?}"), &t, &with_box(vec![r]));
    }
    for stack in [
        "",
        "agentspace-host.agentspace",
        "agentspace-host-",
        "other",
        "Agentbox",
    ] {
        let mut r = record("agentspace", HostState::Ready);
        r.stack = stack.to_string();
        assert_refused_write(&format!("stack {stack:?}"), &t, &with_box(vec![r]));
    }
}

#[test]
fn the_longest_project_is_written() {
    let t = TempHome::new();
    let p = "a".repeat(32);
    write_new(&t, &with_box(vec![record(&p, HostState::Creating)])).unwrap();
    assert_eq!(read(t.path()).unwrap().hosts[1].project, p);
}

#[test]
fn a_refused_first_write_creates_no_registry() {
    let t = TempHome::new();
    let mut r = record("agentspace", HostState::Ready);
    r.instance_id = Some("bad".to_string());
    assert_refused_write("first write", &t, &with_box(vec![r]));
    assert!(!t.hosts_json().exists());
}

// --- Where the file cannot be written ---------------------------------------------------------

#[test]
fn a_folder_at_the_registrys_path_makes_a_write_fail_and_changes_nothing() {
    let t = TempHome::new();
    std::fs::create_dir_all(t.hosts_json()).unwrap();
    let guard = lock(t.path()).unwrap();
    let e = write(t.path(), &seed(), &guard).expect_err("A folder cannot be replaced by a file");
    assert!(is_write_failure(&e), "{e:?}");
    assert_message(&e, t.path());
    assert!(t.hosts_json().is_dir());
    assert!(
        TempHome::names_in(&t.hosts_json()).is_empty(),
        "Nothing may be put in the folder"
    );
    drop(guard);
    assert!(
        TempHome::names_in(&t.state())
            .iter()
            .all(|n| n == "hosts.json" || n == "hosts.lock"),
        "No temporary file may be left"
    );
}

#[test]
fn a_file_where_the_agentspace_folder_belongs_makes_a_write_fail() {
    let t = TempHome::new();
    std::fs::write(t.state(), b"x").unwrap();
    let e = match lock(t.path()) {
        Err(e) => e,
        Ok(guard) => write(t.path(), &seed(), &guard).expect_err("A write must fail"),
    };
    assert_message(&e, t.path());
    assert!(t.state().is_file(), "The file is left as it was");
}

#[test]
fn a_file_where_the_home_folder_belongs_makes_a_write_fail() {
    let t = TempHome::new();
    let home_as_file = t.path().join("a-file");
    std::fs::write(&home_as_file, b"x").unwrap();
    let e = match lock(&home_as_file) {
        Err(e) => e,
        Ok(guard) => write(&home_as_file, &seed(), &guard).expect_err("A write must fail"),
    };
    assert_message(&e, t.path());
    assert!(home_as_file.is_file());
}
