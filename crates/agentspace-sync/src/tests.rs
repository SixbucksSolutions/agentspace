//! Unit tests of `sync` and `deploy` over fakes of the pinned traits, and the helpers the other
//! modules' tests share. Every test works in its own throwaway folder under the temp folder, starts
//! no real AWS CLI, ssh or scp, and never touches the real home.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use agentspace_box::Clock;
use agentspace_ca::CaEnv;
use agentspace_operator_api::{
    Aws, AwsError, Call, CallerFailure, CallerLine, Context, Failure, Mode, Output, Remote,
    SIGN_IN_MESSAGE, Say,
};
use serde_json::{Value, json};

use crate::{Seams, deploy_using, sync_using};

/// A made-up instance ID, outside the identifier scan's pattern.
pub(crate) const ID: &str = "i-1234567890abcdef0";
/// The `InstanceType` parameter the fake stack reports, as `describe-stacks` reads it back.
pub(crate) const STACK_TYPE: &str = "m9g.xlarge";
const FP: &str = "SHA256:fakefingerprint";

/// A throwaway folder under the temp folder, removed when it drops.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    pub(crate) fn new(name: &str) -> Scratch {
        static N: AtomicU64 = AtomicU64::new(0);
        let unique = format!(
            "agentspace-sync-unit-{name}-{}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).unwrap();
        Scratch(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A context whose home and every other folder are inside `dir`.
pub(crate) fn ctx_in(dir: &Path) -> Context {
    Context {
        home: dir.join("home"),
        local_app_data: Some(dir.join("local")),
        system_root: dir.join("root"),
        nested: false,
    }
}

/// A stand-in program in `dir` named `name` (`name.cmd` on Windows) whose body is `body`, as is.
pub(crate) fn raw_stand_in(dir: &Path, name: &str, body: &str) -> PathBuf {
    if cfg!(windows) {
        let path = dir.join(format!("{name}.cmd"));
        std::fs::write(&path, body).unwrap();
        path
    } else {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }
}

/// A stand-in program that exits with `code`.
pub(crate) fn exit_stand_in(dir: &Path, name: &str, code: i32) -> PathBuf {
    if cfg!(windows) {
        raw_stand_in(dir, name, &format!("@exit /b {code}\r\n"))
    } else {
        raw_stand_in(dir, name, &format!("exit {code}"))
    }
}

/// Records every line, in order, as `progress: `, `warning: ` or `detail: `.
#[derive(Default)]
struct FakeSay(RefCell<Vec<String>>);

impl Say for FakeSay {
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

impl FakeSay {
    fn lines(&self) -> Vec<String> {
        self.0.borrow().clone()
    }
}

/// Answers the calls of `sync` and `deploy`: a box `running` since long ago, a stack with an
/// `InstanceId` output, and a patch wait whose host is `Online` and answers `patched`. Records each
/// call as one line.
struct FakeAws {
    calls: RefCell<Vec<String>>,
    deploy: RefCell<Result<Value, AwsError>>,
    stack: RefCell<Value>,
    describe: RefCell<Result<Value, AwsError>>,
    /// `ssm send-command`'s answer.
    send: RefCell<Result<Value, AwsError>>,
    /// The patch check's answer, the invocation's trimmed stdout.
    answer: RefCell<&'static str>,
}

impl Default for FakeAws {
    fn default() -> Self {
        FakeAws {
            calls: RefCell::default(),
            deploy: RefCell::new(Ok(Value::Null)),
            stack: RefCell::new(json!({"Stacks": [{
                "Outputs": [
                    {"OutputKey": "Other", "OutputValue": "x"},
                    {"OutputKey": "InstanceId", "OutputValue": ID}
                ],
                "Parameters": [
                    {"ParameterKey": "RootVolumeGiB", "ParameterValue": "8"},
                    {"ParameterKey": "InstanceType", "ParameterValue": STACK_TYPE}
                ]
            }]})),
            describe: RefCell::new(Ok(json!({"Reservations": [{"Instances": [{
                "State": {"Name": "running"},
                "LaunchTime": "2020-01-01T00:00:00+00:00"
            }]}]}))),
            send: RefCell::new(Ok(json!({"Command": {"CommandId": "c-0123"}}))),
            answer: RefCell::new("patched"),
        }
    }
}

impl Aws for FakeAws {
    fn call(&self, call: &Call) -> Result<Value, AwsError> {
        let output = match call.output {
            Output::Json => "json",
            Output::PassThrough => "pass",
            Output::Ignore => "ignore",
            Output::Checked => "checked",
        };
        self.calls.borrow_mut().push(format!(
            "{} {} [{output}] {}",
            call.service,
            call.operation,
            call.args.join(" | ")
        ));
        match (call.service, call.operation) {
            ("cloudformation", "deploy") => self.deploy.borrow().clone(),
            ("cloudformation", "describe-stacks") => Ok(self.stack.borrow().clone()),
            ("ec2", "describe-instances") => self.describe.borrow().clone(),
            ("ssm", "describe-instance-information") => {
                Ok(json!({"InstanceInformationList": [{"PingStatus": "Online"}]}))
            }
            ("ssm", "send-command") => self.send.borrow().clone(),
            ("ssm", "get-command-invocation") => Ok(json!({
                "Status": "Success",
                "StandardOutputContent": format!("{}\n", self.answer.borrow())
            })),
            _ => Err(AwsError::Failed("unexpected call".into())),
        }
    }
}

/// Records each ssh and scp as one line, `ssh <options> | <remote command>` or `scp <args>`, and
/// answers the `n`th call (from 1) with `codes[n]`, 0 otherwise.
#[derive(Default)]
struct FakeRemote {
    calls: RefCell<Vec<String>>,
    codes: HashMap<usize, i32>,
    cannot_start: Option<usize>,
}

impl FakeRemote {
    fn failing(n: usize, code: i32) -> FakeRemote {
        FakeRemote {
            codes: HashMap::from([(n, code)]),
            ..FakeRemote::default()
        }
    }

    fn answer(&self, line: String) -> Result<i32, Failure> {
        let mut calls = self.calls.borrow_mut();
        calls.push(line);
        let n = calls.len();
        if self.cannot_start == Some(n) {
            return Err(Failure {
                message: "agentspace: AGENTSPACE_SSH_CMD names x, which cannot be run.".into(),
                caller: CallerFailure::CouldNotReach,
            });
        }
        Ok(self.codes.get(&n).copied().unwrap_or(0))
    }

    fn lines(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

impl Remote for FakeRemote {
    fn ssh(&self, options: &[&str], remote_command: Option<&str>) -> Result<i32, Failure> {
        self.answer(format!(
            "ssh {} | {}",
            options.join(" "),
            remote_command.unwrap_or("<none>")
        ))
    }
    fn scp(&self, args: &[&str]) -> Result<i32, Failure> {
        self.answer(format!("scp {}", args.join(" ")))
    }
}

/// A clock that never waits: a sleep moves its monotonic time on at once. Its wall clock is the
/// system's, so the fake box is running since long ago.
#[derive(Default)]
struct FakeClock {
    now: Cell<Duration>,
}

impl Clock for FakeClock {
    fn now(&self) -> Duration {
        self.now.get()
    }
    fn utc_now(&self) -> SystemTime {
        SystemTime::now()
    }
    fn sleep(&self, d: Duration) {
        self.now.set(self.now.get() + d);
    }
}

/// One run's world: a throwaway home, a localswim checkout, a `go` stand-in, and what the CA
/// check and the push did.
struct World {
    dir: Scratch,
    ctx: Context,
    go: Option<PathBuf>,
    checkout: PathBuf,
    ca: Result<String, Failure>,
    pushed: RefCell<Vec<String>>,
    /// Each push as `<alias> <project, or <box>>`.
    pushes: RefCell<Vec<String>>,
    ca_checked: Cell<bool>,
    clock: FakeClock,
}

impl World {
    fn new(name: &str) -> World {
        let dir = Scratch::new(name);
        let ctx = ctx_in(dir.path());
        std::fs::create_dir_all(&ctx.home).unwrap();
        let checkout = dir.path().join("localswim");
        std::fs::create_dir_all(&checkout).unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let go = Some(exit_stand_in(&bin, "go", 0));
        World {
            dir,
            ctx,
            go,
            checkout,
            ca: Ok(FP.to_string()),
            pushed: RefCell::default(),
            pushes: RefCell::default(),
            ca_checked: Cell::new(false),
            clock: FakeClock::default(),
        }
    }

    fn home(&self) -> &Path {
        &self.ctx.home
    }

    fn with_claude_md(self) -> World {
        let path = self.home().join(".claude").join("CLAUDE.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "# Notes\n").unwrap();
        self
    }

    fn with_deploy_files(self) -> World {
        let ssh = self.home().join(".ssh");
        std::fs::create_dir_all(&ssh).unwrap();
        std::fs::write(
            ssh.join("id_ed25519.pub"),
            "  ssh-ed25519 AAAAkey me@laptop\r\n",
        )
        .unwrap();
        let agentspace = self.home().join(".agentspace");
        std::fs::create_dir_all(&agentspace).unwrap();
        std::fs::write(
            agentspace.join("stack-parameters.json"),
            r#"{"version":0,"parameters":{"VpcId":"VPC_X","SubnetId":"SUBNET_Y"}}"#,
        )
        .unwrap();
        self
    }

    fn run<T>(&self, f: impl FnOnce(&Seams) -> T) -> T {
        let ca_check = |_: &Context, _: &CaEnv| {
            self.ca_checked.set(true);
            self.ca.clone()
        };
        let find_go = || self.go.clone();
        let push = |_: &Context, _: &dyn Say, id: &str, alias: &str, host: Option<&str>| {
            self.pushed.borrow_mut().push(id.into());
            self.pushes
                .borrow_mut()
                .push(format!("{alias} {}", host.unwrap_or("<box>")));
        };
        // The command's folder is left in place, so a test finds what the command sent.
        f(&Seams {
            clock: &self.clock,
            ca_check: &ca_check,
            find_go: &find_go,
            localswim_checkout: self.checkout.clone(),
            push: &push,
            folder: crate::folder::CommandFolder::new(),
            ssh_dir: Some(self.home().join(".ssh")),
            operator_dir: None,
        })
    }

    fn sync(&self, say: &FakeSay, aws: &FakeAws, remote: &FakeRemote) -> Result<(), Failure> {
        self.run(|seams| sync_using(&self.ctx, &CaEnv::default(), say, aws, remote, seams))
    }

    fn deploy(&self, say: &FakeSay, aws: &FakeAws, remote: &FakeRemote) -> Result<(), Failure> {
        self.run(|seams| deploy_using(&self.ctx, &CaEnv::default(), say, aws, remote, seams))
    }

    /// `%USERPROFILE%\.agentspace\operator`.
    fn operator_root(&self) -> PathBuf {
        std::path::absolute(self.home())
            .unwrap()
            .join(".agentspace")
            .join("operator")
    }

    /// The command's own folder under the operator's, `operator\<hex>\`: the one folder of a
    /// 32-hex-digit name that a run left (the seams here never remove it).
    fn operator(&self) -> PathBuf {
        let root = self.operator_root();
        let mut found: Vec<PathBuf> = std::fs::read_dir(&root)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| {
                        crate::folder::is_command_folder_name(&e.file_name().to_string_lossy())
                    })
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default();
        assert!(found.len() <= 1, "A command makes one folder");
        found
            .pop()
            .unwrap_or_else(|| root.join("no-command-folder"))
    }

    /// `%USERPROFILE%\.agentspace\hosts.json`.
    fn registry(&self) -> PathBuf {
        std::path::absolute(self.home())
            .unwrap()
            .join(".agentspace")
            .join("hosts.json")
    }

    /// The retired instance-ID cache's path. Only the v0.06 deploy tests below still name it, and
    /// they are the deploy implementer's to move to the registry (`box_instance_id`,
    /// `seed_registry`).
    fn cache(&self) -> PathBuf {
        std::path::absolute(self.home())
            .unwrap()
            .join(".agentspace")
            .join("instance-id.json")
    }

    /// The instance ID the box's record holds, or `None`.
    fn box_instance_id(&self) -> Option<String> {
        let home = std::path::absolute(self.home()).unwrap();
        let hosts = agentspace_hosts::read(&home).unwrap();
        hosts.hosts[0].instance_id.clone()
    }

    /// Writes the registry: the box's record, with `box_id` for its instance ID, and `others` after
    /// it, as `(project, state, instance ID)`.
    fn seed_registry(&self, box_id: Option<&str>, others: &[(&str, agentspace_hosts::HostState)]) {
        let home = std::path::absolute(self.home()).unwrap();
        let mut hosts = agentspace_hosts::Hosts::seed();
        hosts.hosts[0].instance_id = box_id.map(str::to_string);
        for (project, state) in others {
            hosts.hosts.push(agentspace_hosts::Host {
                last_success_at: None,
                platform: None,
                project: project.to_string(),
                stack: agentspace_hosts::stack_name(project),
                state: *state,
                instance_id: None,
                instance_type: None,
                last_attempt_at: None,
            });
        }
        let held = agentspace_hosts::lock(&home).unwrap();
        agentspace_hosts::write(&home, &hosts, &held).unwrap();
    }

    /// The nine calls of operator.md's table, as [`FakeRemote`] records them.
    fn table(&self) -> Vec<String> {
        let home = std::path::absolute(self.home()).unwrap();
        let op = self.operator();
        let ssh = "ssh -o ClearAllForwardings=yes agentspace |";
        vec![
            format!("{ssh} {}", crate::steps::REMOVE_STAGING),
            format!(
                "scp -o ClearAllForwardings=yes -q -r {} agentspace:/tmp/agentspace-system",
                op.join("system").display()
            ),
            format!(
                "scp -o ClearAllForwardings=yes -q {} agentspace:/tmp/agentspace-system/agentspace_ca.pub",
                home.join(".agentspace")
                    .join("ca")
                    .join("agentspace_ca.pub")
                    .display()
            ),
            format!("{ssh} {}", crate::steps::INSTALL),
            format!("{ssh} {}", crate::steps::MAKE_CLAUDE_FOLDER),
            format!(
                "scp -o ClearAllForwardings=yes -q {} agentspace:.claude/CLAUDE.md",
                home.join(".claude").join("CLAUDE.md").display()
            ),
            format!(
                "scp -o ClearAllForwardings=yes -q {} agentspace:/tmp/localswim",
                op.join("localswim-linux-arm64").display()
            ),
            format!("{ssh} {}", crate::steps::INSTALL_LOCALSWIM),
        ]
    }
}

const READY: &str = "progress: Ready: i-1234567890abcdef0";
const COPY_SYSTEM: &str = "progress: Copying box/system to the instance...";
const COPY_CA: &str = "progress: Copying the agentspace SSH CA's public key (SHA256:fakefingerprint) to the instance...";
const COPY_CLAUDE: &str = "progress: Copying ~/.claude/CLAUDE.md to the instance...";
const BUILDING: &str = "progress: Building the localswim CLI for linux/arm64...";
const COPY_CLI: &str = "progress: Copying the localswim CLI to the instance...";
const SKIP: &str =
    "progress: Skipping the localswim CLI: Go or C:\\Projects\\localswim is missing on the laptop.";
const CHECKING: &str =
    "progress: Checking that i-1234567890abcdef0 has finished this boot's patch run...";
const PATCHED: &str = "progress: Patched: i-1234567890abcdef0";
const PATCH_FAILED: &str = "warning: agentspace: The patch run on i-1234567890abcdef0 failed; sync goes on without it. Check: ssh agentspace journalctl -u agentspace-patch";
const PATCHING: &str = "progress: Patching Ubuntu (dist-upgrade, reboot if the kernel changed)...";
/// The patch wait's three calls, as [`FakeAws`] records them, for one check.
const PATCH_CHECK: [&str; 3] = [
    "ssm describe-instance-information [json] --filters | Key=InstanceIds,Values=i-1234567890abcdef0",
    "ssm send-command [json] --instance-ids | i-1234567890abcdef0 | --document-name | AWS-RunShellScript | --comment | agentspace sync: patch check | --parameters | commands=",
    "ssm get-command-invocation [json] --command-id | c-0123 | --instance-id | i-1234567890abcdef0",
];

/// The patch wait's calls among `calls`, with the question's text cut from `send-command`'s line.
fn patch_calls(calls: &[String]) -> Vec<String> {
    calls
        .iter()
        .filter(|c| c.starts_with("ssm "))
        .map(|c| match c.split_once("commands=") {
            Some((head, question)) => {
                assert_eq!(question, agentspace_box::PATCH_QUESTION);
                format!("{head}commands=")
            }
            None => c.clone(),
        })
        .collect()
}

#[test]
fn sync_makes_all_nine_calls_in_order_with_each_steps_message() {
    let w = World::new("sync-all").with_claude_md();
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    w.sync(&say, &aws, &remote).unwrap();
    assert!(w.ca_checked.get());
    assert_eq!(remote.lines(), w.table());
    assert_eq!(*w.pushed.borrow(), vec![ID.to_string()]);
    assert_eq!(
        say.lines(),
        vec![
            READY,
            COPY_SYSTEM,
            COPY_CA,
            CHECKING,
            PATCHED,
            COPY_CLAUDE,
            BUILDING,
            COPY_CLI
        ]
    );
    // The start-and-wait's calls: the cache rebuilt from the stack, the state, the launch time;
    // then the patch wait's, with no `ec2 describe-instances` of its own.
    let calls = aws.calls.borrow().clone();
    assert_eq!(
        calls[..3].to_vec(),
        vec![
            "cloudformation describe-stacks [json] --stack-name | agentbox",
            "ec2 describe-instances [json] --instance-ids | i-1234567890abcdef0",
            "ec2 describe-instances [json] --instance-ids | i-1234567890abcdef0",
        ]
    );
    assert_eq!(calls.len(), 6);
    assert_eq!(patch_calls(&calls[3..]), PATCH_CHECK.to_vec());
    // The ID read from the stack is written into the box's record, in a first change that writes
    // `hosts.json`.
    assert_eq!(w.box_instance_id().as_deref(), Some(ID));
    // The carried box\system, written out just before the copy, in the command's own folder.
    assert!(w.operator().join("system").join("install.sh").is_file());
    assert_eq!(*w.pushes.borrow(), vec!["agentspace <box>".to_string()]);
}

#[test]
fn sync_without_claude_md_or_go_makes_the_four_calls_and_the_push() {
    let mut w = World::new("sync-min");
    w.go = None;
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    w.sync(&say, &aws, &remote).unwrap();
    assert_eq!(remote.lines(), w.table()[..4].to_vec());
    assert_eq!(
        say.lines(),
        vec![READY, COPY_SYSTEM, COPY_CA, CHECKING, PATCHED, SKIP]
    );
    assert_eq!(w.pushed.borrow().len(), 1);
}

#[test]
fn a_missing_localswim_checkout_skips_the_build() {
    let w = World::new("sync-no-checkout");
    std::fs::remove_dir_all(&w.checkout).unwrap();
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    w.sync(&say, &aws, &remote).unwrap();
    assert_eq!(remote.lines().len(), 4);
    assert_eq!(say.lines().last().unwrap(), SKIP);
    assert_eq!(w.pushed.borrow().len(), 1);
}

#[test]
fn each_checked_steps_failure_ends_sync_with_its_message_and_nothing_after() {
    // (call that fails, its message); call 5, CLAUDE.md's folder, is not checked.
    let cases = [
        (
            1,
            "agentspace: Removing the old staging folder on the instance failed.",
        ),
        (2, "agentspace: Copy to the instance failed."),
        (
            3,
            "agentspace: Copying the CA's public key to the instance failed.",
        ),
        (4, "agentspace: Install on the instance failed."),
        (6, "agentspace: Copying CLAUDE.md to the instance failed."),
        (7, "agentspace: Copying the localswim CLI failed."),
        (8, "agentspace: Installing the localswim CLI failed."),
    ];
    for (n, message) in cases {
        for code in [1, 255] {
            let w = World::new("sync-fail").with_claude_md();
            let (say, aws, remote) = (
                FakeSay::default(),
                FakeAws::default(),
                FakeRemote::failing(n, code),
            );
            let failure = w.sync(&say, &aws, &remote).unwrap_err();
            assert_eq!(failure.message, message, "call {n}");
            assert_eq!(remote.lines(), w.table()[..n].to_vec(), "call {n}");
            assert!(w.pushed.borrow().is_empty(), "call {n}");
        }
    }
}

#[test]
fn the_claude_folder_steps_exit_code_is_not_checked() {
    let w = World::new("sync-folder").with_claude_md();
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::failing(5, 1),
    );
    w.sync(&say, &aws, &remote).unwrap();
    assert_eq!(remote.lines(), w.table());
}

#[test]
fn a_program_that_cannot_be_started_fails_with_its_own_message() {
    let w = World::new("sync-cannot-start");
    let remote = FakeRemote {
        cannot_start: Some(2),
        ..FakeRemote::default()
    };
    let failure = w
        .sync(&FakeSay::default(), &FakeAws::default(), &remote)
        .unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: AGENTSPACE_SSH_CMD names x, which cannot be run."
    );
    assert_eq!(remote.lines().len(), 2);
}

#[test]
fn a_failed_ca_check_touches_nothing() {
    let mut w = World::new("sync-ca");
    w.ca = Err(Failure {
        message: "agentspace: C:\\x does not exist; run ca.".into(),
        caller: CallerFailure::CouldNotReach,
    });
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    let failure = w.sync(&say, &aws, &remote).unwrap_err();
    assert_eq!(failure.message, "agentspace: C:\\x does not exist; run ca.");
    assert!(aws.calls.borrow().is_empty());
    assert!(remote.lines().is_empty());
    assert!(say.lines().is_empty());
    assert!(!w.registry().exists());
}

#[test]
fn a_failed_start_and_wait_reaches_no_box() {
    let w = World::new("sync-wait");
    let aws = FakeAws::default();
    *aws.describe.borrow_mut() = Err(AwsError::SignIn);
    let remote = FakeRemote::default();
    let failure = w.sync(&FakeSay::default(), &aws, &remote).unwrap_err();
    assert_eq!(failure.message, SIGN_IN_MESSAGE);
    assert!(remote.lines().is_empty());
    assert!(w.pushed.borrow().is_empty());
}

#[test]
fn the_patch_wait_runs_after_the_install_and_before_row_5() {
    // #0034's sign-in failure at `ssm send-command` fails sync at once: rows 1 to 4 have run, and
    // nothing after them.
    let w = World::new("sync-wait-place").with_claude_md();
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    *aws.send.borrow_mut() = Err(AwsError::SignIn);
    let failure = w.sync(&say, &aws, &remote).unwrap_err();
    assert_eq!(failure.message, SIGN_IN_MESSAGE);
    assert_eq!(remote.lines(), w.table()[..4].to_vec());
    assert!(w.pushed.borrow().is_empty());
    assert_eq!(say.lines(), vec![READY, COPY_SYSTEM, COPY_CA, CHECKING]);
}

#[test]
fn no_patch_wait_follows_an_install_that_failed() {
    let w = World::new("sync-no-wait").with_claude_md();
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::failing(4, 1),
    );
    w.sync(&say, &aws, &remote).unwrap_err();
    assert!(aws.calls.borrow().iter().all(|c| !c.starts_with("ssm ")));
    assert!(!say.lines().contains(&CHECKING.to_string()));
}

#[test]
fn a_failed_patch_run_warns_once_and_sync_goes_on() {
    let w = World::new("sync-patch-failed").with_claude_md();
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    *aws.answer.borrow_mut() = "failed";
    w.sync(&say, &aws, &remote).unwrap();
    assert_eq!(remote.lines(), w.table());
    assert_eq!(*w.pushed.borrow(), vec![ID.to_string()]);
    assert_eq!(
        say.lines(),
        vec![
            READY,
            COPY_SYSTEM,
            COPY_CA,
            CHECKING,
            PATCH_FAILED,
            COPY_CLAUDE,
            BUILDING,
            COPY_CLI
        ]
    );
    // The wait ended at that answer: one check, and no other.
    assert_eq!(patch_calls(&aws.calls.borrow()), PATCH_CHECK.to_vec());
}

#[test]
fn a_later_rows_failure_after_a_failed_patch_run_fails_sync_with_its_message() {
    let w = World::new("sync-patch-failed-later").with_claude_md();
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::failing(6, 1),
    );
    *aws.answer.borrow_mut() = "failed";
    let failure = w.sync(&say, &aws, &remote).unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: Copying CLAUDE.md to the instance failed."
    );
    assert!(say.lines().contains(&PATCH_FAILED.to_string()));
}

#[test]
fn a_patch_run_that_never_finishes_fails_sync_at_thirty_minutes_before_row_5() {
    let w = World::new("sync-patch-deadline").with_claude_md();
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    *aws.answer.borrow_mut() = "pending";
    let failure = w.sync(&say, &aws, &remote).unwrap_err();
    assert_eq!(
        failure.message,
        format!(
            "agentspace: {ID} did not finish its patch run within 30 minutes; check: ssh agentspace journalctl -u agentspace-patch"
        )
    );
    assert_eq!(w.clock.now.get(), Duration::from_secs(30 * 60));
    assert_eq!(remote.lines(), w.table()[..4].to_vec());
    assert!(w.pushed.borrow().is_empty());
    assert_eq!(
        say.lines(),
        vec![READY, COPY_SYSTEM, COPY_CA, CHECKING, PATCHING]
    );
    // 361 checks, 5 seconds apart, each its three calls.
    assert_eq!(patch_calls(&aws.calls.borrow()).len(), 361 * 3);
}

#[test]
fn a_failed_send_command_is_a_check_that_was_not_ready() {
    // The fake answers every `send-command` the same way, so the wait goes on to its deadline,
    // printing the Session Manager message once.
    let w = World::new("sync-send-failed");
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    *aws.send.borrow_mut() = Err(AwsError::Failed("InvalidInstanceId".into()));
    let failure = w.sync(&say, &aws, &remote).unwrap_err();
    assert!(
        failure
            .message
            .contains("did not finish its patch run within 30 minutes")
    );
    assert_eq!(
        say.lines(),
        vec![
            READY,
            COPY_SYSTEM,
            COPY_CA,
            CHECKING,
            "progress: Waiting for Session Manager..."
        ]
    );
}

#[test]
fn a_failed_build_fails_before_its_copy() {
    let mut w = World::new("sync-build");
    w.go = Some(exit_stand_in(w.dir.path(), "badgo", 2));
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    let failure = w.sync(&say, &aws, &remote).unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: Building the localswim CLI failed."
    );
    assert_eq!(remote.lines().len(), 4);
    assert_eq!(say.lines().last().unwrap(), BUILDING);
}

#[test]
fn an_operator_folder_that_cannot_be_written_fails_with_fa16_before_the_copy() {
    let w = World::new("sync-fa16");
    let agentspace = w.operator_root().parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&agentspace).unwrap();
    std::fs::write(w.operator_root(), "a file where the folder belongs").unwrap();
    let remote = FakeRemote::default();
    let failure = w
        .sync(&FakeSay::default(), &FakeAws::default(), &remote)
        .unwrap_err();
    assert_eq!(
        failure.message,
        format!("agentspace: Cannot write {}.", w.operator_root().display())
    );
    // Only the staging folder's removal ran; nothing read the folder.
    assert_eq!(remote.lines(), w.table()[..1].to_vec());
}

#[test]
fn deploy_passes_its_arguments_in_order_then_refreshes_the_cache_and_syncs() {
    let w = World::new("deploy").with_deploy_files();
    w.seed_registry(Some("i-fedcba9876543210f"), &[]);
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    w.deploy(&say, &aws, &remote).unwrap();
    let template = w.operator().join("agentspace.yaml");
    let calls = aws.calls.borrow().clone();
    assert_eq!(
        calls[0],
        format!(
            "cloudformation deploy [pass] --stack-name | agentbox | --template-file | {} | \
             --capabilities | CAPABILITY_IAM | --no-fail-on-empty-changeset | --tags | \
             project=agentbox | --parameter-overrides | SshPublicKey=ssh-ed25519 AAAAkey me@laptop | \
             VpcId=VPC_X | SubnetId=SUBNET_Y",
            template.display()
        )
    );
    assert_eq!(
        calls[1],
        "cloudformation describe-stacks [json] --stack-name | agentbox"
    );
    // The old cache was deleted, so sync read the new one and asked the stack nothing more.
    assert!(calls[2].starts_with("ec2 describe-instances"));
    assert!(calls[3].starts_with("ec2 describe-instances"));
    // Then sync's patch wait.
    assert_eq!(patch_calls(&calls[4..]), PATCH_CHECK.to_vec());
    assert!(std::fs::read(&template).unwrap() == crate::embedded::TEMPLATE);
    // The box's record holds the ID the stack gave, and the retired cache is never written.
    assert_eq!(w.box_instance_id().as_deref(), Some(ID));
    assert!(!w.cache().exists());
    let lines = say.lines();
    assert_eq!(
        lines[0],
        "progress: Deploying stack 'agentbox' in us-west-2..."
    );
    assert_eq!(lines[1], "progress: Instance: i-1234567890abcdef0");
    assert_eq!(lines[2], READY);
    // No CLAUDE.md in this home: six of the table's eight remote calls, then the push.
    assert_eq!(remote.lines().len(), 6);
    assert_eq!(*w.pushed.borrow(), vec![ID.to_string()]);
}

#[test]
fn deploys_aws_failures() {
    let cases = [
        (AwsError::SignIn, SIGN_IN_MESSAGE.to_string()),
        (
            AwsError::Failed("Waiter StackUpdateComplete failed".into()),
            "agentspace: Stack deploy failed; see the stack events in the console.".to_string(),
        ),
        (
            AwsError::CannotRun("agentspace: Could not run x.".into()),
            "agentspace: Could not run x.".to_string(),
        ),
    ];
    for (error, message) in cases {
        let w = World::new("deploy-aws").with_deploy_files();
        w.seed_registry(Some("i-fedcba9876543210f"), &[]);
        let aws = FakeAws::default();
        *aws.deploy.borrow_mut() = Err(error);
        let remote = FakeRemote::default();
        let failure = w.deploy(&FakeSay::default(), &aws, &remote).unwrap_err();
        assert_eq!(failure.message, message);
        assert_eq!(aws.calls.borrow().len(), 1);
        assert!(remote.lines().is_empty());
        assert!(!w.ca_checked.get());
        // A failed deploy leaves the record's instance ID alone.
        assert_eq!(w.box_instance_id().as_deref(), Some("i-fedcba9876543210f"));
    }
}

#[test]
fn a_stack_with_no_instance_id_output_leaves_no_cache() {
    let w = World::new("deploy-no-output").with_deploy_files();
    w.seed_registry(Some("i-fedcba9876543210f"), &[]);
    let aws = FakeAws::default();
    *aws.stack.borrow_mut() = json!({"Stacks": [{"Outputs": []}]});
    let failure = w
        .deploy(&FakeSay::default(), &aws, &FakeRemote::default())
        .unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: Stack 'agentbox' has no InstanceId output."
    );
    // The ID was removed before the read, and a failed read writes none.
    assert_eq!(w.box_instance_id(), None);
}

#[test]
fn deploy_fails_when_its_sync_fails() {
    let w = World::new("deploy-sync").with_deploy_files();
    let failure = w
        .deploy(
            &FakeSay::default(),
            &FakeAws::default(),
            &FakeRemote::failing(4, 1),
        )
        .unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: Install on the instance failed."
    );
}

#[test]
fn deploys_file_failures_come_before_any_aws_call() {
    let w = World::new("deploy-files");
    let key = w.home().join(".ssh").join("id_ed25519.pub");
    let params = w.home().join(".agentspace").join("stack-parameters.json");
    let key_abs = std::path::absolute(&key).unwrap();
    let params_abs = std::path::absolute(&params).unwrap();
    let run = |w: &World| {
        let aws = FakeAws::default();
        let failure = w
            .deploy(&FakeSay::default(), &aws, &FakeRemote::default())
            .unwrap_err();
        assert!(aws.calls.borrow().is_empty());
        failure.message
    };
    assert_eq!(
        run(&w),
        format!("agentspace: {} does not exist.", key_abs.display())
    );
    std::fs::create_dir_all(key.parent().unwrap()).unwrap();
    std::fs::write(&key, " \r\n\t").unwrap();
    assert_eq!(
        run(&w),
        format!("agentspace: {} is empty.", key_abs.display())
    );
    std::fs::write(&key, b"\xff\xfe").unwrap();
    assert_eq!(
        run(&w),
        format!("agentspace: Cannot read {}.", key_abs.display())
    );
    std::fs::remove_file(&key).unwrap();
    std::fs::create_dir(&key).unwrap();
    assert_eq!(
        run(&w),
        format!("agentspace: Cannot read {}.", key_abs.display())
    );
    std::fs::remove_dir(&key).unwrap();
    std::fs::write(&key, "ssh-ed25519 AAAA").unwrap();
    assert_eq!(
        run(&w),
        format!(
            "agentspace: Missing {}; copy infra\\stack-parameters.example.json and fill it in.",
            params_abs.display()
        )
    );
    std::fs::create_dir_all(params.parent().unwrap()).unwrap();
    std::fs::write(&params, r#"{"version":0,"parameters":{"VpcId":""}}"#).unwrap();
    assert_eq!(
        run(&w),
        format!(
            "agentspace: {} is not a valid stack parameters file: the parameter VpcId is empty.",
            params_abs.display()
        )
    );
}

#[test]
fn deploys_messages_go_through_say_so_quiet_quiets_its_sync_too() {
    // Every line deploy and its sync print is a progress line through the one Say, which -Quiet
    // suppresses; none is written anywhere else.
    let w = World::new("deploy-say").with_deploy_files();
    let say = FakeSay::default();
    w.deploy(&say, &FakeAws::default(), &FakeRemote::default())
        .unwrap();
    assert!(say.lines().iter().all(|l| l.starts_with("progress: ")));
    assert!(say.lines().contains(&COPY_SYSTEM.to_string()));
}

/// The operator's own seams, but for `go` and the localswim folder, which are this test's: a test
/// MUST NOT reach the real ones (operator.md, The test seam, "Not covered"). The finder says
/// whether it was asked.
fn with_the_operators_seams<T>(dir: &Scratch, run: impl FnOnce(&Seams) -> T) -> (T, bool) {
    let asked = Cell::new(false);
    let find_go = || {
        asked.set(true);
        None
    };
    let out = Seams::real_except_go(&find_go, dir.path().join("localswim"), run);
    (out, asked.get())
}

#[test]
fn the_operators_own_ca_check_runs_first_and_touches_nothing_when_it_fails() {
    // The real CA check, in a throwaway home whose system folder has no ssh-keygen.exe: `ca`'s
    // step 1 fails before any AWS call or connection.
    let dir = Scratch::new("own-seams-sync");
    let ctx = ctx_in(dir.path());
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    let (result, asked_for_go) = with_the_operators_seams(&dir, |seams| {
        sync_using(&ctx, &CaEnv::default(), &say, &aws, &remote, seams)
    });
    let failure = result.unwrap_err();
    if cfg!(windows) {
        let keygen = ctx
            .system_root
            .join("System32")
            .join("OpenSSH")
            .join("ssh-keygen.exe");
        assert_eq!(
            failure.message,
            format!(
                "agentspace: {} is missing; install the Windows OpenSSH client.",
                std::path::absolute(keygen).unwrap().display()
            )
        );
    }
    assert!(aws.calls.borrow().is_empty());
    assert!(remote.lines().is_empty());
    assert!(say.lines().is_empty());
    assert!(!asked_for_go);
}

#[test]
fn deploy_with_the_operators_own_seams_reads_the_key_before_anything_else() {
    let dir = Scratch::new("own-seams-deploy");
    let ctx = ctx_in(dir.path());
    let (aws, remote) = (FakeAws::default(), FakeRemote::default());
    let (result, asked_for_go) = with_the_operators_seams(&dir, |seams| {
        deploy_using(
            &ctx,
            &CaEnv::default(),
            &FakeSay::default(),
            &aws,
            &remote,
            seams,
        )
    });
    let key = std::path::absolute(&ctx.home)
        .unwrap()
        .join(".ssh")
        .join("id_ed25519.pub");
    assert_eq!(
        result.unwrap_err().message,
        format!("agentspace: {} does not exist.", key.display())
    );
    assert!(aws.calls.borrow().is_empty());
    assert!(remote.lines().is_empty());
    assert!(!asked_for_go);
}

/// The pinned `sync` and `deploy` build from operator.md's fixed folder (sync, step 7). The seams
/// are only made here: nothing is looked up, run, read or tested for.
#[test]
fn the_pinned_pair_names_the_specs_localswim_folder() {
    let folder = crate::with_real_seams(|seams| seams.localswim_checkout.clone());
    assert_eq!(folder, PathBuf::from(r"C:\Projects\localswim"));
}

/// Every `.rs` file under `dir`, with its text.
fn rust_files(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).unwrap();
            out.push((path, text));
        }
    }
}

/// The lines of `text` that call a function named `name` by that bare name or through a path
/// (`name(`, `crate::name(`, `agentspace_sync::name(`), as `line number: text`. A method call
/// (`.name(`), a longer name (`do_name(`, `name_using(`) and the definition (`fn name(`) are not
/// calls of it, and a comment line is not code.
fn calls_of(name: &str, text: &str) -> Vec<String> {
    let call = format!("{name}(");
    let mut found = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("//") {
            continue;
        }
        let mut from = 0;
        while let Some(at) = line[from..].find(&call) {
            let at = from + at;
            let before = &line[..at];
            let part_of_a_longer_name = before
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
            let method = before.ends_with('.');
            let definition = before.trim_end().ends_with("fn");
            if !(part_of_a_longer_name || method || definition) {
                found.push(format!("{}: {}", n + 1, line.trim()));
                break;
            }
            from = at + call.len();
        }
    }
    found
}

/// **No test MAY call the pinned `sync` or `deploy` where a `go` could be found, and no test MAY
/// reach `C:\Projects\localswim`** (operator.md, The test seam, "Not covered"; architect,
/// 2026-11-19, for E0.06-T03-S02). This reads the crate's own source, and holds the tests to it:
/// - The pinned pair is called in two places only, the integration rig's guarded `do_pinned_sync`
///   and `do_pinned_deploy`, each right after the guard that refuses to run unless the process's
///   `PATH` is the rig's one empty folder. Every other test goes through `sync_using` or
///   `deploy_using`, with a `go` finder and a localswim folder of its own.
/// - The fixed folder is named as a path in `go.rs` alone, and outside the library in one
///   constant compare.
///
/// The scans are shown able to find a call and the folder by finding the ones that exist.
#[test]
fn no_test_calls_the_pinned_pair_but_the_rigs_guarded_two() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&manifest.join("src"), &mut files);
    rust_files(&manifest.join("tests"), &mut files);
    files.sort();
    assert!(files.len() >= 12, "The scan found {} files", files.len());

    // The scan itself: it finds a path call and a bare call, and passes over the rest.
    let sample = "    crate::sink(a);\n    w.sink(a);\n    do_sink(a);\n    pub fn sink(\n    // sink(a)\n    sink(b);\n    sink_using(a);\n";
    assert_eq!(
        calls_of("sink", sample),
        vec!["1: crate::sink(a);", "6: sink(b);"]
    );

    let name_of = |path: &Path| {
        path.strip_prefix(manifest)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
    };
    for pinned in ["sync", "deploy"] {
        let mut calls = Vec::new();
        for (path, text) in &files {
            for call in calls_of(pinned, text) {
                calls.push((name_of(path), call));
            }
        }
        assert_eq!(
            calls.len(),
            1,
            "The pinned {pinned} MUST be called only by the rig's guarded entry: {calls:?}"
        );
        let (file, call) = &calls[0];
        assert_eq!(file, "tests/common/mod.rs", "{call}");
        assert!(
            call.ends_with(&format!(
                "agentspace_sync::{pinned}(&ctx, &ca_env(), Target::Box, say, &run, &run)"
            )),
            "{call}"
        );
    }

    // The entries pinned beside them in v0.07 use the same `go` finder and localswim folder, and
    // the rig has no guarded entry for them: no test MAY call one. A test goes through its hidden
    // `_using` twin. In the library's own files a bare name can be a module's helper, so a call
    // there is one through `crate::`.
    assert_eq!(
        calls_of("crate::sink", "    crate::sink(a);\n    sink(b);\n"),
        vec!["1: crate::sink(a);"]
    );
    for pinned in [
        "bootstrap",
        "provision",
        "sync_all",
        "deploy_with_instance_type",
    ] {
        for (path, text) in &files {
            let file = name_of(path);
            let calls = if file.starts_with("tests/") {
                calls_of(pinned, text)
            } else {
                calls_of(&format!("crate::{pinned}"), text)
            };
            assert!(
                calls.is_empty(),
                "No test MAY call the pinned {pinned}, which finds the real go: {file}: {calls:?}"
            );
        }
    }

    // Each guarded entry runs the guard first, and the guard holds the child to an empty PATH.
    let rig = &files
        .iter()
        .find(|(path, _)| name_of(path) == "tests/common/mod.rs")
        .unwrap()
        .1;
    for pinned in ["sync", "deploy"] {
        let entry = format!(
            "    pub fn do_pinned_{pinned}(&self, say: &RecSay) -> Result<(), Failure> {{\n        self.refuse_unless_no_go_can_be_found();\n"
        );
        let body = rig
            .split(&entry)
            .nth(1)
            .unwrap_or_else(|| panic!("do_pinned_{pinned} MUST start with the guard"));
        let body = body.split("\n    }\n").next().unwrap();
        assert!(
            body.contains(&format!("agentspace_sync::{pinned}(")),
            "do_pinned_{pinned} MUST hold the pinned call"
        );
    }
    let guard = rig
        .split("    fn refuse_unless_no_go_can_be_found(&self) {\n")
        .nth(1)
        .unwrap()
        .split("\n    }\n")
        .next()
        .unwrap();
    for needed in [
        "self.child",
        "std::env::var_os(\"PATH\")",
        "folders.len() == 1 && folders[0] == empty",
        "entries.next().is_none()",
        "holds_nothing",
    ] {
        assert!(guard.contains(needed), "The guard lost `{needed}`");
    }
    // The child's PATH is that one folder, whatever the rig's `go` is.
    assert!(rig.contains("let path = self.empty_path_folder();\n"));
    assert!(rig.contains("            .env(\"PATH\", &path)\n"));
    assert_eq!(rig.matches(".env(\"PATH\"").count(), 1);

    // The fixed folder, as a path in a string of its own, written either way: `go.rs`'s constant,
    // and this file's constant compare.
    let parts = ["C:", "Projects", "localswim"];
    let spellings = [
        format!("\"{}\"", parts.join("\\")),
        format!("\"{}\"", parts.join("\\\\")),
    ];
    let mut named = Vec::new();
    for (path, text) in &files {
        for (n, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if !code.starts_with("//") && spellings.iter().any(|s| code.contains(s.as_str())) {
                named.push(format!("{}:{}", name_of(path), n + 1));
            }
        }
    }
    assert_eq!(named.len(), 2, "{named:?}");
    assert!(named[0].starts_with("src/go.rs:"), "{named:?}");
    assert!(named[1].starts_with("src/tests.rs:"), "{named:?}");
}

/// The library reads no environment variable but `PATH`, once, to find `go`, and
/// `AGENTSPACE_SSH_DIR`, once, to find the ssh folder a host's operator alias goes in (operator.md,
/// Environment variables); every other value comes from the `Context`. The scan is proven able to
/// find a read by finding the ones that exist.
#[test]
fn the_library_reads_no_environment_variable_but_path_and_the_ssh_folder() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut reads = Vec::new();
    for entry in std::fs::read_dir(&src).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name == "tests.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        // A module's own tests come after the library's code.
        let library = text.split("#[cfg(test)]\nmod tests {").next().unwrap();
        for (n, line) in library.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            if line.contains("env::var") || line.contains("var_os(") {
                reads.push(format!("{name}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    reads.sort();
    // The third is `provision`'s one look-up over the run's own environment, which the driver's
    // context is built from (operator.md, provision).
    assert_eq!(reads.len(), 3, "{reads:?}");
    assert!(
        reads
            .iter()
            .any(|r| r.contains("|name: &str| std::env::var_os(name)")),
        "{reads:?}"
    );
    assert!(
        reads.iter().any(|r| r.contains("var_os(\"PATH\")")),
        "{reads:?}"
    );
    assert!(
        reads
            .iter()
            .any(|r| r.contains("var_os(\"AGENTSPACE_SSH_DIR\")")),
        "{reads:?}"
    );
}

use agentspace_hosts::{HostState, Target};

impl World {
    fn sync_target(
        &self,
        target: Target<'_>,
        say: &FakeSay,
        aws: &FakeAws,
        remote: &FakeRemote,
    ) -> Result<(), Failure> {
        self.run(|seams| {
            crate::sync_target_using(
                &self.ctx,
                &CaEnv::default(),
                target,
                say,
                aws,
                remote,
                seams,
            )
        })
    }

    fn sync_all(&self, say: &FakeSay, aws: &FakeAws, remote: &FakeRemote) -> Result<(), Failure> {
        self.run(|seams| {
            crate::sync_all_using(&self.ctx, &CaEnv::default(), say, aws, remote, seams)
        })
    }
}

#[test]
fn a_project_with_no_record_fails_before_any_program_starts() {
    let w = World::new("sync-no-host");
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    let failure = w
        .sync_target(Target::Host("agentspace"), &say, &aws, &remote)
        .unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: Project 'agentspace' has no host; provision -Host agentspace makes one."
    );
    assert!(!w.ca_checked.get());
    assert!(aws.calls.borrow().is_empty() && remote.lines().is_empty());
    assert!(say.lines().is_empty());
}

#[test]
fn a_registry_that_is_not_valid_fails_sync_before_any_program_starts() {
    let w = World::new("sync-bad-registry");
    std::fs::create_dir_all(w.registry().parent().unwrap()).unwrap();
    std::fs::write(w.registry(), "not json").unwrap();
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    let failure = w.sync(&say, &aws, &remote).unwrap_err();
    assert!(
        failure
            .message
            .starts_with("agentspace: The host registry "),
        "{}",
        failure.message
    );
    assert!(!w.ca_checked.get());
    assert!(aws.calls.borrow().is_empty() && remote.lines().is_empty());
}

#[test]
fn the_boxs_record_instance_id_is_used_and_the_box_is_reached_over_agentspace() {
    let w = World::new("sync-box-record");
    w.seed_registry(Some(ID), &[]);
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    // `-Host swimcoach` is the box in every respect.
    w.sync_target(Target::Host("swimcoach"), &say, &aws, &remote)
        .unwrap();
    // The record held the ID, so the stack was never asked.
    assert!(
        aws.calls
            .borrow()
            .iter()
            .all(|c| !c.starts_with("cloudformation"))
    );
    assert_eq!(*w.pushes.borrow(), vec!["agentspace <box>".to_string()]);
    assert!(
        remote
            .lines()
            .iter()
            .all(|l| !l.contains("agentspace-host"))
    );
}

#[test]
fn sync_all_syncs_ready_hosts_and_skips_the_others_with_their_line() {
    let w = World::new("sync-all-hosts");
    w.seed_registry(None, &[("agentspace", HostState::Creating)]);
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    w.sync_all(&say, &aws, &remote).unwrap();
    assert_eq!(
        say.lines(),
        vec![
            READY,
            COPY_SYSTEM,
            COPY_CA,
            CHECKING,
            PATCHED,
            BUILDING,
            COPY_CLI,
            "progress: Skipping: the host is creating; its next provisioning run syncs it.",
        ]
    );
    assert_eq!(*w.pushes.borrow(), vec!["agentspace <box>".to_string()]);
    // The skipped host's stack was never asked about.
    assert!(
        aws.calls
            .borrow()
            .iter()
            .all(|c| !c.contains("agentspace-host-agentspace"))
    );
}

#[test]
fn sync_all_goes_on_past_a_failed_host_and_counts_the_failures() {
    let w = World::new("sync-all-fail");
    w.seed_registry(None, &[("agentspace", HostState::Failed)]);
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::failing(4, 1),
    );
    let failure = w.sync_all(&say, &aws, &remote).unwrap_err();
    assert_eq!(failure.message, "agentspace: 1 of 2 hosts failed.");
    assert!(w.pushed.borrow().is_empty());
    assert_eq!(
        say.lines().last().unwrap(),
        "progress: Skipping: the host is failed; its next provisioning run syncs it."
    );
}

#[test]
fn sync_all_does_not_count_a_failed_patch_run_as_a_failed_host() {
    let w = World::new("sync-all-patch-failed");
    w.seed_registry(None, &[]);
    let (say, aws, remote) = (
        FakeSay::default(),
        FakeAws::default(),
        FakeRemote::default(),
    );
    *aws.answer.borrow_mut() = "failed";
    w.sync_all(&say, &aws, &remote).unwrap();
    assert!(say.lines().contains(&PATCH_FAILED.to_string()));
    assert_eq!(*w.pushes.borrow(), vec!["agentspace <box>".to_string()]);
}

#[test]
fn the_sign_in_message_at_the_patch_waits_send_ends_sync_all_at_once() {
    let w = World::new("sync-all-wait-sign-in");
    w.seed_registry(None, &[("agentspace", HostState::Ready)]);
    let aws = FakeAws::default();
    *aws.send.borrow_mut() = Err(AwsError::SignIn);
    let remote = FakeRemote::default();
    let failure = w.sync_all(&FakeSay::default(), &aws, &remote).unwrap_err();
    assert_eq!(failure.message, SIGN_IN_MESSAGE);
    // The box's rows 1 to 4, and nothing of the next host's.
    assert_eq!(remote.lines().len(), 4);
    assert!(
        aws.calls
            .borrow()
            .iter()
            .all(|c| !c.contains("agentspace-host-agentspace"))
    );
}

#[test]
fn the_sign_in_message_ends_sync_all_at_once() {
    let w = World::new("sync-all-sign-in");
    w.seed_registry(None, &[("agentspace", HostState::Ready)]);
    let aws = FakeAws::default();
    *aws.describe.borrow_mut() = Err(AwsError::SignIn);
    let remote = FakeRemote::default();
    let failure = w.sync_all(&FakeSay::default(), &aws, &remote).unwrap_err();
    assert_eq!(failure.message, SIGN_IN_MESSAGE);
    assert!(remote.lines().is_empty());
}

#[test]
fn a_hosts_deadline_hint_names_its_own_alias_and_the_boxs_stays() {
    let w = World::new("sync-hint");
    let places = crate::files::Places::new(&w.ctx);
    let hosts = agentspace_hosts::Hosts::seed();
    let mut host = hosts.hosts[0].clone();
    let message = "agentspace: i-1234567890abcdef0 was not ready within 15 minutes; check: ssh agentspace journalctl -u agentspace-patch";
    let failure = || Failure {
        message: message.to_string(),
        caller: CallerFailure::NotReady,
    };
    let plan = crate::syncing::Plan::for_record(&places, &host);
    assert_eq!(plan.hint(failure()).message, message);
    host.project = "demo".into();
    host.stack = agentspace_hosts::stack_name("demo");
    let plan = crate::syncing::Plan::for_record(&places, &host);
    assert_eq!(plan.alias, "agentspace-host.demo");
    assert_eq!(
        plan.hint(failure()).message,
        message.replace(
            "ssh agentspace journalctl",
            "ssh agentspace-host.demo journalctl"
        )
    );
}

#[test]
fn the_commands_folder_is_removed_as_the_command_ends() {
    let dir = Scratch::new("folder-removed");
    let ctx = ctx_in(dir.path());
    let places = crate::files::Places::new(&ctx);
    let made = with_the_operators_seams(&dir, |seams| seams.folder.path(&places).unwrap()).0;
    assert!(!made.exists());
    assert!(places.operator.is_dir());
}

/// The deploy and provision implementer's tests: `deploy -Host`'s stack steps and the
/// provisioning run's records and log, through the fakes above.
mod host_deploy_and_provision {
    use super::*;
    use agentspace_hosts::HostState;

    /// Answers `describe-stacks` from a queue, first to last, and every other call as [`FakeAws`]
    /// does; records each call as one line.
    struct ScriptedAws {
        stacks: RefCell<Vec<Result<Value, AwsError>>>,
        deploy: Result<Value, AwsError>,
        inner: FakeAws,
    }

    impl ScriptedAws {
        fn new(stacks: Vec<Result<Value, AwsError>>) -> ScriptedAws {
            ScriptedAws {
                stacks: RefCell::new(stacks),
                deploy: Ok(Value::Null),
                inner: FakeAws::default(),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.inner.calls.borrow().clone()
        }
    }

    impl Aws for ScriptedAws {
        fn call(&self, call: &Call) -> Result<Value, AwsError> {
            let answer = self.inner.call(call);
            match (call.service, call.operation) {
                ("cloudformation", "describe-stacks") => {
                    let mut stacks = self.stacks.borrow_mut();
                    if stacks.is_empty() {
                        answer
                    } else {
                        stacks.remove(0)
                    }
                }
                ("cloudformation", "deploy") => self.deploy.clone(),
                _ => answer,
            }
        }
    }

    const PROJECT: &str = "example";
    const STACK: &str = "agentspace-host-example";

    fn missing() -> Result<Value, AwsError> {
        Err(AwsError::Failed(
            "An error occurred (ValidationError): Stack with id x does not exist".into(),
        ))
    }

    fn status(s: &str) -> Result<Value, AwsError> {
        Ok(json!({"Stacks": [{"StackStatus": s}]}))
    }

    fn outputs() -> Result<Value, AwsError> {
        Ok(json!({"Stacks": [{
            "Outputs": [{"OutputKey": "InstanceId", "OutputValue": ID}],
            "Parameters": [{"ParameterKey": "InstanceType", "ParameterValue": STACK_TYPE}]
        }]}))
    }

    fn home(w: &World) -> PathBuf {
        std::path::absolute(w.home()).unwrap()
    }

    fn with_ca(w: World) -> World {
        let ca = w.home().join(".agentspace").join("ca");
        std::fs::create_dir_all(&ca).unwrap();
        std::fs::write(
            ca.join("agentspace_ca.pub"),
            "ssh-ed25519 AAAAcakey agentspace-ca NAME 2026-11-20\r\n",
        )
        .unwrap();
        w
    }

    fn record(w: &World, project: &str) -> agentspace_hosts::Host {
        let hosts = agentspace_hosts::read(&home(w)).unwrap();
        agentspace_hosts::host_for(&hosts, project).unwrap().clone()
    }

    fn host_stack(
        w: &World,
        aws: &ScriptedAws,
        instance_type: Option<&str>,
    ) -> Result<String, Failure> {
        w.run(|seams| {
            crate::deploy::host_stack(
                &w.ctx,
                &CaEnv::default(),
                PROJECT,
                instance_type,
                &FakeSay::default(),
                aws,
                seams,
            )
        })
    }

    #[test]
    fn a_missing_stack_is_created_with_the_creation_parameters_alone() {
        let w = with_ca(World::new("host-create").with_deploy_files());
        // The box's file sizes the box alone: its InstanceType never reaches a new host.
        std::fs::write(
            w.home().join(".agentspace").join("stack-parameters.json"),
            r#"{"version":0,"parameters":{"VpcId":"VPC_X","InstanceType":"m9g.4xlarge","SubnetId":"SUBNET_Y"}}"#,
        )
        .unwrap();
        w.seed_registry(None, &[(PROJECT, HostState::Creating)]);
        let aws = ScriptedAws::new(vec![missing(), outputs()]);
        assert_eq!(host_stack(&w, &aws, None).unwrap(), ID);
        let calls = aws.calls();
        assert_eq!(
            calls[0],
            format!("cloudformation describe-stacks [json] --stack-name | {STACK}")
        );
        assert!(calls[1].starts_with(&format!(
            "cloudformation deploy [pass] --stack-name | {STACK} | --template-file | "
        )));
        assert!(calls[1].ends_with(&format!(
            "--capabilities | CAPABILITY_IAM | --no-fail-on-empty-changeset | --tags | \
             project={STACK} | --parameter-overrides | SshPublicKey=ssh-ed25519 AAAAkey me@laptop | \
             SubnetId=SUBNET_Y | VpcId=VPC_X | HostName={STACK} | CaPublicKey=ssh-ed25519 AAAAcakey"
        )));
        assert!(!calls[1].contains("InstanceType") && !calls[1].contains("NAME"));
        assert_eq!(calls.len(), 3);
        assert!(w.ca_checked.get());
        let r = record(&w, PROJECT);
        assert_eq!(r.instance_id.as_deref(), Some(ID));
        assert_eq!(r.state, HostState::Creating, "deploy never sets a state");
    }

    #[test]
    fn instance_type_comes_last_on_a_create_and_alone_on_an_update() {
        let w = with_ca(World::new("host-sized").with_deploy_files());
        w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
        let aws = ScriptedAws::new(vec![status("REVIEW_IN_PROGRESS"), outputs()]);
        host_stack(&w, &aws, Some("m9g.2xlarge")).unwrap();
        assert!(
            aws.calls()[1]
                .ends_with("CaPublicKey=ssh-ed25519 AAAAcakey | InstanceType=m9g.2xlarge")
        );
        let aws = ScriptedAws::new(vec![status("UPDATE_ROLLBACK_COMPLETE"), outputs()]);
        host_stack(&w, &aws, Some("m9g.2xlarge")).unwrap();
        assert!(aws.calls()[1].ends_with(&format!(
            "--tags | project={STACK} | --parameter-overrides | InstanceType=m9g.2xlarge"
        )));
    }

    #[test]
    fn an_existing_stack_is_updated_with_no_parameter_and_reads_no_file() {
        // No public key, no parameters file and no CA: an update reads none of them.
        let w = World::new("host-update");
        w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
        let aws = ScriptedAws::new(vec![status("CREATE_COMPLETE"), outputs()]);
        host_stack(&w, &aws, None).unwrap();
        assert!(aws.calls()[1].ends_with(&format!("--tags | project={STACK}")));
        assert!(!w.ca_checked.get());
        assert_eq!(record(&w, PROJECT).instance_id.as_deref(), Some(ID));
    }

    #[test]
    fn a_new_stack_needs_subnet_and_vpc_before_any_change() {
        let w = with_ca(World::new("host-no-subnet").with_deploy_files());
        let params = w.home().join(".agentspace").join("stack-parameters.json");
        std::fs::write(&params, r#"{"version":0,"parameters":{"VpcId":"VPC_X"}}"#).unwrap();
        w.seed_registry(None, &[(PROJECT, HostState::Creating)]);
        let aws = ScriptedAws::new(vec![missing()]);
        assert_eq!(
            host_stack(&w, &aws, None).unwrap_err().message,
            format!(
                "agentspace: {} MUST name SubnetId and VpcId for a new host's stack.",
                std::path::absolute(&params).unwrap().display()
            )
        );
        assert_eq!(aws.calls().len(), 1);
    }

    #[test]
    fn a_status_no_update_starts_from_fails_before_any_change() {
        let w = World::new("host-rollback");
        w.seed_registry(None, &[(PROJECT, HostState::Creating)]);
        let aws = ScriptedAws::new(vec![status("ROLLBACK_COMPLETE")]);
        let m = host_stack(&w, &aws, None).unwrap_err().message;
        assert!(m.starts_with(&format!("agentspace: Stack '{STACK}' is ROLLBACK_COMPLETE")));
        assert_eq!(aws.calls().len(), 1);
        let aws = ScriptedAws::new(vec![Ok(json!({"Stacks": [{}]}))]);
        assert_eq!(
            host_stack(&w, &aws, None).unwrap_err().message,
            "agentspace: Unexpected answer from cloudformation describe-stacks: no Stacks[0].StackStatus."
        );
        let aws = ScriptedAws::new(vec![Err(AwsError::SignIn)]);
        assert_eq!(
            host_stack(&w, &aws, None).unwrap_err().message,
            SIGN_IN_MESSAGE
        );
    }

    #[test]
    fn a_project_with_no_record_is_refused() {
        let w = World::new("host-no-record");
        let aws = ScriptedAws::new(vec![]);
        assert_eq!(
            host_stack(&w, &aws, None).unwrap_err().message,
            "agentspace: Project 'example' has no host; provision -Host example makes one."
        );
        assert!(aws.calls().is_empty());
    }

    fn provision(w: &World, aws: &dyn Aws, say: &FakeSay) -> Result<(), Failure> {
        w.run(|seams| {
            crate::provision_using(
                &w.ctx,
                &CaEnv::default(),
                PROJECT,
                say,
                aws,
                &FakeRemote::default(),
                seams,
            )
        })
    }

    fn log_lines(w: &World) -> Vec<String> {
        let path = agentspace_hosts::host_log_path(&home(w), PROJECT);
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|l| l.split_once(' ').unwrap().1.to_string())
            .collect()
    }

    #[test]
    fn an_unlisted_project_with_no_record_writes_nothing() {
        let w = World::new("prov-unlisted");
        let aws = ScriptedAws::new(vec![]);
        let f = provision(&w, &aws, &FakeSay::default()).unwrap_err();
        assert_eq!(
            f.message,
            format!(
                "agentspace: Project 'example' has no host and is not in {}, so no host is made for it.",
                agentspace_hosts::projects_path(&home(&w)).display()
            )
        );
        assert!(!agentspace_hosts::hosts_dir(&home(&w)).exists());
        assert!(!agentspace_hosts::hosts_path(&home(&w)).exists());
        assert!(aws.calls().is_empty());
    }

    #[test]
    fn a_failed_deploy_records_failed_for_a_listed_new_host() {
        let w = World::new("prov-failed");
        let agentspace = w.home().join(".agentspace");
        std::fs::create_dir_all(&agentspace).unwrap();
        std::fs::write(
            agentspace.join("projects.json"),
            r#"{"version":0,"projects":[{"name":"example","approved":"2026-11-20"}]}"#,
        )
        .unwrap();
        let aws = ScriptedAws::new(vec![status("CREATE_IN_PROGRESS")]);
        let say = FakeSay::default();
        let f = provision(&w, &aws, &say).unwrap_err();
        let want = format!(
            "agentspace: Stack '{STACK}' is CREATE_IN_PROGRESS; try again once that operation has finished."
        );
        assert_eq!(f.message, want);
        let r = record(&w, PROJECT);
        assert_eq!(r.state, HostState::Failed);
        assert_eq!(r.stack, STACK);
        assert!(r.last_attempt_at.is_some());
        assert_eq!(
            log_lines(&w),
            vec![
                "run begun".to_string(),
                format!(
                    "deploy failed reason={}",
                    want.strip_prefix("agentspace: ").unwrap()
                ),
            ]
        );
        assert_eq!(
            say.lines(),
            vec!["progress: Provisioning the host of 'example'...".to_string()]
        );
    }

    #[test]
    fn a_failed_step_leaves_a_ready_host_ready_with_its_start_time() {
        let w = World::new("prov-ready");
        w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
        // A project list that is not valid is never read for a host with a record.
        let agentspace = w.home().join(".agentspace");
        std::fs::write(agentspace.join("projects.json"), "not json").unwrap();
        let aws = ScriptedAws::new(vec![status("CREATE_COMPLETE"), outputs()]);
        let mut describe = Err(AwsError::Failed("no such instance".into()));
        std::mem::swap(&mut *aws.inner.describe.borrow_mut(), &mut describe);
        let f = provision(&w, &aws, &FakeSay::default()).unwrap_err();
        assert_eq!(f.message, "agentspace: no such instance");
        let r = record(&w, PROJECT);
        assert_eq!(r.state, HostState::Ready);
        assert_eq!(r.instance_id.as_deref(), Some(ID));
        assert!(r.last_attempt_at.is_some());
        assert_eq!(
            log_lines(&w),
            vec![
                "run begun".to_string(),
                "deploy done".to_string(),
                "start failed reason=no such instance".to_string(),
            ]
        );
    }

    /// Step 5 on a host that is not the box: the start-and-wait's `journalctl` hint names the
    /// host's own alias, never the box's (operator.md, From the command line). The fake AWS has no
    /// SSM to wait on, so the deadline's words, as `agentspace-box` writes them, arrive here as the
    /// step's failure text.
    #[test]
    fn a_hosts_failed_start_names_the_hosts_own_alias_in_its_hint() {
        let w = World::new("prov-hint");
        w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
        let aws = ScriptedAws::new(vec![status("CREATE_COMPLETE"), outputs()]);
        let mut describe = Err(AwsError::Failed(format!(
            "{ID} was not ready within 15 minutes; check: ssh agentspace journalctl -u agentspace-patch"
        )));
        std::mem::swap(&mut *aws.inner.describe.borrow_mut(), &mut describe);
        let f = provision(&w, &aws, &FakeSay::default()).unwrap_err();
        let want = format!(
            "{ID} was not ready within 15 minutes; check: ssh agentspace-host.example journalctl -u agentspace-patch"
        );
        assert_eq!(f.message, format!("agentspace: {want}"));
        assert_eq!(
            log_lines(&w).last().unwrap(),
            &format!("start failed reason={want}")
        );
    }

    /// Step 6's patch wait running out: the run logs `sync failed` with the deadline's message as
    /// its reason, leaves a `ready` host `ready`, prints no sign-in line, and runs no bootstrap.
    #[test]
    fn a_patch_wait_that_runs_out_fails_step_6_with_its_message() {
        let w = World::new("prov-deadline").with_deploy_files();
        w.seed_registry(None, &[]);
        let aws = FakeAws::default();
        *aws.answer.borrow_mut() = "pending";
        let say = FakeSay::default();
        let f = w
            .run(|seams| {
                crate::provision_using(
                    &w.ctx,
                    &CaEnv::default(),
                    agentspace_hosts::BOX_PROJECT,
                    &say,
                    &aws,
                    &FakeRemote::default(),
                    seams,
                )
            })
            .unwrap_err();
        let want = format!(
            "{ID} did not finish its patch run within 30 minutes; check: ssh agentspace journalctl -u agentspace-patch"
        );
        assert_eq!(f.message, format!("agentspace: {want}"));
        let path = agentspace_hosts::host_log_path(&home(&w), agentspace_hosts::BOX_PROJECT);
        let lines: Vec<String> = std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|l| l.split_once(' ').unwrap().1.to_string())
            .collect();
        assert_eq!(
            lines,
            vec![
                "run begun".to_string(),
                "deploy done".to_string(),
                "start done".to_string(),
                format!("sync failed reason={want}"),
            ]
        );
        assert_eq!(
            record(&w, agentspace_hosts::BOX_PROJECT).state,
            HostState::Ready
        );
        assert!(
            say.lines()
                .iter()
                .all(|l| !l.contains("Sign in on this host") && !l.contains("bootstrap"))
        );
    }

    #[test]
    fn a_held_run_lock_fails_with_one_log_line_and_no_record() {
        let w = World::new("prov-locked");
        w.seed_registry(None, &[(PROJECT, HostState::Creating)]);
        let before = std::fs::read(agentspace_hosts::hosts_path(&home(&w))).unwrap();
        let held = agentspace_hosts::try_run_lock(&home(&w), PROJECT)
            .unwrap()
            .unwrap();
        let aws = ScriptedAws::new(vec![]);
        let f = provision(&w, &aws, &FakeSay::default()).unwrap_err();
        drop(held);
        assert_eq!(
            f.message,
            "agentspace: A provisioning run for 'example' is already running."
        );
        assert_eq!(
            log_lines(&w),
            vec![
                "run failed reason=A provisioning run for 'example' is already running."
                    .to_string()
            ]
        );
        assert_eq!(
            std::fs::read(agentspace_hosts::hosts_path(&home(&w))).unwrap(),
            before
        );
        assert!(aws.calls().is_empty());
    }

    /// The tests of v0.09-T03-S02: the type a deploy records and the one a provisioning run finds,
    /// the box's sizing, and `deploy`'s lock (operator.md, deploy and provision step 4; D67, D68,
    /// D71; pins P5, P11, P22, P23, P24, P26, P30, P32, P34, P35 and P36).
    mod sized {
        use super::*;
        use std::ffi::OsString;

        /// What one AWS call found when it was made: the registry's text, and whether each lock
        /// was free to take.
        struct Seen {
            call: String,
            hosts: Option<String>,
            run_lock_free: bool,
            hosts_lock_free: bool,
            leases_lock_free: bool,
        }

        /// Records, at each AWS call, the registry's text and the state of the three locks, then
        /// answers as [`ScriptedAws`] does.
        struct Probe {
            inner: ScriptedAws,
            home: PathBuf,
            project: String,
            state_dir: PathBuf,
            seen: RefCell<Vec<Seen>>,
        }

        impl Probe {
            fn new(w: &World, project: &str, stacks: Vec<Result<Value, AwsError>>) -> Probe {
                Probe {
                    inner: ScriptedAws::new(stacks),
                    home: home(w),
                    project: project.to_string(),
                    state_dir: home(w).join(".agentspace"),
                    seen: RefCell::default(),
                }
            }

            fn calls(&self) -> Vec<String> {
                self.inner.calls()
            }

            /// What the first call to `service operation` found.
            fn at(&self, call: &str) -> Seen {
                let mut seen = self.seen.borrow_mut();
                let i = seen
                    .iter()
                    .position(|s| s.call == call)
                    .unwrap_or_else(|| panic!("No call {call}"));
                seen.remove(i)
            }
        }

        impl Aws for Probe {
            fn call(&self, call: &Call) -> Result<Value, AwsError> {
                let hosts = std::fs::read_to_string(agentspace_hosts::hosts_path(&self.home)).ok();
                let run_lock_free = agentspace_hosts::try_run_lock(&self.home, &self.project)
                    .is_ok_and(|l| l.is_some());
                let hosts_lock_free =
                    agentspace_hosts::try_lock(&self.home).is_ok_and(|l| l.is_some());
                let leases_lock_free = agentspace_state::lock(&self.state_dir).is_ok();
                self.seen.borrow_mut().push(Seen {
                    call: format!("{} {}", call.service, call.operation),
                    hosts,
                    run_lock_free,
                    hosts_lock_free,
                    leases_lock_free,
                });
                self.inner.call(call)
            }
        }

        /// A remote that notes whether the host's `provision.lock` was held at each call.
        struct HeldRemote {
            inner: FakeRemote,
            home: PathBuf,
            project: String,
            calls: Cell<usize>,
            held: Cell<usize>,
        }

        impl Remote for HeldRemote {
            fn ssh(&self, o: &[&str], c: Option<&str>) -> Result<i32, Failure> {
                self.note();
                self.inner.ssh(o, c)
            }
            fn scp(&self, a: &[&str]) -> Result<i32, Failure> {
                self.note();
                self.inner.scp(a)
            }
        }

        impl HeldRemote {
            fn note(&self) {
                self.calls.set(self.calls.get() + 1);
                let free = agentspace_hosts::try_run_lock(&self.home, &self.project)
                    .is_ok_and(|l| l.is_some());
                if !free {
                    self.held.set(self.held.get() + 1);
                }
            }
        }

        fn typed(t: &str) -> Value {
            json!([{"ParameterKey": "InstanceType", "ParameterValue": t}])
        }

        fn answer_with(parameters: Value) -> Result<Value, AwsError> {
            Ok(json!({"Stacks": [{
                "Outputs": [{"OutputKey": "InstanceId", "OutputValue": ID}],
                "Parameters": parameters
            }]}))
        }

        fn set_type(w: &World, project: &str, t: Option<&str>) {
            agentspace_hosts::set_instance_type(&home(w), project, t).unwrap();
        }

        fn registry_text(w: &World) -> Option<String> {
            std::fs::read_to_string(agentspace_hosts::hosts_path(&home(w))).ok()
        }

        /// The `deploy` entry, with the pinned seams.
        fn person(
            w: &World,
            aws: &dyn Aws,
            remote: &dyn Remote,
            target: crate::Target<'_>,
            instance_type: Option<&str>,
        ) -> Result<(), Failure> {
            w.run(|seams| {
                crate::deploy_target_sized_using(
                    &w.ctx,
                    &CaEnv::default(),
                    target,
                    instance_type,
                    &FakeSay::default(),
                    aws,
                    remote,
                    seams,
                )
            })
        }

        fn deploy_call(calls: &[String]) -> String {
            calls
                .iter()
                .find(|c| c.starts_with("cloudformation deploy"))
                .cloned()
                .expect("A cloudformation deploy call")
        }

        // ---- A person's deploy of the box: the file's InstanceType, and -InstanceType ----

        #[test]
        fn the_boxs_plain_deploy_passes_no_instance_type_whatever_the_file_names() {
            let w = World::new("sized-box-plain").with_deploy_files();
            std::fs::write(
                w.home().join(".agentspace").join("stack-parameters.json"),
                r#"{"version":0,"parameters":{"VpcId":"VPC_X","InstanceType":"m9g.4xlarge","SubnetId":"SUBNET_Y"}}"#,
            )
            .unwrap();
            let before =
                std::fs::read(w.home().join(".agentspace").join("stack-parameters.json")).unwrap();
            let aws = ScriptedAws::new(vec![]);
            person(&w, &aws, &FakeRemote::default(), crate::Target::Box, None).unwrap();
            let call = deploy_call(&aws.calls());
            assert!(call.ends_with("VpcId=VPC_X | SubnetId=SUBNET_Y"), "{call}");
            assert!(!call.contains("InstanceType"), "{call}");
            // The file is left byte for byte as it was.
            assert_eq!(
                std::fs::read(w.home().join(".agentspace").join("stack-parameters.json")).unwrap(),
                before
            );
        }

        #[test]
        fn the_boxs_deploy_with_a_type_passes_it_last_for_both_names_of_the_box() {
            for target in [crate::Target::Box, crate::Target::Host("swimcoach")] {
                let w = World::new("sized-box-typed").with_deploy_files();
                std::fs::write(
                    w.home().join(".agentspace").join("stack-parameters.json"),
                    r#"{"version":0,"parameters":{"VpcId":"VPC_X","InstanceType":"m9g.4xlarge","SubnetId":"SUBNET_Y"}}"#,
                )
                .unwrap();
                let aws = ScriptedAws::new(vec![]);
                person(
                    &w,
                    &aws,
                    &FakeRemote::default(),
                    target,
                    Some("m9g.2xlarge"),
                )
                .unwrap();
                let call = deploy_call(&aws.calls());
                // The override is last (pin P32), and the file's own value is never among them.
                assert!(
                    call.ends_with("VpcId=VPC_X | SubnetId=SUBNET_Y | InstanceType=m9g.2xlarge"),
                    "{call}"
                );
                assert!(!call.contains("m9g.4xlarge"), "{call}");
            }
        }

        // ---- Step 4: the removal before the deploy ----

        #[test]
        fn a_deploy_with_a_type_removes_the_record_type_before_the_stack_changes() {
            let w = with_ca(World::new("sized-remove").with_deploy_files());
            w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
            set_type(&w, PROJECT, Some("m9g.large"));
            // The stack reports a type other than the one asked for: the record takes the stack's.
            let aws = Probe::new(
                &w,
                PROJECT,
                vec![status("UPDATE_COMPLETE"), answer_with(typed("m9g.xlarge"))],
            );
            w.run(|seams| {
                crate::deploy::host_stack(
                    &w.ctx,
                    &CaEnv::default(),
                    PROJECT,
                    Some("m9g.2xlarge"),
                    &FakeSay::default(),
                    &aws,
                    seams,
                )
            })
            .unwrap();
            let at_deploy = aws.at("cloudformation deploy");
            let text = at_deploy.hosts.unwrap();
            assert!(!text.contains("instance_type"), "{text}");
            assert!(
                aws.calls()
                    .last()
                    .unwrap()
                    .starts_with("cloudformation describe-stacks")
            );
            let r = record(&w, PROJECT);
            assert_eq!(r.instance_type.as_deref(), Some("m9g.xlarge"));
            assert_eq!(r.instance_id.as_deref(), Some(ID));
            assert_eq!(r.state, HostState::Ready);
        }

        #[test]
        fn a_deploy_with_the_type_the_record_already_holds_removes_it_all_the_same() {
            let w = World::new("sized-same");
            w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
            set_type(&w, PROJECT, Some("m9g.2xlarge"));
            let aws = Probe::new(
                &w,
                PROJECT,
                vec![status("UPDATE_COMPLETE"), answer_with(typed("m9g.2xlarge"))],
            );
            host_stack_with(&w, &aws, Some("m9g.2xlarge")).unwrap();
            assert!(
                !aws.at("cloudformation deploy")
                    .hosts
                    .unwrap()
                    .contains("instance_type")
            );
        }

        fn host_stack_with(
            w: &World,
            aws: &dyn Aws,
            instance_type: Option<&str>,
        ) -> Result<String, Failure> {
            w.run(|seams| {
                crate::deploy::host_stack(
                    &w.ctx,
                    &CaEnv::default(),
                    PROJECT,
                    instance_type,
                    &FakeSay::default(),
                    aws,
                    seams,
                )
            })
        }

        #[test]
        fn a_deploy_without_a_type_writes_nothing_before_the_stack_and_keeps_the_type_if_it_fails()
        {
            let w = World::new("sized-keep");
            w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
            set_type(&w, PROJECT, Some("m9g.large"));
            let before = registry_text(&w).unwrap();
            let mut aws = Probe::new(&w, PROJECT, vec![status("UPDATE_COMPLETE")]);
            aws.inner.deploy = Err(AwsError::Failed("failed".into()));
            let f = host_stack_with(&w, &aws, None).unwrap_err();
            assert_eq!(
                f.message,
                "agentspace: Stack deploy failed; see the stack events in the console."
            );
            assert_eq!(aws.at("cloudformation deploy").hosts.unwrap(), before);
            // A failed deploy that asked for no change kept the type (pin P30).
            assert_eq!(
                record(&w, PROJECT).instance_type.as_deref(),
                Some("m9g.large")
            );
            assert_eq!(registry_text(&w).unwrap(), before);
        }

        #[test]
        fn a_deploy_with_a_type_that_fails_leaves_the_record_with_none() {
            let w = World::new("sized-fail");
            w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
            set_type(&w, PROJECT, Some("m9g.large"));
            let mut aws = Probe::new(&w, PROJECT, vec![status("UPDATE_COMPLETE")]);
            aws.inner.deploy = Err(AwsError::Failed("failed".into()));
            host_stack_with(&w, &aws, Some("m9g.2xlarge")).unwrap_err();
            assert_eq!(record(&w, PROJECT).instance_type, None);
        }

        #[test]
        fn a_missing_registry_is_written_as_the_seed_for_a_type_and_not_for_a_plain_deploy() {
            // With -InstanceType: the box's seed record, with no type, is on disk when the stack
            // changes (pins P2c, P11 and P31).
            let w = World::new("sized-seed").with_deploy_files();
            let aws = Probe::new(&w, agentspace_hosts::BOX_PROJECT, vec![]);
            person(
                &w,
                &aws,
                &FakeRemote::default(),
                crate::Target::Box,
                Some("m9g.2xlarge"),
            )
            .unwrap();
            let text = aws.at("cloudformation deploy").hosts.unwrap();
            assert!(text.contains("\"project\":\"swimcoach\""), "{text}");
            assert!(!text.contains("instance_type"), "{text}");
            // Without it: no registry is written before the stack's deploy.
            let w = World::new("sized-seed-plain").with_deploy_files();
            let aws = Probe::new(&w, agentspace_hosts::BOX_PROJECT, vec![]);
            person(&w, &aws, &FakeRemote::default(), crate::Target::Box, None).unwrap();
            assert_eq!(aws.at("cloudformation deploy").hosts, None);
            // After a successful deploy the record holds what the stack reports, with the ID.
            let r = record(&w, agentspace_hosts::BOX_PROJECT);
            assert_eq!(r.instance_type.as_deref(), Some(STACK_TYPE));
            assert_eq!(r.instance_id.as_deref(), Some(ID));
        }

        // ---- Step 5: the one read ----

        #[test]
        fn a_deploy_records_the_type_and_the_id_from_one_describe_stacks_answer() {
            let w = World::new("sized-one-read");
            w.seed_registry(None, &[(PROJECT, HostState::Creating)]);
            let aws = ScriptedAws::new(vec![
                status("CREATE_COMPLETE"),
                answer_with(json!([
                    {"ParameterKey": "RootVolumeGiB", "ParameterValue": "8"},
                    {"ParameterKey": "InstanceType", "ParameterValue": "m9g.4xlarge"}
                ])),
            ]);
            host_stack_with(&w, &aws, None).unwrap();
            let describes = aws
                .calls()
                .iter()
                .filter(|c| c.starts_with("cloudformation describe-stacks"))
                .count();
            // One for the existence check, one for the read-back that gives both values.
            assert_eq!(describes, 2);
            let r = record(&w, PROJECT);
            assert_eq!(r.instance_type.as_deref(), Some("m9g.4xlarge"));
            assert_eq!(r.instance_id.as_deref(), Some(ID));
            assert_eq!(r.state, HostState::Creating, "Deploy never sets a state");
        }

        #[test]
        fn a_stack_without_a_usable_instance_type_fails_the_deploy_and_leaves_none() {
            let bad = [
                json!([]),
                json!([{"ParameterKey": "RootVolumeGiB", "ParameterValue": "8"}]),
                json!([{"ParameterKey": "InstanceType"}]),
                json!([{"ParameterKey": "InstanceType", "ParameterValue": 4}]),
                json!([{"ParameterKey": "InstanceType", "ParameterValue": "M9G.xlarge"}]),
                json!([{"ParameterKey": "InstanceType", "ParameterValue": "m9g.xlarge\n"}]),
                json!([{"ParameterKey": "InstanceType", "ParameterValue": "m9g"}]),
                json!([
                    {"ParameterKey": "InstanceType", "ParameterValue": "m9g.xlarge"},
                    {"ParameterKey": "InstanceType", "ParameterValue": "m9g.xlarge"}
                ]),
            ];
            for parameters in bad {
                let w = World::new("sized-bad-type");
                w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
                set_type(&w, PROJECT, Some("m9g.large"));
                let aws =
                    ScriptedAws::new(vec![status("UPDATE_COMPLETE"), answer_with(parameters)]);
                let f = host_stack_with(&w, &aws, None).unwrap_err();
                assert_eq!(
                    f.message,
                    format!("agentspace: Stack '{STACK}' has no InstanceType parameter.")
                );
                // The instance ID it read is written alone; the type is gone.
                let r = record(&w, PROJECT);
                assert_eq!(r.instance_id.as_deref(), Some(ID));
                assert_eq!(r.instance_type, None);
            }
            // The answer holds no Parameters at all.
            let w = World::new("sized-no-parameters");
            w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
            set_type(&w, PROJECT, Some("m9g.large"));
            let aws = ScriptedAws::new(vec![status("UPDATE_COMPLETE"), outputs_alone()]);
            host_stack_with(&w, &aws, None).unwrap_err();
            assert_eq!(record(&w, PROJECT).instance_type, None);
        }

        fn outputs_alone() -> Result<Value, AwsError> {
            Ok(json!({"Stacks": [{"Outputs": [{"OutputKey": "InstanceId", "OutputValue": ID}]}]}))
        }

        #[test]
        fn a_deploy_that_cannot_read_the_type_does_not_go_on_to_sync() {
            let w = World::new("sized-no-sync").with_deploy_files();
            let aws = ScriptedAws::new(vec![]);
            *aws.inner.stack.borrow_mut() =
                json!({"Stacks": [{"Outputs": [{"OutputKey": "InstanceId", "OutputValue": ID}]}]});
            let remote = FakeRemote::default();
            let f = person(&w, &aws, &remote, crate::Target::Box, None).unwrap_err();
            assert_eq!(
                f.message,
                "agentspace: Stack 'agentbox' has no InstanceType parameter."
            );
            assert!(remote.lines().is_empty());
            assert!(w.pushed.borrow().is_empty());
            let r = record(&w, agentspace_hosts::BOX_PROJECT);
            assert_eq!(
                (r.instance_id.as_deref(), r.instance_type),
                (Some(ID), None)
            );
        }

        #[test]
        fn a_describe_stacks_that_fails_after_the_deploy_leaves_neither_value() {
            let w = World::new("sized-describe-fails");
            w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
            set_type(&w, PROJECT, Some("m9g.large"));
            let aws = ScriptedAws::new(vec![status("UPDATE_COMPLETE"), Err(AwsError::SignIn)]);
            assert_eq!(
                host_stack_with(&w, &aws, None).unwrap_err().message,
                SIGN_IN_MESSAGE
            );
            let r = record(&w, PROJECT);
            assert_eq!((r.instance_id, r.instance_type), (None, None));
        }

        #[test]
        fn a_stack_with_no_instance_id_output_writes_neither_value() {
            let w = World::new("sized-no-id");
            w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
            set_type(&w, PROJECT, Some("m9g.large"));
            let aws = ScriptedAws::new(vec![
                status("UPDATE_COMPLETE"),
                Ok(json!({"Stacks": [{"Outputs": [], "Parameters": typed("m9g.xlarge")}]})),
            ]);
            assert_eq!(
                host_stack_with(&w, &aws, None).unwrap_err().message,
                format!("agentspace: Stack '{STACK}' has no InstanceId output.")
            );
            let r = record(&w, PROJECT);
            assert_eq!((r.instance_id, r.instance_type), (None, None));
        }

        #[test]
        fn the_instance_type_form_is_the_registrys() {
            for ok in ["m9g.xlarge", "m9g.2xlarge", "c7g.metal-48xl", "0a.0b"] {
                assert!(crate::deploy::is_instance_type(ok), "{ok}");
            }
            let long = format!("a.{}", "b".repeat(63));
            for bad in [
                "",
                "m9g",
                ".xlarge",
                "m9g.",
                "M9g.xlarge",
                "m9g.xlarge\n",
                "m9g..xlarge",
                "m9g.x.large",
                "-m9g.xlarge",
                "m9g.-xlarge",
                "m9g xlarge",
                "m9g_x.large",
                long.as_str(),
            ] {
                assert!(!crate::deploy::is_instance_type(bad), "{bad:?}");
            }
            assert!(crate::deploy::is_instance_type(&format!(
                "a.{}",
                "b".repeat(62)
            )));
        }

        // ---- A person's deploy: the lock ----

        #[test]
        fn a_held_lock_refuses_every_deploy_before_anything_is_done() {
            let cases: [(crate::Target<'_>, &str); 3] = [
                (crate::Target::Box, "swimcoach"),
                (crate::Target::Host("swimcoach"), "swimcoach"),
                (crate::Target::Host(PROJECT), PROJECT),
            ];
            for (target, project) in cases {
                for instance_type in [None, Some("m9g.2xlarge")] {
                    let w = World::new("sized-refused").with_deploy_files();
                    w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
                    set_type(&w, project, Some("m9g.large"));
                    let before = registry_text(&w).unwrap();
                    let held = agentspace_hosts::try_run_lock(&home(&w), project)
                        .unwrap()
                        .unwrap();
                    let aws = ScriptedAws::new(vec![]);
                    let remote = FakeRemote::default();
                    let f = person(&w, &aws, &remote, target, instance_type).unwrap_err();
                    assert_eq!(
                        f.message,
                        format!(
                            "agentspace: A provisioning run or deploy for '{project}' is already running; the deploy did not run."
                        )
                    );
                    assert!(aws.calls().is_empty() && remote.lines().is_empty());
                    assert_eq!(registry_text(&w).unwrap(), before);
                    assert!(!w.operator_root().exists(), "No operator folder is made");
                    assert!(
                        !agentspace_hosts::host_log_path(&home(&w), project).exists(),
                        "The log is the run's alone"
                    );
                    drop(held);
                }
            }
        }

        #[test]
        fn the_lock_comes_before_the_public_key_and_after_the_hosts_lookup() {
            // No id_ed25519.pub in this home: a held lock is reported before the missing key.
            let w = World::new("sized-lock-order");
            let held = agentspace_hosts::try_run_lock(&home(&w), "swimcoach")
                .unwrap()
                .unwrap();
            let aws = ScriptedAws::new(vec![]);
            let f = person(&w, &aws, &FakeRemote::default(), crate::Target::Box, None).unwrap_err();
            assert!(
                f.message
                    .contains("is already running; the deploy did not run.")
            );
            drop(held);
            // A project with no record is reported first, and makes no lock.
            let f = person(
                &w,
                &aws,
                &FakeRemote::default(),
                crate::Target::Host("nothing"),
                None,
            )
            .unwrap_err();
            assert_eq!(
                f.message,
                "agentspace: Project 'nothing' has no host; provision -Host nothing makes one."
            );
            assert!(!agentspace_hosts::host_run_lock_path(&home(&w), "nothing").exists());
            // A registry that is not valid is reported before the lock too.
            std::fs::write(agentspace_hosts::hosts_path(&home(&w)), "not json").unwrap();
            let f = person(&w, &aws, &FakeRemote::default(), crate::Target::Box, None).unwrap_err();
            assert!(f.message.starts_with("agentspace: The host registry "));
            assert!(aws.calls().is_empty());
        }

        #[test]
        fn a_lock_that_cannot_be_opened_is_not_a_free_lock() {
            let w = World::new("sized-lock-folder").with_deploy_files();
            let path = agentspace_hosts::host_run_lock_path(&home(&w), "swimcoach");
            std::fs::create_dir_all(&path).unwrap();
            let aws = ScriptedAws::new(vec![]);
            let f = person(&w, &aws, &FakeRemote::default(), crate::Target::Box, None).unwrap_err();
            assert!(
                f.message
                    .starts_with(&format!("agentspace: Cannot take {}: ", path.display()))
                    && f.message.ends_with('.')
                    && !f.message.contains("os error"),
                "{}",
                f.message
            );
            assert!(aws.calls().is_empty());
        }

        #[test]
        fn a_deploy_holds_its_lock_through_every_call_and_its_sync_and_frees_it_at_the_end() {
            let w = World::new("sized-lock-life").with_deploy_files();
            let aws = Probe::new(&w, "swimcoach", vec![]);
            let remote = HeldRemote {
                inner: FakeRemote::default(),
                home: home(&w),
                project: "swimcoach".into(),
                calls: Cell::new(0),
                held: Cell::new(0),
            };
            person(&w, &aws, &remote, crate::Target::Box, Some("m9g.2xlarge")).unwrap();
            assert!(remote.calls.get() > 0);
            assert_eq!(
                remote.held.get(),
                remote.calls.get(),
                "Held at every ssh and scp"
            );
            assert!(!aws.seen.borrow().is_empty());
            assert!(
                aws.seen.borrow().iter().all(|s| !s.run_lock_free),
                "Held at every AWS call"
            );
            // The registry's lock is never held across an AWS call.
            assert!(aws.seen.borrow().iter().all(|s| s.hosts_lock_free));
            // Free once the deploy has ended.
            assert!(
                agentspace_hosts::try_run_lock(&home(&w), "swimcoach")
                    .unwrap()
                    .is_some()
            );
        }

        // ---- A provisioning run: the type it finds ----

        const EC2: &str = "ec2";
        const LIVE: &str = "2099-01-01T00:00:00Z";
        const EXPIRED: &str = "2000-01-01T00:00:00Z";

        fn lease(
            n: u32,
            project: &str,
            state: &str,
            size: &str,
            driver: Option<&str>,
            expires_at: Option<&str>,
        ) -> agentspace_state::Record {
            let ready = state == "ready";
            agentspace_state::Record {
                sized_for: None,
                plan: None,
                plan_source: None,
                id: format!("8d41b6a2-0c3e-4f57-a1d9-2e6b7c8f90{n:02x}"),
                project: project.to_string(),
                state: state.to_string(),
                platform: "linux/arm64".to_string(),
                profile: "agent-linux".to_string(),
                size: size.to_string(),
                ttl_seconds: 3600,
                created_at: "2026-11-23T10:00:00Z".to_string(),
                expires_at: expires_at.map(str::to_string),
                released_at: (state == "released").then(|| "2026-11-23T11:00:00Z".to_string()),
                ssh: ready.then(|| agentspace_state::Ssh {
                    host_alias: "agentspace-example-0a1b2c3d".to_string(),
                    user: "agent".to_string(),
                }),
                reasons: (state == "unsatisfiable").then(|| {
                    vec![agentspace_state::Reason {
                        code: "no_capacity".to_string(),
                        message: "No capacity.".to_string(),
                    }]
                }),
                driver: driver.map(str::to_string),
            }
        }

        fn pending(n: u32, project: &str, size: &str) -> agentspace_state::Record {
            lease(n, project, "pending", size, Some(EC2), None)
        }

        fn write_leases(dir: &Path, leases: Vec<agentspace_state::Record>) {
            std::fs::create_dir_all(dir).unwrap();
            let bytes = agentspace_state::serialize(&agentspace_state::StateFile { leases })
                .expect("Valid leases");
            std::fs::write(dir.join("leases.json"), bytes).unwrap();
        }

        fn no_env(_: &str) -> Option<OsString> {
            None
        }

        /// A run that stops at its start step, after its deploy step.
        fn run_to_start(
            w: &World,
            project: &str,
            aws: &dyn Aws,
            env: &dyn Fn(&str) -> Option<OsString>,
        ) -> Result<(), Failure> {
            w.run(|seams| {
                crate::provision::run(
                    &w.ctx,
                    &CaEnv::default(),
                    project,
                    &FakeSay::default(),
                    aws,
                    &FakeRemote::default(),
                    seams,
                    env,
                )
            })
        }

        /// A `ready` host of `example` that holds `m9g.large`, whose stack exists, and whose AWS
        /// stops the run at its start step.
        fn ready_host(name: &str) -> (World, ScriptedAws) {
            let w = World::new(name);
            w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
            set_type(&w, PROJECT, Some("m9g.large"));
            let aws = ScriptedAws::new(vec![status("UPDATE_COMPLETE"), outputs()]);
            *aws.inner.describe.borrow_mut() = Err(AwsError::Failed("stop here".into()));
            (w, aws)
        }

        fn deploy_log(w: &World) -> String {
            log_lines(w)
                .into_iter()
                .find(|l| l.starts_with("deploy "))
                .expect("A deploy line")
        }

        #[test]
        fn a_run_gives_its_deploy_the_type_of_the_largest_pending_size() {
            let cases: Vec<(Vec<agentspace_state::Record>, Option<&str>)> = vec![
                (
                    vec![pending(1, PROJECT, "s"), pending(2, PROJECT, "l")],
                    Some("m9g.2xlarge"),
                ),
                (vec![pending(1, PROJECT, "m")], Some("m9g.xlarge")),
                (vec![pending(1, PROJECT, "xxl")], None),
                (
                    vec![
                        lease(1, PROJECT, "pending", "l", Some("other"), None),
                        lease(2, PROJECT, "pending", "l", None, None),
                    ],
                    None,
                ),
                (vec![pending(1, "elsewhere", "xl")], None),
                (
                    vec![
                        lease(1, PROJECT, "released", "xl", Some(EC2), None),
                        lease(2, PROJECT, "unsatisfiable", "xl", Some(EC2), None),
                    ],
                    None,
                ),
                // An expired `ready` record holds nothing (pin P2(a)), so the type is given.
                (
                    vec![
                        lease(1, PROJECT, "ready", "m", Some(EC2), Some(EXPIRED)),
                        pending(2, PROJECT, "l"),
                    ],
                    Some("m9g.2xlarge"),
                ),
                (vec![], None),
            ];
            for (leases, want) in cases {
                let (w, aws) = ready_host("run-type");
                write_leases(&home(&w).join(".agentspace"), leases);
                run_to_start(&w, PROJECT, &aws, &no_env).unwrap_err();
                let call = deploy_call(&aws.calls());
                match want {
                    Some(t) => {
                        assert!(
                            call.ends_with(&format!(
                                "--tags | project={STACK} | --parameter-overrides | InstanceType={t}"
                            )),
                            "{call}"
                        );
                        assert_eq!(deploy_log(&w), format!("deploy done type={t}"));
                    }
                    None => {
                        assert!(
                            call.ends_with(&format!("--tags | project={STACK}")),
                            "{call}"
                        );
                        assert_eq!(deploy_log(&w), "deploy done");
                    }
                }
                // The deploy's step 5 recorded the type the stack reports, whatever was passed.
                assert_eq!(
                    record(&w, PROJECT).instance_type.as_deref(),
                    Some(STACK_TYPE)
                );
            }
        }

        #[test]
        fn a_run_with_no_lease_state_file_passes_no_type_and_goes_on() {
            let (w, aws) = ready_host("run-no-file");
            assert!(!home(&w).join(".agentspace").join("leases.json").exists());
            let f = run_to_start(&w, PROJECT, &aws, &no_env).unwrap_err();
            assert_eq!(f.message, "agentspace: stop here");
            assert!(!deploy_call(&aws.calls()).contains("InstanceType"));
            assert_eq!(deploy_log(&w), "deploy done");
        }

        #[test]
        fn a_live_ready_lease_holds_the_size_and_the_deploy_still_records_the_type() {
            let (w, aws) = ready_host("run-held");
            write_leases(
                &home(&w).join(".agentspace"),
                vec![
                    lease(1, PROJECT, "ready", "m", Some(EC2), Some(LIVE)),
                    pending(2, PROJECT, "l"),
                ],
            );
            run_to_start(&w, PROJECT, &aws, &no_env).unwrap_err();
            let call = deploy_call(&aws.calls());
            assert!(!call.contains("InstanceType"), "{call}");
            assert_eq!(deploy_log(&w), "deploy done");
            assert_eq!(
                record(&w, PROJECT).instance_type.as_deref(),
                Some(STACK_TYPE)
            );
            // Another project's live lease holds nothing of this host.
            let (w, aws) = ready_host("run-held-elsewhere");
            write_leases(
                &home(&w).join(".agentspace"),
                vec![
                    lease(1, "elsewhere", "ready", "m", Some(EC2), Some(LIVE)),
                    pending(2, PROJECT, "l"),
                ],
            );
            run_to_start(&w, PROJECT, &aws, &no_env).unwrap_err();
            assert!(deploy_call(&aws.calls()).ends_with("InstanceType=m9g.2xlarge"));
        }

        #[test]
        fn a_run_removes_the_type_and_frees_its_locks_before_its_first_aws_call() {
            for leases in [vec![pending(1, PROJECT, "l")], vec![]] {
                let w = World::new("run-order");
                w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
                set_type(&w, PROJECT, Some("m9g.large"));
                write_leases(&home(&w).join(".agentspace"), leases);
                let aws = Probe::new(&w, PROJECT, vec![status("UPDATE_COMPLETE"), outputs()]);
                *aws.inner.inner.describe.borrow_mut() = Err(AwsError::Failed("stop".into()));
                run_to_start(&w, PROJECT, &aws, &no_env).unwrap_err();
                let seen = aws.seen.borrow();
                let first = &seen[0];
                assert!(first.call.starts_with("cloudformation describe-stacks"));
                let text = first.hosts.as_deref().unwrap();
                assert!(!text.contains("instance_type"), "{text}");
                assert!(first.hosts_lock_free && first.leases_lock_free);
                assert!(
                    !first.run_lock_free,
                    "The run's own lock is held throughout"
                );
                assert!(
                    seen.iter()
                        .all(|s| s.hosts_lock_free && s.leases_lock_free && !s.run_lock_free)
                );
            }
        }

        #[test]
        fn a_run_reads_the_state_folder_its_environment_names_and_locks_there() {
            let (w, aws) = ready_host("run-state-dir");
            let state = w.dir.path().join("elsewhere").join("state");
            write_leases(&state, vec![pending(1, PROJECT, "xl")]);
            // The home's own folder holds a different answer, which must not be read.
            write_leases(
                &home(&w).join(".agentspace"),
                vec![pending(1, PROJECT, "s")],
            );
            let value = state.clone().into_os_string();
            let env = move |name: &str| (name == "AGENTSPACE_STATE_DIR").then(|| value.clone());
            run_to_start(&w, PROJECT, &aws, &env).unwrap_err();
            assert!(deploy_call(&aws.calls()).ends_with("InstanceType=m9g.4xlarge"));
            assert!(state.join("leases.lock").is_file());
        }

        #[test]
        fn a_run_on_the_box_finds_swimcoachs_leases_and_ignores_the_files_type() {
            let w = World::new("run-box").with_deploy_files();
            std::fs::write(
                w.home().join(".agentspace").join("stack-parameters.json"),
                r#"{"version":0,"parameters":{"VpcId":"VPC_X","InstanceType":"m9g.4xlarge","SubnetId":"SUBNET_Y"}}"#,
            )
            .unwrap();
            write_leases(
                &home(&w).join(".agentspace"),
                vec![pending(1, "swimcoach", "l"), pending(2, PROJECT, "xl")],
            );
            let aws = ScriptedAws::new(vec![]);
            *aws.inner.describe.borrow_mut() = Err(AwsError::Failed("stop here".into()));
            run_to_start(&w, agentspace_hosts::BOX_PROJECT, &aws, &no_env).unwrap_err();
            let call = deploy_call(&aws.calls());
            assert!(
                call.ends_with("VpcId=VPC_X | SubnetId=SUBNET_Y | InstanceType=m9g.2xlarge"),
                "{call}"
            );
            assert!(!call.contains("m9g.4xlarge"));
            let path = agentspace_hosts::host_log_path(&home(&w), agentspace_hosts::BOX_PROJECT);
            let text = std::fs::read_to_string(path).unwrap();
            assert!(text.contains(" deploy done type=m9g.2xlarge\n"), "{text}");
        }

        #[test]
        fn a_failed_deploy_step_logs_what_it_was_asked_for_and_leaves_no_type() {
            for (leases, field) in [
                (vec![pending(1, PROJECT, "l")], " type=m9g.2xlarge"),
                (vec![], ""),
            ] {
                let w = World::new("run-failed");
                w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
                set_type(&w, PROJECT, Some("m9g.large"));
                write_leases(&home(&w).join(".agentspace"), leases);
                let mut aws = ScriptedAws::new(vec![status("UPDATE_COMPLETE")]);
                aws.deploy = Err(AwsError::Failed("failed".into()));
                let f = run_to_start(&w, PROJECT, &aws, &no_env).unwrap_err();
                let message = "Stack deploy failed; see the stack events in the console.";
                assert_eq!(f.message, format!("agentspace: {message}"));
                assert_eq!(
                    deploy_log(&w),
                    format!("deploy failed{field} reason={message}")
                );
                // A `ready` host stays `ready`, and the record is left with no type (pin P30).
                let r = record(&w, PROJECT);
                assert_eq!((r.state, r.instance_type), (HostState::Ready, None));
            }
        }

        #[test]
        fn a_lease_state_file_that_is_not_valid_fails_the_deploy_step_before_any_aws_call() {
            for (state, want) in [
                (HostState::Ready, HostState::Ready),
                (HostState::Creating, HostState::Failed),
            ] {
                let w = World::new("run-bad-leases");
                w.seed_registry(None, &[(PROJECT, state)]);
                set_type(&w, PROJECT, Some("m9g.large"));
                let dir = home(&w).join(".agentspace");
                std::fs::write(dir.join("leases.json"), "not json").unwrap();
                let aws = ScriptedAws::new(vec![]);
                let f = run_to_start(&w, PROJECT, &aws, &no_env).unwrap_err();
                let prefix = format!(
                    "agentspace: The lease state file {} cannot be read: ",
                    dir.join("leases.json").display()
                );
                assert!(
                    f.message.starts_with(&prefix) && f.message.ends_with('.'),
                    "{}",
                    f.message
                );
                assert!(aws.calls().is_empty());
                // The type was removed before the file was read, and no `type` field is logged.
                let r = record(&w, PROJECT);
                assert_eq!((r.state, r.instance_type), (want, None));
                assert_eq!(
                    deploy_log(&w),
                    format!(
                        "deploy failed reason={}",
                        f.message.strip_prefix("agentspace: ").unwrap()
                    )
                );
            }
        }

        #[test]
        fn a_lease_state_file_with_two_records_of_one_id_fails_the_deploy_step() {
            let (w, aws) = ready_host("run-repeat-id");
            let dir = home(&w).join(".agentspace");
            write_leases(&dir, vec![pending(1, PROJECT, "l")]);
            let text = std::fs::read_to_string(dir.join("leases.json")).unwrap();
            let record = text
                .trim_end()
                .strip_prefix("{\"version\":0,\"leases\":[")
                .and_then(|t| t.strip_suffix("]}"))
                .unwrap()
                .to_string();
            std::fs::write(
                dir.join("leases.json"),
                format!("{{\"version\":0,\"leases\":[{record},{record}]}}"),
            )
            .unwrap();
            let f = run_to_start(&w, PROJECT, &aws, &no_env).unwrap_err();
            assert!(
                f.message.starts_with("agentspace: The lease state file ")
                    && f.message.contains(" cannot be read: ")
            );
            assert!(aws.calls().is_empty());
        }

        #[test]
        fn a_lease_state_lock_that_cannot_be_opened_fails_the_step_and_keeps_the_type() {
            let (w, aws) = ready_host("run-lease-lock-folder");
            let dir = home(&w).join(".agentspace");
            std::fs::create_dir_all(dir.join("leases.lock")).unwrap();
            let f = run_to_start(&w, PROJECT, &aws, &no_env).unwrap_err();
            let prefix = format!(
                "agentspace: Cannot take {}: ",
                dir.join("leases.lock").display()
            );
            assert!(
                f.message.starts_with(&prefix) && f.message.ends_with('.'),
                "{}",
                f.message
            );
            assert!(aws.calls().is_empty());
            // Nothing was written: the record keeps its type, and the host stays `ready`.
            let r = record(&w, PROJECT);
            assert_eq!(
                (r.state, r.instance_type.as_deref()),
                (HostState::Ready, Some("m9g.large"))
            );
            assert_eq!(
                deploy_log(&w),
                format!(
                    "deploy failed reason={}",
                    f.message.strip_prefix("agentspace: ").unwrap()
                )
            );
        }

        /// A lock held for the whole 30 seconds: this test takes that long.
        #[test]
        fn a_lease_state_lock_held_past_its_wait_fails_the_step_and_keeps_the_type() {
            let w = World::new("run-lease-lock-busy");
            w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
            set_type(&w, PROJECT, Some("m9g.large"));
            let dir = home(&w).join(".agentspace");
            let held = agentspace_state::lock(&dir).unwrap();
            let f = crate::provision::find_type(&w.ctx, &no_env, &home(&w), PROJECT).unwrap_err();
            drop(held);
            assert_eq!(
                f.message,
                format!(
                    "agentspace: The lease state lock {} is held by another program; try again.",
                    dir.join("leases.lock").display()
                )
            );
            assert_eq!(
                record(&w, PROJECT).instance_type.as_deref(),
                Some("m9g.large")
            );
        }

        /// A lock held for the whole 30 seconds: this test takes that long.
        #[test]
        fn a_registry_lock_held_past_its_wait_fails_the_step_before_any_removal() {
            let w = World::new("run-hosts-lock-busy");
            w.seed_registry(None, &[(PROJECT, HostState::Ready)]);
            set_type(&w, PROJECT, Some("m9g.large"));
            let before = registry_text(&w).unwrap();
            let held = agentspace_hosts::lock(&home(&w)).unwrap();
            let f = crate::provision::find_type(&w.ctx, &no_env, &home(&w), PROJECT).unwrap_err();
            drop(held);
            assert_eq!(
                f.message,
                format!(
                    "agentspace: The host registry's lock {} is held by another program; try again.",
                    agentspace_hosts::hosts_lock_path(&home(&w)).display()
                )
            );
            assert_eq!(registry_text(&w).unwrap(), before);
            // The lease state lock was let go with the step's failure.
            assert!(agentspace_state::lock(&home(&w).join(".agentspace")).is_ok());
        }

        #[test]
        fn a_run_never_takes_the_provision_lock_a_second_time() {
            // The run holds its own lock; its deploy step runs under it and does not try it.
            let (w, aws) = ready_host("run-own-lock");
            run_to_start(&w, PROJECT, &aws, &no_env).unwrap_err();
            let text = log_lines(&w);
            assert!(
                text.iter().any(|l| l.starts_with("deploy done")),
                "{text:?}"
            );
        }

        // ---- The decision itself ----

        #[test]
        fn a_live_ready_lease_of_the_project_decides_no_type_whatever_it_is() {
            let now = agentspace_state::parse_time("2026-11-23T12:00:00Z").unwrap();
            let live = lease(1, PROJECT, "ready", "m", Some(EC2), Some(LIVE));
            let pending_l = pending(2, PROJECT, "l");
            let decide = |leases: &[agentspace_state::Record]| {
                crate::provision::decide_type(leases, PROJECT, now)
            };
            assert_eq!(
                decide(std::slice::from_ref(&pending_l)),
                Some("m9g.2xlarge")
            );
            assert_eq!(decide(&[live.clone(), pending_l.clone()]), None);
            // Another driver's `ready` record still holds the project's host: it is the project's.
            let other = lease(3, PROJECT, "ready", "m", Some("other"), Some(LIVE));
            assert_eq!(decide(&[other, pending_l.clone()]), None);
            // A `ready` record at or after its expiry holds nothing, to the second.
            let expiring = lease(
                4,
                PROJECT,
                "ready",
                "m",
                Some(EC2),
                Some("2026-11-23T12:00:00Z"),
            );
            assert_eq!(decide(&[expiring, pending_l.clone()]), Some("m9g.2xlarge"));
            let later = lease(
                5,
                PROJECT,
                "ready",
                "m",
                Some(EC2),
                Some("2026-11-23T12:00:01Z"),
            );
            assert_eq!(decide(&[later, pending_l]), None);
        }

        // ---- Smaller pieces ----

        #[test]
        fn the_stack_parameters_file_names_instance_type_and_it_is_dropped_after_its_check() {
            let w = World::new("params-unread").with_deploy_files();
            let path = w.home().join(".agentspace").join("stack-parameters.json");
            std::fs::write(
                &path,
                r#"{"version":0,"parameters":{"VpcId":"VPC_X","InstanceType":"m9g.4xlarge"}}"#,
            )
            .unwrap();
            assert_eq!(
                crate::params::read(&path).unwrap(),
                vec![("VpcId".to_string(), "VPC_X".to_string())]
            );
            // The file is still checked whole: a bad value is refused, never skipped.
            std::fs::write(
                &path,
                r#"{"version":0,"parameters":{"VpcId":"VPC_X","InstanceType":""}}"#,
            )
            .unwrap();
            assert!(
                crate::params::read(&path)
                    .unwrap_err()
                    .message
                    .contains("the parameter InstanceType is empty")
            );
        }

        #[test]
        fn a_change_to_a_project_with_no_record_is_the_has_no_host_message() {
            let w = World::new("registry-no-record");
            let f = crate::registry::set_instance(&home(&w), "nothing", None, None).unwrap_err();
            assert_eq!(
                f.message,
                "agentspace: Project 'nothing' has no host; provision -Host nothing makes one."
            );
            let f = crate::registry::set_instance_type(&home(&w), "nothing", None).unwrap_err();
            assert!(f.message.contains("has no host"));
            let f =
                crate::registry::set_last_success_at(&home(&w), "nothing", "2026-11-24T10:00:00Z")
                    .unwrap_err();
            assert!(f.message.contains("has no host"));
        }

        // ---- v0.10: a run's success (D77; pins P22, P23 and P24) ----
        //
        // A whole run cannot succeed in process here: step 7's ssh is a real program found as
        // agentspace-run finds it. Step 9 and its end are tested in provision.rs's own tests, and a
        // whole run by the operator's integration and conformance tests.

        fn seed_success(w: &World, project: &str, at: &str) {
            agentspace_hosts::set_last_success_at(&home(w), project, at).unwrap();
        }

        #[test]
        fn a_run_that_failed_at_a_step_records_no_success_and_its_start_removed_the_old_one() {
            let (w, aws) = ready_host("run-step-failed");
            seed_success(&w, PROJECT, "2026-01-01T00:00:00Z");
            run_to_start(&w, PROJECT, &aws, &no_env).unwrap_err();
            let r = record(&w, PROJECT);
            assert!(r.last_attempt_at.is_some());
            assert_eq!(r.last_success_at, None);
            assert_eq!(r.state, HostState::Ready);
        }
    }
}
