//! The Windows seam's constant rows and its patch rows (docs/operator.md, The platform seam, the
//! `windows/amd64` column; D95, D96, D97, D101).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;

use common::platform;

#[test]
fn it_serves_windows_amd64() {
    assert_eq!(platform().platform(), "windows/amd64");
}

#[test]
fn the_image_parameter_is_amazons_windows_server_2025_full_base() {
    assert_eq!(
        platform().image_parameter(),
        Some("/aws/service/ami-windows-latest/Windows_Server-2025-English-Full-Base")
    );
}

#[test]
fn a_new_stacks_instance_type_is_m8a_xlarge() {
    assert_eq!(platform().new_stack_instance_type(), Some("m8a.xlarge"));
}

#[test]
fn the_extra_creation_parameters_are_the_root_volume_then_the_host_os() {
    assert_eq!(
        platform().new_stack_extra_parameters(),
        &["RootVolumeGiB=30", "HostOs=windows"]
    );
}

#[test]
fn the_root_volume_comes_before_the_host_os() {
    let extra = platform().new_stack_extra_parameters();
    let volume = extra.iter().position(|p| *p == "RootVolumeGiB=30");
    let host_os = extra.iter().position(|p| *p == "HostOs=windows");
    assert!(volume.is_some() && host_os.is_some(), "both are there");
    assert!(volume < host_os, "RootVolumeGiB=30 comes first");
}

#[test]
fn each_extra_parameter_is_one_name_equals_value_argument() {
    for parameter in platform().new_stack_extra_parameters() {
        let (name, value) = parameter.split_once('=').expect("Name=Value");
        assert!(!name.is_empty() && !value.is_empty(), "{parameter}");
        assert!(!parameter.contains(char::is_whitespace), "{parameter}");
    }
}

#[test]
fn the_login_user_is_agent() {
    assert_eq!(platform().login_user(), "agent");
}

#[test]
fn the_session_manager_document_is_the_powershell_one() {
    assert_eq!(platform().ssm_document(), "AWS-RunPowerShellScript");
}

#[test]
fn the_patch_phase_message_is_the_windows_one() {
    assert_eq!(
        platform().patch_messages().phase,
        "Patching Windows (Windows Update, restart if an update needs one)..."
    );
}

#[test]
fn the_patch_log_command_reads_the_patch_log_in_a_form_both_shells_run() {
    assert_eq!(
        platform().patch_messages().log_command,
        "cat C:/ProgramData/agentspace/patch.log"
    );
}

#[test]
fn the_patch_check_runs_the_check_under_powershell_7_by_its_full_path() {
    assert_eq!(
        platform().patch_check_command().expect("the patch check"),
        "& 'C:\\Program Files\\PowerShell\\7\\pwsh.exe' -NoProfile -NonInteractive -File C:\\ProgramData\\agentspace\\agentspace-patch-check.ps1"
    );
}

#[test]
fn the_patch_check_never_names_pwsh_alone() {
    let command = platform().patch_check_command().expect("the patch check");
    let folded = command.to_lowercase();
    for (at, _) in folded.match_indices("pwsh") {
        assert!(
            folded[..at].ends_with("\\7\\"),
            "pwsh at {at} is not the full path's"
        );
        assert!(folded[at..].starts_with("pwsh.exe"), "pwsh at {at}");
    }
    assert!(folded.contains("pwsh"), "PowerShell 7 is named");
}

#[test]
fn the_patch_check_is_not_the_encoded_form_and_not_the_calls_json() {
    let command = platform().patch_check_command().expect("the patch check");
    assert!(!command.contains("-EncodedCommand"));
    assert!(!command.contains("{\"commands\""), "the caller wraps it");
    assert!(!command.starts_with("commands="), "never the shorthand");
}

#[test]
fn no_row_answers_that_the_platform_is_not_built() {
    let platform = platform();
    assert!(platform.remote_command("Write-Output 1").is_ok());
    assert!(platform.bootstrap().is_ok());
    assert!(platform.sync_plan().is_ok());
    assert!(platform.patch_check_command().is_ok());
    assert!(platform.horizon_command(1_795_000_000).is_ok());
    assert!(platform.grow_script().is_ok());
    assert!(
        platform
            .push_zone("Mountain Standard Time", "America/Denver")
            .is_ok()
    );
    assert!(platform.push_script("Mountain Standard Time").is_ok());
}
