//! The platform seam's columns as docs/operator.md, The platform seam, gives them: the rows whose
//! cell is a constant, for each of the three platforms (D91, D94, D95, D97; E0.12-T03-S03, #0256).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. A row that builds a script is the unit
//! tests' (Linux) and `agentspace-windows`' (Windows); this file holds each platform to the
//! table's values for its image parameter, its new stack's type and creation parameters, its login
//! user, its Session Manager document, its patch messages, its localswim target and its bootstrap's
//! delivery, and shows the two Linux columns equal where the table says they share a row.

use agentspace_operator_api::ScriptDelivery;
use agentspace_platforms::for_platform;

const LINUX_AMD64_IMAGE: &str =
    "/aws/service/canonical/ubuntu/server/26.04/stable/current/amd64/hvm/ebs-gp3/ami-id";
const WINDOWS_IMAGE: &str = "/aws/service/ami-windows-latest/Windows_Server-2025-English-Full-Base";

fn seam(platform: &str) -> &'static dyn agentspace_operator_api::HostPlatform {
    for_platform(platform).unwrap_or_else(|| panic!("{platform} has a seam"))
}

#[test]
fn the_image_parameter_row_is_none_for_arm64_and_each_other_platforms_public_parameter() {
    assert_eq!(seam("linux/arm64").image_parameter(), None);
    assert_eq!(
        seam("linux/amd64").image_parameter(),
        Some(LINUX_AMD64_IMAGE)
    );
    assert_eq!(seam("windows/amd64").image_parameter(), Some(WINDOWS_IMAGE));
}

#[test]
fn the_new_stacks_instance_type_row_is_none_for_arm64_and_m8a_xlarge_for_the_others() {
    assert_eq!(seam("linux/arm64").new_stack_instance_type(), None);
    assert_eq!(
        seam("linux/amd64").new_stack_instance_type(),
        Some("m8a.xlarge")
    );
    assert_eq!(
        seam("windows/amd64").new_stack_instance_type(),
        Some("m8a.xlarge")
    );
}

#[test]
fn the_other_creation_parameters_row_is_empty_but_for_windows_two_in_their_order() {
    assert!(seam("linux/arm64").new_stack_extra_parameters().is_empty());
    assert!(seam("linux/amd64").new_stack_extra_parameters().is_empty());
    assert_eq!(
        seam("windows/amd64").new_stack_extra_parameters(),
        &["RootVolumeGiB=30", "HostOs=windows"],
        "RootVolumeGiB=30, then HostOs=windows, each one argument (N3)"
    );
}

#[test]
fn the_login_user_and_the_session_manager_document_rows() {
    for (platform, user, document) in [
        ("linux/arm64", "ubuntu", "AWS-RunShellScript"),
        ("linux/amd64", "ubuntu", "AWS-RunShellScript"),
        ("windows/amd64", "agent", "AWS-RunPowerShellScript"),
    ] {
        assert_eq!(seam(platform).login_user(), user, "{platform}");
        assert_eq!(seam(platform).ssm_document(), document, "{platform}");
    }
}

#[test]
fn the_patch_messages_row() {
    for platform in ["linux/arm64", "linux/amd64"] {
        let m = seam(platform).patch_messages();
        assert_eq!(
            m.phase, "Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...",
            "{platform}"
        );
        assert_eq!(
            m.log_command, "journalctl -u agentspace-patch",
            "{platform}"
        );
    }
    let m = seam("windows/amd64").patch_messages();
    assert_eq!(
        m.phase,
        "Patching Windows (Windows Update, restart if an update needs one)..."
    );
    assert_eq!(m.log_command, "cat C:/ProgramData/agentspace/patch.log");
}

#[test]
fn each_linux_platform_builds_the_localswim_cli_for_its_own_goos_and_goarch() {
    for (platform, arch, file) in [
        ("linux/arm64", "arm64", "localswim-linux-arm64"),
        ("linux/amd64", "amd64", "localswim-linux-amd64"),
    ] {
        let plan = seam(platform).sync_plan().expect("A Linux sync plan");
        assert_eq!(plan.localswim.goos, "linux", "{platform}");
        assert_eq!(plan.localswim.goarch, arch, "{platform}");
        assert_eq!(plan.localswim.file_name, file, "{platform}");
    }
}

#[test]
fn the_linux_sync_plans_agree_but_for_the_localswim_target() {
    let a = seam("linux/arm64").sync_plan().unwrap();
    let b = seam("linux/amd64").sync_plan().unwrap();
    assert_ne!(a.localswim, b.localswim);
    assert_eq!(a.files, b.files);
    assert_eq!(a.staging, b.staging);
    assert_eq!(a.install_command, b.install_command);
    assert_eq!(a.clear_command, b.clear_command);
    assert_eq!(a.claude_folder_command, b.claude_folder_command);
    assert_eq!(a.claude_md_path, b.claude_md_path);
    assert_eq!(a.localswim_staging, b.localswim_staging);
    assert_eq!(a.localswim_install_command, b.localswim_install_command);
}

#[test]
fn both_linux_platforms_send_the_bootstrap_on_one_ssh_stdin_run_by_bash() {
    let repo_bootstrap = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("box")
            .join("bootstrap.sh"),
    )
    .unwrap();
    for platform in ["linux/arm64", "linux/amd64"] {
        let b = seam(platform).bootstrap().expect("A Linux bootstrap");
        assert_eq!(
            b.script,
            repo_bootstrap.as_slice(),
            "{platform}: box/bootstrap.sh as carried"
        );
        assert_eq!(
            b.delivery,
            ScriptDelivery::Stdin {
                remote_command: "bash -s".to_string()
            },
            "{platform}"
        );
    }
}

#[test]
fn the_linux_remote_command_form_gives_the_script_back_unchanged() {
    for platform in ["linux/arm64", "linux/amd64"] {
        for script in [
            "true",
            "echo 'a b' && ls -la ~",
            "x\ty",
            "mkdir -p -- ~/.claude",
        ] {
            assert_eq!(
                seam(platform).remote_command(script).unwrap(),
                script,
                "{platform}"
            );
        }
    }
}
