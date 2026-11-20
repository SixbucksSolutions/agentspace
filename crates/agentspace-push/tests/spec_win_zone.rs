//! The zone the push sends, on Windows: the seam's override, the legacy names, the check, and the
//! laptop's own zone read from the system (docs/operator.md, The time zone; The test seam).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

mod win_support;

use win_support::*;

#[test]
fn the_seam_zone_is_sent_on_the_remote_command_and_covered_by_the_stamp() {
    for zone in [
        "Europe/Paris",
        "Etc/UTC",
        "UTC",
        "America/Argentina/Buenos_Aires",
        "Etc/GMT+5",
        "Asia/Kolkata",
    ] {
        let mut rig = Rig::new();
        rig.set_timezone(Some(zone));
        rig.answer_zone(zone);
        let say = rig.up();
        assert_eq!(rig.calls(), 1, "{zone}: no push");
        assert_warnings(&say, &[], zone);
        let args = rig.args(1);
        assert_eq!(args.len(), 6);
        assert!(
            args[5].contains(zone),
            "{zone}: the remote command does not name it"
        );
        assert!(
            rig.stamp_matches(zone),
            "{zone}: the stamp does not cover it"
        );
    }
}

#[test]
fn the_seven_legacy_names_are_sent_as_their_current_names() {
    let legacy = [
        ("Asia/Calcutta", "Asia/Kolkata"),
        ("Asia/Katmandu", "Asia/Kathmandu"),
        ("Asia/Rangoon", "Asia/Yangon"),
        ("Europe/Kiev", "Europe/Kyiv"),
        ("America/Buenos_Aires", "America/Argentina/Buenos_Aires"),
        ("America/Godthab", "America/Nuuk"),
        ("America/Indianapolis", "America/Indiana/Indianapolis"),
    ];
    let mut rig = Rig::new();
    let mut calls = 0;
    for (old, new) in legacy {
        rig.set_timezone(Some(old));
        rig.answer_zone(new);
        let say = rig.up();
        calls += 1;
        assert_eq!(rig.calls(), calls, "{old}: no push");
        assert_warnings(&say, &[], old);
        let command = rig.args(calls).pop().expect("a remote command");
        assert!(
            command.contains(new),
            "{old}: the remote command does not name {new}"
        );
        assert!(
            !command.contains(old),
            "{old}: the remote command still names it"
        );
        assert!(
            rig.stamp_matches(new),
            "{old}: the stamp does not cover {new}"
        );
    }
}

#[test]
fn a_zone_that_fails_the_check_is_never_sent() {
    let mut rig = Rig::new();
    rig.answer_zone("");
    let mut calls = 0;
    for bad in [
        "-America/New_York",
        "America/New York",
        "a;b",
        "America//New_York",
        "Europe/Paris/",
        "/Europe/Paris",
        "Europe/Pa$ris",
        "Europe/Paris\n",
        "Europe/Par'is",
        "not a zone",
    ] {
        rig.set_timezone(Some(bad));
        let _ = std::fs::remove_file(rig.stamp_path());
        let say = rig.up();
        calls += 1;
        assert_eq!(rig.calls(), calls, "{bad:?}: the token was not pushed");
        assert_warnings(&say, &[NO_ZONE], bad);
        let command = rig.args(calls).pop().expect("a remote command");
        assert!(
            !command.contains(bad.trim()),
            "{bad:?}: the bad zone is on the box's command line"
        );
        assert!(
            rig.stamp_matches(""),
            "{bad:?}: the stamp does not carry an empty zone"
        );
    }
}

fn expected_iana(windows_id: &str) -> Option<&'static str> {
    Some(match windows_id {
        "Pacific Standard Time" => "America/Los_Angeles",
        "Eastern Standard Time" => "America/New_York",
        "Central Standard Time" => "America/Chicago",
        "Mountain Standard Time" => "America/Denver",
        "UTC" => "Etc/UTC",
        "GMT Standard Time" => "Europe/London",
        "W. Europe Standard Time" => "Europe/Berlin",
        "Romance Standard Time" => "Europe/Paris",
        "Tokyo Standard Time" => "Asia/Tokyo",
        _ => return None,
    })
}

/// The laptop's zone is read from the system when `AGENTSPACE_TIMEZONE` is unset or empty, and
/// the box refuses it, so that the warning names what the push sent.
fn zone_the_push_worked_out(seam: Option<&str>) -> (Vec<String>, String) {
    let mut rig = Rig::new();
    rig.set_timezone(seam);
    rig.scenario("zonefailed");
    let say = rig.up();
    assert_eq!(rig.calls(), 1, "The push did not start its ssh");
    let args = rig.args(1);
    (say.warnings(), args.last().cloned().unwrap_or_default())
}

fn check_laptop_zone(warnings: &[String], command: &str) {
    let Some(id) = laptop_windows_zone_id() else {
        // No way to know what the laptop is set to; the push must still have said one thing.
        assert_eq!(
            warnings.len(),
            1,
            "The push gave {} warnings",
            warnings.len()
        );
        return;
    };
    match expected_iana(&id) {
        Some(iana) => {
            let want = zone_not_taken(iana);
            assert!(
                warnings.len() == 1 && warnings[0] == want,
                "The laptop's zone ({id}) was not sent as {iana}; {} warning(s)",
                warnings.len()
            );
            assert!(
                command.contains(iana),
                "The remote command does not name {iana}"
            );
        }
        None => {
            assert_eq!(
                warnings.len(),
                1,
                "The push gave {} warnings",
                warnings.len()
            );
            let w = &warnings[0];
            assert!(
                w == NO_ZONE
                    || w.starts_with("agentspace: The instance did not take the time zone "),
                "An unexpected warning for the laptop's zone ({id})"
            );
        }
    }
}

#[test]
fn with_the_seam_unset_the_laptops_own_zone_is_read_from_the_system() {
    let (warnings, command) = zone_the_push_worked_out(None);
    check_laptop_zone(&warnings, &command);
}

#[test]
fn with_the_seam_empty_the_laptops_own_zone_is_read_from_the_system() {
    let (warnings, command) = zone_the_push_worked_out(Some(""));
    check_laptop_zone(&warnings, &command);
}

#[test]
fn the_seam_zone_takes_the_place_of_the_laptops() {
    let mut rig = Rig::new();
    rig.set_timezone(Some("Pacific/Auckland"));
    rig.scenario("zonefailed");
    let say = rig.up();
    let want = zone_not_taken("Pacific/Auckland");
    assert_warnings(&say, &[want.as_str()], "The seam's zone");
}
