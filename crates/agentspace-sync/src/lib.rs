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
//!   after it run.
//! - **The template and `box\system` are the repository's own, embedded at build time byte for
//!   byte** (D37's addendum), and written out under `%USERPROFILE%\.agentspace\operator` just
//!   before the call that reads them. What is sent to the box at run time, every remote command,
//!   is this crate's own, written from operator.md's steps (D34).
//! - **Every program this crate starts gets its environment through `agentspace-run`'s
//!   `prepare_child_env`**: the AWS CLI, ssh and scp through the pinned [`Aws`] and [`Remote`],
//!   `go` through this crate's build, and the push's ssh through `agentspace-push`.
//! - **The library reads no environment variable but `PATH`, only to find `go`**, and takes every
//!   other path from the [`Context`]. It writes to stdout and stderr only through [`Say`] and the
//!   streams the spec passes through, and never panics on bad input.
//! - **A test MUST NOT reach the real `go` or `C:\Projects\localswim`** (operator.md, The test
//!   seam, "Not covered"): a test that could reach step 7 MUST go through `sync_using` or
//!   `deploy_using` with a `go` finder and a localswim folder of its own, and a test MUST NOT call
//!   [`sync`] or [`deploy`] in a process whose `PATH` could hold a `go`.

#![forbid(unsafe_code)]

mod embedded;
mod files;
mod go;
mod params;
mod steps;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use agentspace_box::{Clock, InstanceIdCache, SystemClock};
use agentspace_ca::CaEnv;
use agentspace_operator_api::{
    Aws, AwsError, Call, CallerFailure, Context, Failure, Output, Remote, Say,
};

use files::{FileCache, Places};

/// The host a command acts on (D49, D55), re-exported so the operator names it through this crate.
pub use agentspace_hosts::Target;

/// The ssh host alias every connection of `sync` goes through (operator.md, The `agentspace` SSH
/// entry).
const ALIAS: &str = "agentspace";
/// The stack's name (operator.md, The stack).
const STACK: &str = "agentbox";

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
    /// `sync`'s step 8: the forced push over `agentspace`, for an instance ID.
    pub push: &'a dyn Fn(&Context, &dyn Say, &str),
}

fn real_find_go() -> Option<PathBuf> {
    go::find_on_path(std::env::var_os("PATH"))
}

fn real_push(ctx: &Context, say: &dyn Say, instance_id: &str) {
    agentspace_push::push(ctx, say, instance_id, ALIAS, true);
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
        run(&Seams {
            clock: &clock,
            ca_check: &agentspace_ca::check_for_sync,
            find_go,
            localswim_checkout,
            push: &real_push,
        })
    }
}

/// `operator sync` (docs/operator.md, sync): checks the CA, starts the box and waits for it,
/// installs `box\system` and the CA's public key, copies `CLAUDE.md`, builds and installs the
/// localswim CLI when the laptop can, and pushes the token, forced.
///
/// `target` names the host (D49). The pin before v0.07-T03 (architect, 2026-11-20, D55) added it:
/// [`Target::Box`] is today's behavior, and another host fails as not built yet until
/// E0.07-T03-S03 builds it.
pub fn sync(
    ctx: &Context,
    ca_env: &CaEnv,
    target: Target<'_>,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    match target {
        Target::Box => with_real_seams(|seams| sync_using(ctx, ca_env, say, aws, remote, seams)),
        Target::Host(_) => Err(not_built_yet("sync -Host")),
    }
}

/// `operator deploy` (docs/operator.md, deploy): creates or updates the stack, refreshes the
/// instance-ID cache, then runs `sync` with the same [`Say`], so `deploy -Quiet` quiets it too.
///
/// `target` as for [`sync`] (the pin before v0.07-T03, D55).
pub fn deploy(
    ctx: &Context,
    ca_env: &CaEnv,
    target: Target<'_>,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    match target {
        Target::Box => with_real_seams(|seams| deploy_using(ctx, ca_env, say, aws, remote, seams)),
        Target::Host(_) => Err(not_built_yet("deploy -Host")),
    }
}

/// `operator bootstrap` (docs/operator.md, bootstrap): runs the embedded toolchain bootstrap on
/// the target's host (D46). A stub pinned by the architect, 2026-11-20, before v0.07-T03 (D55),
/// until E0.07-T03-S03 builds it.
pub fn bootstrap(
    ctx: &Context,
    ca_env: &CaEnv,
    target: Target<'_>,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    let _ = (ctx, ca_env, target, say, aws, remote);
    Err(not_built_yet("bootstrap"))
}

/// `operator provision -Host <project>` (docs/operator.md, provision): the provisioning run
/// (D48). A stub pinned by the architect, 2026-11-20, before v0.07-T03 (D55), until
/// E0.07-T03-S03 builds it.
pub fn provision(
    ctx: &Context,
    ca_env: &CaEnv,
    project: &str,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
) -> Result<(), Failure> {
    let _ = (ctx, ca_env, project, say, aws, remote);
    Err(not_built_yet("provision"))
}

/// The failure of a command the pin names and T03 has not built yet.
fn not_built_yet(command: &str) -> Failure {
    Failure {
        message: format!("agentspace: {command} is not built yet."),
        caller: CallerFailure::Same,
    }
}

/// A failure of one of `sync`'s or `deploy`'s own steps, with its exact message.
fn step_failed(message: &str) -> Failure {
    Failure {
        message: message.to_string(),
        caller: CallerFailure::CouldNotReach,
    }
}

/// One ssh through the alias, with forwards cleared, and its remote command as the last argument;
/// `Ok(true)` when it exited 0.
fn ssh(remote: &dyn Remote, remote_command: &str) -> Result<bool, Failure> {
    let code = remote.ssh(&["-o", steps::CLEAR_FORWARDS, ALIAS], Some(remote_command))?;
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

/// [`sync`] over [`Seams`].
#[doc(hidden)]
pub fn sync_using(
    ctx: &Context,
    ca_env: &CaEnv,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    let places = Places::new(ctx);

    // Step 1: the CA, before the box is touched. A missing, half-made, mismatched or loosened CA
    // never becomes a host's trusted CA (D17).
    let fingerprint = (seams.ca_check)(ctx, ca_env)?;

    // Step 2: start the box and wait for it, ending with `Ready: <id>`.
    let cache = FileCache::new(&places);
    let instance_id = agentspace_box::start_and_wait(aws, say, seams.clock, &cache, STACK)?;

    // Step 3: remove the staging folder, then copy box\system there whole. A failed removal fails
    // before the copy, since a folder left behind would take the copy one level down (D36). The
    // carried files are written out just before the copy (What the prototype took from its
    // checkout).
    say.progress("Copying box/system to the instance...", None);
    check(
        ssh(remote, steps::REMOVE_STAGING)?,
        "agentspace: Removing the old staging folder on the instance failed.",
    )?;
    let system = files::write_system(&places)?;
    let staging_target = format!("{ALIAS}:{}", steps::STAGING);
    check(
        scp(remote, &["-r", &text(&system), &staging_target])?,
        "agentspace: Copy to the instance failed.",
    )?;

    // Step 4: the CA's public key into the staging folder.
    say.progress(
        &format!("Copying the agentspace SSH CA's public key ({fingerprint}) to the instance..."),
        None,
    );
    let ca_target = format!("{ALIAS}:{}/agentspace_ca.pub", steps::STAGING);
    check(
        scp(remote, &[&text(&places.ca_public_key()), &ca_target])?,
        "agentspace: Copying the CA's public key to the instance failed.",
    )?;

    // Step 5: the staged install, as root; the staging folder is removed only if it succeeded.
    check(
        ssh(remote, steps::INSTALL)?,
        "agentspace: Install on the instance failed.",
    )?;

    // Step 6: CLAUDE.md, when the laptop has one. The folder step's exit code is not checked
    // (B-34): the copy after it is the check.
    let claude_md = places.claude_md();
    if claude_md.exists() {
        say.progress("Copying ~/.claude/CLAUDE.md to the instance...", None);
        let _ = ssh(remote, steps::MAKE_CLAUDE_FOLDER)?;
        let target = format!("{ALIAS}:.claude/CLAUDE.md");
        check(
            scp(remote, &[&text(&claude_md), &target])?,
            "agentspace: Copying CLAUDE.md to the instance failed.",
        )?;
    }

    // Step 7: the localswim CLI, when the laptop can build it.
    match (seams.find_go)() {
        Some(go) if seams.localswim_checkout.is_dir() => {
            say.progress("Building the localswim CLI for linux/arm64...", None);
            files::make_operator_folder(&places)?;
            let out = places.localswim_build();
            check(
                go::build(&go, &seams.localswim_checkout, &out),
                "agentspace: Building the localswim CLI failed.",
            )?;
            say.progress("Copying the localswim CLI to the instance...", None);
            let target = format!("{ALIAS}:{}", steps::LOCALSWIM_STAGING);
            check(
                scp(remote, &[&text(&out), &target])?,
                "agentspace: Copying the localswim CLI failed.",
            )?;
            check(
                ssh(remote, steps::INSTALL_LOCALSWIM)?,
                "agentspace: Installing the localswim CLI failed.",
            )?;
        }
        _ => say.progress(
            "Skipping the localswim CLI: Go or C:\\Projects\\localswim is missing on the laptop.",
            None,
        ),
    }

    // Step 8: the push over `agentspace`, forced. It never fails `sync`.
    (seams.push)(ctx, say, &instance_id);
    Ok(())
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

/// [`deploy`] over [`Seams`].
#[doc(hidden)]
pub fn deploy_using(
    ctx: &Context,
    ca_env: &CaEnv,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    let places = Places::new(ctx);

    // Steps 1 and 2: the public key and the stack parameters, both before any AWS call.
    let key = read_public_key(&places)?;
    let parameters = params::read(&places.stack_parameters())?;

    // Step 3.
    say.progress("Deploying stack 'agentbox' in us-west-2...", None);

    // Step 4: the template written just before the call that reads it, then the deploy, its
    // output passed through and its error text scanned for the sign-in test.
    let template = text(&files::write_template(&places)?);
    let key_override = format!("SshPublicKey={key}");
    let overrides: Vec<String> = parameters
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    let mut args: Vec<&str> = vec![
        "--stack-name",
        STACK,
        "--template-file",
        &template,
        "--capabilities",
        "CAPABILITY_IAM",
        "--no-fail-on-empty-changeset",
        "--tags",
        "project=agentbox",
        "--parameter-overrides",
        &key_override,
    ];
    args.extend(overrides.iter().map(String::as_str));
    aws.call(&Call {
        service: "cloudformation",
        operation: "deploy",
        args: &args,
        output: Output::PassThrough,
    })
    .map_err(|e| match e {
        AwsError::Failed(_) => {
            step_failed("agentspace: Stack deploy failed; see the stack events in the console.")
        }
        // #0034's message, or the whole message for a program that cannot be run.
        other => Failure::from(other),
    })?;

    // Step 5: the cache deleted first, so the ID comes from the stack as the deploy left it; then
    // read from the stack's output and written again. A failed read leaves no cache.
    let cache = FileCache::new(&places);
    cache.delete()?;
    let instance_id = agentspace_box::instance_id_from_stack(aws, STACK)?;
    cache.write(&agentspace_box::cache_bytes(&instance_id))?;
    say.progress(&format!("Instance: {instance_id}"), None);

    // Step 6: sync, as if run with deploy's own -Quiet; its failure is deploy's.
    sync_using(ctx, ca_env, say, aws, remote, seams)
}
