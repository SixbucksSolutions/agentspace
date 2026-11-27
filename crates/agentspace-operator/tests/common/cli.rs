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
    "The commands are deploy, up, sync, bootstrap, provision, ca, down, status, grow and guard.";
pub const ALIAS_APPLIES: &str = "agentspace: -Alias applies to up alone.";
pub const ALIAS_INVALID: &str = "agentspace: -Alias MUST be an SSH host alias of letters, digits, '.', '_' and '-' that does not start with '-'.";
pub const COMMANDS: [&str; 10] = [
    "deploy",
    "up",
    "sync",
    "bootstrap",
    "provision",
    "ca",
    "down",
    "status",
    "grow",
    "guard",
];

/// The host checks' messages (docs/operator.md, Command line, "The host checks run next").
pub const HOST_NOT_FOR_CA: &str = "agentspace: -Host does not apply to ca.";
/// Check 1 for `guard`: `ca`'s message with the command's name changed (architect, 2026-11-21,
/// for v0.08-T01, S01 batch).
pub const HOST_NOT_FOR_GUARD: &str = "agentspace: -Host does not apply to guard.";
pub const ALL_APPLIES: &str = "agentspace: -All applies to status, down and sync alone.";
pub const HOST_AND_ALL: &str = "agentspace: -Host and -All cannot be given together.";
pub const PROVISION_NEEDS_HOST: &str = "agentspace: provision needs -Host <project>.";
pub const INSTANCE_TYPE_APPLIES: &str = "agentspace: -InstanceType applies to deploy alone.";

/// The checks v0.12 adds, each right after the `-Host` check it mirrors (docs/operator.md, Command
/// line, checks 1a, 3a and 4a; architect, 2026-11-26, for v0.12-T01, pin P55).
pub const PLATFORM_NOT_FOR_CA: &str = "agentspace: -Platform does not apply to ca.";
pub const PLATFORM_NOT_FOR_GUARD: &str = "agentspace: -Platform does not apply to guard.";
pub const PLATFORM_AND_ALL: &str = "agentspace: -Platform and -All cannot be given together.";
pub const PLATFORM_NEEDS_HOST: &str = "agentspace: -Platform needs -Host <project>.";

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
/// `grow`'s refusal of a `-GiB` of 0 or less (operator.md, Command line, Usage errors).
pub fn gib_not_positive(arg: &str) -> String {
    format!("agentspace: -GiB MUST be 1 or more for grow, not '{arg}'.")
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
/// A `-Platform` value that the lease CLI's parser accepts and that is not supported: the lease
/// CLI's `unsupported_platform` message with `agentspace: ` before it (docs/operator.md, Command
/// line; `<platform>` is the value as the parser normalizes it).
pub fn platform_unsupported(platform: &str) -> String {
    format!(
        "agentspace: Platform '{platform}' is not supported; agentspace supports linux/arm64, linux/amd64 and windows/amd64."
    )
}
/// A `-Platform` value that the parser refuses, reported as given (control characters as `?`).
pub fn platform_not_a_pair(arg: &str) -> String {
    format!("agentspace: Platform '{arg}' is not an os/arch pair. Give one as in 'linux/arm64'.")
}
/// The no-record message for a host on `platform` (normalized, not `linux/arm64`): the host is
/// named by its host name, and the command it suggests holds `-Platform` (docs/operator.md, From
/// the command line; Messages; pins P12 and P44).
pub fn no_host_on(project: &str, platform: &str) -> String {
    format!(
        "agentspace: Project '{project}.{}' has no host; provision -Host {project} -Platform {platform} makes one.",
        platform.replace('/', "-")
    )
}

/// What one run of the program gave.
#[derive(Debug)]
pub struct Run {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// What the throwaway home held after the run, beyond what the harness made.
    pub leftovers: Vec<String>,
    /// The command line the run was given, as the program's own arguments.
    pub args: Vec<String>,
    /// Every file and folder the throwaway home held after the run, beyond the harness's five
    /// folders, each as a `/`-separated path from the home's root.
    pub written: Vec<String>,
}

/// What a `deploy` command line MAY leave in the throwaway home, and nothing else (architect,
/// 2026-11-23, for v0.09-T03, pin P44): the host's `provision.lock` that `deploy` takes before its
/// steps 1 to 3 (operator.md, deploy; P34), and the folders it is made in. The host is the one
/// `-Host` names, lower-cased, and `swimcoach`, the box, when `-Host` is not given. No other
/// command may leave any of them.
pub fn deploy_lock_paths(args: &[String]) -> Vec<String> {
    let mut command: Option<&str> = None;
    let mut host: Option<String> = None;
    let mut platform: Option<String> = None;
    let mut words = args.iter();
    while let Some(word) = words.next() {
        let valued = ["-GiB", "-Alias", "-Host", "-InstanceType", "-Platform"]
            .iter()
            .any(|p| word.eq_ignore_ascii_case(p));
        if valued {
            let value = words.next();
            if word.eq_ignore_ascii_case("-Host") {
                host = value.map(|v| v.trim().to_lowercase());
            }
            if word.eq_ignore_ascii_case("-Platform") {
                platform = value.map(|v| v.trim().to_lowercase());
            }
        } else if !word.starts_with('-') && command.is_none() {
            command = Some(word);
        }
    }
    if !command.is_some_and(|c| c.eq_ignore_ascii_case("deploy")) {
        return Vec::new();
    }
    let mut project = host.unwrap_or_else(|| "swimcoach".to_string());
    // A host off `linux/arm64` has its own folder, `<project>.<os>-<arch>` (operator.md, A host's
    // platform and its names). A line this helper is given never names a form but the three
    // supported platforms' normalized ones, since a refused value writes nothing.
    if let Some(p) = platform.filter(|p| p != "linux/arm64" && p.matches('/').count() == 1) {
        project = format!("{project}.{}", p.replace('/', "-"));
    }
    let hosts = "profile/.agentspace/hosts";
    vec![
        "profile/.agentspace".to_string(),
        hosts.to_string(),
        format!("{hosts}/{project}"),
        format!("{hosts}/{project}/provision.lock"),
    ]
}

/// Every path under `dir`, as `/`-separated paths from `root`, folders before what they hold.
fn walk(root: &Path, dir: &Path, into: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).expect("a folder lists") {
        let path = entry.expect("an entry").path();
        let relative = path.strip_prefix(root).expect("under the root");
        into.push(
            relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/"),
        );
        if path.is_dir() {
            walk(root, &path, into);
        }
    }
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
    /// naming nothing, and the rest of the operator's variables removed, `AGENTSPACE_STATE_DIR`,
    /// `AGENTSPACE_SSH_DIR` and `AGENTSPACE_PROXY_COMMAND` among them.
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
            // Every other seam a run reads: the real guard reads the state folder, and MUST NOT
            // read the test runner's own.
            .env_remove("AGENTSPACE_STATE_DIR")
            .env_remove("AGENTSPACE_SSH_DIR")
            .env_remove("AGENTSPACE_PROXY_COMMAND")
            .env_remove("AGENTSPACE_GUARD_CMD")
            .env_remove("AGENTSPACE_OPERATOR_CMD")
            .env_remove("RUST_BACKTRACE")
            .current_dir(self.dir.path())
            .stdin(Stdio::null());
        c
    }

    fn written(&self) -> Vec<String> {
        let mut found = Vec::new();
        for entry in std::fs::read_dir(self.dir.path()).expect("the home lists") {
            let path = entry.expect("an entry").path();
            let name = path
                .file_name()
                .expect("a name")
                .to_string_lossy()
                .into_owned();
            if HARNESS_FOLDERS.contains(&name.as_str()) {
                walk(self.dir.path(), &path, &mut found);
            } else {
                found.push(name);
            }
        }
        found.sort();
        found
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
            args: command
                .get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect(),
            written: self.written(),
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
    // A `deploy` line MAY leave its host's provision.lock and the folders it is in (P44).
    let allowed = deploy_lock_paths(&r.args);
    let beyond: Vec<&String> = r.written.iter().filter(|p| !allowed.contains(p)).collect();
    assert!(
        beyond.is_empty(),
        "{shown}: wrote into the throwaway home: {beyond:?}"
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
    if command == "guard" {
        assert_reaches_guard(args);
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

/// `guard` reaches its real run, and in the harness's empty throwaway home the run ends at its step
/// 3 (docs/operator.md, guard):
/// - **Step 1** takes `%USERPROFILE%\.agentspace\guard.lock`, making the folder first. Nothing
///   holds it, so the run goes on.
/// - **Step 2** finds no `hosts.json`, which is the box's record alone, and goes on.
/// - **Step 3** finds no `leases.json` in the state folder (`AGENTSPACE_STATE_DIR` is removed by
///   [`Home::command`], so it is `%USERPROFILE%\.agentspace`). **A missing lease state file fails
///   the run** (P18): it prints `agentspace: The lease state file <path> cannot be read: the file
///   does not exist.`, makes no AWS call, logs one `failed step=leases` line with no `host=`
///   field, and exits 1. The seam variables name nothing, so a call would have given the seam's
///   own message instead, and no ssh or scp is started.
///
/// So the run gives exit 1, that one line on stderr, nothing on stdout, and leaves exactly two
/// files in the home, both in `profile\.agentspace`: `guard.lock`, empty, and `guard.log`, one line.
pub fn assert_reaches_guard<S: AsRef<OsStr> + std::fmt::Debug>(args: &[S]) {
    let home = Home::new();
    let r = home.run(args);
    let shown = format!("{args:?}");
    let state = home.path().join("profile").join(".agentspace");
    let message = format!(
        "agentspace: The lease state file {} cannot be read: the file does not exist.",
        state.join("leases.json").display()
    );
    assert_eq!(
        r.code,
        Some(1),
        "{shown}: exit code; stderr was {:?}",
        r.err()
    );
    assert_eq!(r.out(), "", "{shown}: stdout MUST be empty on a failure");
    assert_eq!(r.err(), format!("{message}\n"), "{shown}: stderr");
    // The run writes only into the throwaway profile's `.agentspace` folder.
    assert_eq!(
        r.leftovers,
        ["profile/..."],
        "{shown}: wrote beyond the profile folder"
    );
    let mut names: Vec<String> = std::fs::read_dir(&state)
        .expect("the guard made its state folder")
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["guard.lock", "guard.log"],
        "{shown}: exactly the lock and the log are left"
    );
    assert_eq!(
        std::fs::read(state.join("guard.lock")).expect("the lock file"),
        Vec::<u8>::new(),
        "{shown}: the lock is empty"
    );
    let log = std::fs::read_to_string(state.join("guard.log")).expect("the log");
    let line = log
        .strip_suffix('\n')
        .unwrap_or_else(|| panic!("{shown}: the log's line ends in a line feed: {log:?}"));
    assert!(!line.contains('\n'), "{shown}: one log line: {log:?}");
    // `<time>` is `YYYY-MM-DDTHH:MM:SSZ`, 20 characters, then a space.
    let (time, rest) = line.split_at(20.min(line.len()));
    assert!(
        time.len() == 20
            && time.bytes().enumerate().all(|(i, b)| match i {
                4 | 7 => b == b'-',
                10 => b == b'T',
                13 | 16 => b == b':',
                19 => b == b'Z',
                _ => b.is_ascii_digit(),
            }),
        "{shown}: the log's time is UTC to the whole second: {time:?}"
    );
    let reason = message.strip_prefix("agentspace: ").expect("the prefix");
    assert_eq!(
        rest,
        format!(" failed step=leases reason={reason}"),
        "{shown}: the log's one line, with no host= field"
    );
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
