//! `sync` for one host, and `sync -All` (docs/operator.md, sync, and Every host at once: `-All`;
//! D46, D49, D50, D55).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **`sync`'s order decides what reaches the host, and MUST be operator.md's steps 1 to 8**: the
//!   CA check before the host is touched, the operator's key, certificate and alias for a host that
//!   is not the box, the start-and-wait, then the nine calls of its table in that order, each with
//!   its step's message, the first failure ending the command with nothing after it run.
//! - **Every connection goes through the host's alias**: `agentspace` for the box, exactly as in
//!   v0.06, and `agentspace-host.<project>` for every other host, with that host's push stamp.
//! - **The host is looked up once, as the command starts**, before any program starts: a registry
//!   that cannot be read, or a project with no record, fails the command at once.
//! - **`sync -All` acts on every record in file order**, prints `Host      <project> (<stack>),
//!   <state>` to stderr before each one whatever `-Quiet` says, syncs only the `ready` ones, goes on
//!   past a host that failed, and ends with `agentspace: <n> of <m> hosts failed.` when any did.
//!   #0034's sign-in message ends it at once. It writes `box\system` and builds the localswim CLI
//!   once, in the first part that gets to them.

use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;

use agentspace_box::start_and_wait;
use agentspace_ca::CaEnv;
use agentspace_hosts::{Host, HostState, Hosts, Target, host_alias};
use agentspace_operator_api::{Aws, CallerFailure, Context, Failure, Remote, SIGN_IN_MESSAGE, Say};

use crate::files::{self, Places};
use crate::patch_wait::{self, PatchOutcome};
use crate::registry::{self, RegistryCache};
use crate::{ALIAS, Seams, check, go, host, scp, ssh_to, step_failed, steps, text};

/// The message of a localswim build that failed.
const BUILD_FAILED: &str = "agentspace: Building the localswim CLI failed.";

/// What `sync` and `bootstrap` take from the host they act on.
pub(crate) struct Plan {
    pub(crate) host: Host,
    /// Whether the host is the box, which is reached through the user's own `agentspace` entry.
    pub(crate) is_box: bool,
    /// The alias every ssh and scp goes through.
    pub(crate) alias: String,
    pub(crate) cache: RegistryCache,
}

impl Plan {
    pub(crate) fn for_record(places: &Places, host: &Host) -> Plan {
        let is_box = registry::is_box_record(host);
        Plan {
            alias: if is_box {
                ALIAS.to_string()
            } else {
                host_alias(&host.project)
            },
            cache: RegistryCache::new(places.home(), host),
            host: host.clone(),
            is_box,
        }
    }

    /// The host's project for the push, `None` for the box, whose stamp is v0.06's.
    fn push_host(&self) -> Option<&str> {
        (!self.is_box).then_some(self.host.project.as_str())
    }

    /// Start-and-wait's failure names `ssh agentspace` in its `journalctl` hint, which is the
    /// box's; a host's names its own alias (operator.md, From the command line).
    pub(crate) fn hint(&self, failure: Failure) -> Failure {
        if self.is_box {
            return failure;
        }
        hint_for(&self.alias, failure)
    }
}

/// A start-and-wait failure of a host that is not the box: its `journalctl` hint names `alias`, the
/// host's own, where `agentspace-box` words it for the box (operator.md, From the command line).
pub(crate) fn hint_for(alias: &str, failure: Failure) -> Failure {
    Failure {
        message: failure.message.replace(
            "ssh agentspace journalctl",
            &format!("ssh {alias} journalctl"),
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

/// The localswim build, shared by the parts of one `sync -All`.
#[derive(Clone)]
enum Localswim {
    Untried,
    /// Go or the checkout is missing, and the skip message has been printed.
    Skipped,
    Built(PathBuf),
    Failed,
}

/// What the parts of one command share: `box\system` written out once, and the localswim CLI built
/// once.
pub(crate) struct Shared {
    system: RefCell<Option<PathBuf>>,
    localswim: RefCell<Localswim>,
}

impl Shared {
    pub(crate) fn new() -> Shared {
        Shared {
            system: RefCell::new(None),
            localswim: RefCell::new(Localswim::Untried),
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
        // Whatever `-Quiet` says, so every line after it is that host's.
        let _ = writeln!(
            std::io::stderr(),
            "{:<10}{} ({}), {}",
            "Host",
            record.project,
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

/// One host's part of `sync`: operator.md's steps 1 to 8, with `plan`'s alias in every call, and
/// the patch wait between rows 4 and 5. Returns how the patch wait ended.
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

    // Step 1: the CA, before the host is touched. A missing, half-made, mismatched or loosened CA
    // never becomes a host's trusted CA (D17).
    let fingerprint = (seams.ca_check)(ctx, ca_env)?;

    // Between steps 1 and 2, for a host that is not the box: the operator's key, certificate and
    // alias (operator.md, How the operator reaches a host).
    if !plan.is_box {
        host::prepare(ctx, places, &plan.host.project, seams)?;
    }

    // Step 2: start the host and wait for it, ending with `Ready: <id>`.
    let instance_id = start_and_wait(aws, say, seams.clock, &plan.cache, &plan.host.stack)
        .map_err(|failure| plan.hint(failure))?;

    // Step 3: remove the staging folder, then copy box\system there whole. A failed removal fails
    // before the copy, since a folder left behind would take the copy one level down (D36). The
    // carried files are written out just before the copy, once for all the parts of one command.
    say.progress("Copying box/system to the instance...", None);
    check(
        ssh_to(remote, alias, steps::REMOVE_STAGING)?,
        "agentspace: Removing the old staging folder on the instance failed.",
    )?;
    let written = shared.system.borrow().clone();
    let system = match written {
        Some(system) => system,
        None => {
            let folder = seams.folder.path(places)?;
            let system = files::write_system(places, &folder)?;
            *shared.system.borrow_mut() = Some(system.clone());
            system
        }
    };
    let staging_target = format!("{alias}:{}", steps::STAGING);
    check(
        scp(remote, &["-r", &text(&system), &staging_target])?,
        "agentspace: Copy to the instance failed.",
    )?;

    // Step 4: the CA's public key into the staging folder.
    say.progress(
        &format!("Copying the agentspace SSH CA's public key ({fingerprint}) to the instance..."),
        None,
    );
    let ca_target = format!("{alias}:{}/agentspace_ca.pub", steps::STAGING);
    check(
        scp(remote, &[&text(&places.ca_public_key()), &ca_target])?,
        "agentspace: Copying the CA's public key to the instance failed.",
    )?;

    // Step 5: the staged install, as root; the staging folder is removed only if it succeeded.
    check(
        ssh_to(remote, alias, steps::INSTALL)?,
        "agentspace: Install on the instance failed.",
    )?;

    // Then, before step 6, the patch wait, only after an install that exited 0 (D61's change,
    // #0163): until the host answers `patched`, or `failed`, which warns and goes on (P21). Its
    // deadline fails `sync` here, with nothing after row 4 run.
    let outcome = patch_wait::wait(aws, say, seams.clock, &instance_id, alias)?;

    // Step 6: CLAUDE.md, when the laptop has one. The folder step's exit code is not checked
    // (B-34): the copy after it is the check.
    let claude_md = places.claude_md();
    if claude_md.exists() {
        say.progress("Copying ~/.claude/CLAUDE.md to the instance...", None);
        let _ = ssh_to(remote, alias, steps::MAKE_CLAUDE_FOLDER)?;
        let target = format!("{alias}:.claude/CLAUDE.md");
        check(
            scp(remote, &[&text(&claude_md), &target])?,
            "agentspace: Copying CLAUDE.md to the instance failed.",
        )?;
    }

    // Step 7: the localswim CLI, when the laptop can build it.
    localswim(places, plan, say, remote, seams, shared)?;

    // Step 8: the push over the host's alias, forced, with the host's own stamp. It never fails
    // `sync`.
    (seams.push)(ctx, say, &instance_id, alias, plan.push_host());
    Ok(outcome)
}

/// Step 7: builds (once for the command), copies and installs the localswim CLI.
fn localswim(
    places: &Places,
    plan: &Plan,
    say: &dyn Say,
    remote: &dyn Remote,
    seams: &Seams,
    shared: &Shared,
) -> Result<(), Failure> {
    let alias = plan.alias.as_str();
    let state = shared.localswim.borrow().clone();
    let built = match state {
        Localswim::Built(path) => path,
        Localswim::Skipped => return Ok(()),
        Localswim::Failed => return Err(step_failed(BUILD_FAILED)),
        Localswim::Untried => match (seams.find_go)() {
            Some(go_exe) if seams.localswim_checkout.is_dir() => {
                say.progress("Building the localswim CLI for linux/arm64...", None);
                let folder = seams.folder.path(places)?;
                let out = files::localswim_build(&folder);
                if !go::build(&go_exe, &seams.localswim_checkout, &out) {
                    *shared.localswim.borrow_mut() = Localswim::Failed;
                    return Err(step_failed(BUILD_FAILED));
                }
                *shared.localswim.borrow_mut() = Localswim::Built(out.clone());
                out
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
    say.progress("Copying the localswim CLI to the instance...", None);
    let target = format!("{alias}:{}", steps::LOCALSWIM_STAGING);
    check(
        scp(remote, &[&text(&built), &target])?,
        "agentspace: Copying the localswim CLI failed.",
    )?;
    check(
        ssh_to(remote, alias, steps::INSTALL_LOCALSWIM)?,
        "agentspace: Installing the localswim CLI failed.",
    )
}
