//! The template's Windows first boot: each part pin P15 names, in the order operator.md gives, and
//! no bare `shutdown` or `sshd` (docs/operator.md, The template's Windows first boot, steps 1 to 8;
//! sshd's settings on a Windows host; The tests' seams; D95, D96; E0.12-T03-S03, #0256, pin P15 of
//! v0.12-T02 and pin P13 of v0.12-T03).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every check is a text check of the Windows
//! `UserData` branch of `infra/agentspace.yaml`: nothing is run. A check finds a part by a token
//! the part cannot be written without, so a branch written another way fails it and the test is
//! read before the branch is changed.

mod template_support;

use std::fs;
use std::path::PathBuf;

use template_support::*;

/// The index of `needle` in the branch, failing with the part's name when it is missing.
fn at(w: &str, needle: &str, part: &str) -> usize {
    w.find(needle)
        .unwrap_or_else(|| panic!("The Windows branch MUST hold {part}: no {needle:?} in it"))
}

#[test]
fn the_branch_substitutes_nothing_but_the_two_keys() {
    // A `${Name}` that is not a parameter makes CloudFormation refuse the template; a literal one
    // is written `${!Name}`.
    let w = windows_block().content;
    let mut seen = Vec::new();
    let mut rest = w.as_str();
    while let Some(start) = rest.find("${") {
        let tail = &rest[start..];
        let end = tail.find('}').expect("A ${ MUST close");
        let token = &tail[..=end];
        assert!(
            token == "${SshPublicKey}" || token == "${CaPublicKey}" || token.starts_with("${!"),
            "The Windows branch substitutes {token}, which is not a parameter it may use"
        );
        seen.push(token.to_string());
        rest = &tail[end..];
    }
    assert!(
        seen.iter().any(|t| t == "${SshPublicKey}") && seen.iter().any(|t| t == "${CaPublicKey}"),
        "The branch MUST use SshPublicKey and CaPublicKey, and used {seen:?}"
    );
}

#[test]
fn step_1_turns_off_windows_updates_own_installs_and_restarts() {
    let w = windows_block().content;
    for needle in [
        r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU",
        "NoAutoUpdate",
        "NoAutoRebootWithLoggedOnUsers",
    ] {
        at(&w, needle, "Windows Update's policy (N10)");
    }
}

#[test]
fn step_2_makes_agent_an_administrator_with_a_random_password_written_nowhere() {
    let w = windows_block().content;
    assert!(
        w.contains("New-LocalUser") || w.contains("net user"),
        "The branch MUST make the local user"
    );
    assert!(w.contains("agent"), "The local user is named agent (D95)");
    assert!(
        w.contains("Add-LocalGroupMember") || w.contains("net localgroup"),
        "agent MUST be added to a group"
    );
    assert!(
        w.contains("Administrators"),
        "agent MUST be in Administrators"
    );
    assert!(
        w.contains("PasswordNeverExpires") || w.contains("/expires:never"),
        "The password MUST be set never to expire"
    );
    assert!(
        w.contains("RandomNumberGenerator") || w.contains("RNGCryptoServiceProvider"),
        "The password MUST come from a cryptographic random source"
    );
    assert!(
        !w.contains("Get-Random"),
        "Get-Random is not a cryptographic source"
    );
    // The password is held in memory alone: no line that names it writes it anywhere.
    for line in w.lines().filter(|l| l.to_lowercase().contains("password")) {
        for sink in [
            "Out-File",
            "Set-Content",
            "Add-Content",
            "Write-Host",
            "Write-Output",
            "Tee-Object",
            "[IO.File]",
            "[System.IO.File]",
            ">>",
        ] {
            assert!(
                !line.contains(sink),
                "A line that names the password writes it ({sink}): {line}"
            );
        }
    }
}

#[test]
fn step_3_installs_open_ssh_server_when_absent_then_enables_starts_and_stops_it() {
    let w = windows_block().content;
    let probe = at(
        &w,
        "Get-WindowsCapability",
        "a check of the OpenSSH.Server capability",
    );
    let add = at(&w, "Add-WindowsCapability", "the capability's install");
    assert!(
        w.contains("OpenSSH.Server"),
        "The capability is OpenSSH.Server"
    );
    assert!(
        probe < add,
        "The capability MUST be looked for before it is added: it is added only when absent"
    );
    assert!(
        w.contains("Automatic")
            || w.contains("StartupType auto")
            || w.contains("start= auto")
            || w.contains("start=auto"),
        "The sshd service MUST be set to start automatically"
    );
    let first_start = at(&w, "Start-Service", "sshd's start");
    let stop = at(&w, "Stop-Service", "sshd's stop after its first start");
    assert!(
        first_start < stop,
        "sshd is started once, which makes its host keys and its default sshd_config, then stopped"
    );
    assert!(
        w.rfind("Start-Service").unwrap() > stop,
        "sshd MUST be started again at the branch's end (step 8)"
    );
}

#[test]
fn steps_4_and_5_write_the_cas_trust_and_the_users_key_with_their_access_rules() {
    let w = windows_block().content;
    // A path may be built from its parts (a folder and a file name); the folder and each file's
    // name MUST each be in the branch.
    at(&w, r"C:\ProgramData\ssh", "the ssh folder");
    at(&w, "agentspace_ca.pub", "the CA's public key file");
    at(&w, "${CaPublicKey}", "the CA's public key");
    at(&w, "administrators_authorized_keys", "the user's key file");
    at(&w, "${SshPublicKey}", "the user's public key");
    assert!(
        w.contains("icacls") && w.contains("/inheritance:r"),
        "administrators_authorized_keys MUST have inheritance off"
    );
    assert!(
        (w.contains("SYSTEM") || w.contains("S-1-5-18"))
            && (w.contains("Administrators") || w.contains("S-1-5-32-544")),
        "administrators_authorized_keys MUST grant SYSTEM and Administrators"
    );
}

/// The agentspace lines of `box/windows/sshd_config.agentspace`, as docs/operator.md, sshd's
/// settings on a Windows host, gives them (N11).
const SSHD_LINES: [&str; 10] = [
    "# agentspace begin: installed by agentspace-operator sync from box/windows/sshd_config.agentspace; every sync replaces these lines (D7, D95, D96)",
    "TrustedUserCAKeys __PROGRAMDATA__/ssh/agentspace_ca.pub",
    "PubkeyAuthentication yes",
    "PasswordAuthentication no",
    "KbdInteractiveAuthentication no",
    "AllowUsers agent",
    "LoginGraceTime 30",
    "ClientAliveInterval 15",
    "ClientAliveCountMax 5",
    "# agentspace end",
];

/// Whether each of `lines` is in `text`, in order.
fn in_order(text: &str, lines: &[&str]) -> bool {
    let mut from = 0;
    for l in lines {
        match text[from..].find(l) {
            Some(start) => from += start + l.len(),
            None => return false,
        }
    }
    true
}

#[test]
fn the_order_checker_finds_the_lines_in_order_and_refuses_them_out_of_order() {
    // Proof of `in_order`: it passes the lines as written and fails them swapped.
    let text = SSHD_LINES.join("\n");
    assert!(in_order(&text, &SSHD_LINES));
    let mut swapped = SSHD_LINES;
    swapped.swap(2, 3);
    assert!(!in_order(&text, &swapped));
}

#[test]
fn step_6_holds_the_agentspace_sshd_lines_as_operator_md_gives_them_and_tests_them() {
    let w = windows_block().content;
    assert!(
        in_order(&w, &SSHD_LINES),
        "The Windows branch MUST hold operator.md's agentspace lines of sshd_config, each as written, in order"
    );
    assert!(
        w.contains("sshd.exe") && w.contains(" -t"),
        "The branch MUST run sshd.exe -t on the new file"
    );
}

#[test]
fn step_6_holds_the_sshd_config_agentspace_file_once_that_file_is_built() {
    // box/windows/sshd_config.agentspace is E0.12-T03-S07's: a placeholder until it lands. From
    // then on the template's copy MUST be byte for byte the file's (operator.md, The template's
    // Windows first boot, step 6), which this check compares.
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("box")
        .join("windows")
        .join("sshd_config.agentspace");
    let file = normalized(&fs::read_to_string(path).expect("The file is in the repository"));
    if file.starts_with("# Placeholder") {
        return;
    }
    let lines: Vec<&str> = file.lines().collect();
    assert_eq!(
        lines, SSHD_LINES,
        "The built file holds the lines operator.md gives, which this test's own copy is"
    );
    assert!(in_order(&windows_block().content, &lines));
}

#[test]
fn step_7_sets_windows_powershell_as_the_default_shell_and_no_option() {
    let w = windows_block().content;
    at(&w, r"HKLM:\SOFTWARE\OpenSSH", "the DefaultShell key");
    at(&w, "DefaultShell", "the DefaultShell value");
    at(
        &w,
        r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
        "Windows PowerShell 5.1 as the default shell (pin P15)",
    );
    assert!(
        !w.contains("DefaultShellCommandOption"),
        "No DefaultShellCommandOption is written"
    );
    assert!(
        !w.contains(r"Git\bin\bash.exe"),
        "Git is not installed yet, so the first boot MUST NOT name Git Bash"
    );
}

#[test]
fn the_branch_logs_each_step_to_the_first_boot_log_with_the_time_in_utc() {
    let w = windows_block().content;
    at(&w, r"C:\ProgramData\agentspace", "the agentspace folder");
    at(&w, "first-boot.log", "the first boot's log");
    assert!(
        w.contains("UtcNow") || w.contains("ToUniversalTime"),
        "A line's time MUST be UTC"
    );
}

#[test]
fn the_branch_does_its_steps_in_operator_mds_order() {
    let w = windows_block().content;
    let user = w
        .find("New-LocalUser")
        .or_else(|| w.find("net user"))
        .expect("The branch makes the user");
    let steps = [
        ("Windows Update's policy", at(&w, "NoAutoUpdate", "step 1")),
        ("agent", user),
        ("the OpenSSH capability", at(&w, "OpenSSH.Server", "step 3")),
        ("the CA's key", at(&w, "agentspace_ca.pub", "step 4")),
        (
            "the user's key",
            at(&w, "administrators_authorized_keys", "step 5"),
        ),
        ("the sshd_config lines", at(&w, SSHD_LINES[0], "step 6")),
        ("the default shell", at(&w, "DefaultShell", "step 7")),
        ("sshd's last start", w.rfind("Start-Service").unwrap()),
    ];
    for pair in steps.windows(2) {
        assert!(
            pair[0].1 < pair[1].1,
            "The branch MUST do {} before {}",
            pair[0].0,
            pair[1].0
        );
    }
}

#[test]
fn the_branch_names_sshd_by_full_path_if_at_all() {
    // Pin P13 of v0.12-T03: a SYSTEM task's PATH is the machine's, so no program is named bare.
    let w = windows_block().content;
    assert!(
        w.contains("sshd.exe"),
        "The branch tests the new file with sshd.exe"
    );
    let full = r"C:\Windows\System32\OpenSSH\";
    let mut rest = w.as_str();
    let mut seen = 0;
    while let Some(start) = rest.find("sshd.exe") {
        assert!(
            rest[..start].ends_with(full),
            "sshd.exe MUST be named C:\\Windows\\System32\\OpenSSH\\sshd.exe, never by bare name: {:?}",
            &rest[start.saturating_sub(40)..start + 8]
        );
        seen += 1;
        rest = &rest[start + "sshd.exe".len()..];
    }
    assert!(seen > 0);
    // No statement runs sshd by bare name (a message or a comment may say `sshd -t`).
    for line in w.lines().map(str::trim) {
        assert!(
            !(line.starts_with("sshd ")
                || line.starts_with("sshd.exe ")
                || line.contains("& sshd ")),
            "A bare sshd MUST NOT be run: {line}"
        );
    }
}

#[test]
fn the_branch_names_shutdown_by_full_path_if_at_all() {
    let w = windows_block().content.to_lowercase();
    let mut rest = w.as_str();
    while let Some(start) = rest.find("shutdown") {
        assert!(
            rest[..start].ends_with(r"c:\windows\system32\")
                && rest[start..].starts_with("shutdown.exe"),
            "shutdown MUST be named C:\\Windows\\System32\\shutdown.exe, never by bare name"
        );
        rest = &rest[start + "shutdown".len()..];
    }
}
