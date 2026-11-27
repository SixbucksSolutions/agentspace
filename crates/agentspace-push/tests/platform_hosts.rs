//! The token push to a host of a project on a platform: its stamp per host, the zone for a
//! Windows host, and the push's remote half going through the Windows rows (docs/operator.md,
//! "The stamp", "The token push to a Windows host", "The platform seam"; docs/state.md, "A host's
//! push stamp" and "A host's platform and names"; D91, D96; E0.12-T03-S04, #0257), written from
//! those pages alone.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! A host is named by its host name, `<project>` on `linux/arm64` and `<project>.<os>-<arch>` on
//! every other platform, which is its folder's name too (state.md). Every case here keeps two or
//! three hosts of ONE project, `alpha`, on different platforms, so a stamp that ignored the
//! platform would be shared between them.
//!
//! `agentspace-windows` is a skeleton while this story is built (E0.12-T03-S06), so a Windows
//! host's rows are a stand-in `HostPlatform` here, passed to the push's `push_on`. The tests assert
//! that the right row is asked, with the right input, and that its answer is what travels, and
//! none asserts operator.md's text for a Windows script, which is `agentspace-windows`' own.
//!
//! Every push runs in a throwaway home through the stand-in ssh of `win_support`; nothing reaches
//! a host, and no test prints the token or an environment's value.
#![cfg(windows)]

mod win_support;

use agentspace_operator_api::{
    Bootstrap, CallerFailure, Failure, HostPlatform, PatchMessages, SyncPlan,
};
use agentspace_push::{
    BAD_HOST, Outcome, host_name_is_valid, remote_command, stamp_path, stamp_path_for,
};
use std::path::PathBuf;
use std::sync::Mutex;
use win_support::*;

const WINDOWS_ID: &str = "Mountain Standard Time";
const WIN_HOST: &str = "alpha.windows-amd64";
const AMD_HOST: &str = "alpha.linux-amd64";
const ARM_HOST: &str = "alpha";

// ---------------------------------------------------------------------------------------------
// A stand-in for `windows/amd64`'s rows of the platform seam.
// ---------------------------------------------------------------------------------------------

fn unbuilt() -> Failure {
    Failure {
        message: "agentspace: The stand-in does not build this row.".to_string(),
        caller: CallerFailure::Same,
    }
}

/// Records what each row it implements is asked, and answers a marked text built from the input,
/// so a test can see which input went into which row and that the answer is what travelled.
#[derive(Default)]
struct StandInWindows {
    zone_asked: Mutex<Vec<(String, String)>>,
    script_asked: Mutex<Vec<String>>,
    remote_asked: Mutex<Vec<String>>,
}

const SCRIPT_MARK: &str = "STANDIN_SCRIPT";
const REMOTE_MARK: &str = "STANDIN_REMOTE";

impl StandInWindows {
    fn script_for(zone: &str) -> String {
        format!("{SCRIPT_MARK}_{zone}")
    }

    fn remote_for(script: &str) -> String {
        format!("{REMOTE_MARK}_{script}")
    }
}

impl HostPlatform for StandInWindows {
    fn platform(&self) -> &'static str {
        "windows/amd64"
    }
    fn image_parameter(&self) -> Option<&'static str> {
        None
    }
    fn new_stack_instance_type(&self) -> Option<&'static str> {
        None
    }
    fn new_stack_extra_parameters(&self) -> &'static [&'static str] {
        &[]
    }
    fn login_user(&self) -> &'static str {
        "agent"
    }
    fn ssm_document(&self) -> &'static str {
        "AWS-RunPowerShellScript"
    }
    fn remote_command(&self, script: &str) -> Result<String, Failure> {
        self.remote_asked.lock().unwrap().push(script.to_string());
        Ok(Self::remote_for(script))
    }
    fn bootstrap(&self) -> Result<Bootstrap, Failure> {
        Err(unbuilt())
    }
    fn sync_plan(&self) -> Result<SyncPlan, Failure> {
        Err(unbuilt())
    }
    fn patch_check_command(&self) -> Result<String, Failure> {
        Err(unbuilt())
    }
    fn patch_messages(&self) -> PatchMessages {
        PatchMessages {
            phase: String::new(),
            log_command: String::new(),
        }
    }
    fn horizon_command(&self, _horizon: u64) -> Result<String, Failure> {
        Err(unbuilt())
    }
    fn grow_script(&self) -> Result<String, Failure> {
        Err(unbuilt())
    }
    /// The Windows rule: the laptop's Windows ID itself, and the empty one fails.
    fn push_zone(&self, windows_zone_id: &str, iana: &str) -> Result<String, Failure> {
        self.zone_asked
            .lock()
            .unwrap()
            .push((windows_zone_id.to_string(), iana.to_string()));
        if windows_zone_id.is_empty() {
            return Err(unbuilt());
        }
        Ok(windows_zone_id.to_string())
    }
    fn push_script(&self, zone: &str) -> Result<String, Failure> {
        self.script_asked.lock().unwrap().push(zone.to_string());
        Ok(Self::script_for(zone))
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------------------------

/// Pushes to `host` as the operator does: the real seam for a Linux host, the stand-in's rows for
/// the Windows one, with the laptop's Windows zone ID `WINDOWS_ID`.
fn push_to(rig: &Rig, say: &FakeSay, host: &str, standin: &StandInWindows) -> Outcome {
    rig.answer_zone(zone_of(host));
    if host.ends_with(".windows-amd64") {
        rig.push_on_platform(
            say,
            DEFAULT_ALIAS,
            false,
            Some(host),
            standin,
            Some(WINDOWS_ID),
        )
    } else {
        rig.push_to_host(say, DEFAULT_ALIAS, false, Some(host))
            .expect("The push did not panic")
    }
}

/// The zone a host of this name is sent: the IANA name for a Linux host, the Windows ID for the
/// Windows one.
fn zone_of(host: &str) -> &'static str {
    if host.ends_with(".windows-amd64") {
        WINDOWS_ID
    } else {
        DEFAULT_ZONE
    }
}

fn stamp_of(rig: &Rig, host: &str) -> PathBuf {
    stamp_path_for(&rig.context(), Some(host))
}

fn stamp_bytes_of(rig: &Rig, host: &str) -> Option<Vec<u8>> {
    std::fs::read(stamp_of(rig, host)).ok()
}

// ---------------------------------------------------------------------------------------------
// The stamp's path, per host (state.md, A host's push stamp).
// ---------------------------------------------------------------------------------------------

/// `hosts\<host name>\localswim-pushed.json`: the box's stays v0.06's, a `linux/arm64` host's is
/// `hosts\<project>\`, and any other platform's folder carries the platform. Four hosts of one
/// project and the box have five different stamps.
#[test]
fn a_hosts_stamp_is_in_the_folder_named_for_its_project_and_platform() {
    let ctx = agentspace_push::Context {
        home: PathBuf::from("throwaway-home"),
        local_app_data: None,
        system_root: PathBuf::from("system"),
        nested: false,
    };
    let hosts = PathBuf::from("throwaway-home")
        .join(".agentspace")
        .join("hosts");
    let file = "localswim-pushed.json";
    assert_eq!(
        stamp_path(&ctx),
        PathBuf::from("throwaway-home")
            .join(".agentspace")
            .join(file)
    );
    assert_eq!(stamp_path_for(&ctx, None), stamp_path(&ctx));
    assert_eq!(
        stamp_path_for(&ctx, Some(ARM_HOST)),
        hosts.join("alpha").join(file)
    );
    assert_eq!(
        stamp_path_for(&ctx, Some(WIN_HOST)),
        hosts.join("alpha.windows-amd64").join(file)
    );
    assert_eq!(
        stamp_path_for(&ctx, Some(AMD_HOST)),
        hosts.join("alpha.linux-amd64").join(file)
    );
    let mut all = vec![
        stamp_path(&ctx),
        stamp_path_for(&ctx, Some(ARM_HOST)),
        stamp_path_for(&ctx, Some(WIN_HOST)),
        stamp_path_for(&ctx, Some(AMD_HOST)),
    ];
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 4, "Every host has a stamp of its own");
}

/// state.md, A host's platform and names: a host name is a normalized project, and for a host off
/// `linux/arm64` one `.` and the name part of a supported platform, exactly. A name part that is
/// `linux-arm64` (which never appears in a name), upper-case, unsupported or doubled names no
/// host.
#[test]
fn a_host_name_is_a_project_and_for_other_platforms_the_exact_name_part() {
    for good in [
        "alpha",
        "alpha.windows-amd64",
        "alpha.linux-amd64",
        "a.windows-amd64",
        "a-b.linux-amd64",
    ] {
        assert!(host_name_is_valid(good), "{good}");
    }
    let longest = "a".repeat(32);
    assert!(host_name_is_valid(&format!("{longest}.windows-amd64")));
    for bad in [
        "alpha.linux-arm64",
        "alpha.windows-arm64",
        "alpha.Windows-amd64",
        "alpha.windows-AMD64",
        "alpha.windows",
        "alpha.amd64",
        "alpha.",
        ".windows-amd64",
        "alpha.windows-amd64.windows-amd64",
        "a.b.windows-amd64",
        "Alpha.windows-amd64",
        "alpha.windows-amd64\n",
        "alpha.windows-amd64 ",
        "alpha/windows-amd64",
        "alpha\\windows-amd64",
        "..",
    ] {
        assert!(!host_name_is_valid(bad), "{bad:?}");
    }
    assert!(!host_name_is_valid(&format!(
        "{}.windows-amd64",
        "a".repeat(33)
    )));
}

// ---------------------------------------------------------------------------------------------
// A push for one host never reads or writes another's stamp.
// ---------------------------------------------------------------------------------------------

/// Two hosts of one project on two platforms keep two stamps: a push to each writes its own file
/// and no other, over its own zone, and the box's stamp is never written.
#[test]
fn a_push_writes_its_own_hosts_stamp_alone() {
    let standin = StandInWindows::default();
    for host in [ARM_HOST, AMD_HOST, WIN_HOST] {
        let rig = Rig::new();
        let say = FakeSay::person();
        let outcome = push_to(&rig, &say, host, &standin);
        assert_eq!(outcome, Outcome::Pushed, "{host}: {:?}", say.warnings());
        assert_eq!(rig.calls(), 1, "{host}: one ssh");
        assert_eq!(
            stamp_bytes_of(&rig, host),
            Some(rig.expected_stamp_file(zone_of(host))),
            "{host}: its stamp covers its own zone"
        );
        for other in [ARM_HOST, AMD_HOST, WIN_HOST] {
            if other != host {
                assert!(
                    stamp_bytes_of(&rig, other).is_none(),
                    "{host}: {other}'s stamp was written"
                );
            }
        }
        assert!(
            rig.stamp_bytes().is_none(),
            "{host}: the box's stamp was written"
        );
        let folder = stamp_of(&rig, host).parent().unwrap().to_path_buf();
        let names: Vec<String> = std::fs::read_dir(folder)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["localswim-pushed.json".to_string()], "{host}");
    }
}

/// A push for one host MUST NOT read another's stamp: each host's own matching stamp makes only
/// its own next push not due, and a matching stamp of another platform's host of the same project
/// leaves the push due.
#[test]
fn another_hosts_matching_stamp_never_stops_a_push() {
    let standin = StandInWindows::default();
    let hosts = [ARM_HOST, AMD_HOST, WIN_HOST];
    for host in hosts {
        let rig = Rig::new();
        // Every other host of the project, and the box, hold a stamp that matches their own zone.
        rig.write_stamp_for(DEFAULT_ZONE);
        for other in hosts {
            if other == host {
                continue;
            }
            let path = stamp_of(&rig, other);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, rig.expected_stamp_file(zone_of(other))).unwrap();
        }
        let before: Vec<_> = hosts
            .iter()
            .filter(|h| **h != host)
            .map(|h| stamp_bytes_of(&rig, h))
            .collect();
        let say = FakeSay::person();
        assert_eq!(
            push_to(&rig, &say, host, &standin),
            Outcome::Pushed,
            "{host}"
        );
        assert_eq!(
            rig.calls(),
            1,
            "{host}: another host's stamp stopped the push"
        );
        let after: Vec<_> = hosts
            .iter()
            .filter(|h| **h != host)
            .map(|h| stamp_bytes_of(&rig, h))
            .collect();
        assert_eq!(
            before, after,
            "{host}: the push changed another host's stamp"
        );
        assert!(
            rig.stamp_matches(DEFAULT_ZONE),
            "{host}: the box's stamp changed"
        );

        // Its own stamp, now written, makes the next push not due.
        let again = FakeSay::person();
        assert_eq!(
            push_to(&rig, &again, host, &standin),
            Outcome::NotDue,
            "{host}"
        );
        assert_eq!(
            rig.calls(),
            1,
            "{host}: its own matching stamp did not stop the push"
        );
        assert!(
            again.silent(),
            "{host}: a push that is not due said something"
        );
    }
}

/// The box's stamp and a host's are separate: a push for the box never reads a Windows host's
/// matching stamp, and leaves it alone.
#[test]
fn a_push_for_the_box_never_reads_or_writes_a_windows_hosts_stamp() {
    let standin = StandInWindows::default();
    let rig = Rig::new();
    let win = FakeSay::person();
    assert_eq!(push_to(&rig, &win, WIN_HOST, &standin), Outcome::Pushed);
    let before = stamp_bytes_of(&rig, WIN_HOST).expect("The Windows host's stamp");
    let boxs = FakeSay::person();
    rig.answer_zone(DEFAULT_ZONE);
    rig.push(&boxs, DEFAULT_ALIAS, false);
    assert_eq!(
        rig.calls(),
        2,
        "The Windows host's stamp stopped the box's push"
    );
    assert!(rig.stamp_matches(DEFAULT_ZONE), "The box's stamp is wrong");
    assert_eq!(stamp_bytes_of(&rig, WIN_HOST), Some(before));
}

/// A name that is not a host name is refused before anything is read or written, as before
/// v0.12: the warning, `BadAlias`, no ssh and no stamp, whether the name is a Windows host's with
/// a part that names none, or a `linux/arm64` name written with its platform.
#[test]
fn a_name_that_names_no_host_is_refused_and_writes_nothing() {
    for bad in [
        "alpha.linux-arm64",
        "alpha.windows-arm64",
        "alpha.Windows-amd64",
        "alpha.windows-amd64.windows-amd64",
        "alpha.",
    ] {
        let rig = Rig::new();
        let say = FakeSay::person();
        let outcome = rig.push_to_host(&say, DEFAULT_ALIAS, false, Some(bad));
        assert_eq!(outcome, Some(Outcome::BadAlias), "{bad}");
        assert_eq!(rig.calls(), 0, "{bad}: a push was sent");
        assert_warnings(&say, &[BAD_HOST], bad);
        assert!(
            rig.stamp_bytes().is_none(),
            "{bad}: the box's stamp was written"
        );
        assert!(
            !rig.agentspace_dir().join("hosts").exists(),
            "{bad}: a folder was made"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The zone and the remote half: Linux unchanged, Windows through its rows.
// ---------------------------------------------------------------------------------------------

/// Every Linux text stays byte for byte: a push to either Linux platform's host is sent the
/// remote command of today's box for the laptop's IANA zone, the one ssh argument after the
/// alias, and nothing of a Windows host's.
#[test]
fn a_linux_host_is_sent_todays_remote_command_for_the_iana_zone() {
    for host in [ARM_HOST, AMD_HOST] {
        let rig = Rig::new();
        let say = FakeSay::person();
        assert_eq!(
            rig.push_to_host(&say, DEFAULT_ALIAS, false, Some(host)),
            Some(Outcome::Pushed),
            "{host}: {:?}",
            say.warnings()
        );
        let args = rig.args(1);
        assert_eq!(args.len(), 6, "{host}");
        assert_eq!(
            args[5],
            remote_command(DEFAULT_ZONE),
            "{host}: the remote command is today's"
        );
        assert!(
            !args.iter().any(|a| a.contains(REMOTE_MARK)),
            "{host}: a Windows row travelled"
        );
        assert!(
            rig.stamp_matches_at(&stamp_of(&rig, host), DEFAULT_ZONE),
            "{host}: the stamp covers the IANA zone"
        );
    }
}

/// D96 and operator.md, the seam's push row: for a Windows host the zone is the laptop's Windows
/// zone ID (with the IANA name beside it as the row's other input), the remote half is the row's
/// script for that zone, and the ssh's remote command is the row's wrapping of that script, none
/// of it today's Linux command. The stamp covers the zone as sent.
#[test]
fn a_windows_host_is_sent_the_windows_rows_for_the_laptops_windows_zone() {
    let rig = Rig::new();
    let standin = StandInWindows::default();
    rig.answer_zone(WINDOWS_ID);
    let say = FakeSay::person();
    let outcome = push_to(&rig, &say, WIN_HOST, &standin);
    assert_eq!(outcome, Outcome::Pushed, "{:?}", say.warnings());
    assert_warnings(&say, &[], "A push to a Windows host");
    assert_eq!(
        *standin.zone_asked.lock().unwrap(),
        vec![(WINDOWS_ID.to_string(), DEFAULT_ZONE.to_string())],
        "The zone row is asked once, with the Windows ID and the IANA name"
    );
    assert_eq!(
        *standin.script_asked.lock().unwrap(),
        vec![WINDOWS_ID.to_string()],
        "The script row is asked for the zone the zone row gave"
    );
    let script = StandInWindows::script_for(WINDOWS_ID);
    assert_eq!(
        *standin.remote_asked.lock().unwrap(),
        vec![script.clone()],
        "The remote command row wraps the script row's answer"
    );
    let args = rig.args(1);
    assert_eq!(args.len(), 6);
    assert_eq!(
        args[5],
        StandInWindows::remote_for(&script),
        "The remote command is the row's answer, unchanged"
    );
    assert_ne!(args[5], remote_command(WINDOWS_ID));
    assert_ne!(args[5], remote_command(DEFAULT_ZONE));
    assert!(
        rig.stamp_matches_at(&stamp_of(&rig, WIN_HOST), WINDOWS_ID),
        "The stamp covers the zone as sent, the Windows ID"
    );
    assert!(
        !rig.stamp_matches_at(&stamp_of(&rig, WIN_HOST), DEFAULT_ZONE),
        "The stamp does not cover the IANA name"
    );
}

/// The token goes to the push's stdin alone: never on the command line a Windows host's remote
/// command is part of, and the stdin is the content, whose two lines carry the URL and the token.
#[test]
fn the_token_travels_on_stdin_alone_to_a_windows_host() {
    let rig = Rig::new();
    let standin = StandInWindows::default();
    rig.answer_zone(WINDOWS_ID);
    let say = FakeSay::person();
    assert_eq!(push_to(&rig, &say, WIN_HOST, &standin), Outcome::Pushed);
    for arg in rig.args(1) {
        assert!(!arg.contains(&rig.token), "An argument holds the token");
    }
    assert_eq!(rig.stdin(1), rig.content().into_bytes());
}

/// A laptop whose Windows zone ID cannot be had is the empty zone for a Windows host: it is not
/// sent, the warning is Linux's, the script row is asked for the empty zone, and the stamp covers
/// it empty.
#[test]
fn no_windows_zone_id_is_the_empty_zone_which_is_not_sent() {
    let rig = Rig::new();
    let standin = StandInWindows::default();
    rig.answer_zone("");
    let say = FakeSay::person();
    let outcome = rig.push_on_platform(&say, DEFAULT_ALIAS, false, Some(WIN_HOST), &standin, None);
    assert_eq!(outcome, Outcome::Pushed, "{:?}", say.warnings());
    assert_warnings(&say, &[NO_ZONE], "A push with no Windows zone");
    assert_eq!(*standin.script_asked.lock().unwrap(), vec![String::new()]);
    assert!(
        rig.stamp_matches_at(&stamp_of(&rig, WIN_HOST), ""),
        "The stamp covers the empty zone"
    );
}

/// A Windows host's row that fails to build its remote half is a push that is not sent: a warning,
/// no stamp written, so the next connection tries again.
#[test]
fn a_remote_half_that_cannot_be_built_is_a_push_not_sent_and_no_stamp() {
    struct NoScript(StandInWindows);
    impl HostPlatform for NoScript {
        fn platform(&self) -> &'static str {
            self.0.platform()
        }
        fn image_parameter(&self) -> Option<&'static str> {
            None
        }
        fn new_stack_instance_type(&self) -> Option<&'static str> {
            None
        }
        fn new_stack_extra_parameters(&self) -> &'static [&'static str] {
            &[]
        }
        fn login_user(&self) -> &'static str {
            "agent"
        }
        fn ssm_document(&self) -> &'static str {
            "AWS-RunPowerShellScript"
        }
        fn remote_command(&self, script: &str) -> Result<String, Failure> {
            self.0.remote_command(script)
        }
        fn bootstrap(&self) -> Result<Bootstrap, Failure> {
            Err(unbuilt())
        }
        fn sync_plan(&self) -> Result<SyncPlan, Failure> {
            Err(unbuilt())
        }
        fn patch_check_command(&self) -> Result<String, Failure> {
            Err(unbuilt())
        }
        fn patch_messages(&self) -> PatchMessages {
            self.0.patch_messages()
        }
        fn horizon_command(&self, _h: u64) -> Result<String, Failure> {
            Err(unbuilt())
        }
        fn grow_script(&self) -> Result<String, Failure> {
            Err(unbuilt())
        }
        fn push_zone(&self, w: &str, i: &str) -> Result<String, Failure> {
            self.0.push_zone(w, i)
        }
        fn push_script(&self, _zone: &str) -> Result<String, Failure> {
            Err(unbuilt())
        }
    }
    let rig = Rig::new();
    let standin = NoScript(StandInWindows::default());
    let say = FakeSay::person();
    let outcome = rig.push_on_platform(
        &say,
        DEFAULT_ALIAS,
        false,
        Some(WIN_HOST),
        &standin,
        Some(WINDOWS_ID),
    );
    assert_ne!(
        outcome,
        Outcome::Pushed,
        "A push with no remote half succeeded"
    );
    assert_eq!(rig.calls(), 0, "Nothing was sent");
    assert!(
        stamp_bytes_of(&rig, WIN_HOST).is_none(),
        "A stamp was written for a push that was not sent"
    );
    assert!(!say.warnings().is_empty(), "A push not sent said nothing");
}
