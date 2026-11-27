//! The operator's `sync` and `deploy` (docs/operator.md, sync and deploy; D37, D40, D42).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! # Public API
//!
//! - [`sync`] and [`deploy`], the pinned entry points (D42; the `CaEnv` parameter was added to the
//!   pin by the architect on 2026-11-19, and the [`Target`] parameter on 2026-11-20, before
//!   v0.07-T03, D55), and [`bootstrap`] and [`provision`], stubs pinned the same day for
//!   E0.07-T03-S03 to build. Their signatures are the pin, and a change to them is the
//!   architect's.
//! - [`sync_keyed`], [`deploy_keyed`], [`bootstrap_keyed`] and [`provision_keyed`]: the same four
//!   commands for a host named by its [`HostKey`] (D101; the dev lead's pin P2 for E0.12-T03-S03).
//!   A `linux/arm64` key is today's command, the box's key [`Target::Box`] and any other
//!   project's `Target::Host`; every other platform answers `agentspace_platforms::NOT_BUILT`
//!   until E0.12-T03-S03's second part builds it. Each has a hidden `<name>_using` twin over
//!   `Seams`.
//! - `sync_using` and `deploy_using`, the same over `Seams`, and `Seams::real_except_go`, which
//!   gives them the operator's own clock, CA check and push with a `go` finder and a localswim
//!   folder of the test's: hidden from the docs, for tests alone, since `go`, the localswim
//!   checkout, the clock, the CA check and the push have no variable in the test seam
//!   (operator.md, The test seam, "Not covered"). The operator calls the pinned pair.
//!
//! # Rules
//!
//! - **`sync`'s order decides what reaches the box, and MUST be operator.md's steps 1 to 8**: the
//!   CA check before the box is touched, the start-and-wait, then the nine calls of its table in
//!   that order, each with its step's message, the first failure ending the command with nothing
//!   after it run. **Between rows 4 and 5 it waits for the host's patch run** (`patch_wait.rs`;
//!   The patch wait): `patched` goes on, `failed` warns and goes on, and the 30-minute deadline
//!   fails the command.
//! - **The template and `box\system` are the repository's own, embedded at build time byte for
//!   byte** (D37's addendum), and written out under `%USERPROFILE%\.agentspace\operator` just
//!   before the call that reads them. What is sent to the box at run time, every remote command,
//!   is this crate's own, written from operator.md's steps (D34).
//! - **Every program this crate starts gets its environment through `agentspace-run`'s
//!   `prepare_child_env`**: the AWS CLI, ssh and scp through the pinned [`Aws`] and [`Remote`],
//!   `go` through this crate's build, and the push's ssh through `agentspace-push`.
//! - **`sync` and `bootstrap` reach a host that is not the box over its own alias**,
//!   `agentspace-host.<project>`, written with the operator's key and a fresh one-hour certificate
//!   (`host.rs`), and reach the box over `agentspace`, as in v0.06. **`sync_all`** is `sync -All`
//!   (`syncing.rs`). Each command writes what it sends into a folder of its own under the
//!   operator's, removed as the command ends (`folder.rs`).
//! - **Every `deploy` that is not a provisioning run's own deploy step MUST take its host's
//!   `provision.lock` without waiting and hold it for its whole life, `sync` included, and MUST
//!   refuse before any AWS call while another holds it** (D67's change note; pin P34). It is the
//!   box's `hosts\swimcoach\provision.lock` for the box. A run's deploy step takes nothing more.
//! - **The library reads no environment variable but `PATH`, only to find `go`,
//!   `AGENTSPACE_SSH_DIR`, only to find the ssh folder, and, in [`provision`] alone,
//!   `AGENTSPACE_STATE_DIR`, `AGENTSPACE_SSH_DIR` and `AGENTSPACE_PROXY_COMMAND` for its last
//!   step's driver context**, and takes every other path from the [`Context`]. It writes to stdout and stderr only through [`Say`] and the streams the spec
//!   passes through (`sync -All`'s `Host` lines and its failed hosts' messages, and the passed-through
//!   output of the programs it starts), and never panics on bad input.
//! - **A test MUST NOT reach the real `go` or `C:\Projects\localswim`** (operator.md, The test
//!   seam, "Not covered"): a test that could reach step 7 MUST go through `sync_using` or
//!   `deploy_using` with a `go` finder and a localswim folder of its own, and a test MUST NOT call
//!   [`sync`] or [`deploy`] in a process whose `PATH` could hold a `go`.

#![forbid(unsafe_code)]

mod bootstrap;
mod deploy;
mod embedded;
mod files;
mod folder;
mod go;
mod host;
mod params;
mod patch_wait;
mod provision;
mod registry;
mod steps;
mod syncing;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use agentspace_box::{Clock, SystemClock};
use agentspace_ca::CaEnv;
use agentspace_operator_api::{Aws, CallerFailure, Context, Failure, Remote, Say};

use files::Places;
use folder::CommandFolder;

/// The host a command acts on (D49, D55), re-exported so the operator names it through this crate.
pub use agentspace_hosts::Target;

use agentspace_hosts::HostKey;

/// The ssh host alias every connection of `sync` goes through (operator.md, The `agentspace` SSH
/// entry).
const ALIAS: &str = "agentspace";

/// A forced push: the context, where to speak, the instance ID, the alias to push over and the
/// host's project (`None` for the box).
#[doc(hidden)]
pub type PushFn<'a> = dyn Fn(&Context, &dyn Say, &str, &str, Option<&str>) + 'a;

/// What `sync` and `deploy` use that the test seam does not cover. The pinned entry points use
/// the real ones.
#[doc(hidden)]
pub struct Seams<'a> {
    /// The clock of the start-and-wait step.
    pub clock: &'a dyn Clock,
    /// `sync`'s step 1: `agentspace-ca`'s `check_for_sync`, returning the CA's fingerprint.
    pub ca_check: &'a dyn Fn(&Context, &CaEnv) -> Result<String, Failure>,
    /// `go` on `PATH`, or `None`.
    pub find_go: &'a dyn Fn() -> Option<PathBuf>,
    /// The localswim checkout, `C:\Projects\localswim`.
    pub localswim_checkout: PathBuf,
    /// `sync`'s step 8: the forced push over a host's alias, for an instance ID, an alias and the
    /// host's project (`None` for the box, whose stamp is v0.06's).
    pub push: &'a PushFn<'a>,
    /// The command's own folder under the operator's, `operator\<hex>\`, made on first use and
    /// removed as the command ends (operator.md, The operator's folder, one per command). Every
    /// step of one command shares it.
    pub folder: CommandFolder,
    /// The ssh folder `AGENTSPACE_SSH_DIR` names, or `None` for `%USERPROFILE%\.ssh` (state.md,
    /// What moves which files).
    pub ssh_dir: Option<PathBuf>,
    /// The folder of the running operator, where `agentspace-ssh-proxy.exe` is looked for
    /// (operator.md, How each program finds the others).
    pub operator_dir: Option<PathBuf>,
}

fn real_find_go() -> Option<PathBuf> {
    go::find_on_path(std::env::var_os("PATH"))
}

fn real_push(ctx: &Context, say: &dyn Say, instance_id: &str, alias: &str, host: Option<&str>) {
    agentspace_push::push_to_host(ctx, say, instance_id, alias, true, host);
}

/// `AGENTSPACE_SSH_DIR`, made full, when it is set and not empty.
fn real_ssh_dir() -> Option<PathBuf> {
    let value = std::env::var_os("AGENTSPACE_SSH_DIR").filter(|v| !v.is_empty())?;
    let path = PathBuf::from(value);
    Some(std::path::absolute(&path).unwrap_or(path))
}

/// The folder of the executable the operating system started, as a full path.
fn real_operator_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.parent().map(Path::to_path_buf)
}

/// Runs `run` with what the operator uses: a system clock whose origin is now, the real CA check,
/// `go` on the process's `PATH`, `C:\Projects\localswim`, and `agentspace-push`'s push.
fn with_real_seams<T>(run: impl FnOnce(&Seams) -> T) -> T {
    Seams::real_except_go(&real_find_go, PathBuf::from(go::LOCALSWIM_CHECKOUT), run)
}

impl Seams<'_> {
    /// Runs `run` with the operator's own clock, CA check and forced push, and with the `go`
    /// finder and the localswim folder it is given. For tests alone: a test MUST NOT reach the
    /// real `go` or `C:\Projects\localswim` (operator.md, The test seam, "Not covered"), and this
    /// leaves a test everything of the pinned [`sync`] and [`deploy`] but those two.
    #[doc(hidden)]
    pub fn real_except_go<T>(
        find_go: &dyn Fn() -> Option<PathBuf>,
        localswim_checkout: PathBuf,
        run: impl FnOnce(&Seams) -> T,
    ) -> T {
        let clock = SystemClock::new();
        Seams::real_except_go_and_clock(find_go, localswim_checkout, &clock, run)
    }

    /// [`Seams::real_except_go`], with the clock it is given in place of the operator's own: the
    /// clock of the start-and-wait step and of `sync`'s patch wait, whose 5-second pauses, 1-second
    /// polls and 30-minute deadline a test can then pass without waiting. For tests alone.
    #[doc(hidden)]
    pub fn real_except_go_and_clock<T>(
        find_go: &dyn Fn() -> Option<PathBuf>,
        localswim_checkout: PathBuf,
        clock: &dyn Clock,
        run: impl FnOnce(&Seams) -> T,
    ) -> T {
        let seams = Seams {
            clock,
            ca_check: &agentspace_ca::check_for_sync,
            find_go,
            localswim_checkout,
            push: &real_push,
            folder: CommandFolder::new(),
            ssh_dir: real_ssh_dir(),
            operator_dir: real_operator_dir(),
        };
        let result = run(&seams);
        // The command's folder goes as the command ends, whether it succeeded or failed.
        seams.folder.remove();
        result
    }
}

/// `operator sync` (docs/operator.md, sync): checks the CA, starts the box and waits for it,
/// installs `box\system` and the CA's public key, copies `CLAUDE.md`, builds and installs the
/// localswim CLI when the laptop can, and pushes the token, forced.
///
/// `target` names the host (D49). The pin before v0.07-T03 (architect, 2026-11-20, D55) added it:
/// [`Target::Box`] is today's behavior, over the `agentspace` alias. A host is reached over its own
/// alias, `agentspace-host.<project>`, which `sync` writes with the operator's key and a fresh
/// certificate, and its push uses that host's own stamp (operator.md, sync).
pub fn sync(
    ctx: &Context,
    ca_env: &CaEnv,
    target: Target<'_>,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    with_real_seams(|seams| sync_target_using(ctx, ca_env, target, say, aws, remote, seams))
}

/// `operator sync -All` (docs/operator.md, Every host at once): `sync` for every `ready` host in the
/// registry, one at a time, in the file's order. It writes `box\system` and builds the localswim CLI
/// once, prints `Host      <project> (<stack>), <state>` to stderr before each host, goes on past a
/// host that failed, and ends with `agentspace: <n> of <m> hosts failed.` when any did; #0034's
/// sign-in message ends it at once.
pub fn sync_all(
    ctx: &Context,
    ca_env: &CaEnv,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    with_real_seams(|seams| sync_all_using(ctx, ca_env, say, aws, remote, seams))
}

/// [`sync_all`] over [`Seams`].
#[doc(hidden)]
pub fn sync_all_using(
    ctx: &Context,
    ca_env: &CaEnv,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    syncing::sync_all(ctx, ca_env, say, aws, remote, seams)
}

/// [`sync`] for `target` over [`Seams`]. [`Target::Box`] behaves as [`sync_using`].
#[doc(hidden)]
pub fn sync_target_using(
    ctx: &Context,
    ca_env: &CaEnv,
    target: Target<'_>,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    syncing::sync_target(ctx, ca_env, target, say, aws, remote, seams).map(|_| ())
}

/// `operator deploy` (docs/operator.md, deploy): creates or updates the host's stack, refreshes the
/// instance ID in the host's record, then runs `sync` with the same [`Say`], so `deploy -Quiet`
/// quiets it too.
///
/// `target` as for [`sync`] (the pin before v0.07-T03, D55). Without `-InstanceType`; see
/// [`deploy_with_instance_type`].
pub fn deploy(
    ctx: &Context,
    ca_env: &CaEnv,
    target: Target<'_>,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    deploy_with_instance_type(ctx, ca_env, target, None, say, aws, remote)
}

/// `operator deploy [-Host <project>] [-InstanceType <type>]` (docs/operator.md, deploy):
/// [`deploy`], with `instance_type` the value of `-InstanceType`, which every host takes, the box
/// included (D67, D71; architect, 2026-11-23, for v0.09-T01, pin P5, which deleted v0.07's refusal
/// for the box). Every `deploy` takes its host's `provision.lock` without waiting and holds it for
/// its life, and refuses before any AWS call while another holds it (pin P34). Public API
/// confirmed by the architect, 2026-11-21, for v0.07-T03, since the pinned [`deploy`] has no way to
/// pass `-InstanceType`.
pub fn deploy_with_instance_type(
    ctx: &Context,
    ca_env: &CaEnv,
    target: Target<'_>,
    instance_type: Option<&str>,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    with_real_seams(|seams| {
        deploy_target_sized_using(ctx, ca_env, target, instance_type, say, aws, remote, seams)
    })
}

/// [`deploy`] for `target` over [`Seams`]. [`Target::Box`] behaves as [`deploy_using`].
#[doc(hidden)]
pub fn deploy_target_using(
    ctx: &Context,
    ca_env: &CaEnv,
    target: Target<'_>,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    deploy_target_sized_using(ctx, ca_env, target, None, say, aws, remote, seams)
}

/// [`deploy_with_instance_type`] over [`Seams`].
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn deploy_target_sized_using(
    ctx: &Context,
    ca_env: &CaEnv,
    target: Target<'_>,
    instance_type: Option<&str>,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    // The host is looked up first (a registry that is not valid, or a project with no record, is
    // refused before anything), then its `provision.lock` is tried without waiting and held for the
    // whole life of this `deploy`, its `sync` step included (operator.md, deploy; D67's change
    // note; pin P34). `-Host swimcoach` is the box in every respect (operator.md, From the command
    // line), so the box's lock is `hosts\swimcoach\provision.lock` either way.
    let home = deploy::full_home(ctx);
    let hosts = deploy::read_hosts(&home)?;
    let record = registry::find(&hosts, target)?;
    let is_box = registry::is_box_record(&record);
    let _held = provision::take_person_lock(&home, &record.project)?;
    if is_box {
        // Steps 1 to 5, then step 6: sync, as if run with deploy's own -Quiet; its failure is
        // deploy's.
        deploy::box_stack(ctx, instance_type, say, aws, seams)?;
        sync_target_using(ctx, ca_env, Target::Box, say, aws, remote, seams)
    } else {
        deploy::host_stack(ctx, ca_env, &record.project, instance_type, say, aws, seams)?;
        sync_target_using(ctx, ca_env, target, say, aws, remote, seams)
    }
}

/// `operator bootstrap` (docs/operator.md, bootstrap): runs the embedded toolchain bootstrap on
/// the target's host (D46), over the host's alias, as the login user. The script is
/// `box\bootstrap.sh`, embedded and sent on an ssh's stdin.
pub fn bootstrap(
    ctx: &Context,
    ca_env: &CaEnv,
    target: Target<'_>,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    with_real_seams(|seams| bootstrap_using(ctx, ca_env, target, say, aws, remote, seams))
}

/// [`bootstrap`] over [`Seams`].
// The bootstrap's ssh is started inside this crate, since the pinned `Remote` has no stdin: found by
// `agentspace_run::ssh_found`, given `prepare_child_env`'s environment, and started through
// `agentspace_run::start_guarded` (the architect's ruling P7 for E0.10-T03-S03; bootstrap.rs).
#[doc(hidden)]
pub fn bootstrap_using(
    ctx: &Context,
    ca_env: &CaEnv,
    target: Target<'_>,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    bootstrap::run(ctx, ca_env, target, say, aws, remote, seams)
}

/// `operator provision -Host <project>` (docs/operator.md, provision): the provisioning run
/// (D46, D48, D55). Deploys the host's stack, starts it and waits, runs `sync` and `bootstrap`,
/// records it `ready`, and places the project's pending leases, under the host's run lock and
/// logging to its `provision.log`. `project` is normalized, as the operator's `-Host` gives it.
///
/// Step 9's driver context reads `AGENTSPACE_STATE_DIR`, `AGENTSPACE_SSH_DIR` and
/// `AGENTSPACE_PROXY_COMMAND` from the process's environment (operator.md, Environment variables),
/// as its detached start gave them, and carries the two folders' variables, made full when set
/// and not empty, as its `state_dir_env` and `ssh_dir_env` (driver.md, The detached start, C7).
pub fn provision(
    ctx: &Context,
    ca_env: &CaEnv,
    project: &str,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    let env = |name: &str| std::env::var_os(name);
    with_real_seams(|seams| provision::run(ctx, ca_env, project, say, aws, remote, seams, &env))
}

/// [`provision`] over [`Seams`]. Step 9's driver context is built from the context's home alone:
/// a test never reaches the process's `AGENTSPACE_STATE_DIR`, `AGENTSPACE_SSH_DIR` or
/// `AGENTSPACE_PROXY_COMMAND`, so its leases are those of its throwaway home.
// Architect's to confirm: the hidden entry reads none of the three variables, since a test MUST
// NOT change its own environment (#0137) and so cannot set them for an in-process run.
#[doc(hidden)]
pub fn provision_using(
    ctx: &Context,
    ca_env: &CaEnv,
    project: &str,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    let none = |_: &str| None;
    provision::run(ctx, ca_env, project, say, aws, remote, seams, &none)
}

/// The answer of a `_keyed` entry point for a key whose platform is not built yet: a host that is
/// not `linux/arm64` (E0.12-T03-S03, part 1).
fn not_built() -> Failure {
    Failure {
        message: agentspace_platforms::NOT_BUILT.to_string(),
        caller: CallerFailure::NotAvailable,
    }
}

/// The [`Target`] of a `linux/arm64` key: the box's key is [`Target::Box`], and any other
/// project's is `Target::Host(<project>)`. `None` for a key on any other platform.
fn linux_arm64_target(key: &HostKey) -> Option<Target<'_>> {
    if key.is_box() {
        Some(Target::Box)
    } else if key.is_linux_arm64() {
        Some(Target::Host(&key.project))
    } else {
        None
    }
}

/// [`sync`] for the host `key` names (D101). A `linux/arm64` key is [`sync`] of its [`Target`];
/// a key on any other platform answers `agentspace_platforms::NOT_BUILT` with
/// [`CallerFailure::NotAvailable`], before anything is read or run.
pub fn sync_keyed(
    ctx: &Context,
    ca_env: &CaEnv,
    key: &HostKey,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    let target = linux_arm64_target(key).ok_or_else(not_built)?;
    // [`sync`]'s own body, since the pinned pair is called by the integration rig alone (tests.rs,
    // `no_test_calls_the_pinned_pair_but_the_rigs_guarded_two`).
    with_real_seams(|seams| sync_target_using(ctx, ca_env, target, say, aws, remote, seams))
}

/// [`sync_keyed`] over [`Seams`].
#[doc(hidden)]
pub fn sync_keyed_using(
    ctx: &Context,
    ca_env: &CaEnv,
    key: &HostKey,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    let target = linux_arm64_target(key).ok_or_else(not_built)?;
    sync_target_using(ctx, ca_env, target, say, aws, remote, seams)
}

/// [`deploy_with_instance_type`] for the host `key` names (D101), as [`sync_keyed`] picks it.
pub fn deploy_keyed(
    ctx: &Context,
    ca_env: &CaEnv,
    key: &HostKey,
    instance_type: Option<&str>,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    let target = linux_arm64_target(key).ok_or_else(not_built)?;
    deploy_with_instance_type(ctx, ca_env, target, instance_type, say, aws, remote)
}

/// [`deploy_keyed`] over [`Seams`].
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn deploy_keyed_using(
    ctx: &Context,
    ca_env: &CaEnv,
    key: &HostKey,
    instance_type: Option<&str>,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    let target = linux_arm64_target(key).ok_or_else(not_built)?;
    deploy_target_sized_using(ctx, ca_env, target, instance_type, say, aws, remote, seams)
}

/// [`bootstrap`] for the host `key` names (D101), as [`sync_keyed`] picks it.
pub fn bootstrap_keyed(
    ctx: &Context,
    ca_env: &CaEnv,
    key: &HostKey,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    let target = linux_arm64_target(key).ok_or_else(not_built)?;
    bootstrap(ctx, ca_env, target, say, aws, remote)
}

/// [`bootstrap_keyed`] over [`Seams`].
#[doc(hidden)]
pub fn bootstrap_keyed_using(
    ctx: &Context,
    ca_env: &CaEnv,
    key: &HostKey,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    let target = linux_arm64_target(key).ok_or_else(not_built)?;
    bootstrap_using(ctx, ca_env, target, say, aws, remote, seams)
}

/// [`provision`] for the host `key` names (D101): a `linux/arm64` key is [`provision`] of its
/// project, and a key on any other platform answers `agentspace_platforms::NOT_BUILT` with
/// [`CallerFailure::NotAvailable`], before anything is read or run.
pub fn provision_keyed(
    ctx: &Context,
    ca_env: &CaEnv,
    key: &HostKey,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    if !key.is_linux_arm64() {
        return Err(not_built());
    }
    provision(ctx, ca_env, &key.project, say, aws, remote)
}

/// [`provision_keyed`] over [`Seams`].
#[doc(hidden)]
pub fn provision_keyed_using(
    ctx: &Context,
    ca_env: &CaEnv,
    key: &HostKey,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    if !key.is_linux_arm64() {
        return Err(not_built());
    }
    provision_using(ctx, ca_env, &key.project, say, aws, remote, seams)
}

/// A failure of one of `sync`'s or `deploy`'s own steps, with its exact message.
fn step_failed(message: &str) -> Failure {
    Failure {
        message: message.to_string(),
        caller: CallerFailure::CouldNotReach,
    }
}

/// One ssh through `alias`, with forwards cleared, and its remote command as the last argument;
/// `Ok(true)` when it exited 0.
fn ssh_to(remote: &dyn Remote, alias: &str, remote_command: &str) -> Result<bool, Failure> {
    let code = remote.ssh(&["-o", steps::CLEAR_FORWARDS, alias], Some(remote_command))?;
    Ok(code == 0)
}

/// One scp with forwards cleared and `-q`, then `rest`; `Ok(true)` when it exited 0.
fn scp(remote: &dyn Remote, rest: &[&str]) -> Result<bool, Failure> {
    let mut args = vec!["-o", steps::CLEAR_FORWARDS, "-q"];
    args.extend_from_slice(rest);
    Ok(remote.scp(&args)? == 0)
}

/// Fails with `message` unless `ok`.
fn check(ok: bool, message: &str) -> Result<(), Failure> {
    if ok {
        Ok(())
    } else {
        Err(step_failed(message))
    }
}

fn text(path: &Path) -> String {
    path.display().to_string()
}

/// [`sync`] over [`Seams`] for the box, over the `agentspace` alias, as in v0.06.
#[doc(hidden)]
pub fn sync_using(
    ctx: &Context,
    ca_env: &CaEnv,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    sync_target_using(ctx, ca_env, Target::Box, say, aws, remote, seams)
}

/// Reads `%USERPROFILE%\.ssh\id_ed25519.pub`, trimmed (operator.md, deploy, step 1). A missing
/// file fails with `agentspace: <path> does not exist.`, one that cannot be opened or is not UTF-8
/// with `agentspace: Cannot read <path>.`, and an empty one with `agentspace: <path> is empty.`
/// A file of whitespace alone is empty, since the key is read trimmed.
fn read_public_key(places: &Places) -> Result<String, Failure> {
    let path = places.user_public_key();
    let cannot_read = || step_failed(&format!("agentspace: Cannot read {}.", path.display()));
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(step_failed(&format!(
                "agentspace: {} does not exist.",
                path.display()
            )));
        }
        Err(_) => return Err(cannot_read()),
    };
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    let key = std::str::from_utf8(bytes)
        .map_err(|_| cannot_read())?
        .trim();
    if key.is_empty() {
        return Err(step_failed(&format!(
            "agentspace: {} is empty.",
            path.display()
        )));
    }
    Ok(key.to_string())
}

/// [`deploy`] over [`Seams`], for the box: its steps 1 to 5 as before v0.07, but for the template's
/// folder and the instance ID and type kept in the box's record (operator.md, deploy), then
/// `sync`, all under the box's `provision.lock` (pin P34).
#[doc(hidden)]
pub fn deploy_using(
    ctx: &Context,
    ca_env: &CaEnv,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    deploy_target_sized_using(ctx, ca_env, Target::Box, None, say, aws, remote, seams)
}

#[cfg(test)]
mod keyed_tests {
    //! The `_keyed` entry points of E0.12-T03-S03's part 1: a `linux/arm64` key is today's
    //! command for its target, and a key on any other platform is not built yet.

    use std::cell::RefCell;

    use agentspace_box::SystemClock;
    use agentspace_operator_api::{AwsError, Call, CallerLine, Mode};
    use serde_json::Value;

    use super::*;
    use crate::tests::{Scratch, ctx_in};

    /// An AWS CLI whose every call fails, recording each. Since nothing can succeed, no run here
    /// can reach `sync`'s localswim step or `provision`'s step 9.
    #[derive(Default)]
    struct FailingAws(RefCell<Vec<String>>);

    impl Aws for FailingAws {
        fn call(&self, call: &Call) -> Result<Value, AwsError> {
            let mut line = vec![call.service, call.operation];
            line.extend(call.args);
            self.0.borrow_mut().push(line.join(" "));
            Err(AwsError::Failed("refused by the test".to_string()))
        }
    }

    /// An ssh and scp whose every start exits 255, recording each.
    #[derive(Default)]
    struct FailingRemote(RefCell<Vec<String>>);

    impl Remote for FailingRemote {
        fn ssh(&self, options: &[&str], remote_command: Option<&str>) -> Result<i32, Failure> {
            self.0
                .borrow_mut()
                .push(format!("ssh {} | {remote_command:?}", options.join(" ")));
            Ok(255)
        }
        fn scp(&self, args: &[&str]) -> Result<i32, Failure> {
            self.0.borrow_mut().push(format!("scp {}", args.join(" ")));
            Ok(255)
        }
    }

    /// Records every line.
    #[derive(Default)]
    struct Lines(RefCell<Vec<String>>);

    impl Say for Lines {
        fn mode(&self) -> Mode {
            Mode::Person
        }
        fn progress(&self, person: &str, _caller: Option<CallerLine>) {
            self.0.borrow_mut().push(format!("progress: {person}"));
        }
        fn warning(&self, person: &str) {
            self.0.borrow_mut().push(format!("warning: {person}"));
        }
        fn detail(&self, text: &str) {
            self.0.borrow_mut().push(format!("detail: {text}"));
        }
    }

    /// What one run did: its failure's message and caller, its AWS calls, its ssh and scp, and
    /// its lines.
    type Outcome = (
        Option<(String, CallerFailure)>,
        Vec<String>,
        Vec<String>,
        Vec<String>,
    );

    /// Runs `run` in a fresh throwaway home at `dir`, with seams that find no `go`, and returns
    /// what it did. The home is emptied first, so two runs in the same `dir` start alike.
    fn outcome(
        dir: &Path,
        run: impl FnOnce(&Context, &dyn Say, &dyn Aws, &dyn Remote, &Seams) -> Result<(), Failure>,
    ) -> Outcome {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).unwrap();
        let ctx = ctx_in(dir);
        let (say, aws, remote) = (
            Lines::default(),
            FailingAws::default(),
            FailingRemote::default(),
        );
        let clock = SystemClock::new();
        let result =
            Seams::real_except_go_and_clock(&|| None, dir.join("no-localswim"), &clock, |seams| {
                run(&ctx, &say, &aws, &remote, seams)
            });
        (
            result.err().map(|f| (f.message, f.caller)),
            aws.0.take(),
            remote.0.take(),
            say.0.take(),
        )
    }

    fn not_built_failure() -> Option<(String, CallerFailure)> {
        Some((
            agentspace_platforms::NOT_BUILT.to_string(),
            CallerFailure::NotAvailable,
        ))
    }

    #[test]
    fn a_linux_arm64_key_names_todays_target() {
        assert_eq!(
            linux_arm64_target(&HostKey::linux_arm64("swimcoach")),
            Some(Target::Box)
        );
        assert_eq!(
            linux_arm64_target(&HostKey::linux_arm64("demo")),
            Some(Target::Host("demo"))
        );
        for platform in ["linux/amd64", "windows/amd64"] {
            assert_eq!(
                linux_arm64_target(&HostKey::new("swimcoach", platform)),
                None
            );
            assert_eq!(linux_arm64_target(&HostKey::new("demo", platform)), None);
        }
    }

    /// Each `_keyed` twin of a `linux/arm64` key does what today's function does for its target,
    /// and the box's key and another project's do different things, so the comparison tells them
    /// apart.
    #[test]
    fn a_linux_arm64_key_runs_todays_command_for_its_target() {
        let scratch = Scratch::new("keyed-today");
        let dir = scratch.path().join("home-run");
        let ca = CaEnv::default();
        let keys = [
            (HostKey::linux_arm64("swimcoach"), Target::Box),
            (HostKey::linux_arm64("demo"), Target::Host("demo")),
        ];
        for (key, target) in &keys {
            let target = *target;
            assert_eq!(
                outcome(&dir, |c, s, a, r, seams| {
                    sync_keyed_using(c, &ca, key, s, a, r, seams)
                }),
                outcome(&dir, |c, s, a, r, seams| {
                    sync_target_using(c, &ca, target, s, a, r, seams)
                }),
                "sync {target:?}"
            );
            assert_eq!(
                outcome(&dir, |c, s, a, r, seams| {
                    deploy_keyed_using(c, &ca, key, Some("m9g.large"), s, a, r, seams)
                }),
                outcome(&dir, |c, s, a, r, seams| {
                    deploy_target_sized_using(c, &ca, target, Some("m9g.large"), s, a, r, seams)
                }),
                "deploy {target:?}"
            );
            assert_eq!(
                outcome(&dir, |c, s, a, r, seams| {
                    bootstrap_keyed_using(c, &ca, key, s, a, r, seams)
                }),
                outcome(&dir, |c, s, a, r, seams| {
                    bootstrap_using(c, &ca, target, s, a, r, seams)
                }),
                "bootstrap {target:?}"
            );
        }
        let demo = HostKey::linux_arm64("demo");
        let today = outcome(&dir, |c, s, a, r, seams| {
            provision_using(c, &ca, "demo", s, a, r, seams)
        });
        let keyed = outcome(&dir, |c, s, a, r, seams| {
            provision_keyed_using(c, &ca, &demo, s, a, r, seams)
        });
        assert_eq!(keyed.0, today.0, "provision's failure");
        assert_eq!(keyed.1, today.1, "provision's AWS calls");
        assert!(keyed.0.is_some(), "provision with no registry fails");
        assert_ne!(keyed.0, not_built_failure());

        // The box and another project differ, so each comparison above names its own target.
        let the_box = outcome(&dir, |c, s, a, r, seams| {
            sync_target_using(c, &ca, Target::Box, s, a, r, seams)
        });
        let other = outcome(&dir, |c, s, a, r, seams| {
            sync_target_using(c, &ca, Target::Host("demo"), s, a, r, seams)
        });
        assert_ne!(the_box, other);
    }

    /// Every `_keyed` entry point, public and hidden, answers `NOT_BUILT` for a key on another
    /// platform, before any call, ssh, scp or line. The public four refuse before they reach the
    /// real seams; with every AWS call failing, none could reach `go` or step 9's environment.
    #[test]
    fn a_key_on_another_platform_is_not_built_yet() {
        let scratch = Scratch::new("keyed-not-built");
        let dir = scratch.path().join("home-run");
        let ca = CaEnv::default();
        for platform in ["linux/amd64", "windows/amd64"] {
            for project in ["swimcoach", "demo"] {
                let key = HostKey::new(project, platform);
                let want: Outcome = (not_built_failure(), vec![], vec![], vec![]);
                let runs: Vec<Outcome> = vec![
                    outcome(&dir, |c, s, a, r, seams| {
                        sync_keyed_using(c, &ca, &key, s, a, r, seams)
                    }),
                    outcome(&dir, |c, s, a, r, seams| {
                        deploy_keyed_using(c, &ca, &key, None, s, a, r, seams)
                    }),
                    outcome(&dir, |c, s, a, r, seams| {
                        bootstrap_keyed_using(c, &ca, &key, s, a, r, seams)
                    }),
                    outcome(&dir, |c, s, a, r, seams| {
                        provision_keyed_using(c, &ca, &key, s, a, r, seams)
                    }),
                    outcome(&dir, |c, s, a, r, _| sync_keyed(c, &ca, &key, s, a, r)),
                    outcome(&dir, |c, s, a, r, _| {
                        deploy_keyed(c, &ca, &key, Some("m8a.large"), s, a, r)
                    }),
                    outcome(&dir, |c, s, a, r, _| bootstrap_keyed(c, &ca, &key, s, a, r)),
                    outcome(&dir, |c, s, a, r, _| provision_keyed(c, &ca, &key, s, a, r)),
                ];
                for (n, run) in runs.into_iter().enumerate() {
                    assert_eq!(run, want, "{project} on {platform}, entry {n}");
                }
            }
        }
    }
}
