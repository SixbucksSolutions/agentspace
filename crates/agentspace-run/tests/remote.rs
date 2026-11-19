//! ssh's and scp's calls: the remote command as one last argument, and gap FA-15's refusals
//! (docs/operator.md: What the laptop sends).

mod common;

use agentspace_operator_api::{CallerFailure, Remote};
use common::Rig;

fn rig_ssh(code: i32) -> (Rig, common::Standin) {
    let rig = Rig::new();
    let ssh = rig.standin("ssh").exit(code).write();
    rig.env.set("AGENTSPACE_SSH_CMD", ssh.value());
    (rig, ssh)
}

#[test]
fn remote_command_is_one_argument_and_the_last() {
    let (rig, ssh) = rig_ssh(0);
    let options = [
        "-o",
        "ClearAllForwardings=yes",
        "-o",
        "BatchMode=yes",
        "devbox",
    ];
    let command = "echo one two; echo 'three four'";
    assert_eq!(rig.runner().ssh(&options, Some(command)), Ok(0));
    assert_eq!(
        ssh.args(),
        [
            "-o",
            "ClearAllForwardings=yes",
            "-o",
            "BatchMode=yes",
            "devbox",
            command
        ]
    );
}

#[test]
fn no_remote_command_means_the_options_alone() {
    let (rig, ssh) = rig_ssh(0);
    assert_eq!(
        rig.runner().ssh(&["-o", "BatchMode=yes", "devbox"], None),
        Ok(0)
    );
    assert_eq!(ssh.args(), ["-o", "BatchMode=yes", "devbox"]);
}

#[test]
fn ssh_and_scp_return_the_exit_code_whatever_it_is() {
    for code in [0, 1, 7, 255] {
        let (rig, ssh) = rig_ssh(code);
        assert_eq!(rig.runner().ssh(&["devbox"], Some("true")), Ok(code));
        assert!(ssh.ran());
        let scp = rig.standin("scp").exit(code).write();
        rig.env.set("AGENTSPACE_SCP_CMD", scp.value());
        assert_eq!(rig.runner().scp(&["a", "devbox:b"]), Ok(code));
    }
}

#[test]
fn scp_gets_its_arguments_exactly_as_given() {
    let rig = Rig::new();
    let scp = rig.standin("scp").write();
    rig.env.set("AGENTSPACE_SCP_CMD", scp.value());
    let args = ["-o", "BatchMode=yes", "a file.txt", "devbox:dest dir/"];
    assert_eq!(rig.runner().scp(&args), Ok(0));
    assert_eq!(scp.args(), args);
}

// ---------------------------------------------------------------- FA-15

fn assert_refused(command: &str) {
    let (rig, ssh) = rig_ssh(0);
    let r = rig.runner().ssh(&["devbox"], Some(command));
    let f = r.expect_err(&format!("Accepted {command:?}"));
    assert!(f.message.starts_with("agentspace: "), "{}", f.message);
    assert_ne!(f.caller, CallerFailure::NotReady);
    assert!(!ssh.ran(), "ssh was started for {command:?}");
}

#[test]
fn a_percent_sign_is_refused() {
    assert_refused("echo 100%");
    assert_refused("echo %PATH%");
}

#[test]
fn a_double_quote_is_refused() {
    assert_refused("echo \"hi\"");
    assert_refused("\"");
}

#[test]
fn a_control_character_is_refused() {
    for c in ['\n', '\r', '\t', '\u{1}', '\u{1b}', '\u{7f}', '\0'] {
        assert_refused(&format!("echo a{c}b"));
    }
}

#[test]
fn a_trailing_backslash_is_refused() {
    assert_refused("echo a\\");
    assert_refused("\\");
    assert_refused("echo a\\\\");
}

#[test]
fn a_backslash_inside_is_allowed() {
    let (rig, ssh) = rig_ssh(0);
    assert_eq!(
        rig.runner().ssh(&["devbox"], Some("echo a\\b c\\ d")),
        Ok(0)
    );
    assert_eq!(ssh.args().last().unwrap(), "echo a\\b c\\ d");
}

#[test]
fn other_printable_ascii_survives_whole() {
    let (rig, ssh) = rig_ssh(0);
    let all: String = (0x20u8..0x7f)
        .map(char::from)
        .filter(|c| !matches!(c, '%' | '"' | '\\'))
        .collect();
    let command = format!("echo {all}x");
    assert_eq!(rig.runner().ssh(&["devbox"], Some(&command)), Ok(0));
    assert_eq!(ssh.args().last().unwrap(), &command);
}

#[test]
fn shell_metacharacters_reach_the_stand_in_as_text() {
    let (rig, ssh) = rig_ssh(0);
    let command = "a & b | c < d > e ^ f ! g";
    assert_eq!(rig.runner().ssh(&["devbox"], Some(command)), Ok(0));
    assert_eq!(ssh.args().last().unwrap(), command);
    assert_eq!(ssh.args().len(), 2);
}
