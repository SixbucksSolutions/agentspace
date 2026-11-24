//! `deploy`'s stack steps (docs/operator.md, deploy, steps 1 to 5; The stack; Calls by command):
//! the box's, as before v0.07 but for where its instance ID is kept, and a host's, with its
//! existence check, its creation parameters and its update with every previous value.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **The box's stack MUST be deployed with `SshPublicKey` and every member of
//!   `stack-parameters.json` but `InstanceType`, and never `HostName` or `CaPublicKey`**, so its
//!   change set stays empty (D44; P5). **No host is sized by that file** (D67, D71; pin P6): the
//!   file's `InstanceType` is never read, and `InstanceType=<type>` is passed, last, only when
//!   `-InstanceType` or a provisioning run's deploy step gives one (pin P32).
//! - **A host is never sized by the box's parameters** (P13; D55): a stack `deploy` creates takes
//!   only `SubnetId` and `VpcId` from the file, and a stack it updates is passed no parameter at
//!   all, so every one keeps its previous value, but `InstanceType` when a type is given.
//! - **`CaPublicKey` MUST carry the CA's public key without its comment** (P8): the comment holds
//!   the computer's name, and a stack parameter is kept and shown by CloudFormation.
//! - **The existence check's failures come before any call that changes anything.**
//! - **A deploy that passes `InstanceType` MUST remove the record's `instance_type` under
//!   `hosts.lock` before `cloudformation deploy` runs** (step 4; P11), so a killed or failed
//!   deploy leaves no type that the host might not have.
//! - **The instance ID and the instance type are removed from the host's record in one write
//!   before the deploy's output is read, then written again in one write from that one answer**
//!   (step 5; P11, P23, P30), each under `hosts.lock`. A read-back that fails or is malformed
//!   leaves no type; a stack with no usable `InstanceType` parameter fails the deploy (P22).
//!   `deploy` never sets a `state`.

use std::path::{Path, PathBuf};

use agentspace_ca::CaEnv;
use agentspace_hosts::{Host, Hosts, HostsLock};
use agentspace_operator_api::{Aws, AwsError, Call, Context, Failure, Output, Say};
use serde_json::Value;

use crate::files::{self, Places};
use crate::{Seams, params, read_public_key, registry, step_failed, text};

/// The template's region, named in step 4's line.
const REGION: &str = "us-west-2";

/// What the existence check found (operator.md, deploy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Existence {
    /// No stack, or `REVIEW_IN_PROGRESS`: create it with the creation parameters.
    Create,
    /// A state an update can start from: update it with every previous value.
    Update,
}

/// The home folder made full, as every path of the registry is built from it.
pub(crate) fn full_home(ctx: &Context) -> PathBuf {
    std::path::absolute(&ctx.home).unwrap_or_else(|_| ctx.home.clone())
}

/// Reads the host registry without its lock, failing with the registry's own message.
pub(crate) fn read_hosts(home: &Path) -> Result<Hosts, Failure> {
    registry::read(home)
}

/// Writes the registry under `held`, failing with `agentspace: Cannot write <path>.`
pub(crate) fn write_hosts(home: &Path, hosts: &Hosts, held: &HostsLock) -> Result<(), Failure> {
    agentspace_hosts::write(home, hosts, held)
        .map_err(|_| files::cannot_write(&agentspace_hosts::hosts_path(home)))
}

/// `agentspace: Project '<project>' has no host; provision -Host <project> makes one.`
pub(crate) fn no_host(project: &str) -> Failure {
    step_failed(&format!(
        "agentspace: Project '{project}' has no host; provision -Host {project} makes one."
    ))
}

/// Changes `project`'s record under `hosts.lock`, held from before the read the change starts
/// from until after the file is replaced. `change` returns whether it changed anything; nothing
/// is written when it did not. A project with no record fails with [`no_host`].
pub(crate) fn change_record(
    home: &Path,
    project: &str,
    change: impl FnOnce(&mut Host) -> bool,
) -> Result<(), Failure> {
    let held = registry::lock(home)?;
    let mut hosts = read_hosts(home)?;
    let record = hosts
        .hosts
        .iter_mut()
        .find(|h| h.project == project)
        .ok_or_else(|| no_host(project))?;
    if change(record) {
        write_hosts(home, &hosts, &held)?;
    }
    Ok(())
}

/// The box's steps 1 to 5 (operator.md, deploy): the public key, every member of the stack
/// parameters file but `InstanceType`, the removal of the record's type when `instance_type` is
/// given, the deploy of `agentbox` with `InstanceType=<type>` last, and the instance ID and type
/// written into the box's record. `instance_type` is `-InstanceType`'s value, or a provisioning
/// run's (D67, D71; pins P6 and P32). Returns the instance ID.
pub(crate) fn box_stack(
    ctx: &Context,
    instance_type: Option<&str>,
    say: &dyn Say,
    aws: &dyn Aws,
    seams: &Seams,
) -> Result<String, Failure> {
    let places = Places::new(ctx);
    let home = full_home(ctx);

    // Steps 1 and 2: the public key and the stack parameters, both before any AWS call. The
    // parameters come without the file's `InstanceType`, which no program reads (D67).
    let key = read_public_key(&places)?;
    let parameters = params::read(&places.stack_parameters())?;

    // Step 4's removal, then steps 3 and 4.
    let stack = agentspace_hosts::BOX_STACK;
    remove_type_before_deploy(&home, agentspace_hosts::BOX_PROJECT, instance_type)?;
    say.progress(&format!("Deploying stack '{stack}' in {REGION}..."), None);
    let mut overrides = vec![format!("SshPublicKey={key}")];
    overrides.extend(
        parameters
            .iter()
            .map(|(name, value)| format!("{name}={value}")),
    );
    // The override's place is last (pin P32).
    if let Some(t) = instance_type {
        overrides.push(format!("InstanceType={t}"));
    }
    run_deploy(&places, stack, Some(&overrides), aws, seams)?;

    // Step 5.
    refresh_instance(ctx, agentspace_hosts::BOX_PROJECT, stack, say, aws)
}

/// Step 4's first act: with an `InstanceType` to pass, the record's `instance_type` is removed
/// under `hosts.lock`, whatever it held, before `cloudformation deploy` and before any AWS call
/// that changes anything (D67's change note; pin P11). A record that holds none is left as it is,
/// and a missing registry is written as the seed document. **Without a type nothing is written**,
/// so a person's `deploy` that asks for no change keeps the type until its own read-back (pin
/// P30). A lock not taken, or a write that fails, fails the step with the registry's message,
/// before `cloudformation deploy`.
fn remove_type_before_deploy(
    home: &Path,
    project: &str,
    instance_type: Option<&str>,
) -> Result<(), Failure> {
    if instance_type.is_some() {
        registry::set_instance_type(home, project, None)?;
    }
    Ok(())
}

/// A host's existence check and steps 1 to 5 (operator.md, deploy), for a host that is not the
/// box. `instance_type` is `-InstanceType`'s value, or a provisioning run's. Returns the instance
/// ID.
pub(crate) fn host_stack(
    ctx: &Context,
    ca_env: &CaEnv,
    project: &str,
    instance_type: Option<&str>,
    say: &dyn Say,
    aws: &dyn Aws,
    seams: &Seams,
) -> Result<String, Failure> {
    let places = Places::new(ctx);
    let home = full_home(ctx);
    let hosts = read_hosts(&home)?;
    let record = agentspace_hosts::host_for(&hosts, project).ok_or_else(|| no_host(project))?;
    let stack = record.stack.clone();

    let overrides = match existence(aws, &stack, project)? {
        Existence::Create => {
            // Steps 1 and 2.
            let key = read_public_key(&places)?;
            let path = places.stack_parameters();
            let parameters = params::read(&path)?;
            let find = |name: &str| {
                parameters
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, v)| v.clone())
            };
            let (Some(subnet), Some(vpc)) = (find("SubnetId"), find("VpcId")) else {
                return Err(step_failed(&format!(
                    "agentspace: {} MUST name SubnetId and VpcId for a new host's stack.",
                    path.display()
                )));
            };
            // Step 3: the CA check, then its public key without its comment.
            (seams.ca_check)(ctx, ca_env)?;
            let ca_key = ca_key_for_stack(&places.ca_public_key())?;
            let mut overrides = vec![
                format!("SshPublicKey={key}"),
                format!("SubnetId={subnet}"),
                format!("VpcId={vpc}"),
                format!("HostName={stack}"),
                format!("CaPublicKey={ca_key}"),
            ];
            if let Some(t) = instance_type {
                overrides.push(format!("InstanceType={t}"));
            }
            Some(overrides)
        }
        Existence::Update => instance_type.map(|t| vec![format!("InstanceType={t}")]),
    };

    // Step 4: the type's removal, then the deploy.
    remove_type_before_deploy(&home, project, instance_type)?;
    say.progress(&format!("Deploying stack '{stack}' in {REGION}..."), None);
    run_deploy(&places, &stack, overrides.as_deref(), aws, seams)?;

    // Step 5.
    refresh_instance(ctx, project, &stack, say, aws)
}

/// The existence check: `cloudformation describe-stacks --stack-name <stack>`, read for
/// `Stacks[0].StackStatus` alone (operator.md, deploy; P17 and P19).
pub(crate) fn existence(aws: &dyn Aws, stack: &str, project: &str) -> Result<Existence, Failure> {
    let answer = match aws.call(&Call {
        service: "cloudformation",
        operation: "describe-stacks",
        args: &["--stack-name", stack],
        output: Output::Json,
    }) {
        Ok(answer) => answer,
        Err(AwsError::Failed(text)) if text.to_lowercase().contains("does not exist") => {
            return Ok(Existence::Create);
        }
        Err(e) => return Err(Failure::from(e)),
    };
    let Some(status) = answer
        .pointer("/Stacks/0/StackStatus")
        .and_then(Value::as_str)
    else {
        return Err(step_failed(
            "agentspace: Unexpected answer from cloudformation describe-stacks: no Stacks[0].StackStatus.",
        ));
    };
    decide(status, stack, project)
}

/// What a `StackStatus` makes `deploy` do (operator.md, deploy).
pub(crate) fn decide(status: &str, stack: &str, project: &str) -> Result<Existence, Failure> {
    match status {
        "REVIEW_IN_PROGRESS" => Ok(Existence::Create),
        "CREATE_COMPLETE"
        | "UPDATE_COMPLETE"
        | "UPDATE_ROLLBACK_COMPLETE"
        | "IMPORT_COMPLETE"
        | "IMPORT_ROLLBACK_COMPLETE" => Ok(Existence::Update),
        s if s.ends_with("_IN_PROGRESS") => Err(step_failed(&format!(
            "agentspace: Stack '{stack}' is {s}; try again once that operation has finished."
        ))),
        "ROLLBACK_COMPLETE" => Err(step_failed(&format!(
            "agentspace: Stack '{stack}' is ROLLBACK_COMPLETE, which no update can start from. Look at its events, delete it with 'aws cloudformation delete-stack --region us-west-2 --stack-name {stack}', then run 'agentspace-operator provision -Host {project}' again."
        ))),
        s => Err(step_failed(&format!(
            "agentspace: Stack '{stack}' is {}, which no update can start from. Look at its events; it can still hold the host's instance and disk, so do not delete it unless nothing on the host is to be kept.",
            printable(s)
        ))),
    }
}

/// A status as it is shown in a message: each control character written `?`, so the message
/// stays one line.
fn printable(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// The CA's public key as the stack's `CaPublicKey` takes it (P8; state.md, The CA): the line
/// trimmed of ASCII whitespace, up to its first line break, its first two words split at runs of
/// spaces and tabs, joined by one space, with no comment and no line ending.
pub(crate) fn ca_key_for_stack(path: &Path) -> Result<String, Failure> {
    let cannot_read = || step_failed(&format!("agentspace: Cannot read {}.", path.display()));
    let bytes = std::fs::read(path).map_err(|_| cannot_read())?;
    let text = std::str::from_utf8(&bytes).map_err(|_| cannot_read())?;
    ca_key_from_text(text).ok_or_else(cannot_read)
}

fn ca_key_from_text(text: &str) -> Option<String> {
    let trimmed = text.trim_matches(|c: char| c.is_ascii_whitespace() || c == '\u{b}');
    let line = trimmed
        .split(['\n', '\r', '\u{b}', '\u{c}'])
        .next()
        .unwrap_or_default();
    let mut words = line.split([' ', '\t']).filter(|w| !w.is_empty());
    let (kind, key) = (words.next()?, words.next()?);
    Some(format!("{kind} {key}"))
}

/// Step 4's `cloudformation deploy`: the template written into the command's folder just before
/// the call, its output passed through, and its error text scanned for the sign-in test.
/// `overrides` is `None` for no `--parameter-overrides` at all.
fn run_deploy(
    places: &Places,
    stack: &str,
    overrides: Option<&[String]>,
    aws: &dyn Aws,
    seams: &Seams,
) -> Result<(), Failure> {
    let folder = seams.folder.path(places)?;
    let template = text(&files::write_template(places, &folder)?);
    let tag = format!("project={stack}");
    let mut args: Vec<&str> = vec![
        "--stack-name",
        stack,
        "--template-file",
        &template,
        "--capabilities",
        "CAPABILITY_IAM",
        "--no-fail-on-empty-changeset",
        "--tags",
        &tag,
    ];
    if let Some(overrides) = overrides {
        args.push("--parameter-overrides");
        args.extend(overrides.iter().map(String::as_str));
    }
    aws.call(&Call {
        service: "cloudformation",
        operation: "deploy",
        args: &args,
        output: Output::PassThrough,
    })
    .map(|_| ())
    .map_err(|e| match e {
        AwsError::Failed(_) => {
            step_failed("agentspace: Stack deploy failed; see the stack events in the console.")
        }
        // #0034's message, or the whole message for a program that cannot be run.
        other => Failure::from(other),
    })
}

/// Step 5 (operator.md, deploy; docs/state.md, The instance type; pins P11, P22, P23 and P30): the
/// instance ID **and the instance type** removed from the record in one write, then one
/// `describe-stacks` read for both, then both written in one write, each under `hosts.lock`; then
/// `Instance: <id>`.
///
/// - **`InstanceId` is checked first**: a stack with no usable `InstanceId` output fails with its
///   message and writes nothing, so the record holds neither.
/// - **A missing, repeated, non-string or malformed `InstanceType` parameter** fails the deploy
///   with `agentspace: Stack '<stack>' has no InstanceType parameter.` after the instance ID is
///   written alone, so the record is left with no type.
/// - A failed call, lock or write leaves no instance ID and no type in the record.
fn refresh_instance(
    ctx: &Context,
    project: &str,
    stack: &str,
    say: &dyn Say,
    aws: &dyn Aws,
) -> Result<String, Failure> {
    let home = full_home(ctx);
    registry::set_instance(&home, project, None, None)?;
    let answer = aws
        .call(&Call {
            service: "cloudformation",
            operation: "describe-stacks",
            args: &["--stack-name", stack],
            output: Output::Json,
        })
        .map_err(Failure::from)?;
    // The one answer is read for both, through the same reading of `InstanceId` as every command's.
    let instance_id = agentspace_box::instance_id_from_stack(&Answer(&answer), stack)?;
    let Some(instance_type) = instance_type_parameter(&answer) else {
        registry::set_instance(&home, project, Some(&instance_id), None)?;
        return Err(step_failed(&format!(
            "agentspace: Stack '{stack}' has no InstanceType parameter."
        )));
    };
    registry::set_instance(&home, project, Some(&instance_id), Some(&instance_type))?;
    say.progress(&format!("Instance: {instance_id}"), None);
    Ok(instance_id)
}

/// An [`Aws`] that answers every call with one answer already read, so the instance ID is read from
/// the answer `deploy` also reads the type from, with no second call.
struct Answer<'a>(&'a Value);

impl Aws for Answer<'_> {
    fn call(&self, _: &Call) -> Result<Value, AwsError> {
        Ok(self.0.clone())
    }
}

/// The value of `Stacks[0].Parameters`' one element whose `ParameterKey` is `InstanceType`, when
/// there is exactly one, its `ParameterValue` is a string, and the string is of the form the host
/// registry gives `instance_type` (pin P22); `None` otherwise.
fn instance_type_parameter(answer: &Value) -> Option<String> {
    let parameters = answer.pointer("/Stacks/0/Parameters")?.as_array()?;
    let mut named = parameters
        .iter()
        .filter(|p| p.get("ParameterKey").and_then(Value::as_str) == Some("InstanceType"));
    let (first, second) = (named.next()?, named.next());
    if second.is_some() {
        return None;
    }
    let value = first.get("ParameterValue")?.as_str()?;
    is_instance_type(value).then(|| value.to_string())
}

/// Whether `text` is of the host registry's `instance_type` form (docs/state.md, The host
/// registry): `^[a-z0-9][a-z0-9-]*\.[a-z0-9][a-z0-9-]*$`, at most 64 characters, with no line
/// break after it.
pub(crate) fn is_instance_type(text: &str) -> bool {
    let part = |s: &str| {
        let mut bytes = s.bytes();
        bytes
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    text.len() <= 64
        && text
            .split_once('.')
            .is_some_and(|(family, size)| part(family) && part(size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_status_takes_its_path() {
        let s = "agentspace-host-example";
        assert_eq!(
            decide("REVIEW_IN_PROGRESS", s, "example"),
            Ok(Existence::Create)
        );
        for update in [
            "CREATE_COMPLETE",
            "UPDATE_COMPLETE",
            "UPDATE_ROLLBACK_COMPLETE",
            "IMPORT_COMPLETE",
            "IMPORT_ROLLBACK_COMPLETE",
        ] {
            assert_eq!(
                decide(update, s, "example"),
                Ok(Existence::Update),
                "{update}"
            );
        }
        for busy in ["CREATE_IN_PROGRESS", "UPDATE_ROLLBACK_IN_PROGRESS"] {
            assert_eq!(
                decide(busy, s, "example").unwrap_err().message,
                format!(
                    "agentspace: Stack '{s}' is {busy}; try again once that operation has finished."
                )
            );
        }
        let m = decide("ROLLBACK_COMPLETE", s, "example")
            .unwrap_err()
            .message;
        assert!(m.contains("delete-stack --region us-west-2 --stack-name agentspace-host-example"));
        assert!(m.ends_with("'agentspace-operator provision -Host example' again."));
        let m = decide("UPDATE_ROLLBACK_FAILED", s, "example")
            .unwrap_err()
            .message;
        assert!(m.starts_with(
            "agentspace: Stack 'agentspace-host-example' is UPDATE_ROLLBACK_FAILED, which no update"
        ));
        assert!(!m.contains("delete-stack"));
    }

    #[test]
    fn the_ca_key_drops_its_comment_and_line_ending() {
        assert_eq!(
            ca_key_from_text("ssh-ed25519 AAAAC3Nz agentspace-ca NAME 2026-11-20\r\n").unwrap(),
            "ssh-ed25519 AAAAC3Nz"
        );
        assert_eq!(
            ca_key_from_text(" \tssh-ed25519\t  AAAAC3Nz\n").unwrap(),
            "ssh-ed25519 AAAAC3Nz"
        );
        assert_eq!(ca_key_from_text("ssh-ed25519\nAAAAC3Nz"), None);
        assert_eq!(ca_key_from_text(""), None);
    }
}
