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
//! 3. Finds the host from the argument ([`host`]: no argument is the box,
//!    `agentspace-host.<host name>` is that host, by its project and platform, a lease alias is
//!    looked up in the lease state file, read without its lock, and then the `ready` host of its
//!    record's project on its record's platform, or the box for a `linux/arm64` lease; a lease
//!    alias whose lease is not `ready`, or an argument of lease alias form while the lease state
//!    file cannot be read or is not valid, is refused with exit 1 before the host registry is read,
//!    #0229, and a `ready` lease of another platform with no `ready` host there is refused once
//!    the registry is read, D91 and pin P13 of v0.12-T01), then picks the host's platform seam
//!    with `agentspace_platforms::for_platform` ([`platform_of`]), then runs `up`'s steps on that
//!    host in
//!    its own process, in caller mode (D42): the instance ID from the host's record in the host
//!    registry or its stack, the start-and-wait step of `agentspace-box`, and, unless
//!    `AGENTSPACE_NESTED` was set as the program started, the token push of `agentspace-push` over
//!    the alias, with that host's push stamp (D50). Anything they write to standard output goes to
//!    standard error. The push decides by its stamp alone, so a start of the host makes it due no
//!    more than any other connection does (#0167). Its ssh's stderr is relayed to the
//!    ProxyCommand's standard error line by line, with its nested ProxyCommand's
//!    `The host is ready.` dropped unless `AGENTSPACE_DIAGNOSTIC` is set ([`push_stderr`];
//!    operator.md, What the laptop sends, step 2).
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
use std::sync::{Arc, Mutex};

use agentspace_box::{SystemClock, start_and_wait};
use agentspace_hosts::{Host, host_for_keyed, is_box};
use agentspace_operator::{DEFAULT_ALIAS, Parsed, Printer, context_from, diagnostic_from, parse};
use agentspace_operator_api::{CallerFailure, Context, Failure, HostPlatform, Mode, Say};
use agentspace_push::{Lookup, SshStderr, StderrSink, push_to_host_using};
use agentspace_run::Runner;

pub use agentspace_operator::COULD_NOT_REACH;
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

/// The ProxyCommand's standard error, shared by its printer and its token push's relay, so that
/// each write reaches it whole and the two keep their order.
#[derive(Clone)]
struct SharedStderr(Arc<Mutex<Box<dyn Write + Send>>>);

impl Write for SharedStderr {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut out = self.0.lock().unwrap_or_else(|e| e.into_inner());
        out.write_all(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).flush()
    }
}

/// Where the caller-mode token push sends its ssh's stderr (operator.md, What the laptop sends,
/// step 2; #0167): a pipe whose lines reach `stderr` as they arrive, the nested ProxyCommand's
/// `The host is ready.` dropped, and nothing dropped when `diagnostic`, `AGENTSPACE_DIAGNOSTIC`
/// as the program started, is set. Never the operator's inherited stderr, and never the null
/// device. A person's `up`, `up -Host` and `sync` do not come here: they keep the default,
/// [`SshStderr::Inherit`].
pub fn push_stderr(stderr: StderrSink, diagnostic: bool) -> SshStderr {
    SshStderr::Relay {
        sink: stderr,
        diagnostic,
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
///
/// The push is unforced whether or not the host was started: its stamp alone decides whether it
/// is due (operator.md, The stamp; #0167). Its ssh's stderr goes to `push_stderr`.
#[allow(clippy::too_many_arguments)]
fn up(
    ctx: &Context,
    say: &dyn Say,
    alias: &str,
    host: &Host,
    platform: &dyn HostPlatform,
    env: &Arc<Env>,
    push_stderr: &SshStderr,
) -> Result<String, Failure> {
    check_start_is_built(platform)?;
    let lookup_env = Arc::clone(env);
    let runner = Runner::with_lookup(ctx.clone(), move |name| lookup_env.get(name));
    let cache = HostCache::new(&ctx.home, host);
    let id = start_and_wait(&runner, say, &SystemClock::new(), &cache, &host.stack)?;
    if !ctx.nested {
        let lookup_env = Arc::clone(env);
        let lookup: Arc<Lookup> = Arc::new(move |name| lookup_env.get(name));
        // The box's push is `None`, v0.06's stamp; every other host has a stamp of its own, under
        // its host name, which carries its platform (D50, D91).
        let push_host = push_host_name(host);
        // The push never fails a command; a panic in it is the "Could not send" warning, which
        // `push_to_host_using` says itself.
        let _ = push_to_host_using(
            ctx,
            say,
            &id,
            alias,
            false,
            push_host.as_deref(),
            lookup,
            &|ssh| ssh.with_stderr(push_stderr.clone()),
        );
    }
    Ok(id)
}

/// Step 3's lookup of the host, before `up`'s steps start anything (operator.md, From the
/// ProxyCommand): the context, then a lease alias's lease, so that a lease that is not `ready`, or
/// an argument of lease alias form while the lease records cannot be read, is refused before the
/// host registry is read and before anything is started (#0229; pin P21). Then the registry is
/// read for every argument, so one that is not valid fails here, and the host is chosen from it.
fn find_host(
    alias: Option<&str>,
    env: &Env,
    say: &dyn Say,
    clock: &dyn host::LeaseClock,
) -> Result<(Context, Host), Failure> {
    let ctx = context_from(&|name| env.get(name))?;
    let state_dir = host::state_dir(env.get("AGENTSPACE_STATE_DIR"), &ctx.home);
    let lease = host::lease_key(alias, &state_dir, say, clock)?;
    let hosts = agentspace_hosts::read(&ctx.home)
        .map_err(|error| host::registry_failure(&ctx.home, &error))?;
    let found = host::choose(alias, &hosts, lease.as_ref())?.clone();
    Ok((ctx, found))
}

/// The platform seam of `host`, by its record's platform and through
/// `agentspace_platforms::for_platform` alone (D101's change at T03's start). A record whose
/// platform agentspace does not support fails as a failed `up` does in caller mode, before anything
/// is started. **For the dev lead:** operator.md gives no text for that record, which only a
/// person's edit of the registry makes (a writer adds a record only for a supported platform); the
/// detail line here is this story's reading.
pub fn platform_of(host: &Host) -> Result<&'static dyn HostPlatform, Failure> {
    agentspace_platforms::for_platform(host.platform()).ok_or_else(|| Failure {
        message: format!(
            "agentspace: The host registry's record for {} names a platform agentspace does not support.",
            host.project
        ),
        caller: CallerFailure::CouldNotReach,
    })
}

/// The detail of a connection to a host whose platform's start-and-wait is not built yet.
pub const START_NOT_BUILT: &str =
    "agentspace: Starting and waiting for a host on this platform is not built yet.";

/// Whether `up`'s start-and-wait step can run on `platform` today: `agentspace_box`'s
/// `start_and_wait`, whose patch check is the Linux question sent with `AWS-RunShellScript`, the
/// rows both Linux platforms share (operator.md, The platform seam). **For the dev lead, phase B of
/// E0.12-T03-S04:** once E0.12-T03-S03's first pull request adds the platform-aware
/// `start_and_wait_on`, every platform starts through it with its own patch check and document,
/// and this goes. Until then a host of any other platform fails as a failed `up` does, before
/// anything is started.
pub fn check_start_is_built(platform: &dyn HostPlatform) -> Result<(), Failure> {
    match platform.platform() {
        "linux/arm64" | "linux/amd64" => Ok(()),
        _ => Err(Failure {
            message: START_NOT_BUILT.to_string(),
            caller: CallerFailure::CouldNotReach,
        }),
    }
}

/// The host name the push to `host` keeps its stamp under: `None` for the box, whose stamp is
/// v0.06's, and the host's folder name otherwise, `<project>` on `linux/arm64` and
/// `<project>.<os>-<arch>` on any other platform (D50, D91; pin P3 of v0.12-T03). So two hosts of
/// one project on two platforms never share a stamp.
pub fn push_host_name(host: &Host) -> Option<String> {
    (!is_box(host)).then(|| host.key().folder_name())
}

/// Steps 1 to 6. `Ok` is the exit code, which is the tunnel's.
fn proxy(
    args: &[OsString],
    mut env: Env,
    say: &Printer,
    diagnostic: bool,
    push_stderr: &SshStderr,
) -> Result<i32, Failure> {
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
    // check.
    let (ctx, found) = find_host(alias.as_deref(), &env, say, &host::SystemLeaseClock::new())?;
    let seam = platform_of(&found)?;
    let env = Arc::new(env);
    let mut stdout_guard = platform::StdoutGuard::new();
    let read_id = up(
        &ctx,
        say,
        alias.as_deref().unwrap_or(DEFAULT_ALIAS),
        &found,
        seam,
        &env,
        push_stderr,
    )?;
    // Step 5: the record's own ID, which the tunnel MUST target, the record being the host's by its
    // project and platform (D91). Architect's to confirm: when the registry or the record cannot
    // give one here, such as after a write of `up`'s that failed (which does not fail the
    // connection), the ID `up` read is used rather than a failure.
    let id = agentspace_hosts::read(&ctx.home)
        .ok()
        .and_then(|now| host_for_keyed(&now, &found.key()).and_then(|h| h.instance_id.clone()))
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
    // The printer and the token push's relayed ssh stderr write to the same stream.
    let stderr = Arc::new(Mutex::new(stderr));
    let say = Printer::new(
        Mode::Caller,
        false,
        diagnostic,
        Box::new(SharedStderr(Arc::clone(&stderr))),
    );
    let relay = push_stderr(stderr, diagnostic);
    match proxy(args, env, &say, diagnostic, &relay) {
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
    fn the_push_relays_its_ssh_stderr_and_drops_nothing_only_under_diagnostic() {
        let cap = Capture::default();
        for diagnostic in [false, true] {
            let sink: StderrSink = Arc::new(Mutex::new(cap.clone()));
            let choice = push_stderr(sink, diagnostic);
            assert!(
                matches!(choice, SshStderr::Relay { diagnostic: d, .. } if d == diagnostic),
                "{choice:?}"
            );
        }
    }

    #[test]
    fn the_printer_and_the_relay_share_one_stream() {
        let cap = Capture::default();
        let shared: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(Box::new(cap.clone())));
        let mut printer_side = SharedStderr(Arc::clone(&shared));
        printer_side.write_all(b"one\n").unwrap();
        let sink: StderrSink = shared;
        sink.lock().unwrap().write_all(b"two\r\n").unwrap();
        printer_side.write_all(b"three\n").unwrap();
        printer_side.flush().unwrap();
        assert_eq!(cap.text(), "one\ntwo\r\nthree\n");
    }

    /// A clock fixed before the leases' expiry, which never reads the real clock or sleeps.
    struct FixedClock;

    impl host::LeaseClock for FixedClock {
        fn now(&self) -> i64 {
            agentspace_state::parse_time("2026-11-20T10:30:00Z").unwrap()
        }
        fn elapsed(&self) -> std::time::Duration {
            std::time::Duration::ZERO
        }
        fn pause(&self, _: std::time::Duration) {}
    }

    /// Step 3's lookup in process, then the failure printed as `run` prints it, against a throwaway
    /// home of `name` that holds `leases` as its lease state file (none when `None`) and a host
    /// registry that is not valid, so that a lookup that gets past the lease fails on the registry.
    /// It never reaches step 2, which changes the process's `PATH`. Returns 1 for a failure, as
    /// `run` does, and 0 for a host found.
    fn go_home(name: &str, alias: &str, leases: Option<&[u8]>, diagnostic: bool) -> (i32, String) {
        let home = std::env::temp_dir().join(format!(
            "agentspace-ssh-proxy-lib-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        let state = home.join(".agentspace");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(agentspace_hosts::hosts_path(&home), b"not json").unwrap();
        if let Some(bytes) = leases {
            std::fs::write(state.join("leases.json"), bytes).unwrap();
        }
        let env = Env::from_pairs([
            ("USERPROFILE", home.clone().into_os_string()),
            ("HOME", home.clone().into_os_string()),
            ("LOCALAPPDATA", home.clone().into_os_string()),
            ("APPDATA", home.clone().into_os_string()),
            ("SystemRoot", home.join("no-system-root").into_os_string()),
        ]);
        let cap = Capture::default();
        let say = Printer::new(Mode::Caller, false, diagnostic, Box::new(cap.clone()));
        let code = match find_host(Some(alias), &env, &say, &FixedClock) {
            Ok(_) => 0,
            Err(failure) => {
                report(&say, &failure);
                1
            }
        };
        let _ = std::fs::remove_dir_all(&home);
        (code, cap.text())
    }

    /// A lease state file holding one `released` lease whose alias is `agentspace-agentspace-3a9c5e21`.
    fn released_file() -> Vec<u8> {
        let record = agentspace_state::Record {
            sized_for: None,
            plan: None,
            plan_source: None,
            id: "3a9c5e21-0000-4000-8000-000000000001".to_string(),
            project: "agentspace".to_string(),
            state: "released".to_string(),
            platform: "linux/arm64".to_string(),
            profile: "agent-linux".to_string(),
            size: "medium".to_string(),
            ttl_seconds: 3600,
            created_at: "2026-11-20T10:00:00Z".to_string(),
            expires_at: Some("2026-11-20T11:00:00Z".to_string()),
            released_at: Some("2026-11-20T10:30:00Z".to_string()),
            ssh: None,
            reasons: None,
            driver: Some("ec2".to_string()),
        };
        agentspace_state::serialize(&agentspace_state::StateFile {
            leases: vec![record],
        })
        .unwrap()
    }

    const LEASE: &str = "agentspace-agentspace-3a9c5e21";

    #[test]
    fn a_lease_that_is_not_ready_is_refused_before_the_registry_is_read() {
        let refused = format!(
            "agentspace: The lease for {LEASE} has been released; no connection was opened. Create a new lease.\n"
        );
        for diagnostic in [false, true] {
            let (code, err) = go_home("released", LEASE, Some(&released_file()), diagnostic);
            // The refusal's one line alone: the registry, which is not valid, was never read.
            assert_eq!((code, err.as_str()), (1, refused.as_str()), "{diagnostic}");
        }
        // The same home with no lease state file reaches the registry and fails on it, so the
        // refusal above came before it.
        let (code, err) = go_home("nolease", LEASE, None, false);
        assert_eq!(
            (code, err.as_str()),
            (1, format!("{COULD_NOT_REACH}\n").as_str())
        );
    }

    #[test]
    fn a_lease_form_argument_is_refused_while_the_lease_records_cannot_be_read() {
        let refused = format!(
            "agentspace: The lease records could not be read; no connection was opened for {LEASE}. Run 'agentspace lease list' to see the error.\n"
        );
        let (code, err) = go_home("broken", LEASE, Some(b"not json"), false);
        assert_eq!((code, err.as_str()), (1, refused.as_str()));
        // With the diagnostic, the line naming the file comes first, and the refusal stays last.
        let (code, err) = go_home("brokendiag", LEASE, Some(b"not json"), true);
        assert_eq!(code, 1);
        let lines: Vec<&str> = err.lines().collect();
        assert_eq!(lines.len(), 2, "{err}");
        assert!(
            lines[0].starts_with("agentspace: The lease state file "),
            "{err}"
        );
        assert_eq!(format!("{}\n", lines[1]), refused);
        // An argument not of lease alias form keeps v0.07's rule, and so reaches the registry.
        let (code, err) = go_home(
            "brokenother",
            "conformance-proxy-alias",
            Some(b"not json"),
            false,
        );
        assert_eq!(
            (code, err.as_str()),
            (1, format!("{COULD_NOT_REACH}\n").as_str())
        );
    }

    #[test]
    fn the_env_reads_only_what_it_was_given() {
        let env = Env::from_pairs([("USERPROFILE", "home")]);
        assert_eq!(env.get("USERPROFILE"), Some(OsString::from("home")));
        assert_eq!(env.get("PATH"), None);
    }

    // A host by its project and platform (D91, D101).

    use agentspace_hosts::{HostKey, HostState, Hosts};

    /// Step 3 in process, as [`go_home`] runs it, but with a valid host registry holding `hosts`,
    /// so a lookup that gets past the lease chooses from it. Returns 0 and the chosen host's key
    /// for a host found, and 1 and what was printed for a failure.
    fn go_registry(
        name: &str,
        alias: &str,
        leases: &[agentspace_state::Record],
        hosts: &Hosts,
    ) -> (i32, String, Option<HostKey>) {
        let home = std::env::temp_dir().join(format!(
            "agentspace-ssh-proxy-lib-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        let state = home.join(".agentspace");
        std::fs::create_dir_all(&state).unwrap();
        {
            let held = agentspace_hosts::lock(&home).unwrap();
            agentspace_hosts::write(&home, hosts, &held).unwrap();
        }
        let file = agentspace_state::StateFile {
            leases: leases.to_vec(),
        };
        std::fs::write(
            state.join("leases.json"),
            agentspace_state::serialize(&file).unwrap(),
        )
        .unwrap();
        let env = Env::from_pairs([
            ("USERPROFILE", home.clone().into_os_string()),
            ("HOME", home.clone().into_os_string()),
            ("LOCALAPPDATA", home.clone().into_os_string()),
            ("APPDATA", home.clone().into_os_string()),
            ("SystemRoot", home.join("no-system-root").into_os_string()),
        ]);
        let cap = Capture::default();
        let say = Printer::new(Mode::Caller, false, true, Box::new(cap.clone()));
        let found = match find_host(Some(alias), &env, &say, &FixedClock) {
            Ok((_, host)) => (0, cap.text(), Some(host.key())),
            Err(failure) => {
                report(&say, &failure);
                (1, cap.text(), None)
            }
        };
        let _ = std::fs::remove_dir_all(&home);
        found
    }

    /// A `ready` lease of `agentspace` on `platform`, alias `agentspace-agentspace-<hex8>`.
    fn ready_on(platform: &str, hex8: &str) -> agentspace_state::Record {
        let mut leases = agentspace_state::parse(&released_file()).unwrap().leases;
        let mut record = leases.remove(0);
        record.id = format!("{hex8}-0000-4000-8000-000000000001");
        record.state = "ready".to_string();
        record.released_at = None;
        record.platform = platform.to_string();
        record.ssh = Some(agentspace_state::Ssh {
            host_alias: format!("agentspace-agentspace-{hex8}"),
            user: "ubuntu".to_string(),
        });
        record
    }

    fn two_platforms() -> Hosts {
        let mut hosts = Hosts::seed();
        for platform in ["windows/amd64", "linux/arm64"] {
            hosts.hosts.push(agentspace_hosts::Host::new(
                &HostKey::new("agentspace", platform),
                HostState::Ready,
            ));
        }
        hosts
    }

    #[test]
    fn two_leases_of_one_project_on_two_platforms_reach_two_hosts() {
        let leases = [
            ready_on("windows/amd64", "0e1e2e3e"),
            ready_on("linux/arm64", "0a1a2a3a"),
        ];
        let hosts = two_platforms();
        let (code, err, key) =
            go_registry("twowin", "agentspace-agentspace-0e1e2e3e", &leases, &hosts);
        assert_eq!((code, err.as_str()), (0, ""));
        assert_eq!(key, Some(HostKey::new("agentspace", "windows/amd64")));
        let (code, err, key) = go_registry(
            "twolinux",
            "agentspace-agentspace-0a1a2a3a",
            &leases,
            &hosts,
        );
        assert_eq!((code, err.as_str()), (0, ""));
        assert_eq!(key, Some(HostKey::linux_arm64("agentspace")));
        // Each host's push keeps its own stamp, under its own host name.
        let windows = &hosts.hosts[1];
        let linux = &hosts.hosts[2];
        assert_eq!(
            push_host_name(windows).as_deref(),
            Some("agentspace.windows-amd64")
        );
        assert_eq!(push_host_name(linux).as_deref(), Some("agentspace"));
        assert_eq!(
            push_host_name(&hosts.hosts[0]),
            None,
            "The box's is v0.06's"
        );
    }

    #[test]
    fn a_ready_windows_lease_with_no_ready_windows_host_is_refused_with_its_one_line() {
        let leases = [ready_on("windows/amd64", "0e1e2e3e")];
        let mut hosts = Hosts::seed();
        hosts.hosts.push(agentspace_hosts::Host::new(
            &HostKey::linux_arm64("agentspace"),
            HostState::Ready,
        ));
        // `AGENTSPACE_DIAGNOSTIC` is set in `go_registry`, and adds no line.
        let (code, err, key) = go_registry(
            "noreadywin",
            "agentspace-agentspace-0e1e2e3e",
            &leases,
            &hosts,
        );
        assert_eq!(
            (code, err.as_str()),
            (
                1,
                "agentspace: The lease for agentspace-agentspace-0e1e2e3e has no ready host on its platform; no connection was opened.\n"
            )
        );
        assert_eq!(key, None);
    }

    #[test]
    fn the_start_is_built_for_both_linux_platforms_alone() {
        for platform in ["linux/arm64", "linux/amd64"] {
            let seam = agentspace_platforms::for_platform(platform).unwrap();
            assert_eq!(check_start_is_built(seam), Ok(()), "{platform}");
        }
        let windows = agentspace_platforms::for_platform("windows/amd64").unwrap();
        let failure = check_start_is_built(windows).unwrap_err();
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        assert_eq!(failure.message, START_NOT_BUILT);
    }

    #[test]
    fn a_record_on_a_platform_agentspace_does_not_support_cannot_be_reached() {
        let supported = agentspace_hosts::Host::new(
            &HostKey::new("agentspace", "windows/amd64"),
            HostState::Ready,
        );
        assert_eq!(
            platform_of(&supported).map(|seam| seam.platform()),
            Ok("windows/amd64")
        );
        let other = agentspace_hosts::Host::new(
            &HostKey::new("agentspace", "example/one"),
            HostState::Ready,
        );
        let failure = platform_of(&other).map(|seam| seam.platform()).unwrap_err();
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
    }
}
