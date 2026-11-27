//! What the operator carries (docs/operator.md, What the prototype took from its checkout; D37 and
//! its addendum): the stack's template, embedded at build time byte for byte as it is in the
//! repository, and the stack parameters file's schema.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! `box\system`, `box\bootstrap.sh` and `box\windows` are carried by the platform seam's rows
//! (`agentspace-platforms` and `agentspace-windows`; D101), and this crate keeps no copy of them.
//!
//! These bytes MUST NOT be re-authored, filtered or converted: they are written out at use exactly
//! as embedded. A test compares them with the repository's files.

/// The repository's `infra\agentspace.yaml`, the stack's template.
pub(crate) const TEMPLATE: &[u8] = include_bytes!("../../../infra/agentspace.yaml");

/// `schemas/state/stack-parameters.state.schema.json`, carried inside the operator (state.md,
/// Files in the checkout).
pub(crate) const STACK_PARAMETERS_SCHEMA: &str =
    include_str!("../../../schemas/state/stack-parameters.state.schema.json");

#[cfg(test)]
mod tests {
    use super::*;
    use agentspace_operator_api::HostPlatform;
    use std::path::{Path, PathBuf};

    /// `box\system`'s files and `box\bootstrap.sh`, as the seam's `linux/arm64` rows carry them
    /// (E0.12-T03-S03 part 2 deleted this crate's own copies; pin P1).
    fn box_system() -> &'static [(&'static str, &'static [u8])] {
        agentspace_platforms::LINUX_ARM64.sync_plan().unwrap().files
    }

    fn bootstrap() -> &'static [u8] {
        agentspace_platforms::LINUX_ARM64
            .bootstrap()
            .unwrap()
            .script
    }

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn files_under(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().into_string().unwrap();
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if entry.file_type().unwrap().is_dir() {
                files_under(&entry.path(), &relative, out);
            } else {
                out.push((relative, entry.path()));
            }
        }
    }

    #[test]
    fn the_template_is_the_repository_file_byte_for_byte() {
        let on_disk = std::fs::read(repo().join("infra/agentspace.yaml")).unwrap();
        assert!(
            TEMPLATE == on_disk.as_slice(),
            "The embedded template differs"
        );
    }

    #[test]
    fn box_system_is_the_repository_folder_byte_for_byte() {
        let mut on_disk = Vec::new();
        files_under(&repo().join("box/system"), "", &mut on_disk);
        on_disk.sort();
        let names: Vec<&str> = on_disk.iter().map(|(n, _)| n.as_str()).collect();
        let embedded: Vec<&str> = box_system().iter().map(|(n, _)| *n).collect();
        assert_eq!(embedded, names, "The embedded file list differs");
        for ((name, bytes), (_, path)) in box_system().iter().zip(&on_disk) {
            let disk = std::fs::read(path).unwrap();
            assert!(*bytes == disk.as_slice(), "Embedded {name} differs");
        }
        assert!(names.contains(&"install.sh"));
    }

    #[test]
    fn the_bootstrap_is_the_repository_file_byte_for_byte() {
        let on_disk = std::fs::read(repo().join("box/bootstrap.sh")).unwrap();
        assert!(
            bootstrap() == on_disk.as_slice(),
            "The embedded bootstrap differs"
        );
        assert!(bootstrap().starts_with(b"#!/bin/bash\n"));
    }

    #[test]
    fn the_schema_is_the_repository_file() {
        let on_disk = std::fs::read_to_string(
            repo().join("schemas/state/stack-parameters.state.schema.json"),
        )
        .unwrap();
        assert_eq!(STACK_PARAMETERS_SCHEMA, on_disk);
    }

    // The template's first boots (E0.12-T03-S03 part 2; pin P15 of v0.12-T02; operator.md, The
    // template's Windows first boot).

    fn template() -> &'static str {
        std::str::from_utf8(TEMPLATE).unwrap()
    }

    fn indent(line: &str) -> usize {
        line.len() - line.trim_start().len()
    }

    /// The text of every `Fn::Base64: !Sub |` block scalar of the `UserData` property, in order, as
    /// YAML reads each: its lines de-indented by the first one's indentation, each ending in LF,
    /// with no trailing blank line (the scalar's clip).
    fn user_data_branches() -> Vec<String> {
        let lines: Vec<&str> = template().lines().collect();
        let start = lines.iter().position(|l| l.trim() == "UserData:").unwrap();
        let property = indent(lines[start]);
        let mut branches = Vec::new();
        let mut n = start + 1;
        while n < lines.len() && (lines[n].trim().is_empty() || indent(lines[n]) > property) {
            if lines[n].trim_end().ends_with("Fn::Base64: !Sub |") {
                let marker = indent(lines[n]);
                let mut body = Vec::new();
                let mut m = n + 1;
                while m < lines.len() && (lines[m].trim().is_empty() || indent(lines[m]) > marker) {
                    body.push(lines[m]);
                    m += 1;
                }
                while body.last().is_some_and(|l| l.trim().is_empty()) {
                    body.pop();
                }
                let cut = indent(body[0]);
                let text: String = body
                    .iter()
                    .map(|l| format!("{}\n", if l.trim().is_empty() { "" } else { &l[cut..] }))
                    .collect();
                branches.push(text);
                n = m;
            } else {
                n += 1;
            }
        }
        branches
    }

    /// The Linux branch with the CA, as the template held it before the Windows branch.
    const LINUX_WITH_CA: &str = concat!(
        "#cloud-config\n",
        "package_update: true\n",
        "package_upgrade: true\n",
        "packages: [git, tmux, jq, ripgrep, unzip]\n",
        "ssh_authorized_keys:\n",
        "  - ${SshPublicKey}\n",
        "write_files:\n",
        "  - path: /etc/ssh/agentspace_ca.pub\n",
        "    owner: root:root\n",
        "    permissions: \"0644\"\n",
        "    content: |\n",
        "      ${CaPublicKey}\n",
        "  - path: /etc/ssh/sshd_config.d/50-agentspace-ca.conf\n",
        "    owner: root:root\n",
        "    permissions: \"0644\"\n",
        "    content: |\n",
        "      # Installed by scripts\\agentspace.ps1 sync to /etc/ssh/sshd_config.d/ (decision D7).\n",
        "      # sshd keeps the first value it reads for each keyword and reads this file before\n",
        "      # 60-cloudimg-settings.conf; install.sh checks with sshd -T that both values below took effect.\n",
        "      #\n",
        "      # Trust user certificates signed by the agentspace SSH CA. sync replaces\n",
        "      # /etc/ssh/agentspace_ca.pub with the laptop CA's public key, so a new CA drops the old one.\n",
        "      TrustedUserCAKeys /etc/ssh/agentspace_ca.pub\n",
        "      # No login as root by any means, a CA certificate naming root among them.\n",
        "      PermitRootLogin no\n",
    );

    /// The Linux branch without the CA, as the template held it before the Windows branch.
    const LINUX_WITHOUT_CA: &str = concat!(
        "#cloud-config\n",
        "package_update: true\n",
        "package_upgrade: true\n",
        "packages: [git, tmux, jq, ripgrep, unzip]\n",
        "ssh_authorized_keys:\n",
        "  - ${SshPublicKey}\n",
    );

    #[test]
    fn the_user_data_is_the_windows_branch_then_the_two_linux_branches_unchanged() {
        let branches = user_data_branches();
        assert_eq!(branches.len(), 3, "{branches:?}");
        assert!(branches[0].starts_with("<powershell>\n"));
        assert!(branches[0].ends_with("</powershell>\n"));
        assert_eq!(branches[1], LINUX_WITH_CA);
        assert_eq!(branches[2], LINUX_WITHOUT_CA);
        // The choice: windows picks the first branch, and linux the two, under HasCaPublicKey.
        let text = template();
        let user_data = &text[text.find("      UserData:\n").unwrap()..];
        let order: Vec<usize> = [
            "          - IsWindows\n",
            "          - Fn::Base64: !Sub |\n              <powershell>\n",
            "          - Fn::If:\n              - HasCaPublicKey\n              - Fn::Base64: !Sub |\n",
        ]
        .iter()
        .map(|s| user_data.find(s).unwrap_or_else(|| panic!("{s:?}")))
        .collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
        assert!(text.contains("  IsWindows: !Equals [!Ref HostOs, windows]\n"));
        assert!(text.contains("  HasCaPublicKey: !Not [!Equals [!Ref CaPublicKey, \"\"]]\n"));
    }

    /// `HostOs` and `RootVolumeGiB` default to what `agentbox` has, `linux` and `8`, and the image
    /// stays the pinned `arm64` one, so `agentbox`'s change set stays empty (D44, D94; pin P15).
    #[test]
    fn the_new_parameters_default_to_agentboxs_values() {
        let text = template();
        assert!(text.contains(
            "  HostOs:\n    Type: String\n    Default: linux\n    AllowedValues: [linux, windows]\n"
        ));
        assert!(
            text.contains("  RootVolumeGiB:\n    Type: Number\n    Default: 8\n    MinValue: 8\n")
        );
        assert!(text.contains(
            "  ImageId:\n    Type: AWS::EC2::Image::Id\n    Default: ami-0f53b75f24b254b9d\n"
        ));
        assert!(text.contains("  InstanceType:\n    Type: String\n    Default: m9g.xlarge\n"));
        assert!(text.contains("  HostName:\n    Type: String\n    Default: agentbox\n"));
        assert!(!text.contains('\r') && !text.contains('\t'));
    }

    /// The agentspace lines of `sshd_config`, as operator.md's "sshd's settings on a Windows host"
    /// gives them, one per line.
    fn spec_sshd_lines() -> Vec<String> {
        let doc = std::fs::read_to_string(repo().join("docs/operator.md")).unwrap();
        let at = doc
            .find("**`box/windows/sshd_config.agentspace` holds the agentspace lines**")
            .unwrap();
        let block = &doc[at..];
        let open = block.find("```\n").unwrap() + 4;
        let close = open + block[open..].find("```").unwrap();
        block[open..close]
            .lines()
            .map(|l| l.trim_end_matches('\r').to_string())
            .collect()
    }

    /// The Windows branch's copy of the agentspace lines: its here-string's lines.
    fn template_sshd_lines() -> Vec<String> {
        let windows = &user_data_branches()[0];
        let at = windows.find("$agentspaceLines = @'\n").unwrap();
        let body = &windows[at + "$agentspaceLines = @'\n".len()..];
        let end = body.find("\n'@\n").unwrap();
        body[..end].lines().map(str::to_string).collect()
    }

    /// The template's copy of the agentspace lines is byte for byte the spec's, which is what
    /// `box/windows/sshd_config.agentspace` holds once E0.12-T03-S07 builds it; until then that
    /// file is the architect's placeholder, and either is shown here, never anything else.
    #[test]
    fn the_windows_branchs_sshd_lines_are_box_windows_files() {
        let spec = spec_sshd_lines();
        assert_eq!(spec.len(), 10, "{spec:?}");
        assert!(spec[0].starts_with("# agentspace begin: "));
        assert_eq!(spec[9], "# agentspace end");
        assert_eq!(template_sshd_lines(), spec);
        let file = std::fs::read(repo().join("box/windows/sshd_config.agentspace")).unwrap();
        let text = String::from_utf8(file).unwrap();
        if text.starts_with("# Placeholder (D101)") {
            assert_eq!(text.lines().count(), 1, "The placeholder is one line");
        } else {
            let lines: Vec<String> = text
                .lines()
                .map(|l| l.trim_end_matches('\r').to_string())
                .collect();
            assert_eq!(lines, spec, "box/windows/sshd_config.agentspace differs");
        }
    }

    /// The Windows branch holds each part of operator.md's first boot, in its order (pin P15):
    /// Windows Update's own installs and restarts off; `agent`, with a random password held in
    /// memory alone, in Administrators; OpenSSH Server added when absent, set to start
    /// automatically and started; the CA's trust; `SshPublicKey` for an administrator with two
    /// rules alone; the agentspace lines and `sshd -t`; Windows PowerShell 5.1 as `DefaultShell`;
    /// sshd started. `sshd.exe` is named by its full path alone (pin P13).
    #[test]
    fn the_windows_branch_holds_each_part_of_the_first_boot_in_order() {
        let windows = &user_data_branches()[0];
        let parts = [
            "HKLM:\\SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate\\AU",
            "-Name NoAutoUpdate -PropertyType DWord -Value 1",
            "-Name NoAutoRebootWithLoggedOnUsers -PropertyType DWord -Value 1",
            "[System.Security.Cryptography.RandomNumberGenerator]::Create()",
            "while ($chars.Count -lt 40)",
            "New-LocalUser -Name agent -Password $secret -PasswordNeverExpires",
            "Add-LocalGroupMember -SID 'S-1-5-32-544' -Member agent",
            "if (-not (Get-Service -Name sshd -ErrorAction SilentlyContinue))",
            "Add-WindowsCapability -Online -Name $capability.Name",
            "Set-Service -Name sshd -StartupType Automatic",
            "Start-Service -Name sshd\n    Stop-Service -Name sshd",
            "${CaPublicKey}",
            "if ($caKey -eq '')",
            "(Join-Path $sshDir 'agentspace_ca.pub'), $caKey + \"`n\"",
            "${SshPublicKey}",
            "'administrators_authorized_keys'",
            "/inheritance:r /grant '*S-1-5-18:F' '*S-1-5-32-544:F'",
            "$agentspaceLines = @'",
            "& $sshdExe -t -f $temporary",
            "-Name DefaultShell -PropertyType String -Value 'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe'",
            "Start-Service -Name sshd\n    Write-AgentspaceFirstBootLine 'step 8 done",
        ];
        let mut from = 0;
        for part in parts {
            let at = windows[from..]
                .find(part)
                .unwrap_or_else(|| panic!("Missing or out of order: {part:?}"));
            from += at + part.len();
        }
        // sshd.exe by its full path alone, and no shutdown at all.
        assert!(windows.contains("$sshdExe = 'C:\\Windows\\System32\\OpenSSH\\sshd.exe'"));
        assert_eq!(windows.matches("sshd.exe").count(), 1);
        assert!(!windows.to_lowercase().contains("shutdown"));
        // The password is never written or printed: it is a SecureString built in memory, and
        // nothing writes to a file but the log, the CA's key, the user's key and sshd_config.
        for never in [
            "Out-File",
            "Set-Content",
            "Write-Output",
            "Write-Host",
            "ConvertFrom-SecureString",
        ] {
            assert!(!windows.contains(never), "{never}");
        }
        assert_eq!(windows.matches("Add-Content").count(), 1);
        // Each step's line goes to the log with the time in UTC.
        assert!(windows.contains("$firstBootLog = Join-Path $agentspaceDir 'first-boot.log'"));
        assert!(windows.contains("[DateTime]::UtcNow"));
        // CloudFormation's Sub substitutes the two parameters alone.
        assert_eq!(windows.matches("${").count(), 2);
    }
}
