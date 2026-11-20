//! Each outcome of the push, on Windows, as the box answers it through a stand-in ssh
//! (docs/operator.md, What the box does; What the laptop checks; The stamp).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The stand-in's scenarios are listed in tests/win_support/ssh-standin.ps1. A push never fails
//! the command, so every outcome here is a warning or nothing, and a stamp is written only when
//! the box confirmed the token and, when a zone was sent, the zone.
#![cfg(windows)]

mod win_support;

use win_support::*;

#[test]
fn a_confirmed_push_writes_the_stamp_and_says_only_the_sending_line() {
    let rig = Rig::new();
    let say = rig.up();
    assert_eq!(say.progress_lines(), vec![SENDING.to_string()]);
    assert_warnings(&say, &[], "A confirmed push");
    assert!(
        rig.stamp_matches(DEFAULT_ZONE),
        "The stamp is not the SHA-256 of the instance, the zone and the content, in the state file's form"
    );
    assert!(
        !rig.stamp_temp_left(),
        "The stamp's temporary file was left behind"
    );
}

#[test]
fn the_stamp_folder_is_created_when_it_is_missing() {
    let rig = Rig::new();
    assert!(!rig.agentspace_dir().exists());
    rig.up();
    assert!(rig.stamp_matches(DEFAULT_ZONE));
}

#[test]
fn an_unchanged_token_and_zone_send_nothing_and_say_nothing() {
    let rig = Rig::new();
    rig.up();
    assert_eq!(rig.calls(), 1);
    let before = rig.stamp_bytes();
    let say = rig.up();
    assert_eq!(rig.calls(), 1, "A push that is not due started an ssh");
    assert!(say.silent(), "A push that is not due said something");
    assert!(rig.stamp_bytes() == before, "The stamp changed");
}

#[test]
fn a_changed_zone_makes_the_push_due_and_the_stamp_covers_the_new_zone() {
    let mut rig = Rig::new();
    rig.up();
    rig.set_timezone(Some("Europe/Paris"));
    rig.answer_zone("Europe/Paris");
    let say = rig.up();
    assert_eq!(rig.calls(), 2, "A changed zone did not start an ssh");
    assert_warnings(&say, &[], "A changed zone");
    assert!(rig.stamp_matches("Europe/Paris"));
}

#[test]
fn a_changed_token_makes_the_push_due_and_the_new_token_is_read_afresh() {
    let mut rig = Rig::new();
    rig.up();
    let old_token = rig.token.clone();
    rig.token = dummy_token();
    rig.write_credential();
    let say = rig.up();
    assert_eq!(rig.calls(), 2, "A changed token did not start an ssh");
    assert_warnings(&say, &[], "A changed token");
    let sent = rig.stdin(2);
    assert!(
        bytes_contain(&sent, rig.token.as_bytes()),
        "The second push did not send the new token"
    );
    assert!(
        !bytes_contain(&sent, old_token.as_bytes()),
        "The second push sent the old token"
    );
    assert!(rig.stamp_matches(DEFAULT_ZONE));
}

#[test]
fn a_changed_url_makes_the_push_due() {
    let mut rig = Rig::new();
    rig.up();
    rig.base = "http://127.0.0.1:18899".to_string();
    rig.write_receipt();
    rig.up();
    assert_eq!(rig.calls(), 2, "A changed URL did not start an ssh");
    assert!(rig.stamp_matches(DEFAULT_ZONE));
}

#[test]
fn a_deleted_stamp_makes_the_push_due_and_the_stamp_comes_back_equal() {
    let rig = Rig::new();
    rig.up();
    let first = rig.stamp_bytes();
    std::fs::remove_file(rig.stamp_path()).unwrap();
    rig.up();
    assert_eq!(rig.calls(), 2, "A deleted stamp did not start an ssh");
    assert!(
        rig.stamp_bytes() == first,
        "The stamp did not come back equal"
    );
}

#[test]
fn a_stamp_that_cannot_be_read_makes_the_push_due() {
    for junk in [
        &b""[..],
        b"not json",
        b"{\"version\":0,\"stamp\":\"abc\"}",
        b"\xFF\xFE{\x00}\x00",
    ] {
        let rig = Rig::new();
        std::fs::create_dir_all(rig.agentspace_dir()).unwrap();
        std::fs::write(rig.stamp_path(), junk).unwrap();
        let say = rig.up();
        assert_eq!(
            rig.calls(),
            1,
            "An unreadable stamp did not make the push due"
        );
        assert_warnings(&say, &[], "An unreadable stamp");
        assert!(
            rig.stamp_matches(DEFAULT_ZONE),
            "The stamp was not rewritten"
        );
    }
}

#[test]
fn a_stamp_with_a_byte_order_mark_and_whitespace_is_read_as_current() {
    let rig = Rig::new();
    std::fs::create_dir_all(rig.agentspace_dir()).unwrap();
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend(
        format!(
            "\r\n {{ \"version\": 0, \"stamp\": \"{}\" }}\r\n",
            rig.expected_stamp(DEFAULT_ZONE)
        )
        .into_bytes(),
    );
    std::fs::write(rig.stamp_path(), bytes).unwrap();
    let say = rig.up();
    assert_eq!(
        rig.calls(),
        0,
        "A matching stamp with a mark and whitespace was not accepted"
    );
    assert!(say.silent());
}

#[test]
fn the_stamp_that_the_prototype_wrote_is_never_read_or_removed() {
    let rig = Rig::new();
    std::fs::create_dir_all(rig.agentspace_dir()).unwrap();
    let old = rig.agentspace_dir().join("localswim-pushed.sha256");
    std::fs::write(&old, rig.expected_stamp(DEFAULT_ZONE)).unwrap();
    rig.up();
    assert_eq!(
        rig.calls(),
        1,
        "The prototype's stamp made the push not due"
    );
    assert_eq!(
        std::fs::read_to_string(&old).unwrap(),
        rig.expected_stamp(DEFAULT_ZONE),
        "The prototype's stamp was changed"
    );
}

#[test]
fn an_ssh_that_exits_255_warns_and_writes_no_stamp() {
    let rig = Rig::new();
    rig.scenario("exit255");
    let say = rig.up();
    assert_eq!(rig.calls(), 1);
    assert_warnings(&say, &[NOT_CONFIRMED], "An ssh that exits 255");
    assert!(rig.stamp_bytes().is_none(), "A stamp was written");
}

#[test]
fn a_failed_push_leaves_the_stored_stamp_as_it_was() {
    let rig = Rig::new();
    rig.write_stamp_for("Etc/Earlier");
    let before = rig.stamp_bytes();
    rig.scenario("exit255");
    let say = rig.up();
    assert_warnings(&say, &[NOT_CONFIRMED], "A failed push");
    assert!(rig.stamp_bytes() == before, "The stored stamp changed");
}

#[test]
fn a_non_zero_exit_is_not_a_confirmation_even_with_the_right_hash() {
    let rig = Rig::new();
    rig.scenario("exit1hash");
    let say = rig.up();
    assert_warnings(&say, &[NOT_CONFIRMED], "A non-zero exit with the hash");
    assert!(rig.stamp_bytes().is_none());
}

#[test]
fn an_answer_with_no_hash_line_warns_and_checks_no_zone() {
    let rig = Rig::new();
    rig.scenario("nohash");
    let say = rig.up();
    assert_warnings(&say, &[NOT_CONFIRMED], "An answer with no hash line");
    assert!(rig.stamp_bytes().is_none());
}

#[test]
fn a_hash_that_is_not_the_expected_one_warns() {
    let rig = Rig::new();
    rig.scenario("wronghash");
    let say = rig.up();
    assert_warnings(&say, &[NOT_CONFIRMED], "A wrong hash");
    assert!(rig.stamp_bytes().is_none());
}

#[test]
fn the_hash_is_compared_without_regard_to_case() {
    let rig = Rig::new();
    rig.scenario("upperhash");
    let say = rig.up();
    assert_warnings(&say, &[], "An uppercase hash");
    assert!(rig.stamp_matches(DEFAULT_ZONE));
}

#[test]
fn a_banner_and_crlf_line_ends_and_decoys_before_the_hash_line_are_skipped() {
    let rig = Rig::new();
    rig.scenario("banner");
    let say = rig.up();
    assert_warnings(&say, &[], "A banner, CRLF line ends and decoys");
    assert!(rig.stamp_matches(DEFAULT_ZONE));
}

#[test]
fn a_hash_line_that_ends_the_output_with_no_lf_still_counts() {
    let mut rig = Rig::new();
    // No zone is sent, so the hash line is all the box says.
    rig.set_timezone(Some("not a zone"));
    rig.answer_zone("");
    rig.scenario("nolf");
    let say = rig.up();
    assert_warnings(&say, &[NO_ZONE], "A hash line with no LF");
    assert!(
        rig.stamp_matches(""),
        "The token was not confirmed from the last line"
    );
}

#[test]
fn a_zone_line_that_ends_the_output_with_no_lf_still_counts() {
    let rig = Rig::new();
    rig.scenario("nolfzone");
    let say = rig.up();
    assert_warnings(&say, &[], "A zone line with no LF");
    assert!(rig.stamp_matches(DEFAULT_ZONE));
}

#[test]
fn a_box_that_answers_zone_failed_warns_with_the_zone_and_writes_no_stamp() {
    let rig = Rig::new();
    rig.scenario("zonefailed");
    let say = rig.up();
    let warning = zone_not_taken(DEFAULT_ZONE);
    assert_warnings(&say, &[warning.as_str()], "A box that refuses the zone");
    assert!(rig.stamp_bytes().is_none(), "A stamp was written");
}

#[test]
fn a_box_that_prints_no_zone_line_warns_and_writes_no_stamp() {
    let rig = Rig::new();
    rig.scenario("nozone");
    let say = rig.up();
    let warning = zone_not_taken(DEFAULT_ZONE);
    assert_warnings(&say, &[warning.as_str()], "A box that prints no zone line");
    assert!(rig.stamp_bytes().is_none());
}

#[test]
fn a_zone_line_is_compared_case_sensitively() {
    let rig = Rig::new();
    rig.scenario("zonecase");
    let say = rig.up();
    let warning = zone_not_taken(DEFAULT_ZONE);
    assert_warnings(&say, &[warning.as_str()], "A zone line in another case");
    assert!(rig.stamp_bytes().is_none());
}

#[test]
fn a_zone_line_for_another_zone_is_not_a_confirmation() {
    let rig = Rig::new();
    rig.scenario("zoneother");
    let say = rig.up();
    let warning = zone_not_taken(DEFAULT_ZONE);
    assert_warnings(&say, &[warning.as_str()], "A zone line for another zone");
    assert!(rig.stamp_bytes().is_none());
}

#[test]
fn a_zone_that_was_not_taken_leaves_the_next_push_due() {
    let rig = Rig::new();
    rig.scenario("zonefailed");
    rig.up();
    rig.scenario("ok");
    let say = rig.up();
    assert_eq!(
        rig.calls(),
        2,
        "The next push was not due after a refused zone"
    );
    assert_warnings(&say, &[], "The push after a refused zone");
    assert!(rig.stamp_matches(DEFAULT_ZONE));
}

#[test]
fn a_zone_that_fails_the_check_gives_the_warning_and_a_stamp_with_an_empty_zone() {
    let mut rig = Rig::new();
    rig.set_timezone(Some("not a zone"));
    rig.answer_zone("");
    let say = rig.up();
    assert_eq!(rig.calls(), 1, "The token was not pushed without a zone");
    assert_warnings(&say, &[NO_ZONE], "A zone that fails the check");
    assert!(
        rig.stamp_matches(""),
        "The stamp does not carry an empty zone"
    );
}

#[test]
fn a_stamp_that_cannot_be_written_warns_could_not_send() {
    let rig = Rig::new();
    // A folder where the stamp file goes: the replace cannot succeed.
    std::fs::create_dir_all(rig.stamp_path()).unwrap();
    let say = rig.up();
    assert_eq!(rig.calls(), 1);
    assert_warnings(&say, &[COULD_NOT_SEND], "A stamp that cannot be written");
    assert!(
        rig.stamp_path().is_dir(),
        "The folder at the stamp's path was replaced"
    );
}

#[test]
fn a_slow_but_successful_ssh_is_waited_for() {
    let rig = Rig::new();
    rig.scenario("slow");
    let started = std::time::Instant::now();
    let say = rig.up();
    assert!(
        started.elapsed().as_secs() < 18,
        "A 4-second ssh took too long"
    );
    assert_warnings(&say, &[], "A slow ssh");
    assert!(rig.stamp_matches(DEFAULT_ZONE));
}

#[test]
fn caller_mode_prints_the_same_warnings_and_no_caller_line_for_the_sending_line() {
    let rig = Rig::new();
    rig.scenario("exit255");
    let say = FakeSay::caller();
    rig.push(&say, DEFAULT_ALIAS, false);
    assert_warnings(&say, &[NOT_CONFIRMED], "A failed push in caller mode");
    assert!(say.progress_caller_lines().iter().all(Option::is_none));
}

#[test]
fn the_push_with_no_receipt_is_silent_and_starts_no_ssh() {
    let rig = Rig::new();
    std::fs::remove_file(rig.receipt_path()).unwrap();
    let say = rig.up();
    assert_eq!(rig.calls(), 0);
    assert!(say.silent(), "A missing receipt was reported");
    assert!(rig.stamp_bytes().is_none());
}

#[test]
fn a_push_never_panics_when_every_file_is_missing() {
    let rig = Rig::new();
    std::fs::remove_dir_all(rig.local_swim_dir()).unwrap();
    let say = rig.up();
    assert_eq!(rig.calls(), 0);
    assert!(say.silent());
}
