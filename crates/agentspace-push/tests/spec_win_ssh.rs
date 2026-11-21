//! The push's ssh, on Windows: its program, its arguments, its standard input and its environment
//! (docs/operator.md, What the laptop sends; Environment variables; The test seam).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Every test runs the push against a stand-in `ssh.cmd` that records what it received, in a
//! throwaway world (tests/win_support). Nothing here reads or prints a captured environment's
//! values: the tests assert on names, and on the one value operator.md fixes (`AGENTSPACE_NESTED`).
#![cfg(windows)]

mod win_support;

use win_support::*;

const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

fn without_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(&BOM[..]).unwrap_or(bytes)
}

#[test]
fn the_push_starts_one_ssh_with_the_spec_arguments() {
    let rig = Rig::new();
    let say = FakeSay::person();
    rig.push(&say, "lease-alias-a1", false);

    assert_eq!(rig.calls(), 1, "The push did not start exactly one ssh");
    let args = rig.args(1);
    assert_eq!(args.len(), 6, "The ssh did not get exactly six arguments");
    assert_eq!(args[0], "-o");
    assert_eq!(args[1], "ClearAllForwardings=yes");
    assert_eq!(args[2], "-o");
    assert_eq!(args[3], "BatchMode=yes");
    assert_eq!(
        args[4], "lease-alias-a1",
        "The alias is not the fifth argument"
    );
    assert_warnings(&say, &[], "A confirmed push");
}

#[test]
fn the_remote_command_names_the_zone_and_keeps_to_the_quoting_rule() {
    let rig = Rig::new();
    rig.up();
    let args = rig.args(1);
    assert_eq!(args.len(), 6);
    let command = &args[5];
    assert!(
        command.contains(DEFAULT_ZONE),
        "The remote command does not name the zone"
    );
    assert!(
        !command.contains('%'),
        "The remote command holds a percent sign"
    );
    assert!(
        !command.contains('"'),
        "The remote command holds a double quote"
    );
    assert!(
        !command.chars().any(|c| c.is_control()),
        "The remote command holds a control character"
    );
    assert!(
        !command.ends_with('\\'),
        "The remote command ends in a backslash"
    );
    assert!(!command.trim().is_empty(), "The remote command is empty");
}

#[test]
fn the_remote_command_does_not_carry_the_token_or_the_url() {
    let rig = Rig::new();
    rig.up();
    let joined = rig.args(1).join(" ");
    assert!(!joined.contains(&rig.token), "An argument holds the token");
    assert!(!joined.contains(&rig.base), "An argument holds the URL");
}

#[test]
fn without_a_zone_the_remote_command_is_still_one_argument_and_names_no_zone() {
    let mut rig = Rig::new();
    rig.set_timezone(Some("not a zone"));
    rig.answer_zone("");
    let say = rig.up();
    assert_eq!(rig.calls(), 1);
    let args = rig.args(1);
    assert_eq!(args.len(), 6, "The ssh did not get exactly six arguments");
    assert!(
        !args[5].contains("not a zone"),
        "A zone that fails the check is on the remote command"
    );
    assert_warnings(&say, &[NO_ZONE], "A push without a zone");
}

#[test]
fn the_stdin_is_exactly_the_two_content_lines() {
    let rig = Rig::new();
    rig.up();
    let got = rig.stdin(1);
    // At most a UTF-8 byte order mark ahead (What the laptop sends, step 3), and nothing else.
    assert!(
        without_bom(&got) == rig.content().as_bytes(),
        "The ssh's stdin is not exactly LOCALSWIM_URL=<url> LF LOCALSWIM_TOKEN=<token> LF"
    );
}

#[test]
fn the_stdin_carries_a_token_with_a_dash_and_an_underscore_unchanged() {
    let mut rig = Rig::new();
    rig.token = format!("{}-_", &dummy_token()[..41]);
    assert_eq!(rig.token.len(), 43);
    rig.write_credential();
    let say = rig.up();
    assert_eq!(rig.calls(), 1);
    assert!(
        without_bom(&rig.stdin(1)) == rig.content().as_bytes(),
        "The token did not reach the stdin as its ASCII bytes"
    );
    assert_warnings(&say, &[], "A token with a dash and an underscore");
    assert!(
        rig.stamp_matches(DEFAULT_ZONE),
        "The stamp does not cover the content"
    );
}

#[test]
fn the_stdin_carries_the_url_unchanged() {
    // The base is exactly `http://127.0.0.1:PORT`, so the URL varies in its port alone.
    for base in [
        "http://127.0.0.1:1",
        "http://127.0.0.1:8811",
        "http://127.0.0.1:65535",
    ] {
        let mut rig = Rig::new();
        rig.base = base.to_string();
        rig.write_receipt();
        rig.up();
        assert_eq!(rig.calls(), 1, "A fitting base was refused");
        assert!(
            without_bom(&rig.stdin(1)) == rig.content().as_bytes(),
            "The URL did not reach the stdin unchanged"
        );
    }
}

#[test]
fn the_push_prints_the_sending_line_once_and_no_caller_line() {
    let rig = Rig::new();
    let say = rig.up();
    assert_eq!(
        say.progress_lines(),
        vec![SENDING.to_string()],
        "The push did not print the sending line exactly once"
    );
    assert!(
        say.progress_caller_lines().iter().all(Option::is_none),
        "The sending line has a caller line, and a caller sees nothing for it"
    );
    assert!(say.details().is_empty());
}

#[test]
fn the_stand_in_never_sees_a_posix_fd_state_variable() {
    let rig = Rig::new();
    rig.up();
    let names: Vec<String> = rig.env_pairs(1).into_iter().map(|(k, _)| k).collect();
    assert!(!names.is_empty(), "The stand-in recorded no environment");
    let bad = names.iter().filter(|n| is_fd_state(n)).count();
    assert_eq!(
        bad, 0,
        "The stand-in saw {bad} variable(s) whose name ends _POSIX_FD_STATE"
    );
}

#[test]
fn the_stand_in_sees_agentspace_nested_set_to_one() {
    let rig = Rig::new();
    rig.up();
    let value = rig
        .env_pairs(1)
        .into_iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("AGENTSPACE_NESTED"))
        .map(|(_, v)| v);
    assert!(value.is_some(), "The stand-in saw no AGENTSPACE_NESTED");
    assert!(
        value.as_deref() == Some("1"),
        "The stand-in's AGENTSPACE_NESTED is not 1"
    );
}

#[test]
fn a_process_environment_that_says_nested_zero_still_gives_one() {
    let mut rig = Rig::new();
    rig.set_env("AGENTSPACE_NESTED", Some(std::ffi::OsStr::new("0")));
    rig.up();
    let value = rig
        .env_pairs(1)
        .into_iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("AGENTSPACE_NESTED"))
        .map(|(_, v)| v);
    assert!(
        value.as_deref() == Some("1"),
        "The stand-in's AGENTSPACE_NESTED is not 1 when the process held 0"
    );
}

#[test]
fn the_stand_in_keeps_the_rest_of_the_environment() {
    let rig = Rig::new();
    rig.up();
    let names: Vec<String> = rig.env_pairs(1).into_iter().map(|(k, _)| k).collect();
    for want in ["SystemRoot", "USERPROFILE", "LOCALAPPDATA"] {
        assert!(
            names.iter().any(|n| n.eq_ignore_ascii_case(want)),
            "The stand-in lost the variable {want}"
        );
    }
}

#[test]
fn the_stand_in_sees_the_throwaway_profile_not_a_real_one() {
    let rig = Rig::new();
    rig.up();
    let home = rig
        .env_pairs(1)
        .into_iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("USERPROFILE"))
        .map(|(_, v)| v)
        .unwrap_or_default();
    assert!(
        std::path::Path::new(&home) == rig.home.as_path(),
        "The stand-in's USERPROFILE is not the throwaway home"
    );
}

#[test]
fn a_nested_up_does_not_push() {
    let mut rig = Rig::new();
    rig.nested = true;
    let say = rig.up();
    assert_eq!(rig.calls(), 0, "A nested up started an ssh");
    assert!(say.silent(), "A nested up said something");
    assert!(rig.stamp_bytes().is_none(), "A nested up wrote a stamp");
}

#[test]
fn a_forced_push_runs_even_when_nested() {
    let mut rig = Rig::new();
    rig.nested = true;
    let say = FakeSay::person();
    rig.push(&say, "agentspace", true);
    assert_eq!(
        rig.calls(),
        1,
        "A forced push did not start its ssh when nested"
    );
    assert_eq!(rig.args(1)[4], "agentspace");
    assert_warnings(&say, &[], "A forced push");
    assert!(rig.stamp_matches(DEFAULT_ZONE));
}

#[test]
fn a_forced_push_sends_although_the_stamp_matches() {
    let rig = Rig::new();
    rig.write_stamp_for(DEFAULT_ZONE);
    let say = FakeSay::person();
    rig.push(&say, "agentspace", true);
    assert_eq!(
        rig.calls(),
        1,
        "A forced push did not send with a matching stamp"
    );
    assert_eq!(say.progress_lines(), vec![SENDING.to_string()]);
}

#[test]
fn a_bad_alias_gets_its_warning_and_no_ssh_and_no_sending_line() {
    for alias in ["-oProxyCommand=evil", "a b", "x;y", ""] {
        let rig = Rig::new();
        let say = FakeSay::person();
        rig.push(&say, alias, false);
        assert_eq!(rig.calls(), 0, "A bad alias started an ssh");
        assert_warnings(&say, &[BAD_ALIAS], "A bad alias");
        assert!(
            say.progress_lines().is_empty(),
            "A bad alias printed a sending line"
        );
        assert!(rig.stamp_bytes().is_none(), "A bad alias wrote a stamp");
    }
}

#[test]
fn a_bad_alias_is_refused_before_the_receipt_is_read() {
    let rig = Rig::new();
    std::fs::remove_file(rig.receipt_path()).expect("the receipt");
    let say = FakeSay::person();
    rig.push(&say, "-oProxyCommand=evil", false);
    assert_warnings(&say, &[BAD_ALIAS], "A bad alias with no receipt");
}

#[test]
fn the_seam_path_is_taken_whole_with_its_spaces() {
    let rig = Rig::in_folder("stand ins with spaces");
    let say = rig.up();
    assert_eq!(
        rig.calls(),
        1,
        "A seam path with spaces did not run the stand-in"
    );
    assert_warnings(&say, &[], "A seam path with spaces");
}

#[test]
fn a_relative_seam_path_is_resolved_against_the_working_folder() {
    let mut rig = Rig::new();
    // The working folder is `work`, beside the stand-ins' folder.
    let relative = std::path::Path::new("..")
        .join(rig.standin_dir.file_name().expect("the folder's name"))
        .join("ssh.cmd");
    rig.set_ssh_seam(Some(relative.as_os_str()));
    let say = rig.up();
    assert_eq!(
        rig.calls(),
        1,
        "A relative seam path did not run the stand-in"
    );
    assert_warnings(&say, &[], "A relative seam path");
}

#[test]
fn the_seam_is_read_each_time_a_push_starts_its_ssh() {
    let mut rig = Rig::new();
    let say = rig.up();
    assert_eq!(rig.calls(), 1);
    assert_warnings(&say, &[], "The first push");
    // A second stand-in in another folder takes over at the next push.
    let other = rig.standin_dir.parent().unwrap().join("second");
    std::fs::create_dir_all(&other).unwrap();
    for name in ["ssh.cmd", "ssh-standin.ps1", "scenario.txt", "zone.txt"] {
        std::fs::copy(rig.standin_dir.join(name), other.join(name)).unwrap();
    }
    rig.set_ssh_seam(Some(other.join("ssh.cmd").as_os_str()));
    rig.write_stamp_for("Etc/Elsewhere");
    let say = rig.up();
    assert_eq!(rig.calls(), 1, "The first stand-in ran again");
    assert!(
        other.join("args-1.json").exists(),
        "The second stand-in was not run"
    );
    assert_warnings(&say, &[], "The second push");
}

#[test]
fn an_ssh_the_seam_names_that_does_not_exist_warns_could_not_send_and_never_falls_back() {
    let mut rig = Rig::new();
    let missing = rig.standin_dir.join("no-such-ssh.cmd");
    rig.set_ssh_seam(Some(missing.as_os_str()));
    let say = rig.up();
    assert_eq!(rig.calls(), 0);
    assert_warnings(&say, &[COULD_NOT_SEND], "A missing ssh");
    assert!(rig.stamp_bytes().is_none(), "A missing ssh wrote a stamp");
}

#[test]
fn an_ssh_the_seam_names_that_is_a_folder_warns_could_not_send() {
    let mut rig = Rig::new();
    let folder = rig.standin_dir.clone();
    rig.set_ssh_seam(Some(folder.as_os_str()));
    let say = rig.up();
    assert_eq!(rig.calls(), 0);
    assert_warnings(&say, &[COULD_NOT_SEND], "A folder as the ssh");
    assert!(rig.stamp_bytes().is_none());
}

#[test]
fn with_no_seam_and_no_openssh_under_system_root_the_push_warns_and_starts_nothing() {
    let mut rig = Rig::new();
    // The context's SystemRoot holds no OpenSSH, so the real ssh is out of reach.
    rig.set_ssh_seam(None);
    let say = rig.up();
    assert_eq!(rig.calls(), 0);
    assert_warnings(&say, &[COULD_NOT_SEND], "No ssh at the default path");
    assert!(rig.stamp_bytes().is_none());
}

#[test]
fn an_empty_or_blank_seam_means_the_default_program() {
    for value in ["", "   "] {
        let mut rig = Rig::new();
        rig.set_ssh_seam(Some(std::ffi::OsStr::new(value)));
        let say = rig.up();
        // The default is under the context's SystemRoot, where nothing is.
        assert_eq!(rig.calls(), 0, "A blank seam value ran the stand-in");
        assert_warnings(&say, &[COULD_NOT_SEND], "A blank seam value");
    }
}

#[test]
fn the_stand_in_runs_in_a_throwaway_working_folder_not_the_repository() {
    let rig = Rig::new();
    rig.up();
    let cwd = rig.cwd_of(1);
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(
        !cwd.starts_with(manifest) && !manifest.starts_with(&cwd),
        "The stand-in ran in the repository"
    );
    assert!(
        std::fs::canonicalize(&cwd).ok() == std::fs::canonicalize(&rig.work).ok(),
        "The stand-in did not run in the throwaway working folder"
    );
}
