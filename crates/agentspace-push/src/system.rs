//! The push as the operator runs it: the system's own [`Files`], [`Unprotect`] and [`SshRun`], and
//! the one entry point that wires them to [`push_with`] (operator.md, The token push).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The Windows calls are in `win.rs`. What is here is portable: the pipes, the threads, the clock
//! and the outcomes of the push's ssh, which never touch the token anywhere but its stdin.

use crate::{
    COULD_NOT_SEND, Context, FileRead, Files, Outcome, Say, Seams, SshResult, SshRun, Unprotect,
    push_with_host, read_file,
};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

use crate::platform;

/// The seam variable that names the zone the push sends in place of the laptop's (operator.md,
/// The test seam). The only variable this crate reads, and it reads it as the push works out its
/// zone.
const TIMEZONE_VAR: &str = "AGENTSPACE_TIMEZONE";

/// The push's limit, from the moment its ssh starts: the content written, stdin closed, and the
/// ssh's exit (operator.md, What the laptop sends, steps 3 and 4).
pub const SSH_LIMIT: Duration = Duration::from_secs(20);

/// How long the ssh's stdout is waited for once the ssh has exited on its own (step 5). It does
/// not apply after a kill.
pub const STDOUT_WAIT: Duration = Duration::from_secs(10);

/// A look-up of an environment variable by name: the process's own for the operator, or a
/// stand-in's in a test. A library reads no variable but the seam's.
pub type Lookup = dyn Fn(&str) -> Option<OsString> + Send + Sync;

/// The laptop's files as the operator reads and writes them: a file is read afresh whole, and the
/// stamp is replaced with `agentspace-state`'s `write_atomically` (docs/state.md, Replacing a
/// file), after its folder has been made.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemFiles;

impl Files for SystemFiles {
    fn read(&self, path: &Path) -> FileRead {
        read_file(path)
    }

    fn write_stamp(&self, path: &Path, bytes: &[u8]) -> bool {
        let Some(dir) = path.parent() else {
            return false;
        };
        // A replace never creates a folder: the push makes `%USERPROFILE%\.agentspace` first.
        if !dir.as_os_str().is_empty() && std::fs::create_dir_all(dir).is_err() {
            return false;
        }
        agentspace_state::write_atomically(path, bytes).is_ok()
    }
}

/// The decryption of the credential's DPAPI blob with `CryptUnprotectData` for the Windows user
/// the program runs as (operator.md, The Bot credential). On any other system every blob is
/// refused.
#[derive(Debug, Clone, Copy, Default)]
pub struct DpapiUnprotect;

impl Unprotect for DpapiUnprotect {
    fn unprotect(&self, blob: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
        platform::unprotect(blob)
    }
}

/// A change to the ssh's command before it starts, for a test.
type Setup = dyn Fn(&mut Command) + Send + Sync;

/// The push's one ssh (operator.md, What the laptop sends, steps 2 to 5).
///
/// - **The program** is `agentspace-run`'s `ssh_program`: the one `AGENTSPACE_SSH_CMD` names, read
///   when the ssh is about to start, or Windows OpenSSH's `ssh.exe` by full path. A value that
///   names nothing runnable never falls back to the default: the push does not start.
/// - **The environment** is the operator's with `agentspace-run`'s `prepare_child_env` applied,
///   so the ssh sees no `*_POSIX_FD_STATE` variable and sees `AGENTSPACE_NESTED=1`. The token is in
///   no argument and no variable: it goes to the ssh's stdin pipe, in a thread of its own, and
///   nowhere else.
/// - **The wiring** is a pipe for its stdin and one for its stdout, and its stderr is the
///   operator's.
/// - **The clock** starts before the ssh does, and covers the write, the close and the wait. At
///   the limit the ssh and every process descended from it are terminated through the system's
///   API, the ssh last ([`SshResult::TimedOut`]). Otherwise the exit code and the stdout, which is
///   waited for at most [`STDOUT_WAIT`] more and taken as empty if it does not end.
pub struct SystemSsh {
    ctx: Context,
    lookup: Arc<Lookup>,
    limit: Duration,
    stdout_wait: Duration,
    setup: Option<Arc<Setup>>,
}

impl SystemSsh {
    /// The operator's ssh: the seam variable is read from the real environment, each time the ssh
    /// is about to start.
    pub fn new(ctx: &Context) -> Self {
        Self::with_lookup(ctx, Arc::new(|name| std::env::var_os(name)))
    }

    /// An ssh whose seam variable is read through `lookup`, which MUST answer for a variable's
    /// name as the process environment would.
    pub fn with_lookup(ctx: &Context, lookup: Arc<Lookup>) -> Self {
        SystemSsh {
            ctx: ctx.clone(),
            lookup,
            limit: SSH_LIMIT,
            stdout_wait: STDOUT_WAIT,
            setup: None,
        }
    }

    /// Shortens the two limits. For a test: the operator never calls it, and keeps the spec's 20
    /// and 10 seconds.
    #[must_use]
    pub fn with_limits(mut self, limit: Duration, stdout_wait: Duration) -> Self {
        self.limit = limit;
        self.stdout_wait = stdout_wait;
        self
    }

    /// Changes the ssh's [`Command`] after its arguments are set and before the environment rules
    /// apply, so that a test can give the program a throwaway working folder and home. For a test:
    /// the operator never calls it. It is given the command alone, never the content.
    #[must_use]
    pub fn with_command_setup(
        mut self,
        setup: impl Fn(&mut Command) + Send + Sync + 'static,
    ) -> Self {
        self.setup = Some(Arc::new(setup));
        self
    }

    fn command(&self, program: &Path, args: &[&str]) -> Command {
        let mut cmd = Command::new(program);
        cmd.args(args);
        if let Some(setup) = &self.setup {
            setup(&mut cmd);
        }
        // The operator never changes its own environment: the one function that removes every
        // `*_POSIX_FD_STATE` variable and sets `AGENTSPACE_NESTED=1` is applied to the child's.
        agentspace_run::prepare_child_env(&mut cmd);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        cmd
    }
}

/// Whether an argument reaches the ssh whole on a Windows command line, whatever program takes it
/// (operator.md, What the laptop sends, gap FA-15): no `%`, no double quote, no control character,
/// and no trailing backslash.
fn survives_the_command_line(arg: &str) -> bool {
    !arg.contains(['%', '"']) && !arg.chars().any(char::is_control) && !arg.ends_with('\\')
}

/// Stops a child that was started and cannot be used, or that reached the limit: its whole tree,
/// the ssh last. The push then ends at once (operator.md, What the laptop sends, step 4): nothing
/// waits for the terminated processes to finish ending, and the 10 seconds of step 5 never apply
/// after a kill.
fn abandon(child: &mut Child) {
    platform::terminate_tree(child);
}

impl SshRun for SystemSsh {
    fn run(&self, args: &[&str], stdin: &[u8]) -> SshResult {
        if !args.iter().all(|arg| survives_the_command_line(arg)) {
            return SshResult::CouldNotStart;
        }
        let Ok(program) = agentspace_run::ssh_program_with(&self.ctx, &*self.lookup) else {
            return SshResult::CouldNotStart;
        };

        // The clock starts as the ssh does.
        let deadline = Instant::now() + self.limit;
        let Ok(mut child) = self.command(&program, args).spawn() else {
            return SshResult::CouldNotStart;
        };
        let (Some(mut to_ssh), Some(mut from_ssh)) = (child.stdin.take(), child.stdout.take())
        else {
            abandon(&mut child);
            return SshResult::CouldNotStart;
        };

        // The stdout is read as it is written, so a chatty ssh never fills the pipe, and by a
        // thread that is not waited for after a kill.
        let (sent, received) = mpsc::channel::<Vec<u8>>();
        let reader = std::thread::Builder::new().spawn(move || {
            let mut bytes = Vec::new();
            let _ = from_ssh.read_to_end(&mut bytes);
            let _ = sent.send(bytes);
        });
        // The content is written, and stdin closed, by a thread too, so that a write that blocks
        // is cut off by the same limit. The copy it holds is zeroed when it is dropped. A write
        // that fails because the ssh has already exited is not an error here: the missing
        // confirmation reports it.
        let content = Zeroizing::new(stdin.to_vec());
        let writer = std::thread::Builder::new().spawn(move || {
            let _ = to_ssh.write_all(&content);
            drop(to_ssh);
        });
        if reader.is_err() || writer.is_err() {
            abandon(&mut child);
            return SshResult::CouldNotStart;
        }

        if !platform::wait_for_exit(&mut child, deadline) {
            // At the limit: the ssh's whole tree, the ssh last. Nothing more is read and nothing
            // recorded.
            abandon(&mut child);
            return SshResult::TimedOut;
        }
        let Ok(status) = child.wait() else {
            return SshResult::CouldNotStart;
        };
        // The stdout ends only when every process holding its pipe has closed it, and one the
        // ssh started can outlive the ssh, so the wait is bounded.
        let stdout = received.recv_timeout(self.stdout_wait).unwrap_or_default();
        SshResult::Exited {
            code: status.code().unwrap_or(-1),
            stdout,
        }
    }
}

/// One push as the operator runs it, over a look-up of the environment (operator.md, The token
/// push). It never fails; returns how the push ended, so a test can see it.
///
/// - The seam variables are read through `lookup`: `AGENTSPACE_SSH_CMD`, when the ssh is about to
///   start, and `AGENTSPACE_TIMEZONE`, as the push works out its zone.
/// - The laptop's own zone is read from the system only when no override is set.
pub fn push_using(
    ctx: &Context,
    say: &dyn Say,
    instance_id: &str,
    alias: &str,
    forced: bool,
    lookup: Arc<Lookup>,
) -> Outcome {
    push_using_with(ctx, say, instance_id, alias, forced, lookup, &|ssh| ssh)
}

/// [`push_using`], with `configure` given the push's [`SystemSsh`] before it is used, for a test
/// that shortens its limits or gives its program a throwaway working folder
/// ([`SystemSsh::with_limits`], [`SystemSsh::with_command_setup`]). The operator passes none.
pub fn push_using_with(
    ctx: &Context,
    say: &dyn Say,
    instance_id: &str,
    alias: &str,
    forced: bool,
    lookup: Arc<Lookup>,
    configure: &dyn Fn(SystemSsh) -> SystemSsh,
) -> Outcome {
    push_using_with_host(
        ctx,
        say,
        instance_id,
        alias,
        forced,
        lookup,
        configure,
        None,
    )
}

/// [`push_using_with`] for one host: `host` is `None` for the box, or `Some(project)` for another
/// host, which keeps its own stamp ([`crate::stamp_path_for`]) and never reads or writes the box's
/// (architect, 2026-11-20, for v0.07-T02).
#[allow(clippy::too_many_arguments)]
pub fn push_using_with_host(
    ctx: &Context,
    say: &dyn Say,
    instance_id: &str,
    alias: &str,
    forced: bool,
    lookup: Arc<Lookup>,
    configure: &dyn Fn(SystemSsh) -> SystemSsh,
    host: Option<&str>,
) -> Outcome {
    let ssh = configure(SystemSsh::with_lookup(ctx, Arc::clone(&lookup)));
    let zone = move || {
        let value = lookup(TIMEZONE_VAR).map(|v| v.to_string_lossy().into_owned());
        agentspace_zone::laptop_zone(value.as_deref())
    };
    let seams = Seams {
        files: &SystemFiles,
        unprotect: &DpapiUnprotect,
        ssh: &ssh,
        zone: &zone,
    };
    push_with_host(ctx, say, instance_id, alias, forced, &seams, host)
}

/// One token push (operator.md, The token push), as `up` and `sync` run it. **It never fails a
/// command:** every outcome is a warning through `say`, or nothing.
///
/// - `instance_id` is the instance's ID, which the stamp covers.
/// - `alias` is `up`'s `-Alias`, or `agentspace` for `sync`.
/// - `forced` is `sync`'s push: due whatever the stamp says, and not stopped by
///   `AGENTSPACE_NESTED`. Unforced, a nested `up` (`ctx.nested`) does not push.
///
/// The Bot token is decrypted with DPAPI, sent on the ssh's stdin and nowhere else, and dropped
/// when the push ends. A panic anywhere in the push, which it is not meant to have, becomes the
/// "Could not send" warning rather than a failed command.
pub fn push(ctx: &Context, say: &dyn Say, instance_id: &str, alias: &str, forced: bool) {
    push_to_host(ctx, say, instance_id, alias, forced, None);
}

/// [`push`] for one host: `host` is `None` for the box, whose stamp is v0.06's, or `Some(project)`
/// for another host, whose own stamp ([`crate::stamp_path_for`]) is the only one the push reads and
/// writes, its folder created if missing (architect, 2026-11-20, for v0.07-T02). It never fails a
/// command either.
pub fn push_to_host(
    ctx: &Context,
    say: &dyn Say,
    instance_id: &str,
    alias: &str,
    forced: bool,
    host: Option<&str>,
) {
    let lookup: Arc<Lookup> = Arc::new(|name| std::env::var_os(name));
    let pushed = std::panic::catch_unwind(AssertUnwindSafe(|| {
        push_using_with_host(
            ctx,
            say,
            instance_id,
            alias,
            forced,
            lookup,
            &|ssh| ssh,
            host,
        )
    }));
    if pushed.is_err() {
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| say.warning(COULD_NOT_SEND)));
    }
}
