//! The commands: `up`, `down`, `status`, `grow`, and the dispatch of `ca`, `deploy` and `sync`
//! (docs/operator.md, Commands; Starting the box and waiting for it; Calls by command).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Every command works against the pinned [`Aws`], [`Remote`] and [`Say`], a [`Clock`] and the
//! instance-ID cache, so that a test runs each one against stand-ins. What a command reaches
//! beyond those (the token push, `ca`, `sync`, `deploy`) is [`Reach`].

use std::time::Duration;

use agentspace_box::{Clock, InstanceIdCache};
use agentspace_ca::CaEnv;
use agentspace_operator_api::{Aws, Call, CallerFailure, Context, Failure, Output, Remote, Say};
use serde_json::Value;

use crate::{Command, Request};

/// Where `status`'s report goes: stderr, whatever `-Quiet` says (operator.md, status). The pinned
/// [`Say`] suppresses with `-Quiet`, so the report has a channel of its own.
pub trait Report {
    /// Prints one line of the report, followed by a line feed.
    fn report(&self, line: &str);
}

/// What a command reaches beyond the AWS CLI, ssh and scp.
pub trait Reach {
    /// `up`'s token push, unforced, over `alias`. It never fails a command (operator.md, What the
    /// laptop checks).
    fn push(&self, ctx: &Context, say: &dyn Say, instance_id: &str, alias: &str);
    /// `ca`.
    fn ca(&self, ctx: &Context, env: &CaEnv, say: &dyn Say) -> Result<(), Failure>;
    /// `sync`.
    fn sync(
        &self,
        ctx: &Context,
        env: &CaEnv,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure>;
    /// `deploy`.
    fn deploy(
        &self,
        ctx: &Context,
        env: &CaEnv,
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
    pub cache: &'a dyn InstanceIdCache,
    pub reach: &'a dyn Reach,
}

/// Runs a parsed command.
pub fn dispatch(request: &Request, parts: &Parts) -> Result<(), Failure> {
    match request.command {
        Command::Up => up(parts, &request.alias),
        Command::Down => down(parts),
        Command::Status => status(parts),
        Command::Grow => grow(parts, request.gib),
        Command::Ca => parts.reach.ca(parts.ctx, parts.ca_env, parts.say),
        Command::Sync => {
            parts
                .reach
                .sync(parts.ctx, parts.ca_env, parts.say, parts.aws, parts.remote)
        }
        Command::Deploy => {
            parts
                .reach
                .deploy(parts.ctx, parts.ca_env, parts.say, parts.aws, parts.remote)
        }
    }
}

/// `up` (operator.md, up): reads the instance ID, starts the box and waits for it, ending with
/// `Ready: <id>`, then pushes the token over `alias` unless this is a nested `up`
/// (`AGENTSPACE_NESTED` was set as the program started). The push never fails the command.
pub fn up(parts: &Parts, alias: &str) -> Result<(), Failure> {
    let id = agentspace_box::start_and_wait(parts.aws, parts.say, parts.clock, parts.cache)?;
    if !parts.ctx.nested {
        parts.reach.push(parts.ctx, parts.say, &id, alias);
    }
    Ok(())
}

/// `down` (operator.md, down): asks EC2 to stop the instance, whatever state it is in, and prints
/// `Stopping <id>.` once the request is accepted. It does not wait.
pub fn down(parts: &Parts) -> Result<(), Failure> {
    let id = agentspace_box::instance_id(parts.aws, parts.cache)?;
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

/// `status` (operator.md, status): five lines about the instance, to stderr, whatever `-Quiet`
/// says. It never starts the box, and prints nothing when a call fails.
pub fn status(parts: &Parts) -> Result<(), Failure> {
    let id = agentspace_box::instance_id(parts.aws, parts.cache)?;
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

/// `grow` (operator.md, grow): grows the root volume by `gib` GiB, and the root filesystem with
/// it, online when the instance is running.
pub fn grow(parts: &Parts, gib: i32) -> Result<(), Failure> {
    let id = agentspace_box::instance_id(parts.aws, parts.cache)?;
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
    if instance.state == "running" {
        parts.say.progress("Growing the filesystem online...", None);
        parts.clock.sleep(GROW_WAIT);
        let exit = parts.remote.ssh(
            &["-o", "ClearAllForwardings=yes", "agentspace"],
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
