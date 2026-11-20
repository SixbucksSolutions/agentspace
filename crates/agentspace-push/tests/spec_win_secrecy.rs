//! The token's secrecy in the push, on Windows (docs/operator.md, Secrecy; The test seam).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The token is a dummy. It MUST travel only on the ssh's stdin: in no argument, no environment
//! variable the stand-in sees, no message, and no file on the laptop. A failure here is reported by
//! what held the token, never by the token.
#![cfg(windows)]

mod win_support;

use win_support::*;

/// What on the laptop holds the token, by description: every file under the throwaway home, work and
/// sysroot folders, and every file of the stand-in's folder but the recorded stdin.
fn files_holding_token(rig: &Rig) -> Vec<String> {
    let mut found = Vec::new();
    for dir in [&rig.home, &rig.work, &rig.sysroot] {
        for f in files_under(dir) {
            if let Ok(bytes) = std::fs::read(&f)
                && bytes_contain(&bytes, rig.token.as_bytes())
            {
                found.push(format!(
                    "a file under {}",
                    dir.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }
    for f in files_under(&rig.standin_dir) {
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        if name.starts_with("stdin-") {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&f)
            && bytes_contain(&bytes, rig.token.as_bytes())
        {
            found.push(format!(
                "the stand-in's {}",
                name.split('-').next().unwrap_or("file")
            ));
        }
    }
    found
}

fn env_values_holding(rig: &Rig, n: usize, needle: &str) -> usize {
    rig.env_pairs(n)
        .iter()
        .filter(|(_, v)| v.contains(needle))
        .count()
}

#[test]
fn the_token_is_on_the_stdin_only_after_a_confirmed_push() {
    let rig = Rig::new();
    let say = rig.up();
    assert_warnings(&say, &[], "A confirmed push");

    assert!(
        bytes_contain(&rig.stdin(1), rig.token.as_bytes()),
        "The token did not reach the stdin"
    );
    let args = rig.args(1).join("\u{1}");
    assert!(!args.contains(&rig.token), "An argument holds the token");
    assert_eq!(
        env_values_holding(&rig, 1, &rig.token),
        0,
        "An environment variable the stand-in saw holds the token"
    );
    assert!(
        !say.any_text_contains(&rig.token),
        "A message holds the token"
    );
    let stamp = rig.stamp_bytes().expect("the stamp");
    assert!(
        !bytes_contain(&stamp, rig.token.as_bytes()),
        "The stamp holds the token"
    );
    let held = files_holding_token(&rig);
    assert!(
        held.is_empty(),
        "The token is in {} file(s) on the laptop",
        held.len()
    );
}

#[test]
fn the_token_is_in_no_message_argument_environment_or_file_when_the_box_does_not_confirm() {
    for scenario in [
        "exit255",
        "nohash",
        "wronghash",
        "exit1hash",
        "zonefailed",
        "nozone",
    ] {
        let rig = Rig::new();
        rig.scenario(scenario);
        let say = rig.up();
        assert!(!say.warnings().is_empty(), "{scenario}: no warning");
        assert!(
            !say.any_text_contains(&rig.token),
            "{scenario}: a message holds the token"
        );
        assert!(
            !rig.args(1).join("\u{1}").contains(&rig.token),
            "{scenario}: an argument holds the token"
        );
        assert_eq!(
            env_values_holding(&rig, 1, &rig.token),
            0,
            "{scenario}: an environment variable holds the token"
        );
        let held = files_holding_token(&rig);
        assert!(
            held.is_empty(),
            "{scenario}: the token is in {} file(s) on the laptop",
            held.len()
        );
    }
}

#[test]
fn the_token_is_in_no_message_when_an_ssh_cannot_be_started() {
    let mut rig = Rig::new();
    let missing = rig.standin_dir.join("none.cmd");
    rig.set_ssh_seam(Some(missing.as_os_str()));
    let say = rig.up();
    assert_warnings(&say, &[COULD_NOT_SEND], "An ssh that cannot be started");
    assert!(!say.any_text_contains(&rig.token));
    let held = files_holding_token(&rig);
    assert!(
        held.is_empty(),
        "The token is in {} file(s) on the laptop",
        held.len()
    );
}

#[test]
fn the_url_is_not_in_an_argument_or_the_environment_either() {
    let rig = Rig::new();
    rig.up();
    assert!(!rig.args(1).join("\u{1}").contains(&rig.base));
    assert_eq!(env_values_holding(&rig, 1, &rig.base), 0);
}

#[test]
fn the_seam_variables_carry_no_part_of_the_content() {
    let rig = Rig::new();
    rig.up();
    for (name, value) in rig.env_pairs(1) {
        let upper = name.to_ascii_uppercase();
        if upper.starts_with("AGENTSPACE_") {
            assert!(
                !value.contains("LOCALSWIM_") && !value.contains(&rig.token),
                "The variable {name} carries part of the content"
            );
        }
    }
}

#[test]
fn no_variable_the_push_adds_to_the_environment_holds_the_token() {
    let rig = Rig::new();
    rig.up();
    let planted: Vec<&str> = PLANTED_FD_STATE.to_vec();
    let carriers = rig
        .env_pairs(1)
        .into_iter()
        .filter(|(k, v)| !planted.contains(&k.as_str()) && v.contains(&rig.token))
        .count();
    assert_eq!(carriers, 0, "{carriers} variable(s) hold the token");
}

#[test]
fn the_stamp_is_a_hash_that_covers_the_token_and_is_not_the_token() {
    // One rig at a time: a rig holds the lock on the process environment.
    let first = {
        let rig = Rig::new();
        rig.up();
        rig.stamp_bytes().expect("the stamp")
    };
    let second = {
        let rig = Rig::new();
        rig.up();
        rig.stamp_bytes().expect("the stamp")
    };
    // Each rig has its own dummy token, and the same instance, zone and URL.
    assert!(first != second, "Two tokens gave one stamp");
}
