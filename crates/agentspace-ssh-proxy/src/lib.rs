//! The ProxyCommand, `agentspace-ssh-proxy.exe` (docs/operator.md, The ProxyCommand; What a caller
//! sees; D42).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! ssh runs the ProxyCommand in place of a TCP connection and talks SSH over its standard input
//! and output, so **nothing but the tunnel MUST write to standard output**, and nothing before
//! the tunnel MUST read standard input. Everything the ProxyCommand says goes to standard error,
//! in caller mode ([`agentspace_operator::Printer`] with [`Mode::Caller`]): the caller's progress
//! lines, the caller's failure lines, and no backend text unless `AGENTSPACE_DIAGNOSTIC` is set.
//!
//! # What it does, in order (operator.md, The ProxyCommand)
//!
//! 1. Refuses a second argument, `agentspace: agentspace-ssh-proxy.exe takes at most one argument,
//!    the SSH host alias.`, exit 1. The alias is checked exactly as `up -Alias <alias>` checks
//!    it, before anything starts.
//! 2. Puts the Session Manager plugin's folder and the AWS CLI's folder first on `PATH`, in that
//!    order ([`proxy_path`]).
//! 3. Finds the host from the argument ([`host`]: no argument is the box, `agentspace-host.<project>`
//!    is that project's host, a lease alias is looked up in the lease state file, read without its
//!    lock, and then its project's `ready` host or the box), then runs `up`'s steps on that host in
//!    its own process, in caller mode (D42): the instance ID from the host's record in the host
//!    registry or its stack, the start-and-wait step of `agentspace-box`, and, unless
//!    `AGENTSPACE_NESTED` was set as the program started, the token push of `agentspace-push` over
//!    the alias, with that host's push stamp (D50). Anything they write to standard output goes to
//!    standard error.
//! 4. A failure of step 3 is exit 1, and no tunnel.
//! 5. Reads that host's instance ID from its record again. A registry or a record that cannot be
//!    read here is not a failure when step 3 has the ID: the connection uses the one it read
//!    (operator.md, The host registry, as the operator uses it).
//! 6. Runs the tunnel ([`tunnel`]) with its own standard input and output and exits with its code.
//!    Before it starts, and before a failure exits, a console on standard error is left with the
//!    output mode ssh needs (What it MUST NOT do: "Leave the console broken"; Q17).
//!
//! **The program reads its environment once**, in `main`, into an [`Env`]. Every program it starts
//! gets its environment through `agentspace-run`'s `prepare_child_env`: every `*_POSIX_FD_STATE`
//! variable removed and `AGENTSPACE_NESTED` set to `1`.

#![deny(unsafe_code)]

mod cache;
pub mod host;
// The Windows calls, and the only module with `unsafe` (the Windows one, `win.rs`). On any other
// system `other.rs` stands in, which is NOT THE CONTRACT'S: it exists so that the crate builds and
// its portable logic is tested on Linux.
#[cfg(windows)]
#[allow(unsafe_code)]
#[path = "win.rs"]
mod platform;
#[cfg(not(windows))]
#[path = "other.rs"]
mod platform;
pub mod tunnel;

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::sync::Arc;

use agentspace_box::{SystemClock, start_and_wait};
use agentspace_hosts::{Host, host_for, is_box};
use agentspace_operator::{DEFAULT_ALIAS, Parsed, Printer, context_from, diagnostic_from, parse};
use agentspace_operator_api::{CallerFailure, Context, Failure, Mode, Say};
use agentspace_push::{Lookup, push_to_host_using};
use agentspace_run::Runner;

pub use agentspace_operator::COULD_NOT_REACH;
pub use cache::{CACHE_FILE, FileCache};
pub use host::{HOST_ALIAS_PREFIX, HostCache};

/// The program's file name, which the second-argument refusal names (operator.md, The programs).
pub const PROGRAM_NAME: &str = "agentspace-ssh-proxy.exe";
/// The Session Manager plugin's folder, first on `PATH` (operator.md, The ProxyCommand, step 2).
pub const PLUGIN_FOLDER: &str = r"C:\Program Files\Amazon\SessionManagerPlugin\bin";
/// The AWS CLI's folder, second on `PATH`.
pub const AWS_FOLDER: &str = r"C:\Program Files\Amazon\AWSCLIV2";

/// The variables the program reads, once, as it starts (operator.md, Environment variables).
const NAMES: [&str; 10] = [
    "USERPROFILE",
    "LOCALAPPDATA",
    "SystemRoot",
    "AGENTSPACE_NESTED",
    "AGENTSPACE_STATE_DIR",
    "AGENTSPACE_DIAGNOSTIC",
    "AGENTSPACE_AWS_CMD",
    "AGENTSPACE_SSH_CMD",
    "AGENTSPACE_SCP_CMD",
    "AGENTSPACE_TIMEZONE",
];

/// The environment the program reads once, as it starts. A library below it reads none but through
/// this, so that the rest of the program never reads the process's environment again.
#[derive(Clone, Default)]
pub struct Env {
    vars: HashMap<String, OsString>,
}

/// Names the variables and never shows a value (AGENTS.md: a captured environment's values MUST
/// NOT be printed, logged or formatted).
impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut names: Vec<&str> = self.vars.keys().map(String::as_str).collect();
        names.sort_unstable();
        f.debug_struct("Env").field("names", &names).finish()
    }
}

impl Env {
    /// The process's own environment: the variables the program reads, and `PATH`. A variable that
    /// is unset is absent.
    pub fn from_process() -> Env {
        let mut vars = HashMap::new();
        for name in NAMES.iter().copied().chain(["PATH"]) {
            if let Some(value) = std::env::var_os(name) {
                vars.insert(name.to_string(), value);
            }
        }
        Env { vars }
    }

    /// An environment of exactly these variables, for a test.
    pub fn from_pairs<I, K, V>(pairs: I) -> Env
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<OsString>,
    {
        Env {
            vars: pairs
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        }
    }

    /// A variable's value, or `None` when it is unset.
    pub fn get(&self, name: &str) -> Option<OsString> {
        self.vars.get(name).cloned()
    }

    fn set(&mut self, name: &str, value: OsString) {
        self.vars.insert(name.to_string(), value);
    }
}

/// The `PATH` step 2 gives: the Session Manager plugin's folder and the AWS CLI's folder, in that
/// order, each followed by `;`, then `PATH` as it was (gap FB-10).
pub fn proxy_path(old: Option<&OsStr>) -> OsString {
    let mut path = OsString::from(format!("{PLUGIN_FOLDER};{AWS_FOLDER};"));
    if let Some(old) = old {
        path.push(old);
    }
    path
}

/// The refusal of a second argument (operator.md, The ProxyCommand, step 1).
pub fn second_argument_refusal() -> String {
    format!("agentspace: {PROGRAM_NAME} takes at most one argument, the SSH host alias.")
}

/// Checks `alias` exactly as `up -Alias <alias>` does (operator.md, Command line): the failure is
/// the operator's own `-Alias` message, whatever the alias.
///
/// The operator's parser reads an argument after `-Alias` as the alias, except help, so an alias
/// that starts with `-` (which `-Alias` refuses whatever follows) is checked as a stand-in that
/// the rule refuses for another reason, and gives the same message.
pub fn check_alias(alias: &str) -> Result<(), Failure> {
    let probe = if alias.starts_with('-') { "!" } else { alias };
    match parse(&["up".to_string(), "-Alias".to_string(), probe.to_string()]) {
        Parsed::Failure(message) => Err(Failure {
            message,
            caller: CallerFailure::Same,
        }),
        _ => Ok(()),
    }
}

/// Prints a failure as a caller sees it: the caller's line, then, only under
/// `AGENTSPACE_DIAGNOSTIC`, the person's message after it.
fn report(say: &Printer, failure: &Failure) {
    say.failure(failure);
    if failure.caller != CallerFailure::Same {
        say.detail(&failure.message);
    }
}

/// `up`'s steps in caller mode (operator.md, up; What a caller sees) on `host`: the start-and-wait
/// step, which reads the instance ID from the host's record or its stack, and the token push with
/// the host's own stamp unless this is a nested `up`. Returns the instance ID once the host is
/// ready.
fn up(
    ctx: &Context,
    say: &dyn Say,
    alias: &str,
    host: &Host,
    env: &Arc<Env>,
) -> Result<String, Failure> {
    let lookup_env = Arc::clone(env);
    let runner = Runner::with_lookup(ctx.clone(), move |name| lookup_env.get(name));
    let cache = HostCache::new(&ctx.home, host);
    let id = start_and_wait(&runner, say, &SystemClock::new(), &cache, &host.stack)?;
    if !ctx.nested {
        let lookup_env = Arc::clone(env);
        let lookup: Arc<Lookup> = Arc::new(move |name| lookup_env.get(name));
        // The box's push is `None`, v0.06's stamp; every other host has a stamp of its own (D50).
        let push_host = (!is_box(host)).then_some(host.project.as_str());
        // The push never fails a command; a panic in it is the "Could not send" warning, which
        // `push_to_host_using` says itself.
        let _ = push_to_host_using(ctx, say, &id, alias, false, push_host, lookup, &|ssh| ssh);
    }
    Ok(id)
}

/// Steps 1 to 6. `Ok` is the exit code, which is the tunnel's.
fn proxy(args: &[OsString], mut env: Env, say: &Printer, diagnostic: bool) -> Result<i32, Failure> {
    // Step 1.
    if args.len() > 1 {
        return Err(Failure {
            message: second_argument_refusal(),
            caller: CallerFailure::Same,
        });
    }
    let alias = args.first().map(|a| a.to_string_lossy().into_owned());
    // The alias is checked before anything starts: `up`'s check is the gate before any ssh.
    if let Some(alias) = &alias {
        check_alias(alias)?;
    }

    // Step 2. The tunnel's own command is given the path too, so a system whose process
    // environment cannot be changed still starts it right.
    let path = proxy_path(env.get("PATH").as_deref());
    platform::set_path(&path);
    env.set("PATH", path.clone());

    // Steps 3 to 5, with standard output sent to standard error. The host is found after the alias
    // check, and the registry is read for every argument, so one that is not valid fails here.
    let ctx = context_from(&|name| env.get(name))?;
    let hosts = agentspace_hosts::read(&ctx.home)
        .map_err(|error| host::registry_failure(&ctx.home, &error))?;
    let state_dir = host::state_dir(env.get("AGENTSPACE_STATE_DIR"), &ctx.home);
    let found = host::choose(alias.as_deref(), &hosts, &state_dir, say)?.clone();
    let env = Arc::new(env);
    let mut stdout_guard = platform::StdoutGuard::new();
    let read_id = up(
        &ctx,
        say,
        alias.as_deref().unwrap_or(DEFAULT_ALIAS),
        &found,
        &env,
    )?;
    // Step 5: the record's own ID, which the tunnel MUST target. Architect's to confirm: when the
    // registry or the record cannot give one here, such as after a write of `up`'s that failed
    // (which does not fail the connection), the ID `up` read is used rather than a failure.
    let id = agentspace_hosts::read(&ctx.home)
        .ok()
        .and_then(|now| host_for(&now, &found.project).and_then(|h| h.instance_id.clone()))
        .unwrap_or(read_id);
    // The tunnel's standard output is the ProxyCommand's own.
    stdout_guard.restore();

    // Step 6.
    let program = tunnel::resolve(env.get(tunnel::AWS_VAR).as_deref())?;
    // The ProxyCommand's own work ends here: a console on stderr gets the output mode ssh needs
    // before the tunnel starts (What it MUST NOT do).
    platform::repair_console();
    let code = tunnel::run(&program, &id, &path, diagnostic)?;
    if code != 0 {
        say.failure(&Failure {
            message: tunnel::CONNECTION_FAILED.to_string(),
            caller: CallerFailure::Same,
        });
    }
    Ok(code)
}

/// The whole program: `args` are its arguments without its own name, `env` the environment read
/// as it started, and `stderr` where every message goes. Returns the exit code: the tunnel's, or
/// 1 for any failure before it.
///
/// It runs `up`'s steps for real, starts the tunnel with the process's own standard input and
/// output, and, on Windows, changes the process's `PATH`, so a test runs the program, and does
/// not call this in its own process.
pub fn run(args: &[OsString], env: Env, stderr: Box<dyn Write + Send>) -> i32 {
    let diagnostic = diagnostic_from(&|name| env.get(name));
    let say = Printer::new(Mode::Caller, false, diagnostic, stderr);
    match proxy(args, env, &say, diagnostic) {
        Ok(code) => code,
        Err(failure) => {
            report(&say, &failure);
            platform::repair_console();
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Write for Capture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Capture {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    /// Runs the program in process. Every case here fails before step 2; even so, the home is a
    /// throwaway one, every seam variable names a program that does not exist (so nothing can
    /// fall back to a real one), and `SystemRoot` is unset, so the home check stops anything that
    /// got past the arguments.
    fn go(args: &[&str]) -> (i32, String) {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let home =
            std::env::temp_dir().join(format!("agentspace-ssh-proxy-lib-{}", std::process::id()));
        let missing = home.join("no-such-program.cmd");
        let env = Env::from_pairs([
            ("USERPROFILE", home.clone().into_os_string()),
            ("HOME", home.clone().into_os_string()),
            ("LOCALAPPDATA", home.clone().into_os_string()),
            ("APPDATA", home.into_os_string()),
            ("AGENTSPACE_AWS_CMD", missing.clone().into_os_string()),
            ("AGENTSPACE_SSH_CMD", missing.clone().into_os_string()),
            ("AGENTSPACE_SCP_CMD", missing.into_os_string()),
            ("AGENTSPACE_TIMEZONE", OsString::from("America/New_York")),
        ]);
        let cap = Capture::default();
        let code = run(&args, env, Box::new(cap.clone()));
        (code, cap.text())
    }

    const ALIAS_TEXT: &str = "agentspace: -Alias MUST be an SSH host alias of letters, digits, '.', '_' and '-' that does not start with '-'.\n";

    #[test]
    fn a_second_argument_is_refused_before_anything_else() {
        let (code, err) = go(&["a/b", "extra"]);
        assert_eq!(code, 1);
        assert_eq!(
            err,
            "agentspace: agentspace-ssh-proxy.exe takes at most one argument, the SSH host alias.\n"
        );
        // Two arguments are refused whatever they are, a bad alias among them.
        let (code, err) = go(&["-Fx", ""]);
        assert_eq!(code, 1);
        assert!(err.contains("takes at most one argument"), "{err}");
    }

    #[test]
    fn a_bad_alias_fails_with_the_alias_message_whatever_it_is() {
        for bad in [
            "-Fx", "", "a b", "a/b", "a;b", "-h", "--help", "-Quiet", "a\nb", "é",
        ] {
            let (code, err) = go(&[bad]);
            assert_eq!((code, err.as_str()), (1, ALIAS_TEXT), "{bad:?}");
        }
    }

    #[test]
    fn a_good_alias_passes_the_check() {
        for good in ["agentspace", "conformance-proxy-alias", "dev.box_1-x", "up"] {
            assert_eq!(check_alias(good), Ok(()), "{good}");
        }
    }

    #[test]
    fn the_two_folders_come_first_on_path_in_order() {
        let path = proxy_path(Some(OsStr::new("old1;old2")));
        assert_eq!(
            path,
            OsString::from(
                r"C:\Program Files\Amazon\SessionManagerPlugin\bin;C:\Program Files\Amazon\AWSCLIV2;old1;old2"
            )
        );
        assert_eq!(
            proxy_path(None),
            OsString::from(
                r"C:\Program Files\Amazon\SessionManagerPlugin\bin;C:\Program Files\Amazon\AWSCLIV2;"
            )
        );
    }

    #[test]
    fn the_env_reads_only_what_it_was_given() {
        let env = Env::from_pairs([("USERPROFILE", "home")]);
        assert_eq!(env.get("USERPROFILE"), Some(OsString::from("home")));
        assert_eq!(env.get("PATH"), None);
    }
}
