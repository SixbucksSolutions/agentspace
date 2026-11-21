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

/// Answers the calls of `sync` and `deploy`: a box `running` since long ago, and a stack with an
/// `InstanceId` output. Records each call as one line.
struct FakeAws {
    calls: RefCell<Vec<String>>,
    deploy: RefCell<Result<Value, AwsError>>,
    stack: RefCell<Value>,
    describe: RefCell<Result<Value, AwsError>>,
}

impl Default for FakeAws {
    fn default() -> Self {
        FakeAws {
            calls: RefCell::default(),
            deploy: RefCell::new(Ok(Value::Null)),
            stack: RefCell::new(json!({"Stacks": [{"Outputs": [
                {"OutputKey": "Other", "OutputValue": "x"},
                {"OutputKey": "InstanceId", "OutputValue": ID}
            ]}]})),
            describe: RefCell::new(Ok(json!({"Reservations": [{"Instances": [{
                "State": {"Name": "running"},
                "LaunchTime": "2020-01-01T00:00:00+00:00"
            }]}]}))),
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

/// A clock that never waits.
struct FakeClock;

impl Clock for FakeClock {
    fn now(&self) -> Duration {
        Duration::ZERO
    }
    fn utc_now(&self) -> SystemTime {
        SystemTime::now()
    }
    fn sleep(&self, _: Duration) {}
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
            clock: &FakeClock,
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
                project: project.to_string(),
                stack: agentspace_hosts::stack_name(project),
                state: *state,
                instance_id: None,
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
        vec![READY, COPY_SYSTEM, COPY_CA, COPY_CLAUDE, BUILDING, COPY_CLI]
    );
    // The start-and-wait's calls: the cache rebuilt from the stack, the state, the launch time.
    assert_eq!(
        *aws.calls.borrow(),
        vec![
            "cloudformation describe-stacks [json] --stack-name | agentbox",
            "ec2 describe-instances [json] --instance-ids | i-1234567890abcdef0",
            "ec2 describe-instances [json] --instance-ids | i-1234567890abcdef0",
        ]
    );
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
    assert_eq!(say.lines(), vec![READY, COPY_SYSTEM, COPY_CA, SKIP]);
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
    assert_eq!(calls.len(), 4);
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
        Ok(json!({"Stacks": [{"Outputs": [{"OutputKey": "InstanceId", "OutputValue": ID}]}]}))
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

    #[test]
    fn instance_type_for_the_box_is_refused_before_anything_runs() {
        let w = World::new("box-sized").with_deploy_files();
        let aws = FakeAws::default();
        let f = w
            .run(|seams| {
                crate::deploy_target_sized_using(
                    &w.ctx,
                    &CaEnv::default(),
                    crate::Target::Host("swimcoach"),
                    Some("m9g.2xlarge"),
                    &FakeSay::default(),
                    &aws,
                    &FakeRemote::default(),
                    seams,
                )
            })
            .unwrap_err();
        assert!(
            f.message
                .starts_with("agentspace: -InstanceType needs -Host <project>")
        );
        assert!(aws.calls.borrow().is_empty());
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
}
