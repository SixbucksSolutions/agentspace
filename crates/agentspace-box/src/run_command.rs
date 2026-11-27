//! The `--parameters` value of a Run Command this crate sends (operator.md, The platform seam, the
//! Session Manager document row; pins P17 and P18 of v0.12-T02).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **On `AWS-RunShellScript`, both Linux platforms' document, it MUST be the shorthand
//!   `commands=<command>`**, as before v0.12, so every Linux call is unchanged byte for byte.
//! - **On any other document, `AWS-RunPowerShellScript` among them, it MUST be the JSON value
//!   `{"commands":["<command>"]}`, never the shorthand**, the command JSON-escaped, so its quotes,
//!   brackets and commas reach the host whole.

use agentspace_operator_api::HostPlatform;

/// The document whose commands go as the shorthand.
const SHELL_DOCUMENT: &str = "AWS-RunShellScript";

/// The `--parameters` value that runs `command` on a host of `platform`: `commands=<command>` on
/// `AWS-RunShellScript`, and `{"commands":["<command>"]}` on every other document. Public as
/// `run_command_parameters`, so `agentspace-sync`'s patch wait sends what `up`'s does.
pub fn parameters(platform: &dyn HostPlatform, command: &str) -> String {
    if platform.ssm_document() == SHELL_DOCUMENT {
        format!("commands={command}")
    } else {
        serde_json::json!({ "commands": [command] }).to_string()
    }
}

/// A platform of the tests' own, for the calls a non-Linux platform makes through this crate.
#[cfg(test)]
pub(crate) mod fake {
    use agentspace_operator_api::{
        Bootstrap, CallerFailure, Failure, HostPlatform, PatchMessages, SyncPlan,
    };

    /// The tests' platform: its document, a patch question and a horizon command that hold quotes,
    /// brackets, commas and backslashes, and patch messages of its own. With `fails`, its patch
    /// check and horizon command fail.
    pub(crate) struct TestPlatform {
        pub(crate) document: &'static str,
        pub(crate) fails: bool,
    }

    /// The test platform's patch question.
    pub(crate) const QUESTION: &str =
        r#"& "C:\Program Files\PowerShell\7\pwsh.exe" -File C:\x\check.ps1, [y]"#;
    /// The test platform's phase message.
    pub(crate) const PHASE: &str = "Patching the test platform...";
    /// The test platform's log command.
    pub(crate) const LOG: &str = "cat C:/x/patch.log";
    /// The test platform's failure.
    pub(crate) const FAILED: &str = "agentspace: The test platform failed.";

    impl TestPlatform {
        /// The test platform on Windows' document.
        pub(crate) fn powershell() -> TestPlatform {
            TestPlatform {
                document: "AWS-RunPowerShellScript",
                fails: false,
            }
        }

        fn failure() -> Failure {
            Failure {
                message: FAILED.to_string(),
                caller: CallerFailure::NotAvailable,
            }
        }
    }

    impl HostPlatform for TestPlatform {
        fn platform(&self) -> &'static str {
            "test/platform"
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
            self.document
        }
        fn remote_command(&self, script: &str) -> Result<String, Failure> {
            Ok(script.to_string())
        }
        fn bootstrap(&self) -> Result<Bootstrap, Failure> {
            Err(TestPlatform::failure())
        }
        fn sync_plan(&self) -> Result<SyncPlan, Failure> {
            Err(TestPlatform::failure())
        }
        fn patch_check_command(&self) -> Result<String, Failure> {
            if self.fails {
                Err(TestPlatform::failure())
            } else {
                Ok(QUESTION.to_string())
            }
        }
        fn patch_messages(&self) -> PatchMessages {
            PatchMessages {
                phase: PHASE.to_string(),
                log_command: LOG.to_string(),
            }
        }
        fn horizon_command(&self, horizon: u64) -> Result<String, Failure> {
            if self.fails {
                Err(TestPlatform::failure())
            } else {
                Ok(format!(
                    r#"[IO.File]::WriteAllText("C:\x\lease-horizon", "{horizon}", [Text.Encoding]::ASCII)"#
                ))
            }
        }
        fn grow_script(&self) -> Result<String, Failure> {
            Err(TestPlatform::failure())
        }
        fn push_zone(&self, _windows_zone_id: &str, _iana: &str) -> Result<String, Failure> {
            Err(TestPlatform::failure())
        }
        fn push_script(&self, _zone: &str) -> Result<String, Failure> {
            Err(TestPlatform::failure())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::TestPlatform;
    use super::*;

    #[test]
    fn linux_gets_the_shorthand_unchanged() {
        let command = "echo a, [b] = c";
        assert_eq!(
            parameters(&agentspace_platforms::LINUX_ARM64, command),
            "commands=echo a, [b] = c"
        );
        assert_eq!(
            parameters(&agentspace_platforms::LINUX_AMD64, command),
            "commands=echo a, [b] = c"
        );
    }

    #[test]
    fn a_powershell_document_gets_the_json_value_with_the_command_escaped() {
        let command = r#"& "C:\Program Files\PowerShell\7\pwsh.exe" -File a.ps1, [x]"#;
        let value = parameters(&TestPlatform::powershell(), command);
        assert_eq!(
            value,
            r#"{"commands":["& \"C:\\Program Files\\PowerShell\\7\\pwsh.exe\" -File a.ps1, [x]"]}"#
        );
        assert!(!value.starts_with("commands="));
        let parsed: serde_json::Value = serde_json::from_str(&value).unwrap();
        assert_eq!(parsed["commands"][0], command);
    }

    /// Control characters are escaped in the JSON value, and the value reads back as the command.
    #[test]
    fn a_powershell_documents_json_value_escapes_control_characters() {
        let command = "a\r\nb\tc\u{1}d\u{1f}e\u{7f}";
        let value = parameters(&TestPlatform::powershell(), command);
        assert_eq!(
            value,
            "{\"commands\":[\"a\\r\\nb\\tc\\u0001d\\u001fe\u{7f}\"]}"
        );
        assert!(!value.chars().any(|c| c.is_ascii_control() && c != '\u{7f}'));
        let parsed: serde_json::Value = serde_json::from_str(&value).unwrap();
        assert_eq!(parsed["commands"][0], command);
    }

    #[test]
    fn any_document_but_the_shell_ones_gets_the_json_value() {
        let other = TestPlatform {
            document: "AWS-RunSomethingElse",
            fails: false,
        };
        assert_eq!(parameters(&other, "x"), r#"{"commands":["x"]}"#);
        let shell = TestPlatform {
            document: "AWS-RunShellScript",
            fails: false,
        };
        assert_eq!(parameters(&shell, "x"), "commands=x");
    }

    #[test]
    fn the_windows_platforms_document_gets_the_json_value() {
        let windows = agentspace_platforms::for_platform("windows/amd64").unwrap();
        assert_eq!(windows.ssm_document(), "AWS-RunPowerShellScript");
        assert_eq!(
            parameters(windows, "Write-Output 1"),
            r#"{"commands":["Write-Output 1"]}"#
        );
    }
}
