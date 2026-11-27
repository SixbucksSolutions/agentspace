//! `sync` for one host, and `sync -All` (docs/operator.md, sync, and Every host at once: `-All`;
//! D46, D49, D50, D55, D91, D94).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **`sync`'s order decides what reaches the host, and MUST be operator.md's steps 1 to 8**: the
//!   CA check before the host is touched, the operator's key, certificate and alias for a host that
//!   is not the box, the start-and-wait, then the nine calls of its table in that order, each with
//!   its step's message, the first failure ending the command with nothing after it run.
//! - **Every step a platform supplies is its seam's** (operator.md, The platform seam, the sync
//!   row): the host's files and staging folder, the clear, install and folder commands, `CLAUDE.md`'s
//!   path, the localswim CLI's target, staging path and install, and the patch wait's document,
//!   question and messages. On both Linux platforms those are today's, byte for byte. A platform
//!   whose sync row fails ends the host's part with that failure before the CA check.
//! - **Every connection goes through the host's alias**: `agentspace` for the box, exactly as in
//!   v0.06, and `agentspace-host.<host name>` for every other host, with that host's push stamp,
//!   in the host's folder named by its host name (D91; pin P3 of v0.12-T03).
//! - **The host is looked up once, as the command starts**, before any program starts: a registry
//!   that cannot be read, or a host with no record, fails the command at once.
//! - **`sync -All` acts on every record in file order, every platform's**, prints
//!   `Host      <host name> (<stack>), <state>` to stderr before each one whatever `-Quiet` says,
//!   syncs only the `ready` ones, goes on past a host that failed, and ends with `agentspace: <n> of
//!   <m> hosts failed.` when any did. #0034's sign-in message ends it at once. It writes each
//!   platform's host files once, and builds the localswim CLI once per platform, in the first part
//!   that gets to them; the skip message prints once.

use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;

use agentspace_box::start_and_wait_on;
use agentspace_ca::CaEnv;
use agentspace_hosts::{Host, HostKey, HostState, Hosts, Target};
use agentspace_operator_api::{
    Aws, CallerFailure, Context, Failure, HostPlatform, LocalswimTarget, Remote, SIGN_IN_MESSAGE,
    Say, SyncPlan,
};

use crate::files::{self, Places};
use crate::patch_wait::{self, PatchOutcome};
use crate::registry::{self, RegistryCache};
use crate::{ALIAS, Seams, check, go, host, scp, ssh_to, step_failed, text};

/// The message of a localswim build that failed.
const BUILD_FAILED: &str = "agentspace: Building the localswim CLI failed.";

/// What `sync` and `bootstrap` take from the host they act on.
pub(crate) struct Plan {
    pub(crate) host: Host,
    /// The host's key: its project and its record's platform.
    pub(crate) key: HostKey,
    /// Whether the host is the box, which is reached through the user's own `agentspace` entry.
    pub(crate) is_box: bool,
    /// The alias every ssh and scp goes through.
    pub(crate) alias: String,
    pub(crate) cache: RegistryCache,
    /// The host's platform seam, or `None` for a record whose platform agentspace has no seam for.
    seam: Option<&'static dyn HostPlatform>,
}

impl Plan {
    pub(crate) fn for_record(places: &Places, host: &Host) -> Plan {
        let is_box = registry::is_box_record(host);
        let key = host.key();
        Plan {
            alias: if is_box {
                ALIAS.to_string()
            } else {
                key.operator_alias()
            },
            cache: RegistryCache::new(places.home(), host),
            host: host.clone(),
            key,
            is_box,
            seam: agentspace_platforms::for_platform(host.platform()),
        }
    }

    /// The host's platform seam, or the unsupported platform's message.
    pub(crate) fn platform(&self) -> Result<&'static dyn HostPlatform, Failure> {
        match self.seam {
            Some(seam) => Ok(seam),
            None => crate::seam_for(self.host.platform()),
        }
    }

    /// The host's folder name, which its push stamp is under: `None` for the box, whose stamp is
    /// v0.06's, and the host's name for every other host (D91; pin P3 of v0.12-T03).
    fn push_host(&self) -> Option<String> {
        (!self.is_box).then(|| self.key.host_name())
    }

    /// Start-and-wait's failure names `ssh agentspace <log command>` in its hint, which is the
    /// box's; a host's names its own alias (operator.md, From the command line).
    pub(crate) fn hint(&self, failure: Failure) -> Failure {
        if self.is_box {
            return failure;
        }
        // A record whose platform has no seam never reaches the start-and-wait.
        match self.seam {
            Some(seam) => hint_for(&self.alias, seam, failure),
            None => failure,
        }
    }
}

/// A start-and-wait failure of a host that is not the box: its hint, `ssh agentspace <log
/// command>` with the platform's log command, names `alias`, the host's own, where `agentspace-box`
/// words it for the box (operator.md, From the command line; The platform seam, the patch messages
/// row).
pub(crate) fn hint_for(alias: &str, platform: &dyn HostPlatform, failure: Failure) -> Failure {
    let log_command = platform.patch_messages().log_command;
    Failure {
        message: failure.message.replace(
            &format!("ssh agentspace {log_command}"),
            &format!("ssh {alias} {log_command}"),
        ),
        ..failure
    }
}

/// The host `target` names, read once as the command starts.
pub(crate) fn plan(places: &Places, target: Target<'_>) -> Result<Plan, Failure> {
    let hosts = registry::read(places.home())?;
    let host = registry::find(&hosts, target)?;
    Ok(Plan::for_record(places, &host))
}

/// The host `key` names, read once as the command starts (D91). A platform agentspace has no seam
/// for is refused before the registry is read.
pub(crate) fn plan_keyed(places: &Places, key: &HostKey) -> Result<Plan, Failure> {
    crate::seam_for(&key.platform)?;
    let hosts = registry::read(places.home())?;
    let host = registry::find_keyed(&hosts, key)?;
    Ok(Plan::for_record(places, &host))
}

/// The localswim build, shared by the parts of one `sync -All`.
#[derive(Clone)]
enum Localswim {
    Untried,
    /// Go or the checkout is missing, and the skip message has been printed: every platform skips.
    Skipped,
    /// `go` was found, and the checkout is there.
    Found(PathBuf),
}

/// What the parts of one command share: each platform's host files written out once, and the
/// localswim CLI built once per platform.
pub(crate) struct Shared {
    /// Each folder written, by its name in the command's folder (`system`, `windows`).
    systems: RefCell<Vec<(&'static str, PathBuf)>>,
    localswim: RefCell<Localswim>,
    /// Each build tried, by its file name: the built file, or `None` for a build that failed.
    builds: RefCell<Vec<(&'static str, Option<PathBuf>)>>,
}

impl Shared {
    pub(crate) fn new() -> Shared {
        Shared {
            systems: RefCell::new(Vec::new()),
            localswim: RefCell::new(Localswim::Untried),
            builds: RefCell::new(Vec::new()),
        }
    }
}

/// `sync` for `target` over [`Seams`], and how its patch wait ended, which `provision`'s step 6
/// logs.
pub(crate) fn sync_target(
    ctx: &Context,
    ca_env: &CaEnv,
    target: Target<'_>,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<PatchOutcome, Failure> {
    let places = Places::new(ctx);
    let plan = plan(&places, target)?;
    run(
        ctx,
        ca_env,
        &places,
        &plan,
        say,
        aws,
        remote,
        seams,
        &Shared::new(),
    )
}

/// `sync` for the host `key` over [`Seams`] (D91), and how its patch wait ended.
pub(crate) fn sync_key(
    ctx: &Context,
    ca_env: &CaEnv,
    key: &HostKey,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<PatchOutcome, Failure> {
    let places = Places::new(ctx);
    let plan = plan_keyed(&places, key)?;
    run(
        ctx,
        ca_env,
        &places,
        &plan,
        say,
        aws,
        remote,
        seams,
        &Shared::new(),
    )
}

/// `sync -All` over [`Seams`].
pub(crate) fn sync_all(
    ctx: &Context,
    ca_env: &CaEnv,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
) -> Result<(), Failure> {
    let places = Places::new(ctx);
    let hosts: Hosts = registry::read(places.home())?;
    let total = hosts.hosts.len();
    let shared = Shared::new();
    let mut failed = 0usize;
    for record in &hosts.hosts {
        // Whatever `-Quiet` says, so every line after it is that host's. The host is named by its
        // host name, which on `linux/arm64` is its project (pins P12 and P44 of v0.12-T01).
        let _ = writeln!(
            std::io::stderr(),
            "{:<10}{} ({}), {}",
            "Host",
            record.key().host_name(),
            record.stack,
            record.state.as_str()
        );
        if record.state != HostState::Ready {
            say.progress(
                &format!(
                    "Skipping: the host is {}; its next provisioning run syncs it.",
                    record.state.as_str()
                ),
                None,
            );
            continue;
        }
        let plan = Plan::for_record(&places, record);
        // A part whose patch wait ended on `failed` is not a failed part (P21).
        if let Err(failure) = run(
            ctx, ca_env, &places, &plan, say, aws, remote, seams, &shared,
        ) {
            // #0034's message ends the command at once: every later host's AWS call would fail the
            // same way.
            if failure.message == SIGN_IN_MESSAGE {
                return Err(failure);
            }
            let _ = writeln!(std::io::stderr(), "{}", failure.message);
            failed += 1;
        }
    }
    if failed > 0 {
        return Err(Failure {
            message: format!("agentspace: {failed} of {total} hosts failed."),
            caller: CallerFailure::CouldNotReach,
        });
    }
    Ok(())
}

/// The name of the folder a platform's host files are written to in the command's folder:
/// `windows` for `windows/amd64`'s `box/windows`, and `system` for both Linux platforms'
/// `box/system` (operator.md, sync on a Windows host; The operator's folder, one per command).
fn files_folder_name(platform: &dyn HostPlatform) -> &'static str {
    if platform.platform().starts_with("windows/") {
        "windows"
    } else {
        "system"
    }
}

/// One host's part of `sync`: operator.md's steps 1 to 8, with `plan`'s alias in every call, and
/// the patch wait between rows 4 and 5, each platform step from the host's seam. Returns how the
/// patch wait ended.
#[allow(clippy::too_many_arguments)]
fn run(
    ctx: &Context,
    ca_env: &CaEnv,
    places: &Places,
    plan: &Plan,
    say: &dyn Say,
    aws: &dyn Aws,
    remote: &dyn Remote,
    seams: &Seams,
    shared: &Shared,
) -> Result<PatchOutcome, Failure> {
    let alias = plan.alias.as_str();
    let platform = plan.platform()?;

    // Step 1: the CA, before the host is touched. A missing, half-made, mismatched or loosened CA
    // never becomes a host's trusted CA (D17).
    let fingerprint = (seams.ca_check)(ctx, ca_env)?;

    // Between steps 1 and 2, for a host that is not the box: the operator's key, certificate and
    // alias (operator.md, sync and How the operator reaches a host), whatever the platform's rows
    // then give.
    if !plan.is_box {
        host::prepare_keyed(ctx, places, &plan.key, platform, seams)?;
    }

    // The platform's rows are read before the host is started: a row that fails ends the part
    // before any AWS call, ssh or scp.
    let rows = platform.sync_plan()?;

    // Step 2: start the host and wait for it, ending with `Ready: <id>`.
    let instance_id = start_and_wait_on(
        aws,
        say,
        seams.clock,
        &plan.cache,
        &plan.host.stack,
        platform,
    )
    .map_err(|failure| plan.hint(failure))?;

    // Step 3: remove the staging folder, then copy the host's files there whole. A failed removal
    // fails before the copy, since a folder left behind would take the copy one level down (D36).
    // The carried files are written out just before the copy, once per platform for all the parts
    // of one command.
    say.progress("Copying box/system to the instance...", None);
    if let Some(clear) = &rows.clear_command {
        check(
            ssh_to(remote, alias, clear)?,
            "agentspace: Removing the old staging folder on the instance failed.",
        )?;
    }
    let system = host_files(places, seams, shared, platform, &rows)?;
    let staging_target = format!("{alias}:{}", rows.staging);
    check(
        scp(remote, &["-r", &text(&system), &staging_target])?,
        "agentspace: Copy to the instance failed.",
    )?;

    // Step 4: the CA's public key into the staging folder.
    say.progress(
        &format!("Copying the agentspace SSH CA's public key ({fingerprint}) to the instance..."),
        None,
    );
    let ca_target = format!("{alias}:{}/agentspace_ca.pub", rows.staging);
    check(
        scp(remote, &[&text(&places.ca_public_key()), &ca_target])?,
        "agentspace: Copying the CA's public key to the instance failed.",
    )?;

    // Step 5: the staged install; the staging folder is removed only if it succeeded.
    check(
        ssh_to(remote, alias, &rows.install_command)?,
        "agentspace: Install on the instance failed.",
    )?;

    // Then, before step 6, the patch wait, only after an install that exited 0 (D61's change,
    // #0163): until the host answers `patched`, or `failed`, which warns and goes on (P21). Its
    // deadline fails `sync` here, with nothing after row 4 run.
    let outcome = patch_wait::wait_on(aws, say, seams.clock, &instance_id, alias, platform)?;

    // Step 6: CLAUDE.md, when the laptop has one. The folder step's exit code is not checked
    // (B-34): the copy after it is the check.
    let claude_md = places.claude_md();
    if claude_md.exists() {
        say.progress("Copying ~/.claude/CLAUDE.md to the instance...", None);
        let _ = ssh_to(remote, alias, &rows.claude_folder_command)?;
        let target = format!("{alias}:{}", rows.claude_md_path);
        check(
            scp(remote, &[&text(&claude_md), &target])?,
            "agentspace: Copying CLAUDE.md to the instance failed.",
        )?;
    }

    // Step 7: the localswim CLI, when the laptop can build it, for the host's platform.
    localswim(places, plan, platform, &rows, say, remote, seams, shared)?;

    // Step 8: the push over the host's alias, forced, with the host's own stamp. It never fails
    // `sync`.
    let push_host = plan.push_host();
    (seams.push)(ctx, say, &instance_id, alias, push_host.as_deref());
    Ok(outcome)
}

/// The host's files written out into the command's folder, once per platform for one command:
/// `<folder>\system\` on Linux and `<folder>\windows\` on Windows, holding exactly the seam's
/// files.
fn host_files(
    places: &Places,
    seams: &Seams,
    shared: &Shared,
    platform: &dyn HostPlatform,
    rows: &SyncPlan,
) -> Result<PathBuf, Failure> {
    let name = files_folder_name(platform);
    let written = shared
        .systems
        .borrow()
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, path)| path.clone());
    if let Some(path) = written {
        return Ok(path);
    }
    let folder = seams.folder.path(places)?;
    let path = files::write_system(&folder, name, rows.files)?;
    shared.systems.borrow_mut().push((name, path.clone()));
    Ok(path)
}

/// Step 7: builds (once per platform for the command), copies and installs the localswim CLI.
#[allow(clippy::too_many_arguments)]
fn localswim(
    places: &Places,
    plan: &Plan,
    platform: &dyn HostPlatform,
    rows: &SyncPlan,
    say: &dyn Say,
    remote: &dyn Remote,
    seams: &Seams,
    shared: &Shared,
) -> Result<(), Failure> {
    let alias = plan.alias.as_str();
    let state = shared.localswim.borrow().clone();
    let go_exe = match state {
        Localswim::Skipped => return Ok(()),
        Localswim::Found(go_exe) => go_exe,
        Localswim::Untried => match (seams.find_go)() {
            Some(go_exe) if seams.localswim_checkout.is_dir() => {
                *shared.localswim.borrow_mut() = Localswim::Found(go_exe.clone());
                go_exe
            }
            _ => {
                say.progress(
                    "Skipping the localswim CLI: Go or C:\\Projects\\localswim is missing on the laptop.",
                    None,
                );
                *shared.localswim.borrow_mut() = Localswim::Skipped;
                return Ok(());
            }
        },
    };
    let built = build_once(
        places,
        platform,
        &rows.localswim,
        &go_exe,
        say,
        seams,
        shared,
    )?;
    say.progress("Copying the localswim CLI to the instance...", None);
    let target = format!("{alias}:{}", rows.localswim_staging);
    check(
        scp(remote, &[&text(&built), &target])?,
        "agentspace: Copying the localswim CLI failed.",
    )?;
    check(
        ssh_to(remote, alias, &rows.localswim_install_command)?,
        "agentspace: Installing the localswim CLI failed.",
    )
}

/// The platform's localswim build, made once per command: `Building the localswim CLI for
/// <platform>...`, then `go build` with the platform's `GOOS` and `GOARCH` into its file name in
/// the command's folder. A build that failed fails every later part of its platform at once.
fn build_once(
    places: &Places,
    platform: &dyn HostPlatform,
    target: &LocalswimTarget,
    go_exe: &std::path::Path,
    say: &dyn Say,
    seams: &Seams,
    shared: &Shared,
) -> Result<PathBuf, Failure> {
    let tried = shared
        .builds
        .borrow()
        .iter()
        .find(|(name, _)| *name == target.file_name)
        .map(|(_, built)| built.clone());
    match tried {
        Some(Some(built)) => return Ok(built),
        Some(None) => return Err(step_failed(BUILD_FAILED)),
        None => {}
    }
    say.progress(
        &format!("Building the localswim CLI for {}...", platform.platform()),
        None,
    );
    let folder = seams.folder.path(places)?;
    let out = files::localswim_build(&folder, target);
    if !go::build(go_exe, &seams.localswim_checkout, &out, target) {
        shared.builds.borrow_mut().push((target.file_name, None));
        return Err(step_failed(BUILD_FAILED));
    }
    shared
        .builds
        .borrow_mut()
        .push((target.file_name, Some(out.clone())));
    Ok(out)
}
