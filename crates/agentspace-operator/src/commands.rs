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

use std::time::Duration;

use agentspace_box::{Clock, InstanceIdCache};
use agentspace_ca::CaEnv;
use agentspace_hosts::{BOX_PROJECT, Host as Record, HostState, Hosts, Target, host_alias, is_box};
use agentspace_operator_api::{
    Aws, Call, CallerFailure, Context, Failure, Output, Remote, SIGN_IN_MESSAGE, Say,
};
use serde_json::Value;

use crate::cache::{HostCache, read_registry};
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

/// A host as a command sees it: its record, and what a command takes from it (operator.md, From
/// the command line, "What a command takes from its host").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRef {
    pub project: String,
    /// The record's `stack`: `agentbox` for the box.
    pub stack: String,
    /// The alias the host's own ssh, scp and push use: `agentspace` for the box (the user's own
    /// entry), `agentspace-host.<project>` for every other host.
    pub alias: String,
    pub is_box: bool,
    pub state: HostState,
    /// The record's `instance_id`, when it holds one.
    pub instance_id: Option<String>,
}

impl HostRef {
    fn from_record(record: &Record) -> HostRef {
        let box_host = is_box(record);
        HostRef {
            project: record.project.clone(),
            stack: record.stack.clone(),
            alias: if box_host {
                DEFAULT_ALIAS.to_string()
            } else {
                host_alias(&record.project)
            },
            is_box: box_host,
            state: record.state,
            instance_id: record.instance_id.clone(),
        }
    }

    /// The host as `sync`, `deploy` and `bootstrap` name it.
    fn target(&self) -> Target<'_> {
        if self.is_box {
            Target::Box
        } else {
            Target::Host(&self.project)
        }
    }

    /// The host as the token push names it for its stamp: `None` for the box.
    fn stamp_host(&self) -> Option<&str> {
        (!self.is_box).then_some(self.project.as_str())
    }
}

/// The record `-Host <project>` names, or the box's without `-Host`. A project with no record
/// fails before any program starts or any AWS call is made (operator.md, From the command line).
fn resolve(hosts: &Hosts, project: Option<&str>) -> Result<HostRef, Failure> {
    let project = project.unwrap_or(BOX_PROJECT);
    hosts
        .hosts
        .iter()
        .find(|h| h.project == project)
        .map(HostRef::from_record)
        .ok_or_else(|| Failure {
            message: format!(
                "agentspace: Project '{project}' has no host; provision -Host {project} makes one."
            ),
            caller: CallerFailure::Same,
        })
}

/// Runs a parsed command.
///
/// `ca` and `provision` read no record here: `ca` takes no host, and `provision` finds its own
/// (operator.md, provision, step 1). Every other command finds its host's record, or every record
/// for `-All`, before it starts any program or makes any AWS call.
pub fn dispatch(request: &Request, parts: &Parts) -> Result<(), Failure> {
    match request.command {
        Command::Ca => return parts.reach.ca(parts.ctx, parts.ca_env, parts.say),
        Command::Provision => {
            return parts.reach.provision(
                parts.ctx,
                parts.ca_env,
                request.host.as_deref().unwrap_or(BOX_PROJECT),
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
    let host = resolve(&registry, request.host.as_deref())?;
    match request.command {
        Command::Up => up(
            parts,
            &host,
            request.alias_given.then_some(request.alias.as_str()),
        ),
        Command::Down => down(parts, &host),
        Command::Status => status(parts, &host),
        Command::Grow => grow(parts, &host, request.gib),
        Command::Sync => parts.reach.sync(
            parts.ctx,
            parts.ca_env,
            host.target(),
            parts.say,
            parts.aws,
            parts.remote,
        ),
        Command::Deploy => parts.reach.deploy(
            parts.ctx,
            parts.ca_env,
            host.target(),
            request.instance_type.as_deref(),
            parts.say,
            parts.aws,
            parts.remote,
        ),
        Command::Bootstrap => parts.reach.bootstrap(
            parts.ctx,
            parts.ca_env,
            host.target(),
            parts.say,
            parts.aws,
            parts.remote,
        ),
        Command::Ca | Command::Provision => Ok(()),
    }
}

/// `status -All`, `down -All` and `sync -All` (operator.md, Every host at once): every record, in
/// the file's order, one at a time. Before each host the command prints `Host      <project>
/// (<stack>), <state>`, whatever `-Quiet` says. Each host's part is that command's own steps with
/// the host, but `status` and `down` MUST NOT sign a certificate or write an alias.
///
/// A failure ends that host's part, and the command goes on to the next. At the end, when any host
/// failed, it fails with `agentspace: <n> of <m> hosts failed.`. #0034's sign-in message ends the
/// command at once, since every later host's AWS call would fail the same way.
fn all(parts: &Parts, command: Command, hosts: &[HostRef]) -> Result<(), Failure> {
    let mut failed = 0usize;
    for host in hosts {
        parts.report.report(&format!(
            "Host      {} ({}), {}",
            host.project,
            host.stack,
            host.state.as_str()
        ));
        let part = match command {
            Command::Status => {
                if host.instance_id.is_none() && host.state != HostState::Ready {
                    parts.report.report("Instance  none recorded");
                    Ok(())
                } else {
                    report_status(parts, host)
                }
            }
            Command::Down => {
                if host.instance_id.is_none() && host.state != HostState::Ready {
                    parts
                        .say
                        .progress("Skipping: no instance is recorded for this host.", None);
                    Ok(())
                } else {
                    stop(parts, host)
                }
            }
            _ => {
                if host.state == HostState::Ready {
                    parts.reach.sync(
                        parts.ctx,
                        parts.ca_env,
                        host.target(),
                        parts.say,
                        parts.aws,
                        parts.remote,
                    )
                } else {
                    parts.say.progress(
                        &format!(
                            "Skipping: the host is {}; its next provisioning run syncs it.",
                            host.state.as_str()
                        ),
                        None,
                    );
                    Ok(())
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
    let cache = HostCache::new(&parts.ctx.home, &host.project);
    let id = agentspace_box::start_and_wait(parts.aws, parts.say, parts.clock, &cache, &host.stack)
        .map_err(|failure| for_host(failure, host))?;
    if parts.ctx.nested {
        return Ok(());
    }
    if !host.is_box
        && let Err(failure) = parts.reach.access(parts.ctx, parts.ca_env, &host.project)
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
        host.stamp_host(),
    );
    Ok(())
}

/// The host's instance ID: from its record, or from its stack's output when the record has none,
/// writing it into the record then.
fn instance_id(parts: &Parts, host: &HostRef) -> Result<String, Failure> {
    let cache = HostCache::new(&parts.ctx.home, &host.project);
    agentspace_box::instance_id(parts.aws, &cache as &dyn InstanceIdCache, &host.stack)
}

/// The host's key, certificate and alias, for a host that is not the box. The box has none: it is
/// reached through the user's own entry and key.
fn sign(parts: &Parts, host: &HostRef) -> Result<(), Failure> {
    if host.is_box {
        return Ok(());
    }
    parts.reach.access(parts.ctx, parts.ca_env, &host.project)
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
    Ok(Described {
        state,
        instance_type,
        volume_id,
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
    report_status(parts, host)?;
    sign(parts, host)
}

/// The five lines alone, printed only once every call they need has succeeded. `status -All` runs
/// it for each host, and signs nothing.
fn report_status(parts: &Parts, host: &HostRef) -> Result<(), Failure> {
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
        let exit = parts.remote.ssh(
            &["-o", "ClearAllForwardings=yes", &host.alias],
            Some(GROW_REMOTE_COMMAND),
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
