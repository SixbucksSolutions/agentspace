//! `deploy` creating a stack on a platform (docs/operator.md: deploy, steps 1 to 4; Calls by
//! command; The platform seam; A host's platform and its names): the existence check, the image
//! read once before any call that changes anything, and the creation parameters, each one argument,
//! with the platform's own image and type last. It needs the CA check of step 3, so these runs use a
//! throwaway CA like `platform_signing.rs`'s. `ssh-keygen` has no seam variable, so these runs use
//! the real `SystemRoot` and a throwaway CA that the real `ca` makes in the rig's throwaway home:
//! nothing real is read, signed or reached. A test process that is elevated makes no CA (`ca`
//! refuses), and each test then says so and returns.
//!
//! The operator runs from a copy in the rig's own folder with a stand-in file named
//! `agentspace-ssh-proxy.exe` beside it, which is all the alias's ProxyCommand line needs to name.
//!
//! A case asserts the operator's own output and which host, record and platform each step was
//! given, never a Windows host's own steps, which are the Windows spec's.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use std::fs;
use std::path::PathBuf;

use rig::*;

const AMD64: &str = "linux/amd64";
/// The image the stand-in's `ssm get-parameter` answers with: made up, in the form the page gives.
const IMAGE: &str = "ami-0123456789abcdef0";
const CANONICAL: &str =
    "/aws/service/canonical/ubuntu/server/26.04/stable/current/amd64/hvm/ebs-gp3/ami-id";

struct Signing {
    rig: Rig,
    operator: PathBuf,
}

impl Signing {
    /// A rig holding `records`, a CA, and an operator with its proxy beside it. `None` when the
    /// test process is elevated.
    fn with(records: &[String]) -> Option<Signing> {
        let rig = Rig::standard();
        rig.write_hosts(records);
        let out = rig.op(&["ca"]).real_windows().run();
        if out.err().contains("elevated shell") {
            eprintln!("skipped: the test process is elevated, so ca refuses to create a CA");
            return None;
        }
        assert_eq!(out.code, Some(0), "ca: {:?}", out.err());
        let operator = rig.operator_with_proxy();
        Some(Signing { rig, operator })
    }

    fn run(&self, args: &[&str]) -> Out {
        self.rig.op(args).exe(&self.operator).real_windows().run()
    }

    /// The host's folder under `hosts\`, named by the host's name.
    fn dir(&self, host: &str) -> PathBuf {
        self.rig.host_dir(host)
    }

    /// The CA's public key as `deploy` passes it: the key type and the key, joined by one space.
    fn ca_key(&self) -> String {
        let text = fs::read_to_string(self.rig.state_dir().join("ca").join("agentspace_ca.pub"))
            .expect("the CA's public key");
        text.split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// What `deploy` needs before it creates a stack: the user's public key and the stack
    /// parameters file.
    fn deploy_inputs(&self) {
        fs::create_dir_all(self.rig.profile.join(".ssh")).expect("the .ssh folder");
        fs::write(
            self.rig.profile.join(".ssh").join("id_ed25519.pub"),
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMPLEEXAMP user@example\r\n",
        )
        .expect("the public key");
        fs::write(
            self.rig.state_dir().join("stack-parameters.json"),
            r#"{"version":0,"parameters":{"SubnetId":"SUBNET_VALUE","VpcId":"VPC_VALUE"}}"#,
        )
        .expect("the stack parameters");
    }
}

fn names(call: &Call, text: &str) -> bool {
    call.args.iter().any(|a| a == text)
}

// ---- deploy creating a stack ----

/// The `--parameter-overrides` of a recorded `cloudformation deploy`, each `Name=value` one
/// argument.
fn overrides(call: &Call) -> Vec<String> {
    call.args
        .iter()
        .skip_while(|a| *a != "--parameter-overrides")
        .skip(1)
        .take_while(|a| !a.starts_with("--"))
        .cloned()
        .collect()
}

/// A rig that creates the stack of `agentspace`'s `creating` host `example` on `platform`: its
/// stack does not exist, the image read answers [`IMAGE`], and `cloudformation deploy` fails, so
/// the run ends right after it.
fn creating(platform: &str) -> Option<Signing> {
    let s = Signing::with(&[
        box_rec(Some(ID)),
        host_rec_on("example", platform, "creating", None),
    ])?;
    s.deploy_inputs();
    s.rig.fail_nth(
        "cloudformation-describe-stacks",
        1,
        "An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id x does not exist",
        255,
    );
    s.rig.ok(
        "ssm-get-parameter",
        &format!("{{\"Parameter\":{{\"Value\":\"{IMAGE}\"}}}}"),
    );
    s.rig.answer(
        "cloudformation-deploy",
        None,
        None,
        Some("the stack failed\n"),
        Some(255),
    );
    Some(s)
}

#[test]
fn deploy_creating_a_linux_amd64_stack_reads_the_image_once_and_passes_it_with_the_platforms_type()
{
    let Some(s) = creating(AMD64) else { return };
    let out = s.run(&["deploy", "-Host", "example", "-Platform", AMD64]);
    assert_eq!(out.code, Some(1), "{:?}", out.err());
    assert_eq!(
        s.rig.keys(),
        [
            "cloudformation-describe-stacks",
            "ssm-get-parameter",
            "cloudformation-deploy"
        ],
        "the existence check, then the image read, before the call that changes anything"
    );
    let calls = s.rig.calls();
    assert!(
        names(&calls[0], "agentspace-linux-amd64-host-example"),
        "{:?}",
        calls[0].args
    );
    assert_eq!(
        calls[1].args,
        aws_args("ssm", "get-parameter", &["--name", CANONICAL]),
        "Canonical's public parameter for the Ubuntu 26.04 amd64 image"
    );
    assert!(
        names(&calls[2], "agentspace-linux-amd64-host-example"),
        "{:?}",
        calls[2].args
    );
    let given = overrides(&calls[2]);
    assert!(given[0].starts_with("SshPublicKey="), "{given:?}");
    assert_eq!(
        given[1..],
        [
            "SubnetId=SUBNET_VALUE".to_string(),
            "VpcId=VPC_VALUE".to_string(),
            "HostName=agentspace-linux-amd64-host-example".to_string(),
            format!("CaPublicKey={}", s.ca_key()),
            format!("ImageId={IMAGE}"),
            "InstanceType=m8a.xlarge".to_string(),
        ],
        "the image after the CA key, and the platform's type last"
    );
    // The image ID is in no line the operator prints.
    assert!(
        !out.err().contains(IMAGE) && !out.out().contains(IMAGE),
        "{:?}",
        out.err()
    );
    assert_eq!(
        out.err_lines().last().map(String::as_str),
        Some("agentspace: Stack deploy failed; see the stack events in the console.")
    );
    // The host's own folder holds its lock, and no other is made.
    assert!(
        s.dir("example.linux-amd64")
            .join("provision.lock")
            .is_file(),
        "{:?}",
        s.rig.tree(&s.rig.state_dir().join("hosts"))
    );
    assert!(!s.dir("example").exists());
}

#[test]
fn deploy_creating_a_linux_amd64_stack_with_an_instance_type_passes_that_type_last() {
    let Some(s) = creating(AMD64) else { return };
    let _ = s.run(&[
        "deploy",
        "-Host",
        "example",
        "-Platform",
        AMD64,
        "-InstanceType",
        "m8a.2xlarge",
    ]);
    let calls = s.rig.calls_of("cloudformation-deploy");
    assert_eq!(calls.len(), 1);
    let given = overrides(&calls[0]);
    assert_eq!(
        given[given.len() - 2..],
        [
            format!("ImageId={IMAGE}"),
            "InstanceType=m8a.2xlarge".to_string()
        ],
        "{given:?}"
    );
    assert!(
        !given.iter().any(|g| g == "InstanceType=m8a.xlarge"),
        "the platform's own type is not passed beside it: {given:?}"
    );
}

#[test]
fn deploy_creating_a_linux_arm64_stack_reads_no_image_and_passes_none() {
    let Some(s) = creating("linux/arm64") else {
        return;
    };
    let _ = s.run(&["deploy", "-Host", "example", "-Platform", "linux/arm64"]);
    assert_eq!(
        s.rig.keys(),
        ["cloudformation-describe-stacks", "cloudformation-deploy"],
        "no `ssm get-parameter`"
    );
    let calls = s.rig.calls();
    assert!(
        names(&calls[0], "agentspace-host-example"),
        "{:?}",
        calls[0].args
    );
    let given = overrides(&calls[1]);
    assert_eq!(
        given[1..],
        [
            "SubnetId=SUBNET_VALUE".to_string(),
            "VpcId=VPC_VALUE".to_string(),
            "HostName=agentspace-host-example".to_string(),
            format!("CaPublicKey={}", s.ca_key()),
        ],
        "a linux/arm64 stack's parameters are exactly as before v0.12: no image, no type"
    );
    assert!(
        s.dir("example").join("provision.lock").is_file(),
        "the linux/arm64 host's folder is its project's name"
    );
}

#[test]
fn a_failed_image_read_fails_deploy_before_the_call_that_changes_anything() {
    let Some(s) = creating(AMD64) else { return };
    s.rig.fail(
        "ssm-get-parameter",
        "An error occurred (AccessDeniedException) when calling the GetParameter operation: not allowed",
        254,
    );
    let out = s.run(&["deploy", "-Host", "example", "-Platform", AMD64]);
    assert_eq!(out.code, Some(1), "{:?}", out.err());
    assert_eq!(out.out(), "");
    assert_eq!(
        out.err_lines().last().map(String::as_str),
        Some(
            "agentspace: An error occurred (AccessDeniedException) when calling the GetParameter operation: not allowed"
        ),
        "the AWS CLI's own text, under AWS failures"
    );
    assert_eq!(
        s.rig.keys(),
        ["cloudformation-describe-stacks", "ssm-get-parameter"],
        "no `cloudformation deploy`"
    );
}

#[test]
fn an_image_read_with_no_value_or_a_malformed_one_fails_with_the_pages_message_and_none_of_the_answer()
 {
    for answer in [
        "{\"Parameter\":{}}".to_string(),
        "{\"Parameter\":{\"Value\":7}}".to_string(),
        "{\"Parameter\":{\"Value\":\"not-an-image-ID-secret\"}}".to_string(),
        "{\"Parameter\":{\"Value\":\"ami-0123456789abcdeXX\"}}".to_string(),
        "{}".to_string(),
    ] {
        let Some(s) = creating(AMD64) else { return };
        s.rig.ok("ssm-get-parameter", &answer);
        let out = s.run(&["deploy", "-Host", "example", "-Platform", AMD64]);
        assert_eq!(out.code, Some(1), "{answer}: {:?}", out.err());
        assert_eq!(
            out.err_lines().last().map(String::as_str),
            Some("agentspace: Unexpected answer from ssm get-parameter: no Parameter.Value."),
            "{answer}"
        );
        assert!(
            !out.err().contains("secret") && !out.err().contains("ami-0123456789abcdeXX"),
            "{answer}: the message holds no part of the answer: {:?}",
            out.err()
        );
        assert_eq!(
            s.rig.keys(),
            ["cloudformation-describe-stacks", "ssm-get-parameter"],
            "{answer}: before any call that changes anything"
        );
    }
}
