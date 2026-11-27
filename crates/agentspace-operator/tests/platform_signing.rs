//! A project's hosts on three platforms, each reached by its own `-Platform` with its own record,
//! folder, key, certificate, alias, push stamp and stack name (docs/operator.md: How the operator
//! reaches a host; A host's platform and its names; deploy, step 3; The platform seam; docs/state.md:
//! A host's folder, A host's operator alias). `ssh-keygen` has no seam variable, so these runs use
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
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rig::*;

const ZONE: &str = "America/New_York";
const AMD64: &str = "linux/amd64";
const WINDOWS: &str = "windows/amd64";
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

    /// The box and one host of `agentspace` on each of the three platforms.
    fn three_platforms() -> Option<Signing> {
        Signing::with(&[
            box_rec(Some(ID)),
            host_rec_on("agentspace", "linux/arm64", "ready", Some(OTHER_ID)),
            host_rec_on("agentspace", AMD64, "ready", Some(THIRD_ID)),
            host_rec_on("agentspace", WINDOWS, "ready", Some(WINDOWS_ID)),
        ])
    }

    fn run(&self, args: &[&str]) -> Out {
        self.rig.op(args).exe(&self.operator).real_windows().run()
    }

    /// The host's folder under `hosts\`, named by the host's name.
    fn dir(&self, host: &str) -> PathBuf {
        self.rig.host_dir(host)
    }

    fn alias(&self, host: &str) -> PathBuf {
        self.rig.host_alias_path(host)
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

fn ssh_keygen() -> PathBuf {
    Path::new(&std::env::var_os("SystemRoot").expect("SystemRoot"))
        .join("System32")
        .join("OpenSSH")
        .join("ssh-keygen.exe")
}

fn keygen_text(args: &[&Path]) -> String {
    let out = Command::new(ssh_keygen())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("ssh-keygen runs");
    assert!(out.status.success(), "ssh-keygen {args:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn status_lines(id: &str) -> String {
    format!(
        "Instance  {id}\nState     running\nType      m9g.xlarge\nDisk      8 GiB gp3 ({VOL})\nSSM       online\n"
    )
}

/// The principals `ssh-keygen -L` lists for a certificate.
fn principals(listing: &str) -> Vec<&str> {
    listing
        .lines()
        .skip_while(|l| !l.trim_start().starts_with("Principals:"))
        .skip(1)
        .take_while(|l| {
            let t = l.trim();
            !t.is_empty() && !t.contains(':')
        })
        .map(str::trim)
        .collect()
}

fn names(call: &Call, text: &str) -> bool {
    call.args.iter().any(|a| a == text)
}

// ---- Three hosts of one project, each by its own -Platform ----

#[test]
fn each_platforms_host_has_its_own_key_certificate_alias_and_folder() {
    let Some(s) = Signing::three_platforms() else {
        return;
    };
    // (the `-Platform` argument, the host's name, the instance, the login user, the report's
    // platform line)
    let hosts: [(Option<&str>, &str, &str, &str, &str); 3] = [
        (None, "agentspace", OTHER_ID, "ubuntu", ""),
        (
            Some(AMD64),
            "agentspace.linux-amd64",
            THIRD_ID,
            "ubuntu",
            "Platform  linux/amd64\n",
        ),
        (
            Some(WINDOWS),
            "agentspace.windows-amd64",
            WINDOWS_ID,
            "agent",
            "Platform  windows/amd64\n",
        ),
    ];
    for (platform, host, id, user, platform_line) in hosts {
        let mut args = vec!["status", "-Host", "agentspace"];
        if let Some(p) = platform {
            args.extend(["-Platform", p]);
        }
        let before = s.rig.calls().len();
        let out = s.run(&args);
        assert_eq!(out.code, Some(0), "{host}: {:?}", out.err());
        assert_eq!(
            out.err(),
            format!("{platform_line}{}", status_lines(id)),
            "{host}: its own instance's report"
        );
        let calls = s.rig.calls();
        assert_eq!(
            calls[before].args,
            aws_args("ec2", "describe-instances", &["--instance-ids", id]),
            "{host}"
        );

        // The folder named by the host's name holds the key, its public half and the certificate.
        let dir = s.dir(host);
        let mut names_in_dir: Vec<String> = fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{host}: its folder: {e}"))
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names_in_dir.sort();
        assert_eq!(
            names_in_dir,
            ["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"],
            "{host}"
        );
        // The certificate: its own key ID, and the login user of its platform as the principal.
        let listing = keygen_text(&[
            Path::new("-L"),
            Path::new("-f"),
            &dir.join("id_ed25519-cert.pub"),
        ]);
        assert!(
            listing.contains(&format!("Key ID: \"operator-{host}\"")),
            "{host}: {listing}"
        );
        assert_eq!(principals(&listing), [user], "{host}: {listing}");

        // The alias: its own file, `Host` line, `HostKeyAlias`, owner mark, user and key paths.
        let text = fs::read_to_string(s.alias(host)).unwrap_or_else(|e| panic!("{host}: {e}"));
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            lines[0].starts_with(&format!("# agentspace host {host}; ")),
            "{host}: the owner mark names the host's name: {:?}",
            lines[0]
        );
        assert_eq!(lines[1], format!("Host agentspace-host.{host}"), "{host}");
        for wanted in [
            format!("  User {user}"),
            format!("  HostKeyAlias agentspace-host.{host}"),
            "  IdentitiesOnly yes".to_string(),
            "  StrictHostKeyChecking accept-new".to_string(),
        ] {
            assert!(
                lines.contains(&wanted.as_str()),
                "{host}: {wanted:?} in {text:?}"
            );
        }
        let identity = lines
            .iter()
            .find(|l| l.starts_with("  IdentityFile \""))
            .unwrap_or_else(|| panic!("{host}: an IdentityFile in {text:?}"));
        assert!(
            identity.ends_with(&format!("hosts/{host}/id_ed25519\"")),
            "{host}: {identity:?}"
        );
        let proxy = lines
            .iter()
            .find(|l| l.starts_with("  ProxyCommand "))
            .unwrap_or_else(|| panic!("{host}: a ProxyCommand in {text:?}"));
        assert!(
            proxy.ends_with(&format!(" agentspace-host.{host}")),
            "{host}: the proxy is given this host's alias: {proxy:?}"
        );
    }

    // Three hosts, three of each: no two share a key, a certificate, an alias or a folder.
    let keys: Vec<Vec<u8>> = [
        "agentspace",
        "agentspace.linux-amd64",
        "agentspace.windows-amd64",
    ]
    .iter()
    .map(|h| fs::read(s.dir(h).join("id_ed25519")).expect("a key"))
    .collect();
    assert!(
        keys[0] != keys[1] && keys[1] != keys[2] && keys[0] != keys[2],
        "each host has a key of its own"
    );
    let mut aliases: Vec<String> = fs::read_dir(s.alias("agentspace").parent().unwrap())
        .expect("the alias folder")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    aliases.sort();
    assert_eq!(
        aliases,
        [
            "agentspace-host.agentspace",
            "agentspace-host.agentspace.linux-amd64",
            "agentspace-host.agentspace.windows-amd64"
        ]
    );
}

#[test]
fn a_command_for_one_platforms_host_leaves_the_other_hosts_files_as_they_were() {
    let Some(s) = Signing::three_platforms() else {
        return;
    };
    assert_eq!(s.run(&["status", "-Host", "agentspace"]).code, Some(0));
    let arm_key = fs::read(s.dir("agentspace").join("id_ed25519")).unwrap();
    let arm_cert = fs::read(s.dir("agentspace").join("id_ed25519-cert.pub")).unwrap();
    let arm_alias = fs::read(s.alias("agentspace")).unwrap();
    // A certificate's times are whole seconds, so wait for the clock to move on.
    std::thread::sleep(std::time::Duration::from_millis(2100));
    let out = s.run(&["status", "-Host", "agentspace", "-Platform", AMD64]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(
        fs::read(s.dir("agentspace").join("id_ed25519")).unwrap(),
        arm_key
    );
    assert_eq!(
        fs::read(s.dir("agentspace").join("id_ed25519-cert.pub")).unwrap(),
        arm_cert,
        "the arm64 host's certificate is not signed again by a command for the amd64 host"
    );
    assert_eq!(fs::read(s.alias("agentspace")).unwrap(), arm_alias);
    assert!(
        !s.dir("agentspace.windows-amd64").exists()
            && !s.alias("agentspace.windows-amd64").exists(),
        "the windows host's files are made by a command for it alone"
    );
}

#[test]
fn every_command_given_a_hosts_platform_signs_for_that_host_and_never_all() {
    let Some(s) = Signing::three_platforms() else {
        return;
    };
    s.rig.instance("stopped");
    for line in [
        vec!["down", "-Host", "agentspace", "-Platform", AMD64],
        vec!["status", "-Host", "agentspace", "-Platform", AMD64],
        vec!["up", "-Host", "agentspace", "-Platform", AMD64],
        vec!["grow", "-Host", "agentspace", "-Platform", AMD64],
    ] {
        let _ = fs::remove_dir_all(s.rig.state_dir().join("hosts"));
        let _ = fs::remove_dir_all(s.rig.profile.join(".ssh"));
        let out = s.run(&line);
        assert_eq!(out.code, Some(0), "{line:?}: {:?}", out.err());
        let dir = s.dir("agentspace.linux-amd64");
        assert!(dir.join("id_ed25519").is_file(), "{line:?}: the key");
        assert!(
            dir.join("id_ed25519-cert.pub").is_file(),
            "{line:?}: the certificate"
        );
        assert!(
            s.alias("agentspace.linux-amd64").is_file(),
            "{line:?}: the alias"
        );
        assert!(
            !s.dir("agentspace").exists(),
            "{line:?}: no other host's folder"
        );
        assert!(
            !s.alias("agentspace").exists(),
            "{line:?}: no other host's alias"
        );
    }
    for line in [vec!["status", "-All"], vec!["down", "-All"]] {
        let _ = fs::remove_dir_all(s.rig.state_dir().join("hosts"));
        let _ = fs::remove_dir_all(s.rig.profile.join(".ssh"));
        let out = s.run(&line);
        assert_eq!(out.code, Some(0), "{line:?}: {:?}", out.err());
        assert!(
            !s.rig.state_dir().join("hosts").exists()
                && !s.rig.profile.join(".ssh").join("agentspace.d").exists(),
            "{line:?}: -All signs for no host, a platform's included"
        );
    }
}

#[test]
fn an_alias_file_that_is_another_hosts_is_left_alone_and_named_by_the_hosts_name() {
    // The amd64 host's alias path holds the arm64 host's owner mark: another host's, so the command
    // fails and changes nothing. The message names the host by its host name (pins P12 and P44).
    let Some(s) = Signing::three_platforms() else {
        return;
    };
    let path = s.alias("agentspace.linux-amd64");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let theirs = "# agentspace host agentspace; written by agentspace\nHost x\n  HostName y\n";
    fs::write(&path, theirs).unwrap();
    let out = s.run(&["status", "-Host", "agentspace", "-Platform", AMD64]);
    let err = out.err();
    assert_eq!(out.code, Some(1), "{err:?}");
    assert!(
        err.starts_with(&format!(
            "Platform  linux/amd64\n{}",
            status_lines(THIRD_ID)
        )),
        "{err:?}"
    );
    let last = err.lines().last().unwrap();
    assert!(
        last.starts_with("agentspace: ")
            && last.ends_with(
                " is not agentspace's alias for 'agentspace.linux-amd64'; it was left as it is."
            )
            && last.contains("agentspace-host.agentspace.linux-amd64"),
        "{last:?}"
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        theirs,
        "the file is left as it was"
    );

    // The other way: the arm64 host's alias path holds the amd64 host's mark.
    let path = s.alias("agentspace");
    let theirs = "# agentspace host agentspace.linux-amd64; written by agentspace\nHost x\n";
    fs::write(&path, theirs).unwrap();
    let out = s.run(&["status", "-Host", "agentspace"]);
    let last = out.err().lines().last().unwrap().to_string();
    assert_eq!(out.code, Some(1), "{:?}", out.err());
    assert!(
        last.ends_with(" is not agentspace's alias for 'agentspace'; it was left as it is."),
        "{last:?}"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), theirs);
}

// ---- The push and grow over a platform's own alias ----

#[test]
fn up_pushes_over_the_platforms_own_alias_and_keeps_that_hosts_own_stamp() {
    let Some(s) = Signing::three_platforms() else {
        return;
    };
    s.rig.localswim();
    let hash = sha256_hex(s.rig.content().as_bytes());
    s.rig.answer(
        "ssh",
        None,
        Some(&format!("{hash}  -\nzone {ZONE}\n")),
        None,
        Some(0),
    );
    let out = s.run(&["up", "-Host", "agentspace", "-Platform", AMD64]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let ssh = s.rig.calls_of("ssh");
    assert_eq!(ssh.len(), 1, "one push: {:?}", out.err());
    assert_eq!(
        &ssh[0].args[..5],
        [
            "-o",
            "ClearAllForwardings=yes",
            "-o",
            "BatchMode=yes",
            "agentspace-host.agentspace.linux-amd64"
        ],
        "the platform's own alias, not the arm64 host's"
    );
    assert_eq!(
        fs::read(
            s.dir("agentspace.linux-amd64")
                .join("localswim-pushed.json")
        )
        .ok(),
        Some(
            format!(
                "{{\"version\":0,\"stamp\":\"{}\"}}",
                expected_stamp(THIRD_ID, ZONE, &s.rig.content())
            )
            .into_bytes()
        ),
        "the host's stamp covers the host's own instance ID, in the host's own folder"
    );
    assert!(
        !s.rig.stamp_path().exists(),
        "the box's stamp is never written"
    );
    assert!(
        !s.dir("agentspace").exists(),
        "the project's linux/arm64 host's folder is untouched"
    );
    // The stamp now matches: a second `up` for that host sends nothing.
    let again = s.run(&["up", "-Host", "agentspace", "-Platform", AMD64]);
    assert_eq!(again.code, Some(0), "{:?}", again.err());
    assert_eq!(s.rig.calls_of("ssh").len(), 1, "nothing is sent twice");
    // The project's other host has no stamp, so a push is due for it.
    let other = s.run(&["up", "-Host", "agentspace"]);
    assert_eq!(other.code, Some(0), "{:?}", other.err());
    let ssh = s.rig.calls_of("ssh");
    assert_eq!(ssh.len(), 2, "the arm64 host's own push");
    assert_eq!(ssh[1].args[4], "agentspace-host.agentspace");
}

#[test]
fn up_with_an_alias_and_a_platform_pushes_over_that_alias_and_stamps_the_platforms_host() {
    let Some(s) = Signing::three_platforms() else {
        return;
    };
    s.rig.localswim();
    let hash = sha256_hex(s.rig.content().as_bytes());
    s.rig.answer(
        "ssh",
        None,
        Some(&format!("{hash}  -\nzone {ZONE}\n")),
        None,
        Some(0),
    );
    let out = s.run(&[
        "up",
        "-Host",
        "agentspace",
        "-Platform",
        AMD64,
        "-Alias",
        "custom.alias",
    ]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let ssh = s.rig.calls_of("ssh");
    assert_eq!(ssh.len(), 1);
    assert_eq!(ssh[0].args[4], "custom.alias");
    assert!(
        s.dir("agentspace.linux-amd64")
            .join("localswim-pushed.json")
            .is_file(),
        "-Host and -Platform pick the host's stamp; -Alias picks only the alias"
    );
}

#[test]
fn grow_goes_through_the_platforms_own_alias_after_signing() {
    let Some(s) = Signing::three_platforms() else {
        return;
    };
    s.rig
        .answer("ssh", None, Some("DF-OUTPUT\n"), None, Some(0));
    let out = s.run(&[
        "grow",
        "-Host",
        "agentspace",
        "-Platform",
        AMD64,
        "-GiB",
        "3",
    ]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.out(), "DF-OUTPUT\n");
    let ssh = &s.rig.calls_of("ssh")[0];
    assert_eq!(
        &ssh.args[..3],
        [
            "-o",
            "ClearAllForwardings=yes",
            "agentspace-host.agentspace.linux-amd64"
        ],
        "one ssh through the platform's own alias with forwards cleared"
    );
    assert!(
        s.dir("agentspace.linux-amd64")
            .join("id_ed25519-cert.pub")
            .is_file(),
        "signed before the online step"
    );
    assert!(
        s.rig.calls()[0].args.contains(&THIRD_ID.to_string()),
        "the amd64 host's own instance"
    );
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
