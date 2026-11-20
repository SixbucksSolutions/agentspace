//! The commands against stand-ins: each one's calls in order, each failure's message, `-Quiet`,
//! and `up` with `AGENTSPACE_NESTED` set.

use super::*;
use crate::cache::FileInstanceIdCache;
use crate::cli::{DEFAULT_ALIAS, DEFAULT_GIB};
use crate::testing::{FakeAws, FakeClock, FakeReach, FakeRemote, FakeSay, TempHome};
use agentspace_box::cache_bytes;
use agentspace_operator_api::AwsError;
use serde_json::json;

const ID: &str = "i-1234567890abcdef0";
const VOLUME: &str = "vol-1234567890abcdef0";

fn instance(state: &str) -> Value {
    json!({"Reservations": [{"Instances": [{
        "State": {"Name": state},
        "LaunchTime": "2020-01-01T00:00:00+00:00",
        "InstanceType": "m9g.xlarge",
        "RootDeviceName": "/dev/sda1",
        "BlockDeviceMappings": [
            {"DeviceName": "/dev/sdb", "Ebs": {"VolumeId": "vol-fedcba9876543210f"}},
            {"DeviceName": "/dev/sda1", "Ebs": {"VolumeId": VOLUME}}
        ]
    }]}]})
}

fn volume(size: i64) -> Value {
    json!({"Volumes": [{"Size": size}]})
}

fn ping(status: &str) -> Value {
    json!({"InstanceInformationList": [{"PingStatus": status}]})
}

/// A throwaway home that already holds the instance-ID cache.
fn home_with_cache(name: &str) -> TempHome {
    let home = TempHome::new(name);
    let cache = FileInstanceIdCache::new(&home.ctx());
    cache.write(&cache_bytes(ID)).unwrap();
    home
}

struct Rig {
    say: FakeSay,
    aws: FakeAws,
    remote: FakeRemote,
    clock: FakeClock,
    reach: FakeReach,
    cache: FileInstanceIdCache,
    ctx: Context,
    ca_env: CaEnv,
}

impl Rig {
    fn new(
        home: &TempHome,
        quiet: bool,
        nested: bool,
        answers: Vec<Result<Value, AwsError>>,
    ) -> Self {
        Self::with_exit(home, quiet, nested, answers, 0)
    }

    fn with_exit(
        home: &TempHome,
        quiet: bool,
        nested: bool,
        answers: Vec<Result<Value, AwsError>>,
        exit: i32,
    ) -> Self {
        let ctx = home.ctx_nested(nested);
        Rig {
            say: FakeSay::new(quiet),
            aws: FakeAws::new(answers),
            remote: FakeRemote::new(exit),
            clock: FakeClock::new(),
            reach: FakeReach::default(),
            cache: FileInstanceIdCache::new(&ctx),
            ctx,
            ca_env: CaEnv::default(),
        }
    }

    fn run(&self, request: &Request) -> Result<(), Failure> {
        dispatch(
            request,
            &Parts {
                ctx: &self.ctx,
                ca_env: &self.ca_env,
                say: &self.say,
                report: &self.say,
                aws: &self.aws,
                remote: &self.remote,
                clock: &self.clock,
                cache: &self.cache,
                reach: &self.reach,
            },
        )
    }
}

fn request(command: Command) -> Request {
    Request {
        command,
        gib: DEFAULT_GIB,
        alias: DEFAULT_ALIAS.to_string(),
        alias_given: false,
        quiet: false,
    }
}

fn ok(value: Value) -> Result<Value, AwsError> {
    Ok(value)
}

// ---- up ----

#[test]
fn up_starts_nothing_for_a_box_running_a_long_time_and_pushes_over_the_default_alias() {
    let home = home_with_cache("up-running");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(instance("running")), ok(instance("running"))],
    );
    rig.run(&request(Command::Up)).unwrap();
    assert_eq!(
        rig.aws.calls(),
        vec![
            format!("ec2 describe-instances --instance-ids {ID}"),
            format!("ec2 describe-instances --instance-ids {ID}"),
        ]
    );
    assert_eq!(rig.say.lines(), vec![format!("Ready: {ID}")]);
    assert_eq!(rig.reach.calls(), vec![format!("push {ID} agentspace")]);
}

#[test]
fn up_pushes_over_the_alias_it_was_given() {
    let home = home_with_cache("up-alias");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(instance("running")), ok(instance("running"))],
    );
    let mut req = request(Command::Up);
    req.alias = "lease-a.b_c".to_string();
    req.alias_given = true;
    rig.run(&req).unwrap();
    assert_eq!(rig.reach.calls(), vec![format!("push {ID} lease-a.b_c")]);
}

#[test]
fn a_nested_up_does_not_push() {
    let home = home_with_cache("up-nested");
    let rig = Rig::new(
        &home,
        false,
        true,
        vec![ok(instance("running")), ok(instance("running"))],
    );
    rig.run(&request(Command::Up)).unwrap();
    assert_eq!(rig.say.lines(), vec![format!("Ready: {ID}")]);
    assert!(rig.reach.calls().is_empty());
}

#[test]
fn quiet_up_prints_nothing_and_still_pushes() {
    let home = home_with_cache("up-quiet");
    let rig = Rig::new(
        &home,
        true,
        false,
        vec![ok(instance("running")), ok(instance("running"))],
    );
    rig.run(&request(Command::Up)).unwrap();
    assert!(rig.say.lines().is_empty());
    assert_eq!(rig.reach.calls().len(), 1);
}

#[test]
fn up_starts_a_stopped_box_waits_for_it_and_then_pushes() {
    let home = home_with_cache("up-stopped");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![
            ok(instance("stopped")),
            ok(json!({})),
            ok(ping("Online")),
            ok(json!({"Command": {"CommandId": "command-1"}})),
            ok(json!({"Status": "Success", "StandardOutputContent": "patched\n"})),
        ],
    );
    rig.run(&request(Command::Up)).unwrap();
    let calls = rig.aws.calls();
    assert_eq!(
        calls[0],
        format!("ec2 describe-instances --instance-ids {ID}")
    );
    assert_eq!(calls[1], format!("ec2 start-instances --instance-ids {ID}"));
    assert!(calls[2].starts_with("ssm describe-instance-information"));
    assert!(calls[3].starts_with("ssm send-command"));
    assert!(calls[4].starts_with("ssm get-command-invocation"));
    assert_eq!(
        rig.say.lines(),
        vec![format!("Starting {ID}..."), format!("Ready: {ID}")]
    );
    assert_eq!(rig.reach.calls(), vec![format!("push {ID} agentspace")]);
}

#[test]
fn up_rebuilds_a_missing_cache_from_the_stack_and_writes_it() {
    let home = TempHome::new("up-rebuild");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![
            ok(json!({"Stacks": [{"Outputs": [
                {"OutputKey": "Other", "OutputValue": "x"},
                {"OutputKey": "InstanceId", "OutputValue": ID}
            ]}]})),
            ok(instance("running")),
            ok(instance("running")),
        ],
    );
    rig.run(&request(Command::Up)).unwrap();
    assert_eq!(
        rig.aws.calls()[0],
        "cloudformation describe-stacks --stack-name agentbox"
    );
    let written = std::fs::read(rig.cache.path()).unwrap();
    assert_eq!(agentspace_box::parse_cache(&written).as_deref(), Some(ID));
}

#[test]
fn a_stack_with_no_instance_id_output_fails_and_writes_no_cache() {
    let home = TempHome::new("up-no-output");
    let rig = Rig::new(&home, false, false, vec![ok(json!({"Stacks": [{}]}))]);
    let failure = rig.run(&request(Command::Up)).unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: Stack 'agentbox' has no InstanceId output."
    );
    assert!(!rig.cache.path().exists());
    assert!(rig.reach.calls().is_empty());
}

#[test]
fn an_unopenable_cache_fails_with_fa16s_message_and_no_aws_call() {
    let home = TempHome::new("up-unopenable");
    let rig = Rig::new(&home, false, false, vec![]);
    std::fs::create_dir_all(rig.cache.path()).unwrap();
    let failure = rig.run(&request(Command::Up)).unwrap_err();
    assert_eq!(
        failure.message,
        format!(
            "agentspace: Cannot read the instance-ID cache {}; remove it and run the command again.",
            rig.cache.path().display()
        )
    );
    assert!(rig.aws.calls().is_empty());
    assert!(rig.reach.calls().is_empty());
}

#[test]
fn an_instance_in_another_state_fails_up_and_nothing_is_pushed() {
    let home = home_with_cache("up-terminated");
    let rig = Rig::new(&home, false, false, vec![ok(instance("terminated"))]);
    let failure = rig.run(&request(Command::Up)).unwrap_err();
    assert_eq!(
        failure.message,
        format!("agentspace: Instance {ID} is terminated.")
    );
    assert!(rig.reach.calls().is_empty());
}

#[test]
fn an_expired_sign_in_fails_every_aws_command_with_the_sign_in_message() {
    for command in [Command::Up, Command::Down, Command::Status, Command::Grow] {
        let home = home_with_cache("sign-in");
        let rig = Rig::new(&home, false, false, vec![Err(AwsError::SignIn)]);
        let failure = rig.run(&request(command)).unwrap_err();
        assert_eq!(
            failure.message,
            agentspace_operator_api::SIGN_IN_MESSAGE,
            "{command:?}"
        );
        assert_eq!(failure.caller, CallerFailure::Same);
    }
}

#[test]
fn another_aws_failure_is_agentspace_and_the_clis_text() {
    let home = home_with_cache("aws-failed");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![Err(AwsError::Failed("It went wrong.".to_string()))],
    );
    let failure = rig.run(&request(Command::Status)).unwrap_err();
    assert_eq!(failure.message, "agentspace: It went wrong.");
}

// ---- down ----

#[test]
fn down_reads_the_stop_answer_no_further_than_its_exit_code() {
    // Operator.md, What is read from each answer: `ec2 stop-instances` is `Checked`, never
    // parsed, so an answer that is not JSON cannot fail the command.
    let home = home_with_cache("down-checked");
    let rig = Rig::new(&home, false, false, vec![ok(Value::Null)]);
    rig.run(&request(Command::Down)).unwrap();
    assert_eq!(rig.aws.outputs(), vec![Output::Checked]);
}

#[test]
fn down_asks_ec2_to_stop_and_says_so() {
    let home = home_with_cache("down");
    let rig = Rig::new(&home, false, false, vec![ok(json!({}))]);
    rig.run(&request(Command::Down)).unwrap();
    assert_eq!(
        rig.aws.calls(),
        vec![format!("ec2 stop-instances --instance-ids {ID}")]
    );
    assert_eq!(rig.say.lines(), vec![format!("Stopping {ID}.")]);
}

#[test]
fn quiet_down_prints_nothing() {
    let home = home_with_cache("down-quiet");
    let rig = Rig::new(&home, true, false, vec![ok(json!({}))]);
    rig.run(&request(Command::Down)).unwrap();
    assert!(rig.say.lines().is_empty());
}

#[test]
fn a_down_that_ec2_refuses_prints_no_stopping_line() {
    let home = home_with_cache("down-refused");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![Err(AwsError::Failed("Refused.".to_string()))],
    );
    let failure = rig.run(&request(Command::Down)).unwrap_err();
    assert_eq!(failure.message, "agentspace: Refused.");
    assert!(rig.say.lines().is_empty());
}

// ---- status ----

fn status_lines(ssm: &str, state: &str) -> Vec<String> {
    vec![
        format!("Instance  {ID}"),
        format!("State     {state}"),
        "Type      m9g.xlarge".to_string(),
        format!("Disk      8 GiB gp3 ({VOLUME})"),
        format!("SSM       {ssm}"),
    ]
}

#[test]
fn status_of_a_running_box_asks_session_manager() {
    for (answer, shown) in [
        (ping("Online"), "online"),
        (ping("ConnectionLost"), "not yet"),
        (json!({"InstanceInformationList": []}), "not yet"),
    ] {
        let home = home_with_cache("status-running");
        let rig = Rig::new(
            &home,
            false,
            false,
            vec![ok(instance("running")), ok(volume(8)), ok(answer)],
        );
        rig.run(&request(Command::Status)).unwrap();
        assert_eq!(
            rig.aws.calls(),
            vec![
                format!("ec2 describe-instances --instance-ids {ID}"),
                format!("ec2 describe-volumes --volume-ids {VOLUME}"),
                format!("ssm describe-instance-information --filters Key=InstanceIds,Values={ID}"),
            ]
        );
        assert_eq!(rig.say.lines(), status_lines(shown, "running"));
    }
}

#[test]
fn status_of_a_box_that_is_not_running_never_asks_session_manager() {
    for state in [
        "pending",
        "stopping",
        "stopped",
        "shutting-down",
        "terminated",
    ] {
        let home = home_with_cache("status-stopped");
        let rig = Rig::new(
            &home,
            false,
            false,
            vec![ok(instance(state)), ok(volume(8))],
        );
        rig.run(&request(Command::Status)).unwrap();
        assert_eq!(rig.aws.calls().len(), 2, "{state}");
        assert_eq!(rig.say.lines(), status_lines("-", state));
    }
}

#[test]
fn status_prints_whatever_quiet_says() {
    let home = home_with_cache("status-quiet");
    let rig = Rig::new(
        &home,
        true,
        false,
        vec![ok(instance("stopped")), ok(volume(8))],
    );
    rig.run(&request(Command::Status)).unwrap();
    assert_eq!(rig.say.lines(), status_lines("-", "stopped"));
}

#[test]
fn status_never_starts_the_box() {
    let home = home_with_cache("status-never-starts");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(instance("stopped")), ok(volume(8))],
    );
    rig.run(&request(Command::Status)).unwrap();
    assert!(
        rig.aws
            .calls()
            .iter()
            .all(|c| !c.starts_with("ec2 start-instances"))
    );
}

#[test]
fn status_prints_no_line_when_a_call_fails() {
    let home = home_with_cache("status-fails");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![
            ok(instance("running")),
            Err(AwsError::Failed("No volume.".to_string())),
        ],
    );
    assert!(rig.run(&request(Command::Status)).is_err());
    assert!(rig.say.lines().is_empty());
}

#[test]
fn status_fails_on_the_first_missing_member_by_its_path() {
    let without = |member: &str| {
        let mut answer = instance("running");
        answer["Reservations"][0]["Instances"][0]
            .as_object_mut()
            .unwrap()
            .remove(member);
        answer
    };
    let cases = [
        ("State", "Reservations[0].Instances[0].State.Name"),
        ("InstanceType", "Reservations[0].Instances[0].InstanceType"),
        (
            "RootDeviceName",
            "Reservations[0].Instances[0].RootDeviceName",
        ),
        (
            "BlockDeviceMappings",
            "Reservations[0].Instances[0].BlockDeviceMappings[].Ebs.VolumeId",
        ),
    ];
    for (member, path) in cases {
        let home = home_with_cache("status-missing");
        let rig = Rig::new(&home, false, false, vec![ok(without(member))]);
        let failure = rig.run(&request(Command::Status)).unwrap_err();
        assert_eq!(
            failure.message,
            format!("agentspace: Unexpected answer from ec2 describe-instances: no {path}."),
            "{member}"
        );
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
    }
}

#[test]
fn status_fails_for_a_volume_with_no_size() {
    let home = home_with_cache("status-no-size");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(instance("stopped")), ok(json!({"Volumes": [{}]}))],
    );
    let failure = rig.run(&request(Command::Status)).unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: Unexpected answer from ec2 describe-volumes: no Volumes[0].Size."
    );
}

// ---- grow ----

#[test]
fn grow_of_a_running_box_changes_the_volume_waits_and_grows_the_filesystem_over_ssh() {
    let home = home_with_cache("grow-running");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(instance("running")), ok(volume(8)), ok(json!({}))],
    );
    rig.run(&request(Command::Grow)).unwrap();
    assert_eq!(
        rig.aws.calls(),
        vec![
            format!("ec2 describe-instances --instance-ids {ID}"),
            format!("ec2 describe-volumes --volume-ids {VOLUME}"),
            format!("ec2 modify-volume --volume-id {VOLUME} --size 10"),
        ]
    );
    assert_eq!(
        rig.say.lines(),
        vec![
            format!(
                "Growing {VOLUME} from 8 to 10 GiB (EBS allows one change per volume every 6 hours)..."
            ),
            "Growing the filesystem online...".to_string(),
        ]
    );
    assert_eq!(*rig.clock.slept.borrow(), vec![Duration::from_secs(10)]);
    let calls = rig.remote.calls.borrow();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].0,
        vec!["-o", "ClearAllForwardings=yes", "agentspace"]
    );
    assert_eq!(calls[0].1.as_deref(), Some(GROW_REMOTE_COMMAND));
}

#[test]
fn grow_reads_the_modify_answer_no_further_than_its_exit_code() {
    // Operator.md, What is read from each answer: `ec2 modify-volume` is `Checked`, never
    // parsed, so an answer that is not JSON cannot fail the command.
    let home = home_with_cache("grow-checked");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(instance("stopped")), ok(volume(8)), ok(Value::Null)],
    );
    rig.run(&request(Command::Grow)).unwrap();
    assert_eq!(
        rig.aws.outputs(),
        vec![Output::Json, Output::Json, Output::Checked]
    );
}

#[test]
fn grow_adds_the_given_gib_to_the_size() {
    let home = home_with_cache("grow-gib");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(instance("stopped")), ok(volume(20)), ok(json!({}))],
    );
    let mut req = request(Command::Grow);
    req.gib = 7;
    rig.run(&req).unwrap();
    assert_eq!(
        rig.aws.calls()[2],
        format!("ec2 modify-volume --volume-id {VOLUME} --size 27")
    );
}

#[test]
fn grow_passes_a_size_that_is_not_larger_to_aws_to_refuse() {
    let home = home_with_cache("grow-zero");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![
            ok(instance("stopped")),
            ok(volume(8)),
            Err(AwsError::Failed("Not larger.".to_string())),
        ],
    );
    let mut req = request(Command::Grow);
    req.gib = 0;
    let failure = rig.run(&req).unwrap_err();
    assert_eq!(
        rig.aws.calls()[2],
        format!("ec2 modify-volume --volume-id {VOLUME} --size 8")
    );
    assert_eq!(failure.message, "agentspace: Not larger.");
    assert!(rig.remote.calls.borrow().is_empty());
}

#[test]
fn grow_of_a_stopped_box_starts_no_ssh_and_says_it_grows_at_boot() {
    let home = home_with_cache("grow-stopped");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(instance("stopped")), ok(volume(8)), ok(json!({}))],
    );
    rig.run(&request(Command::Grow)).unwrap();
    assert_eq!(
        rig.say.lines()[1],
        "The filesystem grows on its own at next boot."
    );
    assert!(rig.remote.calls.borrow().is_empty());
    assert!(rig.clock.slept.borrow().is_empty());
    assert!(
        rig.aws
            .calls()
            .iter()
            .all(|c| !c.starts_with("ec2 start-instances"))
    );
}

#[test]
fn a_failed_online_grow_fails_grow_and_says_the_volume_stands() {
    for exit in [1, 255] {
        let home = home_with_cache("grow-ssh-fails");
        let rig = Rig::with_exit(
            &home,
            false,
            false,
            vec![ok(instance("running")), ok(volume(8)), ok(json!({}))],
            exit,
        );
        let failure = rig.run(&request(Command::Grow)).unwrap_err();
        assert_eq!(
            failure.message,
            "agentspace: Growing the filesystem online failed; the volume is already 10 GiB, and the filesystem grows on its own at next boot."
        );
    }
}

#[test]
fn quiet_grow_prints_nothing_but_still_waits() {
    let home = home_with_cache("grow-quiet");
    let rig = Rig::new(
        &home,
        true,
        false,
        vec![ok(instance("running")), ok(volume(8)), ok(json!({}))],
    );
    rig.run(&request(Command::Grow)).unwrap();
    assert!(rig.say.lines().is_empty());
    assert_eq!(rig.clock.slept.borrow().len(), 1);
    assert_eq!(rig.remote.calls.borrow().len(), 1);
}

#[test]
fn grow_does_not_read_the_instance_type() {
    let home = home_with_cache("grow-no-type");
    let mut answer = instance("stopped");
    answer["Reservations"][0]["Instances"][0]
        .as_object_mut()
        .unwrap()
        .remove("InstanceType");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(answer), ok(volume(8)), ok(json!({}))],
    );
    rig.run(&request(Command::Grow)).unwrap();
}

#[test]
fn the_remote_command_obeys_fa15s_rule_and_installs_nothing() {
    let command = GROW_REMOTE_COMMAND;
    assert!(!command.contains('%'));
    assert!(!command.contains('"'));
    assert!(!command.chars().any(char::is_control));
    assert!(!command.ends_with('\\'));
    assert!(!command.contains("apt") && !command.contains("install"));
    // Root, never prompting; the three tools the spec names.
    assert!(command.starts_with("sudo -n "));
    for tool in ["growpart", "resize2fs", "df -h"] {
        assert!(command.contains(tool), "{tool}");
    }
    // The size is printed only if the filesystem's grow succeeded.
    assert!(command.contains("resize2fs $dev && df -h /"));
}

// ---- ca, sync, deploy ----

#[test]
fn ca_sync_and_deploy_go_to_their_libraries_and_their_failures_pass_through() {
    for command in [Command::Ca, Command::Sync, Command::Deploy] {
        let home = TempHome::new("reach");
        let mut rig = Rig::new(&home, false, false, vec![]);
        rig.run(&request(command)).unwrap();
        assert_eq!(rig.reach.calls(), vec![command.name().to_string()]);
        assert!(rig.aws.calls().is_empty());
        rig.reach = FakeReach {
            fail: Some("agentspace: It failed.".to_string()),
            ..FakeReach::default()
        };
        let failure = rig.run(&request(command)).unwrap_err();
        assert_eq!(failure.message, "agentspace: It failed.");
    }
}
