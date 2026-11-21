//! What the tests of a host's deploy, sync, bootstrap and provision share (E0.07-T03-S03, #0152).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Written from docs/operator.md, deploy
//! (the existence check, steps 1 to 5, and the AWS calls of Calls by command), The operator's
//! folder, one per command, and Which host a command acts on.
#![cfg(windows)]
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use crate::common::expect::command_folder_of;
use crate::common::hosts::{PROJECT, Seed, alias_of, stack_of};
use crate::common::{ID, ID2, Log, Rig, repo_root, svec};

/// A throwaway home with a CA, a public key, a parameters file naming `SubnetId`, `VpcId` and two
/// members the box's size alone is set by, a registry holding the box and a `creating` record of
/// [`PROJECT`], and (when `beside`) a program folder with the ProxyCommand program in it.
pub fn host_rig(beside: bool) -> Rig {
    let rig = Rig::new(false);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.write_home(
        ".agentspace/stack-parameters.json",
        br#"{"version":0,"parameters":{"SubnetId":"SUBNET_VALUE","VpcId":"VPC_VALUE","InstanceType":"m9g.8xlarge","RootVolumeGiB":"40"}}"#,
    );
    if beside {
        rig.beside_operator(true);
    }
    rig.seed_hosts(&[Seed::new(PROJECT, "creating")]);
    rig
}

/// A throwaway home for a host's `sync` or `bootstrap`: a CA, a program folder with the
/// ProxyCommand program in it, and a registry holding the box and a `ready` record of [`PROJECT`]
/// whose instance ID is `ID2`, which is not the one the retired cache holds. `go` says whether the
/// rig's `go` finder finds the stand-in.
pub fn sync_rig(go: bool) -> Rig {
    let rig = Rig::new(go);
    rig.make_ca();
    rig.beside_operator(true);
    rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID2)]);
    rig
}

/// What the registry says of [`PROJECT`] when a provisioning run begins.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// No record, and the project is on the project list.
    NoRecord,
    /// A `creating` record, left by a run that died.
    Creating,
    /// A `failed` record.
    Failed,
    /// A `ready` record, with an old instance ID and an old attempt.
    Ready,
}

/// The `last_attempt_at` a seeded record of [`Kind::Ready`] holds.
pub const OLD_ATTEMPT: &str = "2026-11-19T00:00:00Z";

/// The lines the stand-ins print for a provisioning run, so a test finds where a program's output
/// sits in the log among the run's own lines.
pub const DEPLOY_OUT: &str = "DEPLOY-OUT-LINE";
pub const INSTALL_OUT: &str = "INSTALL-OUT-LINE";
pub const BOOTSTRAP_OUT: &str = "BOOTSTRAP-OUT-LINE";

/// A throwaway home for a provisioning run of [`PROJECT`], as `kind` says. `inject` adds the rules
/// that make a step fail, and runs first, so its rules are tried before the defaults'. A host
/// that was never `ready` has no stack yet, and a `ready` one has its stack. Every stand-in
/// watches `hosts.json` and probes the run's lock and the registry's, the ssh stand-in keeps its
/// standard input, and `go` is not found, so the localswim CLI's build is skipped.
pub fn prov_rig_with(kind: Kind, inject: impl FnOnce(&Rig)) -> Rig {
    let rig = Rig::new(false);
    inject(&rig);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.write_home(
        ".agentspace/stack-parameters.json",
        br#"{"version":0,"parameters":{"SubnetId":"SUBNET_VALUE","VpcId":"VPC_VALUE","InstanceType":"m9g.8xlarge"}}"#,
    );
    rig.beside_operator(true);
    rig.list_projects(&[PROJECT]);
    rig.capture_ssh_stdin();
    let stack = stack_of(PROJECT);
    match kind {
        Kind::NoRecord => rig.stack_missing(&stack),
        Kind::Creating => {
            rig.seed_hosts(&[Seed::new(PROJECT, "creating")]);
            rig.stack_missing(&stack);
        }
        Kind::Failed => {
            rig.seed_hosts(&[Seed::new(PROJECT, "failed")]);
            rig.stack_missing(&stack);
        }
        Kind::Ready => {
            rig.seed_hosts(&[Seed::new(PROJECT, "ready").id(ID).attempt(OLD_ATTEMPT)]);
            rig.stack_status("CREATE_COMPLETE");
        }
    }
    rig.rule(
        "aws",
        0,
        "cloudformation deploy",
        0,
        &format!("{DEPLOY_OUT}\\n"),
        "",
    );
    rig.rule("ssh", 0, "install.sh", 0, &format!("{INSTALL_OUT}\\n"), "");
    rig.rule("ssh", 0, "bash", 0, &format!("{BOOTSTRAP_OUT}\\n"), "");
    watch_registry(&rig);
    rig.probe_lock(agentspace_hosts::host_run_lock_path(&rig.home, PROJECT));
    rig.probe_lock(agentspace_hosts::hosts_lock_path(&rig.home));
    rig
}

/// [`prov_rig_with`] with nothing made to fail.
pub fn prov_rig(kind: Kind) -> Rig {
    prov_rig_with(kind, |_| {})
}

/// The record of `project` in a watched registry's content: from its `{"project":"<project>"` to
/// the first `}` after it, or an empty string when there is none.
pub fn record_in(content: &str, project: &str) -> String {
    let start = format!("{{\"project\":\"{project}\"");
    match content.find(&start) {
        Some(at) => {
            let rest = &content[at..];
            let end = rest.find('}').map_or(rest.len(), |e| e + 1);
            rest[..end].to_string()
        }
        None => String::new(),
    }
}

/// The value of a record's `last_attempt_at`, when it has one.
pub fn attempt_in(record: &str) -> Option<String> {
    let key = "\"last_attempt_at\":\"";
    let at = record.find(key)? + key.len();
    record.get(at..at + 20).map(str::to_string)
}

/// Whether a record's text says `state`.
pub fn state_is(record: &str, state: &str) -> bool {
    record.contains(&format!("\"state\":\"{state}\""))
}

/// The line a run ends with for a host that was not `ready` when it began (operator.md, provision,
/// step 10).
pub fn sign_in_line(project: &str) -> String {
    format!(
        "Sign in on this host once, by hand: run 'agentspace-operator status -Host {project}' first and again after an hour, then see \"Signing in on a new host\" in the agentspace README."
    )
}

/// The CA's public key as `CaPublicKey` carries it: the key type and the base64 key, joined by
/// one space, with no comment and no line ending (operator.md, deploy, step 3).
pub fn ca_key(rig: &Rig) -> String {
    let path = rig
        .home
        .join(".agentspace")
        .join("ca")
        .join("agentspace_ca.pub");
    let text = fs::read_to_string(path).expect("The throwaway CA's public key is there");
    let mut words = text.split_whitespace();
    let kind = words.next().expect("A public key has a type");
    let key = words.next().expect("A public key has a key");
    format!("{kind} {key}")
}

/// The AWS CLI's `cloudformation deploy` arguments for `stack` and `template`, then
/// `--parameter-overrides` and `overrides` when there are any (operator.md, Calls by command).
pub fn deploy_call_args(stack: &str, template: &str, overrides: &[String]) -> Vec<String> {
    let mut v = svec(&[
        "cloudformation",
        "deploy",
        "--region",
        "us-west-2",
        "--stack-name",
        stack,
        "--template-file",
    ]);
    v.push(template.to_string());
    v.extend(svec(&[
        "--capabilities",
        "CAPABILITY_IAM",
        "--no-fail-on-empty-changeset",
        "--tags",
    ]));
    v.push(format!("project={stack}"));
    if !overrides.is_empty() {
        v.push("--parameter-overrides".to_string());
        v.extend(overrides.iter().cloned());
    }
    v
}

/// The overrides of a stack `deploy` creates: the key, `SubnetId` and `VpcId` of the file,
/// `HostName` and `CaPublicKey`, in that order, and no other member of the file.
pub fn creation_overrides(rig: &Rig, stack: &str) -> Vec<String> {
    vec![
        format!("SshPublicKey={}", crate::common::KEY_LINE),
        "SubnetId=SUBNET_VALUE".to_string(),
        "VpcId=VPC_VALUE".to_string(),
        format!("HostName={stack}"),
        format!("CaPublicKey={}", ca_key(rig)),
    ]
}

/// The `describe-stacks` call of the existence check and of the instance ID's read.
pub fn describe_stacks_args(stack: &str) -> Vec<String> {
    svec(&[
        "cloudformation",
        "describe-stacks",
        "--region",
        "us-west-2",
        "--output",
        "json",
        "--stack-name",
        stack,
    ])
}

/// The argument after `--template-file` in a `cloudformation deploy` call.
pub fn template_arg(args: &[String]) -> String {
    match args.iter().position(|a| a == "--template-file") {
        Some(i) if i + 1 < args.len() => args[i + 1].clone(),
        _ => panic!("The cloudformation deploy call names no template file"),
    }
}

/// Checks that the template the `n`th AWS call names is `agentspace.yaml` in a folder of the
/// command's own, `operator\<hex>\`, that it was there, byte for byte the repository's, when the
/// call started, and returns the folder's `<hex>`.
pub fn check_template(rig: &Rig, log: &Log, n: u32, template: &str) -> String {
    let want = rig.home.join(".agentspace").join("operator");
    let hex = command_folder_of(template);
    rig.check(
        hex.is_some(),
        &format!(
            "The template MUST be in the command's own folder `operator\\<hex>\\`: {template}"
        ),
    );
    let hex = hex.unwrap_or_default();
    rig.same_path(
        "The template file named to the AWS CLI",
        template,
        &want.join(&hex).join("agentspace.yaml"),
    );
    let bytes = fs::read(repo_root().join("infra").join("agentspace.yaml")).unwrap();
    let snap = log.file_snap("aws", n);
    rig.check(
        snap.len() == 1
            && snap[0].1 == bytes.len() as u64
            && snap[0].2 == crate::common::fnv(&bytes),
        "The template MUST exist, byte for byte the repository's infra/agentspace.yaml, when the deploy call starts",
    );
    hex
}

/// Checks that `got`, a message, is `want` as a message that names a path writes it: the same
/// text, with `/` and `\` taken as one and the case of the path ignored.
pub fn same_message(rig: &Rig, what: &str, got: &str, want: &str) {
    rig.check(
        crate::common::norm(got) == crate::common::norm(want),
        &format!("{what}: expected\n  {want}\nbut got\n  {got}"),
    );
}

/// The folder the registry's record of `project` is watched through: `hosts.json` itself.
pub fn watch_registry(rig: &Rig) {
    rig.watch_file(rig.hosts_json_path());
}

/// The instance the stack's `InstanceId` output names in these tests.
pub const STACK_INSTANCE: &str = ID2;

/// The path of the host's alias, for a message that names it.
pub fn alias_path(rig: &Rig, project: &str) -> PathBuf {
    rig.alias_file(project)
}

/// The names a host's tests use together.
pub fn names() -> (String, String) {
    (stack_of(PROJECT), alias_of(PROJECT))
}
