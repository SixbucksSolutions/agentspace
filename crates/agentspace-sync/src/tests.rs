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
        let push = |_: &Context, _: &dyn Say, id: &str| self.pushed.borrow_mut().push(id.into());
        f(&Seams {
            clock: &FakeClock,
            ca_check: &ca_check,
            find_go: &find_go,
            localswim_checkout: self.checkout.clone(),
            push: &push,
        })
    }

    fn sync(&self, say: &FakeSay, aws: &FakeAws, remote: &FakeRemote) -> Result<(), Failure> {
        self.run(|seams| sync_using(&self.ctx, &CaEnv::default(), say, aws, remote, seams))
    }

    fn deploy(&self, say: &FakeSay, aws: &FakeAws, remote: &FakeRemote) -> Result<(), Failure> {
        self.run(|seams| deploy_using(&self.ctx, &CaEnv::default(), say, aws, remote, seams))
    }

    fn operator(&self) -> PathBuf {
        std::path::absolute(self.home())
            .unwrap()
            .join(".agentspace")
            .join("operator")
    }

    fn cache(&self) -> PathBuf {
        std::path::absolute(self.home())
            .unwrap()
            .join(".agentspace")
            .join("instance-id.json")
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
    assert_eq!(
        std::fs::read_to_string(w.cache()).unwrap(),
        r#"{"version":0,"instance_id":"i-1234567890abcdef0"}"#
    );
    // The carried box\system, written out just before the copy.
    assert!(w.operator().join("system").join("install.sh").is_file());
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
    assert!(!w.cache().exists());
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
    let agentspace = w.operator().parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&agentspace).unwrap();
    std::fs::write(w.operator(), "a file where the folder belongs").unwrap();
    let remote = FakeRemote::default();
    let failure = w
        .sync(&FakeSay::default(), &FakeAws::default(), &remote)
        .unwrap_err();
    assert_eq!(
        failure.message,
        format!("agentspace: Cannot write {}.", w.operator().display())
    );
    // Only the staging folder's removal ran; nothing read the folder.
    assert_eq!(remote.lines(), w.table()[..1].to_vec());
}

#[test]
fn deploy_passes_its_arguments_in_order_then_refreshes_the_cache_and_syncs() {
    let w = World::new("deploy").with_deploy_files();
    std::fs::write(
        w.cache(),
        r#"{"version":0,"instance_id":"i-fedcba9876543210f"}"#,
    )
    .unwrap();
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
    assert_eq!(
        std::fs::read_to_string(w.cache()).unwrap(),
        r#"{"version":0,"instance_id":"i-1234567890abcdef0"}"#
    );
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
        std::fs::write(
            w.cache(),
            r#"{"version":0,"instance_id":"i-fedcba9876543210f"}"#,
        )
        .unwrap();
        let aws = FakeAws::default();
        *aws.deploy.borrow_mut() = Err(error);
        let remote = FakeRemote::default();
        let failure = w.deploy(&FakeSay::default(), &aws, &remote).unwrap_err();
        assert_eq!(failure.message, message);
        assert_eq!(aws.calls.borrow().len(), 1);
        assert!(remote.lines().is_empty());
        assert!(!w.ca_checked.get());
        // A failed deploy leaves the cache alone.
        assert!(w.cache().exists());
    }
}

#[test]
fn a_stack_with_no_instance_id_output_leaves_no_cache() {
    let w = World::new("deploy-no-output").with_deploy_files();
    std::fs::write(
        w.cache(),
        r#"{"version":0,"instance_id":"i-fedcba9876543210f"}"#,
    )
    .unwrap();
    let aws = FakeAws::default();
    *aws.stack.borrow_mut() = json!({"Stacks": [{"Outputs": []}]});
    let failure = w
        .deploy(&FakeSay::default(), &aws, &FakeRemote::default())
        .unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: Stack 'agentbox' has no InstanceId output."
    );
    assert!(!w.cache().exists());
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
                "agentspace_sync::{pinned}(&ctx, &ca_env(), say, &run, &run)"
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

/// The library reads no environment variable but `PATH`, once, to find `go` (operator.md,
/// Environment variables); every other value comes from the `Context`. The scan is proven able to
/// find a read by finding the one that exists.
#[test]
fn the_library_reads_no_environment_variable_but_path() {
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
            if line.contains("env::var") || line.contains("var_os(") {
                reads.push(format!("{name}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert_eq!(reads.len(), 1, "{reads:?}");
    assert!(reads[0].contains("var_os(\"PATH\")"), "{reads:?}");
}
