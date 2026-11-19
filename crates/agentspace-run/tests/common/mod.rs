//! Shared helpers: stand-in programs, their recordings, and the one adapter to the crate's API.
//!
//! Every call into `agentspace-run` beyond the pinned `agentspace-operator-api` is in the
//! "Adapters" section below, so the dev lead can adapt the tests without touching an assertion.
#![allow(dead_code)]

use agentspace_operator_api::{Aws, Context, Failure, Remote};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

// ---------------------------------------------------------------- Adapters (the assumed API)

/// ASSUMED: `agentspace_run::Runner::with_lookup(ctx, lookup)` builds the type that implements `Aws` and
/// `Remote`. `lookup` answers the seam's variables (`AGENTSPACE_AWS_CMD` and the two others) and
/// is called each time a program is about to start.
pub fn make_runner(ctx: Context, env: &Env) -> impl Aws + Remote {
    let env = env.clone();
    agentspace_run::Runner::with_lookup(ctx, move |name: &str| env.get(name))
}

/// The same, reading the real process environment (used inside a child test process).
pub fn make_runner_real_env(ctx: Context) -> impl Aws + Remote {
    agentspace_run::Runner::with_lookup(ctx, |name: &str| std::env::var_os(name))
}

/// ASSUMED: `agentspace_run::ssh_program(&Context) -> Result<PathBuf, Failure>` (the card).
/// It reads the real process environment, so only a child process calls it.
pub fn ssh_program(ctx: &Context) -> Result<PathBuf, Failure> {
    agentspace_run::ssh_program(ctx)
}

// ---------------------------------------------------------------- The injectable environment

#[derive(Clone, Default)]
pub struct Env(Arc<Mutex<HashMap<String, OsString>>>);

impl Env {
    pub fn set(&self, name: &str, value: impl Into<OsString>) {
        self.0
            .lock()
            .unwrap()
            .insert(name.to_string(), value.into());
    }
    pub fn unset(&self, name: &str) {
        self.0.lock().unwrap().remove(name);
    }
    pub fn get(&self, name: &str) -> Option<OsString> {
        self.0.lock().unwrap().get(name).cloned()
    }
}

// ---------------------------------------------------------------- A throwaway rig

/// Tests of one binary run one at a time: a script being written in one thread while another
/// forks makes the exec fail with ETXTBSY, which would be a flake of the harness, not a finding.
static SERIAL: Mutex<()> = Mutex::new(());

pub struct Rig {
    _serial: std::sync::MutexGuard<'static, ()>,
    pub dir: TempDir,
    pub ctx: Context,
    pub env: Env,
}

impl Rig {
    pub fn new() -> Rig {
        Rig::with_nested(false)
    }
    pub fn with_nested(nested: bool) -> Rig {
        let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::Builder::new().prefix("asrun-").tempdir().unwrap();
        let home = dir.path().join("home");
        let root = dir.path().join("root");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        let ctx = Context {
            home,
            local_app_data: None,
            system_root: root,
            nested,
        };
        Rig {
            _serial: serial,
            dir,
            ctx,
            env: Env::default(),
        }
    }
    pub fn runner(&self) -> impl Aws + Remote {
        make_runner(self.ctx.clone(), &self.env)
    }
    /// A stand-in under `<rig>/<sub>/`, where `sub` may hold a space.
    pub fn standin(&self, sub: &str) -> Standin {
        Standin::new(&self.dir.path().join(sub))
    }
    /// A stand-in installed as Windows OpenSSH's `ssh.exe` or `scp.exe` under the context's
    /// `SystemRoot` (`name` is the file name).
    pub fn default_openssh(&self, name: &str) -> Standin {
        Standin::new(&self.ctx.system_root.join("System32").join("OpenSSH")).named(name)
    }
}

// ---------------------------------------------------------------- Stand-in programs

/// A stand-in program: records its arguments (`args.txt`, `argc.txt`), its stdin (`stdin.txt`)
/// the values of the few variables the tests ask about (`env.txt`, see `RECORDED`) and the NAMES of
/// its `*_POSIX_FD_STATE` variables (`fdnames.txt`) beside itself, never its whole environment (a
/// captured environment can hold a secret, and a test MUST NOT print one). It writes `out.txt` and
/// `err.txt` to its stdout and stderr, and exits with a chosen code. Shell script on Unix, `.cmd`
/// file on Windows.
pub struct Standin {
    pub dir: PathBuf,
    name: String,
    code: i32,
    out: String,
    err: String,
}

impl Standin {
    pub fn new(dir: &Path) -> Standin {
        Standin {
            dir: dir.to_path_buf(),
            name: if cfg!(windows) { "prog.cmd" } else { "prog" }.to_string(),
            code: 0,
            out: String::new(),
            err: String::new(),
        }
    }
    pub fn named(mut self, name: &str) -> Standin {
        self.name = name.to_string();
        self
    }
    pub fn exit(mut self, code: i32) -> Standin {
        self.code = code;
        self
    }
    pub fn stdout(mut self, text: &str) -> Standin {
        self.out = text.to_string();
        self
    }
    pub fn stderr(mut self, text: &str) -> Standin {
        self.err = text.to_string();
        self
    }
    /// Writes the program and returns it (made executable on Unix).
    pub fn write(self) -> Standin {
        std::fs::create_dir_all(&self.dir).unwrap();
        std::fs::write(self.dir.join("out.txt"), &self.out).unwrap();
        std::fs::write(self.dir.join("err.txt"), &self.err).unwrap();
        let script = if cfg!(windows) {
            format!(
                "@echo off\r\n\
                 setlocal DisableDelayedExpansion\r\n\
                 set \"D=%~dp0\"\r\n\
                 type nul > \"%D%args.txt\"\r\n\
                 set n=0\r\n\
                 :loop\r\n\
                 if \"%~1\"==\"\" goto done\r\n\
                 >>\"%D%args.txt\" echo(%1\r\n\
                 set /a n+=1\r\n\
                 shift\r\n\
                 goto loop\r\n\
                 :done\r\n\
                 >\"%D%argc.txt\" echo %n%\r\n\
                 type nul > \"%D%env.txt\"\r\n\
                 {win_named}\
                 for /f \"delims==\" %%a in ('set') do @>>\"%D%allnames.txt\" echo(%%a\r\n\
                 findstr /e /c:\"{suffix}\" \"%D%allnames.txt\" > \"%D%fdnames.txt\"\r\n\
                 del \"%D%allnames.txt\"\r\n\
                 findstr \"^\" > \"%D%stdin.txt\"\r\n\
                 type \"%D%out.txt\"\r\n\
                 type \"%D%err.txt\" 1>&2\r\n\
                 exit /b {code}\r\n",
                win_named = recorded_script(true),
                suffix = FD_STATE_SUFFIX,
                code = self.code
            )
        } else {
            format!(
                "#!/bin/sh\n\
                 d=$(dirname \"$0\")\n\
                 echo $# > \"$d/argc.txt\"\n\
                 : > \"$d/args.txt\"\n\
                 for a in \"$@\"; do printf '%s\\n' \"$a\" >> \"$d/args.txt\"; done\n\
                 : > \"$d/env.txt\"\n\
                 {unix_named}\
                 env | sed -n 's/^\\([^=]*{suffix}\\)=.*/\\1/p' > \"$d/fdnames.txt\"\n\
                 timeout 5 cat > \"$d/stdin.txt\"\n\
                 cat \"$d/out.txt\"\n\
                 cat \"$d/err.txt\" >&2\n\
                 exit {code}\n",
                unix_named = recorded_script(false),
                suffix = FD_STATE_SUFFIX,
                code = self.code
            )
        };
        let path = self.path();
        std::fs::write(&path, script).unwrap();
        make_executable(&path);
        self
    }
    pub fn path(&self) -> PathBuf {
        self.dir.join(&self.name)
    }
    pub fn value(&self) -> OsString {
        self.path().into_os_string()
    }
    /// Whether the stand-in has been started.
    pub fn ran(&self) -> bool {
        self.dir.join("env.txt").exists()
    }
    pub fn args(&self) -> Vec<String> {
        assert!(self.ran(), "The stand-in was never started");
        let argc: usize = read(&self.dir.join("argc.txt")).trim().parse().unwrap();
        let text = read(&self.dir.join("args.txt"));
        let args: Vec<String> = if argc == 0 {
            Vec::new()
        } else {
            text.strip_suffix('\n')
                .unwrap_or(&text)
                .split('\n')
                .map(unrecord)
                .collect()
        };
        assert_eq!(args.len(), argc, "Argument count and lines differ");
        args
    }
    pub fn stdin(&self) -> String {
        read(&self.dir.join("stdin.txt"))
    }
    /// The value the program saw for `name`, which MUST be one of `RECORDED`; `None` if it was
    /// unset. No other variable's value is ever recorded.
    pub fn env_var(&self, name: &str) -> Option<String> {
        assert!(
            RECORDED.contains(&name),
            "{name} is not a recorded variable"
        );
        read(&self.dir.join("env.txt")).lines().find_map(|l| {
            l.trim_end_matches('\r')
                .split_once('=')
                .filter(|(n, _)| *n == name)
                .map(|(_, v)| v.to_string())
        })
    }
    /// The NAMES, never the values, of the variables the program saw that end `_POSIX_FD_STATE`.
    pub fn fd_state_names(&self) -> Vec<String> {
        read(&self.dir.join("fdnames.txt"))
            .lines()
            .map(|l| l.trim_end_matches('\r').to_string())
            .filter(|l| !l.is_empty())
            .collect()
    }
}

/// The only variables whose values a stand-in records.
const RECORDED: [&str; 2] = ["AGENTSPACE_NESTED", "KEEP_ME_FD_STATE_NOT"];
/// The suffix of the variables a stand-in records by name only.
const FD_STATE_SUFFIX: &str = "_POSIX_FD_STATE";

/// Script lines that append `NAME=value` to `env.txt` for each `RECORDED` variable that is set.
fn recorded_script(windows: bool) -> String {
    let mut out = String::new();
    for name in RECORDED {
        if windows {
            // `set NAME` prints every variable that starts with NAME and no others.
            out.push_str(&format!("set {name} 2>nul >> \"%D%env.txt\"\r\n"));
        } else {
            out.push_str(&format!(
                "[ \"${{{name}+x}}\" ] && printf '%s=%s\\n' {name} \"${{{name}}}\" >> \"$d/env.txt\"\n"
            ));
        }
    }
    out
}

/// One recorded argument as the program received it. The `.cmd` stand-in records `%1` as it
/// stands, quotes included, since `%~1` would let `cmd.exe` read the `&`, `|`, `<` and `>` of an
/// unquoted copy as its own; `Command` quotes every argument that holds a space or one of
/// `cmd.exe`'s special characters, so one pair of enclosing quotes is removed here, and `echo`'s
/// CR. A shell script records each argument exactly.
fn unrecord(line: &str) -> String {
    if cfg!(windows) {
        let line = line.strip_suffix('\r').unwrap_or(line);
        line.strip_prefix('"')
            .and_then(|l| l.strip_suffix('"'))
            .unwrap_or(line)
            .to_string()
    } else {
        line.to_string()
    }
}

/// The result of starting a stand-in that a test wrote at a default program's name, `ssh.exe` or
/// `scp.exe` under the context's `SystemRoot`. Returns whether it ran.
///
/// On Unix the stand-in runs, and this checks it did. On Windows a stand-in is a `.cmd` script,
/// which the system cannot start under an `.exe` name, so this checks for the default's own
/// "Could not run" line naming that path (docs/operator.md, Programs it starts): that shows the
/// default, by full path, was the program chosen.
pub fn started_default(got: Result<i32, Failure>, standin: &Standin) -> bool {
    if cfg!(windows) {
        let f = got.expect_err("A .cmd script started as an .exe");
        assert_eq!(
            f.message,
            format!("agentspace: Could not run {}.", standin.path().display())
        );
        false
    } else {
        assert_eq!(got, Ok(0));
        assert!(standin.ran(), "The default was not started");
        true
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}
#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// A file that exists but the system cannot start (no execute permission on Unix; a text file
/// that is not an executable on Windows).
pub fn not_runnable_file(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(if cfg!(windows) { "plain.txt" } else { "plain" });
    std::fs::write(&path, "just text\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    path
}

/// The seam's refusal text, as operator.md gives it.
pub fn cannot_run_message(variable: &str, value: &str) -> String {
    format!("agentspace: {variable} names {value}, which cannot be run.")
}

/// ASSUMED (architect ruling, 2026-11-19): `agentspace_run::prepare_child_env(&mut Command)` removes
/// every `*_POSIX_FD_STATE` variable and sets `AGENTSPACE_NESTED=1`.
pub fn prepare_child_env(cmd: &mut std::process::Command) {
    agentspace_run::prepare_child_env(cmd)
}
