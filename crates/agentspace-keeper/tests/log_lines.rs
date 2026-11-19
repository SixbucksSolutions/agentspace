//! The bytes of every log line (docs/keeper.md, Log lines; docs/state.md, The keeper's logs).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. **The keeper appends its
//! lines to `board-holder.log`, and writes each line to stdout as well, as it is logged.** The
//! file is UTF-8 without a byte order mark, one line per entry, each ending in LF. A line is
//! `<time> <action> lease=<lease> alias=<alias> pid=<pid>[ reason=<reason>]`, the time the pass
//! time in `YYYY-MM-DDTHH:MM:SSZ`; a whitespace character inside a value is written `_`; `reason`
//! is for `failed` and `skipped` only, each run of control characters in it is one space, it is
//! trimmed, and an empty one is `Unknown.`. A pass writes one action line, before which it MAY
//! write one `rotated` line. **A pass whose own line is dropped ends as a pass that met an
//! error**, and a one-pass run exits 1.

mod common;

use agentspace_keeper_api::{HolderRecord, Started, Stopped};
use common::*;

// ---- The examples of the doc, byte for byte ----

const EX_ID: &str = "0b6f4c1e-8d1a-4f0e-9c3b-2a5d7e9f1c44";
const EX_ALIAS: &str = "agentspace-swimcoach-0b6f4c1e";

fn example_record(pid: u32) -> HolderRecord {
    HolderRecord {
        pid,
        process_started_at: "2026-11-18T05:06:59.500Z".to_string(),
        lease_id: EX_ID.to_string(),
        alias: EX_ALIAS.to_string(),
    }
}

#[test]
fn the_started_example_is_what_a_start_writes() {
    let h = Harness::new();
    h.write_leases(vec![ready_with_alias(
        EX_ID,
        "2026-11-18T09:00:00Z",
        EX_ALIAS,
    )]);
    h.set_start(Started::Holder(example_record(24112)));
    h.pass("2026-11-18T05:07:00Z");
    assert_eq!(
        h.log_bytes(),
        b"2026-11-18T05:07:00Z started lease=0b6f4c1e-8d1a-4f0e-9c3b-2a5d7e9f1c44 alias=agentspace-swimcoach-0b6f4c1e pid=24112\n"
    );
}

#[test]
fn the_kept_example_is_what_a_kept_holder_writes() {
    let h = Harness::new();
    h.write_leases(vec![ready_with_alias(
        EX_ID,
        "2026-11-18T09:00:00Z",
        EX_ALIAS,
    )]);
    h.trust(&example_record(24112));
    h.pass("2026-11-18T05:07:30Z");
    assert_eq!(
        h.log_bytes(),
        b"2026-11-18T05:07:30Z kept lease=0b6f4c1e-8d1a-4f0e-9c3b-2a5d7e9f1c44 alias=agentspace-swimcoach-0b6f4c1e pid=24112\n"
    );
}

#[test]
fn the_stopped_example_is_what_a_stop_writes() {
    let h = Harness::new();
    h.write_leases(vec![ready_with_alias(
        EX_ID,
        "2026-11-18T06:00:01Z",
        EX_ALIAS,
    )]);
    h.trust(&example_record(24112));
    h.pass("2026-11-18T06:00:01Z");
    assert_eq!(
        h.log_bytes(),
        b"2026-11-18T06:00:01Z stopped lease=0b6f4c1e-8d1a-4f0e-9c3b-2a5d7e9f1c44 alias=agentspace-swimcoach-0b6f4c1e pid=24112\n"
    );
}

#[test]
fn the_idle_and_skipped_examples_are_what_those_passes_write() {
    let h = Harness::new();
    h.pass("2026-11-18T06:01:36Z");
    assert_eq!(
        h.log_bytes(),
        b"2026-11-18T06:01:36Z idle lease=- alias=- pid=-\n"
    );

    let h = Harness::new();
    *h.fakes.locks.pass_mode.borrow_mut() = PassMode::Skipped;
    h.pass("2026-11-18T06:02:06Z");
    assert_eq!(
        h.log_bytes(),
        b"2026-11-18T06:02:06Z skipped lease=- alias=- pid=- reason=locked\n"
    );
}

#[test]
fn the_rotated_line_is_the_time_and_dashes() {
    let h = Harness::new();
    std::fs::write(h.ssh_log_path(), vec![b'x'; 1_048_577]).unwrap();
    h.pass(NOW);
    assert_eq!(
        h.log_bytes(),
        b"2026-11-19T12:00:00Z rotated lease=- alias=- pid=-\n2026-11-19T12:00:00Z idle lease=- alias=- pid=-\n"
    );
}

// ---- The file's bytes ----

#[test]
fn the_file_is_utf8_without_a_byte_order_mark_with_lf_line_endings() {
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.set_start(Started::Failed("Zażółć gęślą jaźń \u{1F600}".into()));
    h.pass(NOW);
    let bytes = h.log_bytes();
    assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]), "A byte order mark");
    assert!(!bytes.contains(&b'\r'), "A carriage return");
    assert_eq!(bytes.last(), Some(&b'\n'));
    assert_eq!(bytes.iter().filter(|b| **b == b'\n').count(), 1);
    let text = String::from_utf8(bytes).expect("UTF-8");
    assert!(
        text.contains("reason=Zażółć gęślą jaźń \u{1F600}\n"),
        "{text}"
    );
}

#[test]
fn lines_are_appended_never_rewritten() {
    let h = Harness::new();
    let old = "2026-11-18T05:07:00Z idle lease=- alias=- pid=-\nsome older line\n";
    std::fs::write(h.log_path(), old).unwrap();
    h.pass("2026-11-19T12:00:00Z");
    h.pass("2026-11-19T12:00:30Z");
    assert_eq!(
        h.log(),
        format!(
            "{old}{}{}",
            idle_line("2026-11-19T12:00:00Z"),
            idle_line("2026-11-19T12:00:30Z")
        )
    );
}

#[test]
fn the_log_is_opened_for_each_line_so_a_log_that_was_removed_comes_back() {
    let h = Harness::new();
    h.pass("2026-11-19T12:00:00Z");
    std::fs::remove_file(h.log_path()).unwrap();
    h.pass("2026-11-19T12:00:30Z");
    assert_eq!(h.log(), idle_line("2026-11-19T12:00:30Z"));
}

#[test]
fn the_log_is_never_rotated_whatever_its_size() {
    let h = Harness::new();
    let old = "x".repeat(1_100_000) + "\n";
    std::fs::write(h.log_path(), &old).unwrap();
    h.pass(NOW);
    assert_eq!(h.log(), format!("{old}{}", idle_line(NOW)));
    assert!(!h.state.join("board-holder.log.1").exists());
}

#[test]
fn every_line_is_also_written_to_stdout_as_it_is_logged() {
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.start_echo(4242);
    std::fs::write(h.ssh_log_path(), vec![b'x'; 1_048_577]).unwrap();
    let before = h.log_bytes();
    let ran = h.pass(NOW);
    assert_eq!(ran.exit, 0);
    assert_eq!(ran.stderr, "");
    assert_eq!(ran.stdout.lines().count(), 2, "{}", ran.stdout);
    assert_stdout_is_log_since(&h, &before, &ran);
}

#[test]
fn a_pass_writes_one_action_line_and_at_most_one_rotated_line_before_it() {
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.trust(&holder_for(4242, ID_A));
    std::fs::write(h.ssh_log_path(), vec![b'x'; 1_048_577]).unwrap();
    h.pass(NOW);
    let log = h.log();
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains(" rotated "));
    assert!(lines[1].contains(" kept "));
}

#[test]
fn the_time_is_the_whole_second_in_utc_with_a_z() {
    let h = Harness::new();
    h.fakes.clock.set_ms(ms_of(2026, 1, 2, 3, 4, 5) + 999);
    h.pass_on_clock();
    assert_eq!(h.log(), idle_line("2026-01-02T03:04:05Z"));
    let h = Harness::new();
    h.fakes.clock.set_ms(ms_of(2026, 12, 31, 23, 59, 59));
    h.pass_on_clock();
    assert_eq!(h.log(), idle_line("2026-12-31T23:59:59Z"));
}

// ---- Values and reasons ----

#[test]
fn any_whitespace_character_inside_a_value_is_written_as_an_underscore() {
    let h = Harness::new();
    let r = HolderRecord {
        pid: 4242,
        process_started_at: "2026-11-19T11:30:00.123Z".to_string(),
        lease_id: "id with\tblanks".to_string(),
        alias: "agentspace x\u{a0}y\u{2003}z".to_string(),
    };
    h.trust(&r);
    h.write_leases(vec![]);
    h.pass(NOW);
    assert_eq!(
        h.log(),
        "2026-11-19T12:00:00Z stopped lease=id_with_blanks alias=agentspace_x_y_z pid=4242\n"
    );
}

#[test]
fn a_failed_stops_line_has_the_records_values_with_the_same_rule() {
    let h = Harness::new();
    let r = HolderRecord {
        pid: 9,
        process_started_at: "2026-11-19T11:30:00.123Z".to_string(),
        lease_id: ID_A.to_string(),
        alias: "a b".to_string(),
    };
    h.trust(&r);
    h.write_leases(vec![]);
    h.set_stop(Stopped::StillRunning);
    h.pass(NOW);
    assert_eq!(
        h.log(),
        format!(
            "{NOW} failed lease={ID_A} alias=a_b pid=9 reason=The holder is still running after it was stopped.\n"
        )
    );
}

#[test]
fn reason_is_the_last_field_and_runs_to_the_end_of_the_line_blanks_inside_it_kept() {
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.set_start(Started::Failed("one  two three".into()));
    h.pass(NOW);
    assert_eq!(
        h.log(),
        format!(
            "{NOW} failed lease={ID_A} alias={} pid=- reason=one  two three\n",
            alias_of(ID_A)
        )
    );
}

#[test]
fn no_line_but_failed_and_skipped_carries_a_reason() {
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.start_echo(4242);
    h.pass(NOW);
    h.pass("2026-11-19T12:00:10Z");
    for l in h.log().lines() {
        assert!(!l.contains(" reason="), "{l}");
    }
}

// ---- A line that cannot be written ----

fn make_log_unwritable(h: &Harness) {
    // A folder where the file belongs: every open of it fails.
    std::fs::create_dir_all(h.log_path()).unwrap();
}

#[test]
fn a_pass_whose_own_line_cannot_be_logged_exits_1_and_what_it_did_stands() {
    let h = Harness::new();
    make_log_unwritable(&h);
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.set_start(Started::Holder(HolderRecord {
        pid: 31337,
        process_started_at: "2026-11-19T11:59:58.500Z".to_string(),
        lease_id: ID_A.to_string(),
        alias: alias_of(ID_A),
    }));
    let ran = h.pass(NOW);
    assert_eq!(ran.exit, 1);
    // A holder it started stays started and recorded.
    assert_eq!(h.starts(), 1);
    assert_eq!(h.record().map(|r| r.pid), Some(31337));
    assert_eq!(h.stops(), 0);
    assert_lock_released_last(&h.events());
}

#[test]
fn a_skipped_line_is_a_passs_own_line_so_losing_it_exits_1() {
    let h = Harness::new();
    make_log_unwritable(&h);
    *h.fakes.locks.pass_mode.borrow_mut() = PassMode::Skipped;
    let ran = h.pass(NOW);
    assert_eq!(ran.exit, 1);
}

#[test]
fn an_idle_line_that_cannot_be_logged_exits_1() {
    let h = Harness::new();
    make_log_unwritable(&h);
    let ran = h.pass(NOW);
    assert_eq!(ran.exit, 1);
    assert_lock_released_last(&h.events());
}

#[test]
fn a_stop_stands_when_its_line_is_dropped() {
    let h = Harness::new();
    make_log_unwritable(&h);
    let r = holder_for(4242, ID_A);
    h.trust(&r);
    h.write_leases(vec![]);
    let ran = h.pass(NOW);
    assert_eq!(ran.exit, 1);
    assert_eq!(h.stops(), 1);
    assert_eq!(h.record(), None);
}

#[test]
fn only_the_one_action_line_decides_the_exit_code_when_the_log_works() {
    // The exit codes of the other branches, once for each: ran and logged means 0, whatever the
    // action (a failed start included).
    let h = Harness::new();
    h.write_leases(vec![ready(ID_A, LATER)]);
    h.set_start(Started::Failed("x".into()));
    assert_eq!(h.pass(NOW).exit, 0);
}
