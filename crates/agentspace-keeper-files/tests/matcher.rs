//! The holder command and its matcher (docs/keeper.md, The holder command).
//!
//! The paths are Windows paths written as text: the matcher reads a command line, which is text,
//! and compares paths as Windows does, so these cases hold on any host.
mod common;

use std::path::Path;

use agentspace_keeper_api::Matcher;
use common::*;

const SSH: &str = r"C:\Tools\OpenSSH\ssh.exe";
const SSH_SP: &str = r"C:\Program Files\OpenSSH\ssh.exe";
const STUB: &str = r"C:\Temp\stub.cmd";
const STUB_SP: &str = r"C:\Temp dir\my stub.cmd";
const CMD: &str = r"C:\Windows\System32\cmd.exe";
const ARGS: &str = "-N -o ExitOnForwardFailure=yes";

fn holds(line: &str, ssh: &str, alias: &str) -> bool {
    matcher().is_holder(line, Path::new(ssh), alias)
}

fn command(ssh: &str, alias: &str) -> String {
    matcher().holder_command(Path::new(ssh), alias)
}

#[track_caller]
fn yes(line: &str, ssh: &str) {
    assert!(holds(line, ssh, ALIAS), "Should match: {line:?} for {ssh}");
}

#[track_caller]
fn no(line: &str, ssh: &str) {
    assert!(
        !holds(line, ssh, ALIAS),
        "Should not match: {line:?} for {ssh}"
    );
}

// ---- holder_command's exact strings ----

#[test]
fn an_exe_holder_command_is_quoted_program_and_the_fixed_arguments() {
    assert_eq!(
        command(SSH, ALIAS),
        format!(r#""{SSH}" -N -o ExitOnForwardFailure=yes {ALIAS}"#)
    );
}

#[test]
fn an_exe_with_spaces_in_its_path_is_quoted_whole() {
    assert_eq!(
        command(SSH_SP, ALIAS),
        format!(r#""{SSH_SP}" -N -o ExitOnForwardFailure=yes {ALIAS}"#)
    );
}

#[test]
fn a_cmd_holder_command_runs_through_system32_cmd_with_d_and_c() {
    assert_eq!(
        command(STUB, ALIAS),
        format!(r#""{CMD}" /d /c ""{STUB}" -N -o ExitOnForwardFailure=yes {ALIAS}""#)
    );
}

#[test]
fn a_cmd_path_with_spaces_is_quoted_inside_the_outer_quotes() {
    assert_eq!(
        command(STUB_SP, ALIAS),
        format!(r#""{CMD}" /d /c ""{STUB_SP}" -N -o ExitOnForwardFailure=yes {ALIAS}""#)
    );
}

#[test]
fn a_bat_and_an_upper_case_extension_are_batch_files_too() {
    for stub in [
        r"C:\Temp\stub.bat",
        r"C:\Temp\STUB.CMD",
        r"C:\Temp\Stub.Bat",
    ] {
        assert_eq!(
            command(stub, ALIAS),
            format!(r#""{CMD}" /d /c ""{stub}" -N -o ExitOnForwardFailure=yes {ALIAS}""#),
            "{stub}"
        );
    }
}

#[test]
fn the_alias_is_the_last_argument_whatever_it_is() {
    assert!(command(SSH, "other-alias.1").ends_with(" other-alias.1"));
}

#[test]
fn the_keepers_own_form_is_recognized() {
    for ssh in [SSH, SSH_SP, STUB, STUB_SP, r"C:\Temp\stub.bat"] {
        yes(&command(ssh, ALIAS), ssh);
    }
}

#[test]
fn the_keepers_own_form_is_not_another_alias() {
    for ssh in [SSH, STUB] {
        assert!(!holds(
            &command(ssh, ALIAS),
            ssh,
            "agentspace-demo-ffffffff"
        ));
    }
}

// ---- The table's cases that match ----

#[test]
fn an_exe_quoted_matches() {
    yes(&format!(r#""{SSH}" {ARGS} {ALIAS}"#), SSH);
}

#[test]
fn an_exe_unquoted_in_another_case_with_extra_blanks_and_a_tab_matches() {
    yes(
        &format!("c:\\tools\\openssh\\SSH.EXE  -N\t-o   ExitOnForwardFailure=yes  {ALIAS} "),
        SSH,
    );
}

#[test]
fn an_exe_with_forward_slashes_matches() {
    yes(&format!("C:/Tools/OpenSSH/ssh.exe {ARGS} {ALIAS}"), SSH);
}

#[test]
fn a_path_with_dot_segments_matches_its_normal_form() {
    yes(
        &format!(r#""C:\Tools\.\Other\..\OpenSSH\ssh.exe" {ARGS} {ALIAS}"#),
        SSH,
    );
}

#[test]
fn a_cmd_under_cmd_with_c_and_no_d_matches() {
    yes(&format!(r#"{CMD} /c ""{STUB}" {ARGS} {ALIAS}""#), STUB);
}

#[test]
fn cmd_in_capitals_with_d_and_c_in_capitals_matches() {
    yes(
        &format!(r#"C:\WINDOWS\SYSTEM32\CMD.EXE /D /C ""{STUB}" {ARGS} {ALIAS}""#),
        STUB,
    );
}

#[test]
fn cmd_quoted_with_d_and_c_matches() {
    yes(&format!(r#""{CMD}" /d /c ""{STUB}" {ARGS} {ALIAS}""#), STUB);
}

#[test]
fn two_quotes_around_a_path_with_spaces_are_kept_and_match() {
    yes(&format!(r#"{CMD} /c "{STUB_SP}" {ARGS} {ALIAS}"#), STUB_SP);
}

#[test]
fn an_exe_quoted_with_spaces_matches() {
    yes(&format!(r#""{SSH_SP}" {ARGS} {ALIAS}"#), SSH_SP);
}

#[test]
fn a_batch_run_directly_matches() {
    yes(&format!(r#""{STUB}" {ARGS} {ALIAS}"#), STUB);
}

// ---- The table's cases that do not match ----

#[test]
fn an_exe_with_spaces_unquoted_does_not_match() {
    no(&format!("{SSH_SP} {ARGS} {ALIAS}"), SSH_SP);
}

#[test]
fn another_alias_a_longer_one_or_another_case_does_not_match() {
    let other = format!(r#""{SSH}" {ARGS} "#);
    assert!(!holds(
        &format!("{other}agentspace-demo-ffffffff"),
        SSH,
        ALIAS
    ));
    assert!(!holds(&format!("{other}{ALIAS}x"), SSH, ALIAS));
    assert!(!holds(
        &format!("{other}{}", ALIAS.to_uppercase()),
        SSH,
        ALIAS
    ));
    // And the same line is the holder for the alias it names.
    assert!(holds(
        &format!("{other}{ALIAS}x"),
        SSH,
        &format!("{ALIAS}x")
    ));
}

#[test]
fn an_alias_quoted_with_more_text_inside_does_not_match() {
    no(&format!(r#""{SSH}" {ARGS} "{ALIAS} extra""#), SSH);
    no(&format!(r#""{SSH}" {ARGS} "{ALIAS}x""#), SSH);
}

#[test]
fn an_extra_argument_after_the_alias_or_before_dash_n_does_not_match() {
    no(&format!(r#""{SSH}" {ARGS} {ALIAS} extra"#), SSH);
    no(&format!(r#""{SSH}" -x {ARGS} {ALIAS}"#), SSH);
    no(&format!(r#""{SSH}" extra {ARGS} {ALIAS}"#), SSH);
}

#[test]
fn a_lower_case_n_or_an_o_without_its_value_does_not_match() {
    no(
        &format!(r#""{SSH}" -n -o ExitOnForwardFailure=yes {ALIAS}"#),
        SSH,
    );
    no(&format!(r#""{SSH}" -N -o {ALIAS}"#), SSH);
    no(&format!(r#""{SSH}" -N {ALIAS}"#), SSH);
}

#[test]
fn the_option_value_is_compared_exactly_with_its_case() {
    no(
        &format!(r#""{SSH}" -N -o exitonforwardfailure=yes {ALIAS}"#),
        SSH,
    );
    no(
        &format!(r#""{SSH}" -N -o ExitOnForwardFailure=no {ALIAS}"#),
        SSH,
    );
    no(
        &format!(r#""{SSH}" -N -O ExitOnForwardFailure=yes {ALIAS}"#),
        SSH,
    );
}

#[test]
fn another_program_with_the_same_arguments_does_not_match() {
    no(
        &format!(r#""C:\Tools\OpenSSH\other.exe" {ARGS} {ALIAS}"#),
        SSH,
    );
    no(&format!(r#""C:\Elsewhere\ssh.exe" {ARGS} {ALIAS}"#), SSH);
}

#[test]
fn an_exe_under_cmd_does_not_match() {
    no(&format!(r#"{CMD} /c ""{SSH}" {ARGS} {ALIAS}""#), SSH);
    no(&format!(r#""{CMD}" /d /c ""{SSH}" {ARGS} {ALIAS}""#), SSH);
}

#[test]
fn a_cmd_under_a_cmd_that_is_not_system32s_does_not_match() {
    for other in [
        r"C:\Other\cmd.exe",
        r"C:\Windows\cmd.exe",
        r"C:\Windows\SysWOW64\cmd.exe",
    ] {
        no(&format!(r#"{other} /c ""{STUB}" {ARGS} {ALIAS}""#), STUB);
    }
}

#[test]
fn a_cmd_under_k_or_s_c_does_not_match() {
    no(&format!(r#"{CMD} /k ""{STUB}" {ARGS} {ALIAS}""#), STUB);
    no(&format!(r#"{CMD} /s /c ""{STUB}" {ARGS} {ALIAS}""#), STUB);
    no(
        &format!(r#"{CMD} /d /s /c ""{STUB}" {ARGS} {ALIAS}""#),
        STUB,
    );
}

#[test]
fn a_cmd_with_another_alias_an_extra_argument_or_a_trailing_command_does_not_match() {
    no(
        &format!(r#"{CMD} /c ""{STUB}" {ARGS} agentspace-demo-ffffffff""#),
        STUB,
    );
    no(
        &format!(r#"{CMD} /c ""{STUB}" {ARGS} {ALIAS} extra""#),
        STUB,
    );
    no(
        &format!(r#"{CMD} /c ""{STUB}" {ARGS} {ALIAS}" & calc"#),
        STUB,
    );
}

#[test]
fn an_empty_line_does_not_match() {
    no("", SSH);
    no("   ", SSH);
    no("", STUB);
}

#[test]
fn a_relative_ssh_exe_matches_nothing() {
    for line in [
        format!("ssh.exe {ARGS} {ALIAS}"),
        format!(r#""ssh.exe" {ARGS} {ALIAS}"#),
        format!(r#""{SSH}" {ARGS} {ALIAS}"#),
    ] {
        no(&line, "ssh.exe");
    }
    no(&format!(r"..\ssh.exe {ARGS} {ALIAS}"), r"..\ssh.exe");
    no(&format!(r"Tools\ssh.exe {ARGS} {ALIAS}"), r"Tools\ssh.exe");
}

#[test]
fn a_line_with_no_program_but_arguments_does_not_match() {
    no(&format!("{ARGS} {ALIAS}"), SSH);
}

#[test]
fn a_batch_ssh_exe_matches_only_cmds_in_system32_by_its_full_path() {
    // Rule 2: the program of the cmd form is `<System32>\cmd.exe` as a normalized full path.
    yes(
        &format!(r#"C:/Windows/System32/cmd.exe /c ""{STUB}" {ARGS} {ALIAS}""#),
        STUB,
    );
    no(&format!(r#"cmd.exe /c ""{STUB}" {ARGS} {ALIAS}""#), STUB);
    no(&format!(r#"cmd /c ""{STUB}" {ARGS} {ALIAS}""#), STUB);
}

// ---- self_check ----

#[test]
fn the_matcher_passes_its_own_check() {
    assert_eq!(matcher().self_check(), Ok(()));
}

// ---- The splitter's own cases (rule 1) ----

#[test]
fn the_specs_three_splitter_cases() {
    assert_eq!(
        split(r#"p "a b" c\\d e\"f g\\"h i""#),
        ["p", "a b", r"c\\d", r#"e"f"#, r"g\h i"]
    );
    assert_eq!(split(r#"p """a""""#), ["p", r#""a""#]);
    assert_eq!(split(r#""C:\x\"y  z "#), [r"C:\x\", "y", "z"]);
}

#[test]
fn the_first_token_ends_at_a_blank_when_unquoted_with_no_escapes() {
    assert_eq!(split("prog\ta  b"), ["prog", "a", "b"]);
    assert_eq!(split(r"C:\a\b.exe x"), [r"C:\a\b.exe", "x"]);
    assert_eq!(split(r#"prog\"x y"#), [r#"prog\"x"#, "y"]);
}

#[test]
fn a_program_quote_with_no_close_runs_to_the_end() {
    assert_eq!(split(r#""C:\a b\x.exe"#), [r"C:\a b\x.exe"]);
}

#[test]
fn blanks_and_tabs_separate_arguments_and_quotes_group_them() {
    assert_eq!(split("p  a\tb   c "), ["p", "a", "b", "c"]);
    assert_eq!(split(r#"p "a  b"c d"#), ["p", "a  bc", "d"]);
    assert_eq!(split("p"), ["p"]);
    assert_eq!(split(""), Vec::<String>::new());
}

#[test]
fn backslashes_before_a_quote_follow_the_2n_and_2n_plus_1_rule() {
    assert_eq!(split(r#"p a\"b"#), ["p", r#"a"b"#]);
    assert_eq!(split(r#"p a\\"b c"#), ["p", r"a\b c"]);
    assert_eq!(split(r#"p a\\\"b"#), ["p", r#"a\"b"#]);
    assert_eq!(split(r"p a\\b"), ["p", r"a\\b"]);
}

#[test]
fn quotes_that_follow_one_another_are_read_by_a_count() {
    // Outside quoting, `""` is an empty quoted text and leaves quoting off.
    assert_eq!(split(r#"p "" x"#), ["p", "", "x"]);
    assert_eq!(split(r#"p a""b"#), ["p", "ab"]);
    // Inside quoting, `""` gives one literal quote and ends the quoting.
    assert_eq!(split(r#"p "a""b c"#), ["p", r#"a"b"#, "c"]);
    // Inside quoting, `"""` gives one literal quote and keeps the quoting.
    assert_eq!(split(r#"p "a"""b c""#), ["p", r#"a"b c"#]);
    // Outside quoting, `"""` gives one literal quote and leaves quoting off.
    assert_eq!(split(r#"p """ x"#), ["p", r#"""#, "x"]);
}
