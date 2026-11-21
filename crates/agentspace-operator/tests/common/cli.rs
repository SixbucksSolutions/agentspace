//! Black-box helpers: run the built `agentspace-operator` against a throwaway home and read the
//! help blocks from docs/operator.md. This file calls nothing in the crate's own API, so the
//! command-line tests build whatever the library looks like (docs/operator.md, Command line).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![allow(dead_code)]

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// The text every unknown-command and no-command message ends with.
pub const COMMANDS_SENTENCE: &str =
    "The commands are deploy, up, sync, bootstrap, provision, ca, down, status and grow.";
pub const ALIAS_APPLIES: &str = "agentspace: -Alias applies to up alone.";
pub const ALIAS_INVALID: &str = "agentspace: -Alias MUST be an SSH host alias of letters, digits, '.', '_' and '-' that does not start with '-'.";
pub const COMMANDS: [&str; 9] = [
    "deploy",
    "up",
    "sync",
    "bootstrap",
    "provision",
    "ca",
    "down",
    "status",
    "grow",
];

/// The host checks' messages (docs/operator.md, Command line, "The host checks run next").
pub const HOST_NOT_FOR_CA: &str = "agentspace: -Host does not apply to ca.";
pub const ALL_APPLIES: &str = "agentspace: -All applies to status, down and sync alone.";
pub const HOST_AND_ALL: &str = "agentspace: -Host and -All cannot be given together.";
pub const PROVISION_NEEDS_HOST: &str = "agentspace: provision needs -Host <project>.";
pub const INSTANCE_TYPE_APPLIES: &str = "agentspace: -InstanceType applies to deploy alone.";
pub const INSTANCE_TYPE_NEEDS_HOST: &str = "agentspace: -InstanceType needs -Host <project> of a host that is not the box; the box is sized through stack-parameters.json.";

/// A command line that names `command` and holds nothing else that its checks need: `provision`
/// requires `-Host`, so it gets one (operator.md, the host checks, check 4).
pub fn command_args(command: &str) -> Vec<&str> {
    if command.eq_ignore_ascii_case("provision") {
        vec![command, "-Host", "x"]
    } else {
        vec![command]
    }
}

pub fn no_command() -> String {
    format!("agentspace: No command given. {COMMANDS_SENTENCE}")
}
pub fn unknown_command(arg: &str) -> String {
    format!("agentspace: Unknown command '{arg}'. {COMMANDS_SENTENCE}")
}
pub fn unexpected(arg: &str) -> String {
    format!("agentspace: Unexpected argument '{arg}'.")
}
pub fn unknown_parameter(arg: &str) -> String {
    format!("agentspace: Unknown parameter '{arg}'.")
}
pub fn given_twice(name: &str) -> String {
    format!("agentspace: {name} is given more than once.")
}
pub fn needs_value(name: &str) -> String {
    format!("agentspace: {name} needs a value.")
}
pub fn not_a_number(arg: &str) -> String {
    format!("agentspace: -GiB MUST be a whole number, not '{arg}'.")
}
pub fn not_a_project(arg: &str) -> String {
    format!(
        "agentspace: -Host MUST be a project name of 1 to 32 letters, digits and '-' that starts and ends with a letter or digit, not '{arg}'."
    )
}
pub fn not_an_instance_type(arg: &str) -> String {
    format!(
        "agentspace: -InstanceType MUST be an EC2 instance type such as m9g.xlarge, not '{arg}'."
    )
}
pub fn no_host(project: &str) -> String {
    format!("agentspace: Project '{project}' has no host; provision -Host {project} makes one.")
}

/// What one run of the program gave.
#[derive(Debug)]
pub struct Run {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// What the throwaway home held after the run, beyond what the harness made.
    pub leftovers: Vec<String>,
}

impl Run {
    pub fn out(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
    pub fn err(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// A throwaway home, with `USERPROFILE`, `HOME`, `LOCALAPPDATA` and `APPDATA` pointing into it.
/// Nothing the operator names (the AWS CLI, ssh, scp) can run: each seam variable names a path
/// that does not exist, so a program that starts one fails with the seam's own message instead of
/// reaching anything real.
pub struct Home {
    dir: tempfile::TempDir,
}

const HARNESS_FOLDERS: [&str; 5] = ["profile", "home", "localappdata", "appdata", "systemroot"];

pub const USERPROFILE_UNSET: &str =
    "agentspace: USERPROFILE is not set, so there is no home folder.";
pub const SYSTEMROOT_UNSET: &str =
    "agentspace: SystemRoot is not set, so Windows' programs cannot be found.";

impl Home {
    pub fn new() -> Home {
        let dir = tempfile::tempdir().expect("a throwaway home");
        for name in HARNESS_FOLDERS {
            std::fs::create_dir(dir.path().join(name)).expect("a throwaway folder");
        }
        Home { dir }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn missing_program(&self) -> PathBuf {
        self.dir.path().join("no-such-program")
    }

    /// The command, ready to run: environment pointed into the home, the three seam variables
    /// naming nothing, and the rest of the operator's variables removed.
    pub fn command<S: AsRef<OsStr>>(&self, args: &[S]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_agentspace-operator"));
        c.args(args)
            .env("USERPROFILE", self.dir.path().join("profile"))
            .env("HOME", self.dir.path().join("home"))
            .env("LOCALAPPDATA", self.dir.path().join("localappdata"))
            .env("APPDATA", self.dir.path().join("appdata"))
            .env("SystemRoot", self.dir.path().join("systemroot"))
            // An empty folder: no run can find a real program on PATH, `go` among them
            // (rulings for E0.06-T03-S02), nor the AWS CLI's fallback.
            .env("PATH", self.dir.path().join("systemroot"))
            .env("AGENTSPACE_AWS_CMD", self.missing_program())
            .env("AGENTSPACE_SSH_CMD", self.missing_program())
            .env("AGENTSPACE_SCP_CMD", self.missing_program())
            .env_remove("AGENTSPACE_NESTED")
            .env_remove("AGENTSPACE_DIAGNOSTIC")
            .env_remove("AGENTSPACE_TIMEZONE")
            .env_remove("RUST_BACKTRACE")
            .current_dir(self.dir.path())
            .stdin(Stdio::null());
        c
    }

    fn leftovers(&self) -> Vec<String> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir(self.dir.path()).expect("the home lists") {
            let entry = entry.expect("an entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            if !HARNESS_FOLDERS.contains(&name.as_str()) {
                found.push(name);
            } else if std::fs::read_dir(entry.path())
                .expect("a folder lists")
                .next()
                .is_some()
            {
                found.push(format!("{name}/..."));
            }
        }
        found.sort();
        found
    }

    pub fn run<S: AsRef<OsStr>>(&self, args: &[S]) -> Run {
        self.run_command(self.command(args))
    }

    /// Runs a command built by `command`, after the caller changed its environment.
    pub fn run_command(&self, mut command: Command) -> Run {
        let out = command.output().expect("the program starts");
        Run {
            code: out.status.code(),
            stdout: out.stdout,
            stderr: out.stderr,
            leftovers: self.leftovers(),
        }
    }
}

/// Runs `args` with the environment changed by `change`: `env_remove("USERPROFILE")` and the like.
pub fn run_with_env<S: AsRef<OsStr>>(args: &[S], change: impl FnOnce(&mut Command)) -> Run {
    let home = Home::new();
    let mut c = home.command(args);
    change(&mut c);
    home.run_command(c)
}

/// Runs the program with `args` in a fresh throwaway home.
pub fn run<S: AsRef<OsStr>>(args: &[S]) -> Run {
    Home::new().run(args)
}

/// Splits a shown command line into arguments on single spaces: `"up -Alias x"`.
pub fn words(line: &str) -> Vec<&str> {
    line.split(' ').filter(|w| !w.is_empty()).collect()
}

/// A usage error or a refusal: one `agentspace: ` line on stderr, nothing on stdout, exit 1,
/// no program started and nothing written.
pub fn assert_failure<S: AsRef<OsStr> + std::fmt::Debug>(args: &[S], message: &str) {
    let r = run(args);
    assert_failure_run(&format!("{args:?}"), &r, message);
}

pub fn assert_failure_run(shown: &str, r: &Run, message: &str) {
    assert_eq!(
        r.code,
        Some(1),
        "{shown}: exit code; stderr was {:?}",
        r.err()
    );
    assert_eq!(r.out(), "", "{shown}: stdout MUST be empty on a failure");
    assert_eq!(r.err(), format!("{message}\n"), "{shown}: stderr");
    assert!(
        r.leftovers.is_empty(),
        "{shown}: wrote into the throwaway home: {:?}",
        r.leftovers
    );
}

/// A command line that passes every check reaches its command. In the harness's throwaway home no
/// instance-ID cache exists and the seam's three variables name nothing, so each command that
/// needs the AWS CLI stops at its first call with the seam's own failure, and `ca` stops at its
/// first step, the missing `ssh-keygen.exe` of the throwaway `SystemRoot`. `deploy`, `sync`,
/// `bootstrap` and `provision` are agentspace-sync's: see [`assert_reaches_sync_crate`].
pub fn assert_reaches_command<S: AsRef<OsStr> + std::fmt::Debug>(args: &[S], command: &str) {
    if matches!(command, "deploy" | "sync" | "bootstrap" | "provision") {
        assert_reaches_sync_crate(args, command);
        return;
    }
    let home = Home::new();
    let r = home.run(args);
    let expected = match command {
        "ca" => format!(
            "agentspace: {} is missing; install the Windows OpenSSH client.",
            home.path()
                .join("systemroot")
                .join("System32")
                .join("OpenSSH")
                .join("ssh-keygen.exe")
                .display()
        ),
        _ => format!(
            "agentspace: AGENTSPACE_AWS_CMD names {}, which cannot be run.",
            home.missing_program().display()
        ),
    };
    assert_failure_run(&format!("{args:?}"), &r, &expected);
}

/// `deploy`, `sync`, `bootstrap` or `provision` reaches agentspace-sync, and stops there in the
/// throwaway home at the earliest step the home cannot meet, with that step's own message
/// (docs/operator.md, deploy, sync, bootstrap, provision):
/// - `deploy` (the box's) at its step 1, the missing `%USERPROFILE%\.ssh\id_ed25519.pub`;
/// - `sync` at its step 1, the CA check's missing `ssh-keygen.exe`;
/// - `bootstrap` at its step 1, the start-and-wait's first AWS call, which the seam's AWS CLI,
///   naming nothing, cannot make;
/// - `provision -Host <project>`, a project with no record, at its step 1: the project is not in
///   the missing project list.
///
/// Each is exit 1, the one line, nothing on stdout and nothing written in the home. `PATH` names an
/// empty folder ([`Home::command`]), so no run can find the real `go`, and none gets as far as the
/// localswim checkout.
pub fn assert_reaches_sync_crate<S: AsRef<OsStr> + std::fmt::Debug>(args: &[S], command: &str) {
    let home = Home::new();
    let r = home.run(args);
    let profile = home.path().join("profile");
    let expected = match command {
        "deploy" => format!(
            "agentspace: {} does not exist.",
            profile.join(".ssh").join("id_ed25519.pub").display()
        ),
        "sync" => format!(
            "agentspace: {} is missing; install the Windows OpenSSH client.",
            home.path()
                .join("systemroot")
                .join("System32")
                .join("OpenSSH")
                .join("ssh-keygen.exe")
                .display()
        ),
        "bootstrap" => format!(
            "agentspace: AGENTSPACE_AWS_CMD names {}, which cannot be run.",
            home.missing_program().display()
        ),
        "provision" => {
            let words: Vec<String> = args
                .iter()
                .map(|a| a.as_ref().to_string_lossy().into_owned())
                .collect();
            let project = words
                .iter()
                .position(|w| w.eq_ignore_ascii_case("-Host"))
                .and_then(|i| words.get(i + 1))
                .map(|p| p.trim().to_lowercase())
                .expect("provision is given -Host");
            format!(
                "agentspace: Project '{project}' has no host and is not in {}, so no host is made for it.",
                profile.join(".agentspace").join("projects.json").display()
            )
        }
        other => panic!("{other} is not one of agentspace-sync's commands"),
    };
    assert_failure_run(&format!("{args:?}"), &r, &expected);
}

/// A command line given as a string of words, and the failure it MUST give.
pub fn assert_line_fails(line: &str, message: &str) {
    assert_failure(&words(line), message);
}

/// Help: the expected block on stdout, nothing on stderr, exit 0.
pub fn assert_help<S: AsRef<OsStr> + std::fmt::Debug>(args: &[S], level: &str) {
    let r = run(args);
    let expected = help_text(level);
    assert_eq!(
        r.code,
        Some(0),
        "{args:?}: exit code; stderr was {:?}",
        r.err()
    );
    assert_eq!(r.err(), "", "{args:?}: help writes nothing to stderr");
    assert_eq!(
        r.out(),
        expected,
        "{args:?}: help for {level:?} byte for byte"
    );
    assert!(
        r.stdout.ends_with(b"\n"),
        "{args:?}: the last line ends in a line feed"
    );
    assert!(!r.stdout.contains(&b'\r'), "{args:?}: no carriage return");
    assert!(
        r.leftovers.is_empty(),
        "{args:?}: wrote into the throwaway home: {:?}",
        r.leftovers
    );
}

pub fn assert_line_help(line: &str, level: &str) {
    assert_help(&words(line), level);
}

/// docs/operator.md, found from this crate's folder.
pub fn operator_md_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/operator.md")
}

/// Every help block of docs/operator.md's Commands section, by level: `top` and each command's
/// name. A block is the fenced code that follows a line `**Help:**`. Each is returned with a
/// line feed after every line, the last included, and no carriage return, whatever the checkout's
/// line endings are.
pub fn help_blocks() -> &'static HashMap<String, String> {
    static BLOCKS: OnceLock<HashMap<String, String>> = OnceLock::new();
    BLOCKS.get_or_init(|| {
        let path = operator_md_path();
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut blocks = HashMap::new();
        let mut in_commands = false;
        let mut level: Option<String> = None;
        let mut lines = text.lines();
        while let Some(line) = lines.next() {
            if let Some(h2) = line.strip_prefix("## ") {
                in_commands = h2.trim() == "Commands";
                level = None;
                continue;
            }
            if !in_commands {
                continue;
            }
            if let Some(h3) = line.strip_prefix("### ") {
                level = Some(if h3.trim() == "The top level" {
                    "top".to_string()
                } else {
                    h3.trim().to_string()
                });
                continue;
            }
            // `**Help:**` for a command; the top level's marker names the architect's ruling
            // (`**Help** (architect, ...):`), so a marker that starts `**Help** (` counts too.
            let marker = line.trim_end();
            if marker == "**Help:**" || marker.starts_with("**Help** (") {
                let level = level.clone().expect("a Help block under a level heading");
                // The fence opens on the next non-empty line.
                let mut fence = lines.next().expect("a fence after Help:");
                while fence.trim().is_empty() {
                    fence = lines.next().expect("a fence after Help:");
                }
                assert!(
                    fence.trim_end().starts_with("```"),
                    "Help: for {level} is followed by a fence"
                );
                let mut block = String::new();
                for body in lines.by_ref() {
                    if body.trim_end().starts_with("```") {
                        break;
                    }
                    block.push_str(body);
                    block.push('\n');
                }
                let was = blocks.insert(level.clone(), block);
                assert!(was.is_none(), "two Help: blocks for {level}");
            }
        }
        blocks
    })
}

pub fn help_text(level: &str) -> String {
    help_blocks()
        .get(level)
        .unwrap_or_else(|| panic!("docs/operator.md has no help block for {level:?}"))
        .clone()
}
