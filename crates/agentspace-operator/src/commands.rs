//! The commands: `up`, `down`, `status`, `grow`, and the dispatch of `ca`, `deploy`, `sync`,
//! `bootstrap` and `provision`, for the box, for one host and for every host (docs/operator.md,
//! Commands; Which host a command acts on; Starting the box and waiting for it; Calls by command).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Every command works against the pinned [`Aws`], [`Remote`] and [`Say`], a [`Clock`] and the host
//! registry in the home folder, so that a test runs each one against stand-ins. What a command
//! reaches beyond those (the token push, `ca`, a host's key, certificate and alias, `sync`,
//! `deploy`, `bootstrap` and `provision`) is [`Reach`].

use std::path::PathBuf;
use std::time::Duration;

use agentspace_box::{Clock, InstanceIdCache};
use agentspace_ca::CaEnv;
use agentspace_hosts::{Host as Record, HostKey, HostState, Hosts, Target, host_for_keyed, is_box};
use agentspace_operator_api::{
    Aws, Call, CallerFailure, Context, Failure, HostPlatform, Output, Remote, SIGN_IN_MESSAGE, Say,
};
use serde_json::Value;

use crate::cache::{HostCache, read_registry};
use crate::status::{self, LeaseView};
use crate::{Command, DEFAULT_ALIAS, Request};

/// Where `status`'s report goes: stderr, whatever `-Quiet` says (operator.md, status). The pinned
/// [`Say`] suppresses with `-Quiet`, so the report has a channel of its own. The `Host` line that
/// `-All` prints before each host and the failure line of a host that failed go here too: neither
/// is suppressed by `-Quiet` (operator.md, Messages).
pub trait Report {
    /// Prints one line of the report, followed by a line feed.
    fn report(&self, line: &str);
    /// Prints a failure's one line, for a host of `-All` whose part failed and the command goes on.
    fn failure(&self, failure: &Failure);
}

/// What a command reaches beyond the AWS CLI, ssh and scp.
pub trait Reach {
    /// `up`'s token push, unforced, over `alias`, with the stamp of `host`: `None` for the box and
    /// `Some(project)` for another host. It never fails a command (operator.md, What the laptop
    /// checks).
    fn push(
        &self,
        ctx: &Context,
        say: &dyn Say,
        instance_id: &str,
        alias: &str,
        host: Option<&str>,
    );
    /// `ca`.
    fn ca(&self, ctx: &Context, env: &CaEnv, say: &dyn Say) -> Result<(), Failure>;
    /// The operator's key, a fresh one-hour certificate and the alias for the host of `project`,
    /// which is not the box: the four steps of operator.md, How the operator reaches a host.
    fn access(&self, ctx: &Context, env: &CaEnv, project: &str) -> Result<(), Failure>;
    /// `sync`, which signs for a host itself, between its steps 1 and 2.
    fn sync(
        &self,
        ctx: &Context,
        env: &CaEnv,
        target: Target<'_>,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure>;
    /// `sync -All`: every host's part, its `Host` lines, skips, failed hosts' messages and closing
    /// count are agentspace-sync's own, which writes `system\` and builds localswim once
    /// (operator.md, Every host at once; architect, 2026-11-21, for v0.07-T03).
    fn sync_all(
        &self,
        ctx: &Context,
        env: &CaEnv,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure>;
    /// `deploy`, with `-InstanceType` when it was given.
    #[allow(clippy::too_many_arguments)]
    fn deploy(
        &self,
        ctx: &Context,
        env: &CaEnv,
        target: Target<'_>,
        instance_type: Option<&str>,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure>;
    /// `bootstrap`.
    fn bootstrap(
        &self,
        ctx: &Context,
        env: &CaEnv,
        target: Target<'_>,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure>;
    /// `provision -Host <project>`.
    fn provision(
        &self,
        ctx: &Context,
        env: &CaEnv,
        project: &str,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure>;

    /// The platform seam's rows for a normalized platform (operator.md, The platform seam), or
    /// `None` for one agentspace does not support. The default is
    /// [`agentspace_platforms::for_platform`], the one function that picks a host's rows (D101);
    /// a test overrides it with a stand-in.
    fn seam(&self, platform: &str) -> Option<&dyn HostPlatform> {
        agentspace_platforms::for_platform(platform)
    }

    /// The operator's key, certificate and alias for the host `key`, which is not the box. The
    /// default calls [`Reach::access`] for a `linux/arm64` host, and fails as not built for any
    /// other, so a stand-in that implements only the project's methods keeps their meaning.
    fn access_keyed(&self, ctx: &Context, env: &CaEnv, key: &HostKey) -> Result<(), Failure> {
        if key.is_linux_arm64() {
            self.access(ctx, env, &key.project)
        } else {
            Err(not_built())
        }
    }

    /// `sync` of the host `key`. The default calls [`Reach::sync`] with the `linux/arm64` host's
    /// [`Target`], and fails as not built for any other platform: a bare `Target` names a project
    /// alone, so it MUST NOT carry a host of another platform (#183's review).
    fn sync_keyed(
        &self,
        ctx: &Context,
        env: &CaEnv,
        key: &HostKey,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure> {
        match linux_arm64_target(key) {
            Some(target) => self.sync(ctx, env, target, say, aws, remote),
            None => Err(not_built()),
        }
    }

    /// `deploy` of the host `key`, with `-InstanceType` when it was given; the default as
    /// [`Reach::sync_keyed`]'s, over [`Reach::deploy`].
    #[allow(clippy::too_many_arguments)]
    fn deploy_keyed(
        &self,
        ctx: &Context,
        env: &CaEnv,
        key: &HostKey,
        instance_type: Option<&str>,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure> {
        match linux_arm64_target(key) {
            Some(target) => self.deploy(ctx, env, target, instance_type, say, aws, remote),
            None => Err(not_built()),
        }
    }

    /// `bootstrap` of the host `key`; the default as [`Reach::sync_keyed`]'s, over
    /// [`Reach::bootstrap`].
    fn bootstrap_keyed(
        &self,
        ctx: &Context,
        env: &CaEnv,
        key: &HostKey,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure> {
        match linux_arm64_target(key) {
            Some(target) => self.bootstrap(ctx, env, target, say, aws, remote),
            None => Err(not_built()),
        }
    }

    /// `provision -Host <project> [-Platform <platform>]` of the host `key`. The default calls
    /// [`Reach::provision`] with the project for a `linux/arm64` host, and fails as not built for
    /// any other platform.
    fn provision_keyed(
        &self,
        ctx: &Context,
        env: &CaEnv,
        key: &HostKey,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure> {
        if key.is_linux_arm64() {
            self.provision(ctx, env, &key.project, say, aws, remote)
        } else {
            Err(not_built())
        }
    }
    /// The state folder whose `leases.json` `status -All` reads: `AGENTSPACE_STATE_DIR` when it is
    /// set and not empty, made full, else `<home>\.agentspace` (operator.md, Environment
    /// variables). The default is the home's, for a stand-in that moves nothing.
    fn lease_state_dir(&self, ctx: &Context) -> PathBuf {
        ctx.home.join(".agentspace")
    }
    /// The text `status -All`'s `Guard` line gives the stop time at the instant `unix`:
    /// `<h:mm><am|pm>`, rounded up to the next whole minute, in the zone the system is set to when
    /// the line is printed, with that instant's daylight saving rule applied; `AGENTSPACE_TIMEZONE`
    /// does not change it (operator.md, status -All). The default is
    /// [`agentspace_zone::local_clock_time_or_utc`], the one formatter of the guard's stop time
    /// (D63) with the one fallback: for a system that gives no offset for the instant it is
    /// `<h:mm><am|pm> UTC`, so it is never read as the laptop's own time. The operator never
    /// converts a time itself and keeps no fallback of its own, so a test that needs a fixed zone
    /// overrides this with a stand-in that yields the text.
    fn clock_time(&self, unix: i64) -> String {
        agentspace_zone::local_clock_time_or_utc(unix)
    }
    /// `guard`'s run (operator.md, guard; D56, D63), which the dispatch calls with the state
    /// folder [`Reach::lease_state_dir`] gives and the clock's reading to the whole second. The
    /// default is the real run, [`agentspace_guard::run`]; a test overrides it with a stand-in.
    fn guard(
        &self,
        input: &agentspace_guard::Input,
        say: &dyn Say,
        aws: &dyn Aws,
    ) -> Result<agentspace_guard::Report, Failure> {
        agentspace_guard::run(input, say, aws)
    }
}

/// Everything a command works against.
pub struct Parts<'a> {
    pub ctx: &'a Context,
    /// `USERNAME`, `COMPUTERNAME` and the OneDrive folders, read as the program started.
    pub ca_env: &'a CaEnv,
    pub say: &'a dyn Say,
    pub report: &'a dyn Report,
    pub aws: &'a dyn Aws,
    pub remote: &'a dyn Remote,
    pub clock: &'a dyn Clock,
    pub reach: &'a dyn Reach,
}

/// The failure of a step whose platform's rows are not built yet: the seam skeleton's own
/// (`agentspace_platforms::NOT_BUILT`), so a host of a platform no step serves yet fails before it
/// is reached, rather than through another platform's step.
pub(crate) fn not_built() -> Failure {
    Failure {
        message: agentspace_platforms::NOT_BUILT.to_string(),
        caller: CallerFailure::NotAvailable,
    }
}

/// The `linux/arm64` host `key` as a bare [`Target`], which names a project alone, or `None` for a
/// host of any other platform, which a `Target` cannot name (#183's review). The box is swimcoach's
/// `linux/arm64` host (D91).
pub(crate) fn linux_arm64_target(key: &HostKey) -> Option<Target<'_>> {
    if !key.is_linux_arm64() {
        None
    } else if key.is_box() {
        Some(Target::Box)
    } else {
        Some(Target::Host(&key.project))
    }
}

/// Whether the host's own steps are today's Linux steps: both Linux platforms share every step the
/// operator runs itself (operator.md, The platform seam: "the two Linux columns share every row but
/// the image, a new stack's instance type and `GOARCH`").
// Phase A of E0.12-T03-S02: until agentspace-platforms' Linux rows are built (E0.12-T03-S03), a
// Linux host keeps today's code byte for byte, and only another platform takes its rows from the
// seam. Phase B takes every platform's rows from the seam and removes this.
fn on_linux(key: &HostKey) -> bool {
    key.platform.starts_with("linux/")
}

/// A host as a command sees it: its record, and what a command takes from it (operator.md, From
/// the command line, "What a command takes from its host").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRef {
    pub project: String,
    /// The host's key, its project and its platform (D91): every step that names the host's folder,
    /// alias, stack or record takes its name from it, never from the project alone.
    pub key: HostKey,
    /// The record's `stack`: `agentbox` for the box.
    pub stack: String,
    /// The alias the host's own ssh, scp and push use: `agentspace` for the box (the user's own
    /// entry), `agentspace-host.<host name>` for every other host: `agentspace-host.<project>` on
    /// `linux/arm64`, and `agentspace-host.<project>.<os>-<arch>` on any other platform (D91).
    pub alias: String,
    pub is_box: bool,
    pub state: HostState,
    /// The record's `instance_id`, when it holds one.
    pub instance_id: Option<String>,
}

impl HostRef {
    fn from_record(record: &Record) -> HostRef {
        let box_host = is_box(record);
        let key = record.key();
        HostRef {
            project: record.project.clone(),
            stack: record.stack.clone(),
            alias: if box_host {
                DEFAULT_ALIAS.to_string()
            } else {
                key.operator_alias()
            },
            key,
            is_box: box_host,
            state: record.state,
            instance_id: record.instance_id.clone(),
        }
    }

    /// The host's name: `<project>` on `linux/arm64`, `<project>.<os>-<arch>` on any other
    /// platform (operator.md, A host's platform and its names).
    pub fn host_name(&self) -> String {
        self.key.host_name()
    }

    /// The host as the token push names it for its stamp: `None` for the box, else the host's
    /// folder's name, which is its project on `linux/arm64`.
    fn stamp_host(&self) -> Option<String> {
        (!self.is_box).then(|| self.key.folder_name())
    }
}

/// The record of the host `key` names: `-Host`'s project, or the box's without `-Host`, on the
/// command's platform, both compared exactly (D91). A host with no record fails before any program
/// starts or any AWS call is made (operator.md, From the command line), and a project's host on
/// one platform never stands in for its host on another.
fn resolve(hosts: &Hosts, key: &HostKey) -> Result<HostRef, Failure> {
    host_for_keyed(hosts, key)
        .map(HostRef::from_record)
        .ok_or_else(|| Failure {
            message: no_host_message(key),
            caller: CallerFailure::Same,
        })
}

/// `agentspace: Project '<project>' has no host; provision -Host <project> makes one.`, byte for
/// byte on `linux/arm64`; on any other platform the host is named by its host name, and
/// ` -Platform <platform>` follows `-Host <project>` (operator.md, From the command line, Messages;
/// pins P12 and P44 of v0.12-T01).
fn no_host_message(key: &HostKey) -> String {
    let project = &key.project;
    if key.is_linux_arm64() {
        format!("agentspace: Project '{project}' has no host; provision -Host {project} makes one.")
    } else {
        format!(
            "agentspace: Project '{}' has no host; provision -Host {project} -Platform {} makes one.",
            key.host_name(),
            key.platform
        )
    }
}

/// How a command that did not fail with a message ended: the program's exit code, 0 or 1, is
/// [`Outcome::exit_code`]. A failure with a message is the `Err` of [`dispatch`], which the caller
/// prints and exits 1 for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The command succeeded: exit 0.
    Done,
    /// The command failed and has already said why, through its own lines, so nothing more is
    /// printed: exit 1. `guard` ends so when a log line was dropped and no host failed (operator.md,
    /// guard, Its log).
    FailedAfterSaying,
}

impl Outcome {
    /// The program's exit code: 0 for [`Outcome::Done`], 1 for [`Outcome::FailedAfterSaying`].
    pub fn exit_code(self) -> i32 {
        match self {
            Outcome::Done => 0,
            Outcome::FailedAfterSaying => 1,
        }
    }
}

/// Runs a parsed command.
///
/// `guard` is its own: it acts on every host in one run, reads the registry itself, and may end in
/// [`Outcome::FailedAfterSaying`] (operator.md, guard). Every other command ends in
/// [`Outcome::Done`] or a [`Failure`].
pub fn dispatch(request: &Request, parts: &Parts) -> Result<Outcome, Failure> {
    if request.command == Command::Guard {
        return crate::guard::guard(parts);
    }
    dispatch_command(request, parts).map(|()| Outcome::Done)
}

/// Runs a parsed command that is not `guard`.
///
/// `ca`, `provision` and `sync -All` read no record here: `ca` takes no host, `provision` finds its
/// own (operator.md, provision, step 1), and `sync -All` reads the registry itself. Every other
/// command finds its host's record, or every record for `-All`, before it starts any program or
/// makes any AWS call.
fn dispatch_command(request: &Request, parts: &Parts) -> Result<(), Failure> {
    match request.command {
        Command::Ca => return parts.reach.ca(parts.ctx, parts.ca_env, parts.say),
        Command::Provision => {
            // The command line gives `provision` a `-Host` always, and `-Platform` or none.
            return parts.reach.provision_keyed(
                parts.ctx,
                parts.ca_env,
                &request.host_key(),
                parts.say,
                parts.aws,
                parts.remote,
            );
        }
        Command::Sync if request.all => {
            return parts.reach.sync_all(
                parts.ctx,
                parts.ca_env,
                parts.say,
                parts.aws,
                parts.remote,
            );
        }
        _ => {}
    }

    let registry = read_registry(&parts.ctx.home)?;
    if request.all {
        let hosts: Vec<HostRef> = registry.hosts.iter().map(HostRef::from_record).collect();
        return all(parts, request.command, &hosts);
    }
    let host = resolve(&registry, &request.host_key())?;
    match request.command {
        Command::Up => up(
            parts,
            &host,
            request.alias_given.then_some(request.alias.as_str()),
        ),
        Command::Down => down(parts, &host),
        Command::Status => status(parts, &host),
        Command::Grow => grow(parts, &host, request.gib),
        Command::Sync => parts.reach.sync_keyed(
            parts.ctx,
            parts.ca_env,
            &host.key,
            parts.say,
            parts.aws,
            parts.remote,
        ),
        Command::Deploy => parts.reach.deploy_keyed(
            parts.ctx,
            parts.ca_env,
            &host.key,
            request.instance_type.as_deref(),
            parts.say,
            parts.aws,
            parts.remote,
        ),
        Command::Bootstrap => parts.reach.bootstrap_keyed(
            parts.ctx,
            parts.ca_env,
            &host.key,
            parts.say,
            parts.aws,
            parts.remote,
        ),
        Command::Ca | Command::Provision | Command::Guard => Ok(()),
    }
}

/// `status -All` and `down -All` (operator.md, Every host at once): every record, in the file's
/// order, one at a time. `sync -All` is agentspace-sync's [`Reach::sync_all`], and never comes here.
/// Before each host the command prints `Host      <project> (<stack>), <state>`, whatever `-Quiet`
/// says. Each host's part is that command's own steps with the host, but `status` and `down` MUST
/// NOT sign a certificate or write an alias.
///
/// A failure ends that host's part, and the command goes on to the next. At the end, when any host
/// failed, it fails with `agentspace: <n> of <m> hosts failed.`. #0034's sign-in message ends the
/// command at once, since every later host's AWS call would fail the same way.
fn all(parts: &Parts, command: Command, hosts: &[HostRef]) -> Result<(), Failure> {
    let mut failed = 0usize;
    // `status -All` reads the lease state file once, before the first host's part, and takes the
    // report's time as it reads it (operator.md, status -All). A file it cannot read leaves out
    // every host's `Leases` and `Guard` lines and is said once, after the last host.
    let lease_dir = parts.reach.lease_state_dir(parts.ctx);
    let leases: Option<Result<LeaseView, String>> = matches!(command, Command::Status)
        .then(|| status::read_leases(&lease_dir, status::unix_seconds(parts.clock.utc_now())));
    let view = leases.as_ref().and_then(|read| read.as_ref().ok());
    for host in hosts {
        // From v0.12 the line names the host by its host name, which is its project on
        // `linux/arm64`, so that platform's line is unchanged (pins P12 and P44 of v0.12-T01).
        parts.report.report(&format!(
            "Host      {} ({}), {}",
            host.host_name(),
            host.stack,
            host.state.as_str()
        ));
        let part = match command {
            Command::Status => {
                // A host off `linux/arm64` gets its platform first, before anything else of its
                // part, a failure line included (D91; pin P57 of v0.12-T01).
                report_platform(parts, host);
                if host.instance_id.is_none() && host.state != HostState::Ready {
                    parts.report.report("Instance  none recorded");
                    if let Some(view) = view {
                        parts.report.report(&view.leases_line(&host.key));
                    }
                    Ok(())
                } else {
                    report_status(parts, host, view)
                }
            }
            // The command line gives `-All` to `status`, `down` and `sync` alone, and `sync -All`
            // is dispatched to agentspace-sync, so this is `down`.
            _ => {
                if host.instance_id.is_none() && host.state != HostState::Ready {
                    parts
                        .say
                        .progress("Skipping: no instance is recorded for this host.", None);
                    Ok(())
                } else {
                    stop(parts, host)
                }
            }
        };
        if let Err(failure) = part {
            if failure.message == SIGN_IN_MESSAGE {
                return Err(failure);
            }
            // Architect's to confirm: a failed host's own message is printed as its failure line,
            // whatever `-Quiet` says, before the final `<n> of <m> hosts failed.` line.
            parts.report.failure(&failure);
            failed += 1;
        }
    }
    if let Some(Err(reason)) = &leases {
        // Part of `status`'s report: on the stream the host lines go to, whatever `-Quiet` says,
        // and never a host's failure, so neither counted nor a change of the exit code (P25, P27).
        parts
            .report
            .report(&status::unreadable_message(&lease_dir, reason));
    }
    if failed > 0 {
        return Err(Failure {
            message: format!("agentspace: {failed} of {} hosts failed.", hosts.len()),
            caller: CallerFailure::Same,
        });
    }
    Ok(())
}

/// The deadline message's `ssh agentspace` is the box's alias; for another host it is the host's
/// (operator.md, Starting the box and waiting for it, step 3).
// Architect's to confirm: the deadline text is rewritten for a host by replacing its `ssh agentspace
// journalctl` with the host's alias, since operator.md gives no separate text for a host.
fn for_host(mut failure: Failure, host: &HostRef) -> Failure {
    if !host.is_box {
        failure.message = failure.message.replace(
            "ssh agentspace journalctl",
            &format!("ssh {} journalctl", host.alias),
        );
    }
    failure
}

/// A failed step's message as the cause in a sentence: without its `agentspace: ` prefix and its
/// last period.
fn cause(message: &str) -> &str {
    let text = message.strip_prefix("agentspace: ").unwrap_or(message);
    text.strip_suffix('.').unwrap_or(text)
}

/// `up` (operator.md, up): reads the host's instance ID, starts the host and waits for it, ending
/// with `Ready: <id>`, then pushes the token over `alias`, which is `-Alias` or the host's own,
/// unless this is a nested `up` (`AGENTSPACE_NESTED` was set as the program started). The push
/// never fails the command.
///
/// **A person's `up` of a host that is not the box, not nested, first makes the operator's key,
/// certificate and alias for it**, whether or not the push is due. A failure there is a push
/// failure: it names its cause, skips the push and fails nothing.
pub fn up(parts: &Parts, host: &HostRef, alias: Option<&str>) -> Result<(), Failure> {
    let cache = HostCache::keyed(&parts.ctx.home, &host.key);
    let id = start_and_wait(parts, host, &cache).map_err(|failure| for_host(failure, host))?;
    if parts.ctx.nested {
        return Ok(());
    }
    if !host.is_box
        && let Err(failure) = parts.reach.access_keyed(parts.ctx, parts.ca_env, &host.key)
    {
        parts.say.warning(&format!(
            "agentspace: Could not send the localswim token, because {}; the board will not work on the host this session.",
            cause(&failure.message)
        ));
        return Ok(());
    }
    parts.reach.push(
        parts.ctx,
        parts.say,
        &id,
        alias.unwrap_or(&host.alias),
        host.stamp_host().as_deref(),
    );
    Ok(())
}

/// `up`'s start and wait (operator.md, Starting the box and waiting for it), for the host's stack
/// and its record's instance ID.
// Phase A of E0.12-T03-S02: a Linux host keeps today's `agentspace_box::start_and_wait`, whose
// patch check is both Linux platforms' (operator.md, The platform seam). A host of another
// platform fails as not built before any AWS call, since today's check would ask it Linux's
// question. Phase B calls `agentspace_box::start_and_wait_on(aws, say, clock, cache, stack,
// platform)` with the host's seam for every platform.
fn start_and_wait(parts: &Parts, host: &HostRef, cache: &HostCache) -> Result<String, Failure> {
    if !on_linux(&host.key) {
        return Err(not_built());
    }
    agentspace_box::start_and_wait(parts.aws, parts.say, parts.clock, cache, &host.stack)
}

/// The host's instance ID: from its record, or from its stack's output when the record has none,
/// writing it into the record then. The record is the host's own, by its key.
fn instance_id(parts: &Parts, host: &HostRef) -> Result<String, Failure> {
    let cache = HostCache::keyed(&parts.ctx.home, &host.key);
    agentspace_box::instance_id(parts.aws, &cache as &dyn InstanceIdCache, &host.stack)
}

/// The host's key, certificate and alias, for a host that is not the box, by its key. The box has
/// none: it is reached through the user's own entry and key.
fn sign(parts: &Parts, host: &HostRef) -> Result<(), Failure> {
    if host.is_box {
        return Ok(());
    }
    parts.reach.access_keyed(parts.ctx, parts.ca_env, &host.key)
}

/// `Platform  <platform>`, for a host off `linux/arm64`, the label padded to 10 characters: part of
/// `status`'s report, so it prints whatever `-Quiet` says, and needs no AWS call. A `linux/arm64`
/// host, the box included, gets none, so its report is byte for byte as v0.11's (D91; pin P57 of
/// v0.12-T01).
fn report_platform(parts: &Parts, host: &HostRef) {
    if !host.key.is_linux_arm64() {
        parts
            .report
            .report(&format!("Platform  {}", host.key.platform));
    }
}

/// `down` (operator.md, down): stops the host, then, for a host that is not the box, writes its
/// alias and signs a fresh one-hour certificate for it. A failure there fails `down`, and the stop
/// already requested stands.
pub fn down(parts: &Parts, host: &HostRef) -> Result<(), Failure> {
    stop(parts, host)?;
    sign(parts, host)
}

/// The stop alone: asks EC2 to stop the instance, whatever state it is in, and prints
/// `Stopping <id>.` once the request is accepted. It does not wait. `down -All` runs it for each
/// host, and signs nothing.
fn stop(parts: &Parts, host: &HostRef) -> Result<(), Failure> {
    let id = instance_id(parts, host)?;
    parts.aws.call(&Call {
        service: "ec2",
        operation: "stop-instances",
        args: &["--instance-ids", &id],
        output: Output::Checked,
    })?;
    parts.say.progress(&format!("Stopping {id}."), None);
    Ok(())
}

const DESCRIBE_INSTANCES: &str = "ec2 describe-instances";
const DESCRIBE_VOLUMES: &str = "ec2 describe-volumes";
const DESCRIBE_INSTANCE_INFORMATION: &str = "ssm describe-instance-information";

const STATE_NAME: Member = Member {
    pointer: "/Reservations/0/Instances/0/State/Name",
    path: "Reservations[0].Instances[0].State.Name",
};
const INSTANCE_TYPE: Member = Member {
    pointer: "/Reservations/0/Instances/0/InstanceType",
    path: "Reservations[0].Instances[0].InstanceType",
};
const LAUNCH_TIME: Member = Member {
    pointer: "/Reservations/0/Instances/0/LaunchTime",
    path: "Reservations[0].Instances[0].LaunchTime",
};
const ROOT_DEVICE_NAME: Member = Member {
    pointer: "/Reservations/0/Instances/0/RootDeviceName",
    path: "Reservations[0].Instances[0].RootDeviceName",
};
/// No `BlockDeviceMappings` element with the `RootDeviceName`, or one with no `Ebs.VolumeId`
/// (operator.md, Calls by command, the text after FB-08's table; architect, 2026-11-19, for
/// E0.06-T03-S01).
const ROOT_VOLUME_ID: Member = Member {
    pointer: "",
    path: "Reservations[0].Instances[0].BlockDeviceMappings[].Ebs.VolumeId",
};
const VOLUME_SIZE: Member = Member {
    pointer: "/Volumes/0/Size",
    path: "Volumes[0].Size",
};
const PING_STATUS: Member = Member {
    pointer: "/InstanceInformationList/0/PingStatus",
    path: "InstanceInformationList[0].PingStatus",
};

/// A member of an answer: its JSON pointer, and its path as Calls by command writes it.
#[derive(Clone, Copy)]
struct Member {
    pointer: &'static str,
    path: &'static str,
}

/// `agentspace: Unexpected answer from <call>: no <member>.` (operator.md, Calls by command,
/// FB-08), which a caller sees as "Could not reach the host".
fn unexpected(call: &str, member: Member) -> Failure {
    Failure {
        message: format!(
            "agentspace: Unexpected answer from {call}: no {}.",
            member.path
        ),
        caller: CallerFailure::CouldNotReach,
    }
}

fn string_member<'v>(answer: &'v Value, call: &str, member: Member) -> Result<&'v str, Failure> {
    answer
        .pointer(member.pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| unexpected(call, member))
}

/// The instance's members `status` and `grow` read (operator.md, Calls by command, FB-08): each
/// reads only its own, and a member it does not read never fails it.
struct Described {
    state: String,
    instance_type: Option<String>,
    volume_id: String,
    /// The `LaunchTime` text, as the answer holds it: read by `status -All` for a host that gets a
    /// `Guard` line, and by no other command line (operator.md, status -All).
    launch_time: Option<String>,
}

/// One `ec2 describe-instances --instance-ids <id>` (JSON). The state, the root device's name and
/// the root volume's ID are read by both commands, and the type only when `with_type`, in that
/// order: the first member that is missing fails.
fn describe_instance(aws: &dyn Aws, id: &str, with_type: bool) -> Result<Described, Failure> {
    let answer = aws.call(&Call {
        service: "ec2",
        operation: "describe-instances",
        args: &["--instance-ids", id],
        output: Output::Json,
    })?;
    let state = string_member(&answer, DESCRIBE_INSTANCES, STATE_NAME)?.to_string();
    let instance_type = if with_type {
        Some(string_member(&answer, DESCRIBE_INSTANCES, INSTANCE_TYPE)?.to_string())
    } else {
        None
    };
    let root_device = string_member(&answer, DESCRIBE_INSTANCES, ROOT_DEVICE_NAME)?;
    let volume_id = answer
        .pointer("/Reservations/0/Instances/0/BlockDeviceMappings")
        .and_then(Value::as_array)
        .and_then(|mappings| {
            mappings
                .iter()
                .find(|m| m.get("DeviceName").and_then(Value::as_str) == Some(root_device))
        })
        .and_then(|m| m.pointer("/Ebs/VolumeId"))
        .and_then(Value::as_str)
        .ok_or_else(|| unexpected(DESCRIBE_INSTANCES, ROOT_VOLUME_ID))?
        .to_string();
    let launch_time = answer
        .pointer(LAUNCH_TIME.pointer)
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(Described {
        state,
        instance_type,
        volume_id,
        launch_time,
    })
}

/// One `ec2 describe-volumes --volume-ids <vol>` (JSON): `Volumes[0].Size`, in GiB.
fn volume_size(aws: &dyn Aws, volume_id: &str) -> Result<i64, Failure> {
    let answer = aws.call(&Call {
        service: "ec2",
        operation: "describe-volumes",
        args: &["--volume-ids", volume_id],
        output: Output::Json,
    })?;
    answer
        .pointer(VOLUME_SIZE.pointer)
        .and_then(Value::as_i64)
        .ok_or_else(|| unexpected(DESCRIBE_VOLUMES, VOLUME_SIZE))
}

/// Whether Session Manager reports the instance's agent `Online`, from one
/// `ssm describe-instance-information` (JSON). An empty list is not online.
fn ssm_online(aws: &dyn Aws, id: &str) -> Result<bool, Failure> {
    let filter = format!("Key=InstanceIds,Values={id}");
    let answer = aws.call(&Call {
        service: "ssm",
        operation: "describe-instance-information",
        args: &["--filters", &filter],
        output: Output::Json,
    })?;
    let list = answer
        .pointer("/InstanceInformationList")
        .and_then(Value::as_array)
        .ok_or_else(|| unexpected(DESCRIBE_INSTANCE_INFORMATION, PING_STATUS))?;
    if list.is_empty() {
        return Ok(false);
    }
    Ok(string_member(&answer, DESCRIBE_INSTANCE_INFORMATION, PING_STATUS)? == "Online")
}

/// `status` (operator.md, status): five lines about the host's instance, to stderr, whatever
/// `-Quiet` says. It never starts the host, and prints nothing when a call fails. **For a host that
/// is not the box it then writes the host's alias and signs a fresh one-hour certificate**, and a
/// failure there fails `status` after its report.
pub fn status(parts: &Parts, host: &HostRef) -> Result<(), Failure> {
    // A host off `linux/arm64` gets `Platform  <platform>` first, before the five, as `status -All`
    // prints it (pin P57 of v0.12-T01).
    report_platform(parts, host);
    // No lease state file is read, and no `LaunchTime`: the five lines alone, as before v0.07.
    report_status(parts, host, None)?;
    sign(parts, host)
}

/// The five lines, printed only once every call they need has succeeded, and, for `status -All`
/// when its lease state file was read (`leases`), the `Leases` line and the `Guard` line after
/// them (`status::LeaseView::lines`). `status -All` runs it for each host, and signs nothing.
///
/// A running host that no live lease explains needs its `LaunchTime`, from the same answer: one
/// that is missing, or not of its form, fails the host's part before its five lines.
fn report_status(parts: &Parts, host: &HostRef, leases: Option<&LeaseView>) -> Result<(), Failure> {
    let id = instance_id(parts, host)?;
    let instance = describe_instance(parts.aws, &id, true)?;
    let size = volume_size(parts.aws, &instance.volume_id)?;
    let ssm = if instance.state == "running" {
        if ssm_online(parts.aws, &id)? {
            "online"
        } else {
            "not yet"
        }
    } else {
        "-"
    };
    // The `LaunchTime` is read only for a host that gets a `Guard` line, and fails its part before
    // any line prints (operator.md, status -All).
    let launch_time = match leases {
        Some(view) if view.needs_launch_time(&host.key, &instance.state) => Some(
            instance
                .launch_time
                .as_deref()
                .and_then(status::parse_launch_time)
                .ok_or_else(|| unexpected(DESCRIBE_INSTANCES, LAUNCH_TIME))?,
        ),
        _ => None,
    };
    let instance_type = instance.instance_type.unwrap_or_default();
    // `gp3` is a constant (operator.md, status; Q16).
    for line in [
        format!("Instance  {id}"),
        format!("State     {}", instance.state),
        format!("Type      {instance_type}"),
        format!("Disk      {size} GiB gp3 ({})", instance.volume_id),
        format!("SSM       {ssm}"),
    ] {
        parts.report.report(&line);
    }
    if let Some(view) = leases {
        for line in view.lines(&host.key, &instance.state, launch_time, &|unix| {
            parts.reach.clock_time(unix)
        }) {
            parts.report.report(&line);
        }
    }
    Ok(())
}

/// How long `grow` waits before the online grow's ssh (operator.md, grow, step 3).
const GROW_WAIT: Duration = Duration::from_secs(10);

/// What the box runs to grow its root filesystem online, as root and never prompting: the
/// partition to the end of the disk (a failure, such as a partition with nothing to grow, does not
/// stop the next step), then the filesystem to fill the partition, and, only if that succeeded,
/// the root filesystem's size and use in units a person reads. It exits non-zero when the
/// filesystem's grow or the table failed, and 0 otherwise.
///
/// The root filesystem's device is found, so the command does not name today's NVMe device. It
/// installs nothing, holds no `%` or double quote and no control character, and does not end in a
/// backslash (gap FA-15), so it reaches ssh whole.
const GROW_REMOTE_COMMAND: &str = "sudo -n sh -c 'dev=$(findmnt -n -o SOURCE /); \
disk=/dev/$(lsblk -n -o PKNAME $dev); part=$(cat /sys/class/block/${dev##*/}/partition); \
growpart $disk $part; resize2fs $dev && df -h /'";

/// `grow`'s remote step for the host: the platform's online grow (operator.md, The platform seam,
/// the online grow row), in its remote command form.
// Phase A of E0.12-T03-S02: both Linux platforms keep today's `GROW_REMOTE_COMMAND`, byte for byte;
// any other platform takes its rows from its seam, `grow_script` wrapped by `remote_command`, and
// a row's failure fails `grow` with the row's message, after the volume's change, which stands.
// Phase B takes every platform's from its seam and deletes `GROW_REMOTE_COMMAND`.
fn grow_remote_command(parts: &Parts, host: &HostRef) -> Result<String, Failure> {
    if on_linux(&host.key) {
        return Ok(GROW_REMOTE_COMMAND.to_string());
    }
    let seam = parts.reach.seam(&host.key.platform).ok_or_else(not_built)?;
    seam.remote_command(&seam.grow_script()?)
}

/// `grow` (operator.md, grow): grows the host's root volume by `gib` GiB, and the root filesystem
/// with it, online when the instance is running.
///
/// For a host that is not the box, its key, certificate and alias are made after step 2, whether or
/// not the host is running, and before step 3's ssh, which goes through the host's alias. A failure
/// there fails `grow`, and the volume's change stands.
pub fn grow(parts: &Parts, host: &HostRef, gib: i32) -> Result<(), Failure> {
    let id = instance_id(parts, host)?;
    let instance = describe_instance(parts.aws, &id, false)?;
    let size = volume_size(parts.aws, &instance.volume_id)?;
    let new = size + i64::from(gib);
    let volume = &instance.volume_id;
    parts.say.progress(
        &format!(
            "Growing {volume} from {size} to {new} GiB (EBS allows one change per volume every 6 hours)..."
        ),
        None,
    );
    let new_text = new.to_string();
    parts.aws.call(&Call {
        service: "ec2",
        operation: "modify-volume",
        args: &["--volume-id", volume, "--size", &new_text],
        output: Output::Checked,
    })?;
    sign(parts, host)?;
    if instance.state == "running" {
        parts.say.progress("Growing the filesystem online...", None);
        parts.clock.sleep(GROW_WAIT);
        let command = grow_remote_command(parts, host)?;
        let exit = parts.remote.ssh(
            &["-o", "ClearAllForwardings=yes", &host.alias],
            Some(&command),
        )?;
        if exit != 0 {
            return Err(Failure {
                message: format!(
                    "agentspace: Growing the filesystem online failed; the volume is already {new} GiB, and the filesystem grows on its own at next boot."
                ),
                caller: CallerFailure::CouldNotReach,
            });
        }
    } else {
        parts
            .say
            .progress("The filesystem grows on its own at next boot.", None);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
