//! The stack's template, `infra/agentspace.yaml`, as v0.12 changes it: its parameters, the choice
//! of the Windows branch, and the two Linux branches (docs/operator.md, The stack; The template's
//! Windows first boot; D44, D94; E0.12-T03-S03, #0256, pin P15). The Windows branch's own parts are
//! `template_windows.rs`'s.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every check is a text check of the
//! template as it is in the repository: no CloudFormation, no AWS and no program is run.
//!
//! - **`HostOs` and `RootVolumeGiB` default to what `agentbox` has**, and every other parameter,
//!   resource and property stays, so `agentbox`'s change set stays empty (D44). The live change set
//!   is the change-set checker's, not a test's.
//! - **The Windows `UserData` branch is chosen by `HostOs`.**
//! - **The two Linux branches are byte for byte what they were** at `5d88698`, the commit that
//!   started v0.12-T03, whose template is `fixtures/template-5d88698.yaml`.
//!
//! The extractor is proven on the fixture first, since a search that finds nothing reads the same
//! as a clean template.

mod template_support;

use template_support::*;

#[test]
fn the_extractor_finds_the_baselines_two_linux_branches_and_no_windows_one() {
    let base = baseline();
    let all = blocks(&base);
    assert_eq!(all.len(), 2, "The baseline's UserData has two branches");
    assert!(all.iter().all(|b| !is_windows(b)));
    assert!(all[0].content.starts_with("#cloud-config\n"));
    assert!(
        all[0].content.contains("/etc/ssh/agentspace_ca.pub")
            && all[0].content.contains("PermitRootLogin no"),
        "The first branch is the one that installs the CA"
    );
    assert!(
        !all[1].content.contains("agentspace_ca.pub")
            && all[1].content.ends_with("- ${SshPublicKey}"),
        "The second branch is the one with no CA"
    );
}

#[test]
fn host_os_and_root_volume_default_to_what_agentbox_has() {
    let t = template();
    let host_os = parameter(&t, "HostOs");
    assert_eq!(member(&host_os, "Type").as_deref(), Some("String"));
    assert_eq!(
        member(&host_os, "Default").as_deref(),
        Some("linux"),
        "HostOs MUST default to linux, which agentbox has"
    );
    let allowed = host_os.join("\n");
    assert!(
        allowed.contains("AllowedValues")
            && allowed.contains("linux")
            && allowed.contains("windows"),
        "HostOs MUST allow linux and windows"
    );
    let root = parameter(&t, "RootVolumeGiB");
    assert_eq!(
        member(&root, "Default").as_deref(),
        Some("8"),
        "RootVolumeGiB MUST default to 8, which agentbox has"
    );
}

#[test]
fn every_other_parameters_type_and_default_is_what_it_was() {
    let (t, b) = (template(), baseline());
    for name in [
        "InstanceType",
        "ImageId",
        "SubnetId",
        "VpcId",
        "SshPublicKey",
        "HostName",
        "CaPublicKey",
    ] {
        let now = parameter(&t, name);
        let before = parameter(&b, name);
        for key in ["Type", "Default", "AllowedPattern"] {
            assert_eq!(
                member(&now, key),
                member(&before, key),
                "Parameter {name}'s {key} MUST be what it was: agentbox's change set stays empty"
            );
        }
    }
    assert_eq!(
        member(&parameter(&t, "ImageId"), "Default").as_deref(),
        Some("ami-0f53b75f24b254b9d"),
        "ImageId's default MUST stay the pinned arm64 image"
    );
}

/// The baseline's lines that the new template MUST still hold, in order: every line that is not a
/// comment, a blank, a description or part of the UserData property.
fn kept_baseline_lines(b: &str) -> Vec<String> {
    let lines: Vec<&str> = b.lines().collect();
    let user_data_at = lines.iter().position(|l| l.trim() == "UserData:").unwrap();
    let user_data_indent = indent(lines[user_data_at]);
    let mut keep = Vec::new();
    let mut in_user_data = false;
    let mut in_description = None::<usize>;
    for (n, l) in lines.iter().enumerate() {
        if n == user_data_at {
            in_user_data = true;
            continue;
        }
        if in_user_data {
            if l.trim().is_empty() || indent(l) > user_data_indent {
                continue;
            }
            in_user_data = false;
        }
        if let Some(d) = in_description {
            if l.trim().is_empty() || indent(l) > d {
                continue;
            }
            in_description = None;
        }
        let trimmed = l.trim();
        if trimmed.starts_with("Description:") || trimmed.starts_with("ConstraintDescription:") {
            in_description = Some(indent(l));
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        keep.push(l.to_string());
    }
    keep
}

#[test]
fn the_filter_that_keeps_the_baseline_lines_keeps_most_of_the_baseline() {
    // Proof of the checker below: it must find lines to hold the template to.
    let kept = kept_baseline_lines(&baseline());
    assert!(kept.len() > 60, "Kept only {} lines", kept.len());
    assert!(
        kept.iter().any(|l| l == "      ImageId: !Ref ImageId")
            && kept.iter().any(|l| l == "  IdleAlarm:"),
        "The resources' own lines are among them"
    );
    assert!(
        !kept.iter().any(|l| l.contains("PermitRootLogin")),
        "No UserData line is"
    );
}

#[test]
fn nothing_of_the_template_but_user_data_was_taken_away_or_changed() {
    // Every kept line of the baseline is still in the template, in the same order: the new
    // template only adds (the two parameters, the condition and the Windows branch).
    let t = template();
    let mut now = t.lines();
    for line in kept_baseline_lines(&baseline()) {
        assert!(
            now.any(|l| l == line),
            "A line of the baseline template is gone or moved, so agentbox's change set may not be empty: {line:?}"
        );
    }
}

/// The name of the condition whose definition tests `HostOs` against `windows`.
fn windows_condition(t: &str) -> String {
    t.lines()
        .skip_while(|l| *l != "Conditions:")
        .skip(1)
        .take_while(|l| l.starts_with(' ') || l.trim().is_empty())
        .find(|l| l.contains("HostOs") && l.contains("windows"))
        .and_then(|l| l.split(':').next())
        .map(|n| n.trim().to_string())
        .expect("A condition MUST test HostOs against windows")
}

#[test]
fn the_windows_branch_is_chosen_by_host_os() {
    let t = template();
    let name = windows_condition(&t);
    let def = t
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("{name}:")))
        .unwrap();
    assert!(
        def.contains("!Equals") && def.contains("!Ref HostOs"),
        "The condition MUST be an equality of HostOs: {def}"
    );
    let lines: Vec<&str> = t.lines().collect();
    let ud = lines.iter().position(|l| l.trim() == "UserData:").unwrap();
    let w = windows_block();
    let opener = lines
        .iter()
        .position(|l| l.trim() == w.opener)
        .expect("The Windows block's opener is in the file");
    assert!(
        ud < opener && lines[ud..opener].join("\n").contains(&name),
        "UserData MUST choose the Windows branch with the HostOs condition {name}, before the branch"
    );
}

#[test]
fn the_windows_branch_is_a_powershell_user_data_script() {
    let w = windows_block();
    assert_eq!(
        w.content.lines().next().map(str::trim_end),
        Some("<powershell>"),
        "The branch MUST open with <powershell>, which EC2Launch runs once under Windows PowerShell 5.1"
    );
    assert_eq!(
        w.content.lines().last().map(str::trim_end),
        Some("</powershell>"),
        "The branch MUST end with </powershell>"
    );
}

#[test]
fn the_linux_branches_are_byte_for_byte_unchanged() {
    let now = linux_blocks(&template());
    let before = linux_blocks(&baseline());
    assert_eq!(before.len(), 2, "The baseline has two Linux branches");
    assert_eq!(
        now.len(),
        2,
        "The template MUST keep both Linux branches, with and without CaPublicKey"
    );
    for (n, (a, b)) in now.iter().zip(&before).enumerate() {
        assert_eq!(
            a.content,
            b.content,
            "Linux branch {} MUST be byte for byte what it was at 5d88698",
            n + 1
        );
    }
}

#[test]
fn the_linux_branches_stay_the_two_arms_of_has_ca_public_key_in_their_order() {
    let t = template();
    assert!(
        t.contains("HasCaPublicKey: !Not [!Equals [!Ref CaPublicKey, \"\"]]"),
        "The HasCaPublicKey condition MUST stay as it was"
    );
    let all = blocks(&t);
    let first = all
        .iter()
        .position(|b| !is_windows(b) && b.content.contains("agentspace_ca.pub"));
    let second = all
        .iter()
        .position(|b| !is_windows(b) && !b.content.contains("agentspace_ca.pub"));
    assert!(
        first.zip(second).is_some_and(|(a, b)| a < b),
        "The Linux branch that installs the CA MUST come before the one that does not, as the If's two arms"
    );
}
