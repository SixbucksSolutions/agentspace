//! The commands against stand-ins: each one's calls in order, each failure's message, `-Quiet`,
//! and `up` with `AGENTSPACE_NESTED` set.

use super::*;
use crate::cli::{DEFAULT_ALIAS, DEFAULT_GIB};
use crate::testing::{FakeAws, FakeClock, FakeReach, FakeRemote, FakeSay, TempHome};
use agentspace_hosts::hosts_path;
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

const OTHER_ID: &str = "i-fedcba9876543210f";

fn record(project: &str, state: HostState, instance_id: Option<&str>) -> Record {
    Record {
        last_success_at: None,
        platform: None,
        project: project.to_string(),
        stack: agentspace_hosts::stack_name(project),
        state,
        instance_id: instance_id.map(str::to_string),
        instance_type: None,
        last_attempt_at: None,
    }
}

/// A throwaway home that holds this host registry.
fn home_with_registry(name: &str, hosts: &Hosts) -> TempHome {
    let home = TempHome::new(name);
    let h = home.ctx().home;
    let held = agentspace_hosts::lock(&h).unwrap();
    agentspace_hosts::write(&h, hosts, &held).unwrap();
    home
}

/// A throwaway home whose host registry holds the box's record with an instance ID.
fn home_with_cache(name: &str) -> TempHome {
    let mut hosts = Hosts::seed();
    hosts.hosts[0].instance_id = Some(ID.to_string());
    home_with_registry(name, &hosts)
}

/// A throwaway home whose host registry holds the box's record and the `agentspace` project's host,
/// each with an instance ID of its own.
fn home_with_two_hosts(name: &str, state: HostState) -> TempHome {
    let mut hosts = Hosts::seed();
    hosts.hosts[0].instance_id = Some(ID.to_string());
    hosts
        .hosts
        .push(record("agentspace", state, Some(OTHER_ID)));
    home_with_registry(name, &hosts)
}

fn host_request(command: Command, project: &str) -> Request {
    Request {
        host: Some(project.to_string()),
        ..request(command)
    }
}

fn all_request(command: Command) -> Request {
    Request {
        all: true,
        ..request(command)
    }
}

struct Rig {
    say: FakeSay,
    aws: FakeAws,
    remote: FakeRemote,
    clock: FakeClock,
    reach: FakeReach,
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
            ctx,
            ca_env: CaEnv::default(),
        }
    }

    fn run(&self, request: &Request) -> Result<(), Failure> {
        done(dispatch(
            request,
            &Parts {
                ctx: &self.ctx,
                ca_env: &self.ca_env,
                say: &self.say,
                report: &self.say,
                aws: &self.aws,
                remote: &self.remote,
                clock: &self.clock,
                reach: &self.reach,
            },
        ))
    }
}

/// A command that is not `guard` ends in `Done` or a failure; `guard`'s outcomes are tested in
/// guard.rs and tests/guard_dispatch.rs.
fn done(result: Result<Outcome, Failure>) -> Result<(), Failure> {
    result.map(|outcome| assert_eq!(outcome, Outcome::Done))
}

fn request(command: Command) -> Request {
    Request {
        command,
        gib: DEFAULT_GIB,
        alias: DEFAULT_ALIAS.to_string(),
        alias_given: false,
        host: None,
        all: false,
        instance_type: None,
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
    assert_eq!(rig.reach.calls(), vec![format!("push {ID} agentspace -")]);
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
    assert_eq!(rig.reach.calls(), vec![format!("push {ID} lease-a.b_c -")]);
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
    assert_eq!(rig.reach.calls(), vec![format!("push {ID} agentspace -")]);
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
    // The first change writes the registry, holding the box's record with the ID.
    let hosts = agentspace_hosts::read(&rig.ctx.home).unwrap();
    assert_eq!(hosts.hosts.len(), 1);
    assert_eq!(hosts.hosts[0].instance_id.as_deref(), Some(ID));
    assert_eq!(hosts.hosts[0].stack, "agentbox");
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
    assert!(
        !hosts_path(&rig.ctx.home).exists(),
        "No registry is written"
    );
    assert!(rig.reach.calls().is_empty());
}

#[test]
fn a_registry_that_is_not_valid_fails_every_host_command_with_its_name_and_no_aws_call() {
    for command in [
        Command::Up,
        Command::Down,
        Command::Status,
        Command::Grow,
        Command::Sync,
        Command::Deploy,
        Command::Bootstrap,
    ] {
        let home = TempHome::new("registry-invalid");
        let rig = Rig::new(&home, false, false, vec![]);
        let path = hosts_path(&rig.ctx.home);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{\"version\":0,\"hosts\":[]}").unwrap();
        let failure = rig.run(&request(command)).unwrap_err();
        assert!(
            failure.message.starts_with(&format!(
                "agentspace: The host registry {} cannot be read: ",
                path.display()
            )),
            "{command:?}: {}",
            failure.message
        );
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        assert!(rig.aws.calls().is_empty(), "{command:?}");
        assert!(rig.reach.calls().is_empty(), "{command:?}");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"{\"version\":0,\"hosts\":[]}"
        );
    }
}

#[test]
fn ca_and_provision_read_no_registry() {
    let home = TempHome::new("registry-ca");
    let rig = Rig::new(&home, false, false, vec![]);
    let path = hosts_path(&rig.ctx.home);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"not json").unwrap();
    rig.run(&request(Command::Ca)).unwrap();
    rig.run(&host_request(Command::Provision, "agentspace"))
        .unwrap();
    assert_eq!(
        rig.reach.calls(),
        vec!["ca".to_string(), "provision agentspace".to_string()]
    );
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
    for (command, called) in [
        (Command::Ca, "ca"),
        (Command::Sync, "sync box"),
        (Command::Deploy, "deploy box -"),
        (Command::Bootstrap, "bootstrap box"),
    ] {
        let home = TempHome::new("reach");
        let mut rig = Rig::new(&home, false, false, vec![]);
        rig.run(&request(command)).unwrap();
        assert_eq!(rig.reach.calls(), vec![called.to_string()]);
        assert!(rig.aws.calls().is_empty());
        rig.reach = FakeReach::default()
            .failing(called.split(' ').next().unwrap(), "agentspace: It failed.");
        let failure = rig.run(&request(command)).unwrap_err();
        assert_eq!(failure.message, "agentspace: It failed.");
    }
}

// ---- -Host ----

const HOST_STACK: &str = "agentspace-host-agentspace";
const HOST_ALIAS: &str = "agentspace-host.agentspace";

#[test]
fn a_project_with_no_record_fails_every_command_but_ca_and_provision_before_anything_starts() {
    for command in [
        Command::Up,
        Command::Down,
        Command::Status,
        Command::Grow,
        Command::Sync,
        Command::Deploy,
        Command::Bootstrap,
    ] {
        let home = home_with_cache("no-record");
        let rig = Rig::new(&home, false, false, vec![]);
        let failure = rig.run(&host_request(command, "nobody")).unwrap_err();
        assert_eq!(
            failure.message,
            "agentspace: Project 'nobody' has no host; provision -Host nobody makes one.",
            "{command:?}"
        );
        assert!(rig.aws.calls().is_empty(), "{command:?}");
        assert!(rig.reach.calls().is_empty(), "{command:?}");
        assert!(rig.remote.calls.borrow().is_empty(), "{command:?}");
    }
}

#[test]
fn up_of_another_host_uses_its_stack_its_record_its_alias_and_its_stamp() {
    let mut hosts = Hosts::seed();
    hosts.hosts[0].instance_id = Some(ID.to_string());
    hosts
        .hosts
        .push(record("agentspace", HostState::Creating, None));
    let home = home_with_registry("up-host", &hosts);
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![
            ok(
                json!({"Stacks": [{"Outputs": [{"OutputKey": "InstanceId", "OutputValue": OTHER_ID}]}]}),
            ),
            ok(instance("running")),
            ok(instance("running")),
        ],
    );
    rig.run(&host_request(Command::Up, "agentspace")).unwrap();
    assert_eq!(
        rig.aws.calls(),
        vec![
            format!("cloudformation describe-stacks --stack-name {HOST_STACK}"),
            format!("ec2 describe-instances --instance-ids {OTHER_ID}"),
            format!("ec2 describe-instances --instance-ids {OTHER_ID}"),
        ]
    );
    // The key, certificate and alias come before the push, and the push names the host's alias and
    // its own stamp.
    assert_eq!(
        rig.reach.calls(),
        vec![
            "access agentspace".to_string(),
            format!("push {OTHER_ID} {HOST_ALIAS} agentspace"),
        ]
    );
    // The ID is written into that record alone, and nothing else in it changes.
    let read = agentspace_hosts::read(&rig.ctx.home).unwrap();
    assert_eq!(read.hosts[0].instance_id.as_deref(), Some(ID));
    assert_eq!(read.hosts[1].instance_id.as_deref(), Some(OTHER_ID));
    assert_eq!(read.hosts[1].state, HostState::Creating);
}

#[test]
fn up_with_host_swimcoach_is_the_same_as_no_host() {
    let home = home_with_cache("up-swimcoach");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(instance("running")), ok(instance("running"))],
    );
    rig.run(&host_request(Command::Up, "swimcoach")).unwrap();
    assert_eq!(rig.reach.calls(), vec![format!("push {ID} agentspace -")]);
}

#[test]
fn up_of_another_host_pushes_over_the_alias_given_and_a_nested_up_signs_and_pushes_nothing() {
    let home = home_with_two_hosts("up-host-alias", HostState::Ready);
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(instance("running")), ok(instance("running"))],
    );
    let mut req = host_request(Command::Up, "agentspace");
    req.alias = "mine".to_string();
    req.alias_given = true;
    rig.run(&req).unwrap();
    assert_eq!(
        rig.reach.calls(),
        vec![
            "access agentspace".to_string(),
            format!("push {OTHER_ID} mine agentspace")
        ]
    );

    let home = home_with_two_hosts("up-host-nested", HostState::Ready);
    let rig = Rig::new(
        &home,
        false,
        true,
        vec![ok(instance("running")), ok(instance("running"))],
    );
    rig.run(&host_request(Command::Up, "agentspace")).unwrap();
    assert!(rig.reach.calls().is_empty());
}

#[test]
fn a_failure_to_sign_in_for_up_is_a_push_failure_that_names_its_cause_and_exits_0() {
    let home = home_with_two_hosts("up-host-no-ca", HostState::Ready);
    let mut rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(instance("running")), ok(instance("running"))],
    );
    rig.reach =
        FakeReach::default().failing("access", "agentspace: The CA does not exist; run ca.");
    rig.run(&host_request(Command::Up, "agentspace")).unwrap();
    assert_eq!(
        rig.say.lines(),
        vec![
            format!("Ready: {OTHER_ID}"),
            "agentspace: Could not send the localswim token, because The CA does not exist; run ca; the board will not work on the host this session."
                .to_string(),
        ]
    );
    assert_eq!(rig.reach.calls(), vec!["access agentspace".to_string()]);
}

#[test]
fn the_deadline_message_names_the_hosts_alias_for_a_host_that_is_not_the_box() {
    let box_failure = {
        let home = home_with_cache("deadline-box");
        let mut answers = vec![ok(instance("pending"))];
        answers.extend((0..300).map(|_| ok(json!({"InstanceInformationList": []}))));
        let rig = Rig::new(&home, false, false, answers);
        rig.run(&request(Command::Up)).unwrap_err()
    };
    assert_eq!(
        box_failure.message,
        format!(
            "agentspace: {ID} was not ready within 15 minutes; check: ssh agentspace journalctl -u agentspace-patch"
        )
    );
    let home = home_with_two_hosts("deadline-host", HostState::Ready);
    let mut answers = vec![ok(instance("pending"))];
    answers.extend((0..300).map(|_| ok(json!({"InstanceInformationList": []}))));
    let rig = Rig::new(&home, false, false, answers);
    let failure = rig
        .run(&host_request(Command::Up, "agentspace"))
        .unwrap_err();
    assert_eq!(
        failure.message,
        format!(
            "agentspace: {OTHER_ID} was not ready within 15 minutes; check: ssh {HOST_ALIAS} journalctl -u agentspace-patch"
        )
    );
    assert!(rig.reach.calls().is_empty());
}

#[test]
fn down_of_another_host_stops_its_instance_and_then_signs() {
    let home = home_with_two_hosts("down-host", HostState::Ready);
    let rig = Rig::new(&home, false, false, vec![ok(json!({}))]);
    rig.run(&host_request(Command::Down, "agentspace")).unwrap();
    assert_eq!(
        rig.aws.calls(),
        vec![format!("ec2 stop-instances --instance-ids {OTHER_ID}")]
    );
    assert_eq!(rig.say.lines(), vec![format!("Stopping {OTHER_ID}.")]);
    assert_eq!(rig.reach.calls(), vec!["access agentspace".to_string()]);
}

#[test]
fn a_failed_sign_in_after_down_fails_it_and_the_stop_stands_and_the_box_never_signs() {
    let home = home_with_two_hosts("down-host-no-ca", HostState::Ready);
    let mut rig = Rig::new(&home, false, false, vec![ok(json!({}))]);
    rig.reach =
        FakeReach::default().failing("access", "agentspace: The CA does not exist; run ca.");
    let failure = rig
        .run(&host_request(Command::Down, "agentspace"))
        .unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: The CA does not exist; run ca."
    );
    assert_eq!(rig.say.lines(), vec![format!("Stopping {OTHER_ID}.")]);

    let home = home_with_two_hosts("down-box", HostState::Ready);
    let rig = Rig::new(&home, false, false, vec![ok(json!({}))]);
    rig.run(&host_request(Command::Down, "swimcoach")).unwrap();
    assert!(rig.reach.calls().is_empty());
}

#[test]
fn status_of_another_host_reports_it_and_then_signs_and_a_failure_there_follows_the_report() {
    let home = home_with_two_hosts("status-host", HostState::Ready);
    let mut rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(instance("stopped")), ok(volume(8))],
    );
    rig.reach =
        FakeReach::default().failing("access", "agentspace: The CA does not exist; run ca.");
    let failure = rig
        .run(&host_request(Command::Status, "agentspace"))
        .unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: The CA does not exist; run ca."
    );
    assert_eq!(
        rig.say.lines(),
        vec![
            format!("Instance  {OTHER_ID}"),
            "State     stopped".to_string(),
            "Type      m9g.xlarge".to_string(),
            format!("Disk      8 GiB gp3 ({VOLUME})"),
            "SSM       -".to_string(),
        ]
    );
    assert_eq!(
        rig.aws.calls()[0],
        format!("ec2 describe-instances --instance-ids {OTHER_ID}")
    );
}

#[test]
fn grow_of_another_host_signs_before_its_ssh_and_names_the_hosts_alias() {
    let home = home_with_two_hosts("grow-host", HostState::Ready);
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(instance("running")), ok(volume(8)), ok(json!({}))],
    );
    rig.run(&host_request(Command::Grow, "agentspace")).unwrap();
    assert_eq!(rig.reach.calls(), vec!["access agentspace".to_string()]);
    let calls = rig.remote.calls.borrow();
    assert_eq!(
        calls[0].0,
        vec!["-o", "ClearAllForwardings=yes", HOST_ALIAS]
    );
    assert_eq!(calls[0].1.as_deref(), Some(GROW_REMOTE_COMMAND));
}

#[test]
fn grow_of_a_host_that_is_not_running_still_signs_and_a_failure_leaves_the_volume_changed() {
    let home = home_with_two_hosts("grow-host-stopped", HostState::Ready);
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(instance("stopped")), ok(volume(8)), ok(json!({}))],
    );
    rig.run(&host_request(Command::Grow, "agentspace")).unwrap();
    assert_eq!(rig.reach.calls(), vec!["access agentspace".to_string()]);
    assert!(rig.remote.calls.borrow().is_empty());

    let home = home_with_two_hosts("grow-host-no-ca", HostState::Ready);
    let mut rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(instance("running")), ok(volume(8)), ok(json!({}))],
    );
    rig.reach =
        FakeReach::default().failing("access", "agentspace: The CA does not exist; run ca.");
    let failure = rig
        .run(&host_request(Command::Grow, "agentspace"))
        .unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: The CA does not exist; run ca."
    );
    assert!(rig.aws.calls()[2].starts_with("ec2 modify-volume"));
    assert!(rig.remote.calls.borrow().is_empty());
}

#[test]
fn sync_deploy_bootstrap_and_provision_name_their_host() {
    let home = home_with_two_hosts("targets", HostState::Ready);
    let rig = Rig::new(&home, false, false, vec![]);
    rig.run(&host_request(Command::Sync, "agentspace")).unwrap();
    rig.run(&host_request(Command::Sync, "swimcoach")).unwrap();
    rig.run(&host_request(Command::Bootstrap, "agentspace"))
        .unwrap();
    rig.run(&host_request(Command::Deploy, "agentspace"))
        .unwrap();
    let mut with_type = host_request(Command::Deploy, "agentspace");
    with_type.instance_type = Some("m9g.2xlarge".to_string());
    rig.run(&with_type).unwrap();
    rig.run(&host_request(Command::Provision, "agentspace"))
        .unwrap();
    assert_eq!(
        rig.reach.calls(),
        vec![
            "sync host:agentspace",
            "sync box",
            "bootstrap host:agentspace",
            "deploy host:agentspace -",
            "deploy host:agentspace m9g.2xlarge",
            "provision agentspace",
        ]
    );
    assert!(rig.aws.calls().is_empty());
}

// ---- -All ----

fn three_hosts(name: &str) -> TempHome {
    let mut hosts = Hosts::seed();
    hosts.hosts[0].instance_id = Some(ID.to_string());
    hosts
        .hosts
        .push(record("agentspace", HostState::Ready, Some(OTHER_ID)));
    hosts
        .hosts
        .push(record("newproject", HostState::Creating, None));
    home_with_registry(name, &hosts)
}

#[test]
fn status_all_reports_every_host_in_the_files_order_and_signs_nothing() {
    let home = three_hosts("status-all");
    let rig = Rig::new(
        &home,
        true,
        false,
        vec![
            ok(instance("stopped")),
            ok(volume(8)),
            ok(instance("stopped")),
            ok(volume(8)),
        ],
    );
    rig.run(&all_request(Command::Status)).unwrap();
    let lines = rig.say.lines();
    assert_eq!(lines[0], "Host      swimcoach (agentbox), ready");
    assert_eq!(lines[1], format!("Instance  {ID}"));
    assert_eq!(
        lines[6],
        format!("Host      agentspace ({HOST_STACK}), ready")
    );
    assert_eq!(lines[7], format!("Instance  {OTHER_ID}"));
    assert_eq!(
        lines[12..14],
        [
            "Host      newproject (agentspace-host-newproject), creating",
            "Instance  none recorded"
        ]
    );
    // The home holds no lease state file: no host gets a `Leases` line, and the report says so
    // once, after the last host, whatever `-Quiet` says (operator.md, status -All).
    let leases_file = home.ctx().home.join(".agentspace").join("leases.json");
    assert_eq!(
        lines[14..],
        [format!(
            "agentspace: The lease state file {} cannot be read: the file does not exist.",
            leases_file.display()
        )]
    );
    assert!(rig.reach.calls().is_empty(), "status -All signs nothing");
    assert_eq!(rig.aws.calls().len(), 4, "No call for the skipped host");
}

#[test]
fn status_all_goes_on_past_a_failed_host_and_counts_it() {
    let home = three_hosts("status-all-fails");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![
            Err(AwsError::Failed("No such instance.".to_string())),
            ok(instance("running")),
            ok(volume(8)),
            ok(ping("Online")),
        ],
    );
    let failure = rig.run(&all_request(Command::Status)).unwrap_err();
    assert_eq!(failure.message, "agentspace: 1 of 3 hosts failed.");
    let lines = rig.say.lines();
    assert_eq!(lines[0], "Host      swimcoach (agentbox), ready");
    assert_eq!(lines[1], "agentspace: No such instance.");
    assert_eq!(
        lines[2],
        format!("Host      agentspace ({HOST_STACK}), ready")
    );
    assert_eq!(lines[3], format!("Instance  {OTHER_ID}"));
}

#[test]
fn a_sign_in_failure_ends_an_all_command_at_once() {
    let home = three_hosts("status-all-sign-in");
    let rig = Rig::new(&home, false, false, vec![Err(AwsError::SignIn)]);
    let failure = rig.run(&all_request(Command::Status)).unwrap_err();
    assert_eq!(failure.message, agentspace_operator_api::SIGN_IN_MESSAGE);
    assert_eq!(
        rig.say.lines(),
        vec!["Host      swimcoach (agentbox), ready"]
    );
    assert_eq!(rig.aws.calls().len(), 1);
}

#[test]
fn down_all_stops_each_host_that_has_an_instance_and_skips_the_rest_and_signs_nothing() {
    let home = three_hosts("down-all");
    let rig = Rig::new(&home, false, false, vec![ok(json!({})), ok(json!({}))]);
    rig.run(&all_request(Command::Down)).unwrap();
    assert_eq!(
        rig.aws.calls(),
        vec![
            format!("ec2 stop-instances --instance-ids {ID}"),
            format!("ec2 stop-instances --instance-ids {OTHER_ID}"),
        ]
    );
    assert_eq!(
        rig.say.lines(),
        vec![
            "Host      swimcoach (agentbox), ready".to_string(),
            format!("Stopping {ID}."),
            format!("Host      agentspace ({HOST_STACK}), ready"),
            format!("Stopping {OTHER_ID}."),
            "Host      newproject (agentspace-host-newproject), creating".to_string(),
            "Skipping: no instance is recorded for this host.".to_string(),
        ]
    );
    assert!(rig.reach.calls().is_empty(), "down -All signs nothing");
}

#[test]
fn quiet_down_all_still_prints_the_host_lines() {
    let home = three_hosts("down-all-quiet");
    let rig = Rig::new(&home, true, false, vec![ok(json!({})), ok(json!({}))]);
    rig.run(&all_request(Command::Down)).unwrap();
    assert_eq!(rig.say.lines().len(), 3);
}

#[test]
fn sync_all_is_agentspace_syncs_once_and_the_operator_prints_and_loops_over_nothing() {
    // `sync -All`'s hosts, `Host` lines, skips and count are agentspace-sync's `sync_all`
    // (architect, 2026-11-21, for v0.07-T03): the operator calls it once and adds nothing.
    let home = three_hosts("sync-all");
    let rig = Rig::new(&home, false, false, vec![]);
    rig.run(&all_request(Command::Sync)).unwrap();
    assert_eq!(rig.reach.calls(), vec!["sync-all"]);
    assert!(rig.say.lines().is_empty(), "{:?}", rig.say.lines());
    assert!(rig.aws.calls().is_empty());
}

#[test]
fn sync_all_fails_with_agentspace_syncs_own_failure_and_nothing_added() {
    let home = three_hosts("sync-all-fails");
    let mut rig = Rig::new(&home, false, false, vec![]);
    rig.reach = FakeReach::default().failing("sync-all", "agentspace: 2 of 3 hosts failed.");
    let failure = rig.run(&all_request(Command::Sync)).unwrap_err();
    assert_eq!(failure.message, "agentspace: 2 of 3 hosts failed.");
    assert_eq!(rig.reach.calls(), vec!["sync-all"]);
    assert!(rig.say.lines().is_empty(), "{:?}", rig.say.lines());
}

#[test]
fn sync_all_is_dispatched_before_the_operator_reads_the_registry() {
    // agentspace-sync reads the registry itself, so an invalid one is its failure to give.
    let home = TempHome::new("sync-all-invalid-registry");
    let registry = agentspace_hosts::hosts_path(&home.ctx().home);
    std::fs::create_dir_all(registry.parent().unwrap()).unwrap();
    std::fs::write(&registry, b"not json").unwrap();
    let rig = Rig::new(&home, false, false, vec![]);
    rig.run(&all_request(Command::Sync)).unwrap();
    assert_eq!(rig.reach.calls(), vec!["sync-all"]);
}

// ---- status -All's lease lines, in process, in a zone the test chooses ----
//
// `Reach::clock_time` is the seam for the stop time's text (operator.md, status -All: the time is in
// the laptop's local time, read from the system when the line is printed). A test's `Reach` gives
// the text in any zone it likes, made by `agentspace_zone::clock_time_at` (the one formatter, which
// rounds up to the next whole minute), so the cases below fix the zone, and read the real clock
// only for the times they set relative to it. The cases that read the real system's zone are in
// `tests/status_leases.rs`.

/// A [`Reach`] that is a [`FakeReach`], but for the stop time's text in the zone it is given.
struct Zoned {
    inner: FakeReach,
    offset: fn(i64) -> i32,
}

impl Reach for Zoned {
    fn clock_time(&self, unix: i64) -> String {
        agentspace_zone::clock_time_at(unix, (self.offset)(unix))
    }
    fn push(&self, ctx: &Context, say: &dyn Say, id: &str, alias: &str, host: Option<&str>) {
        self.inner.push(ctx, say, id, alias, host);
    }
    fn ca(&self, ctx: &Context, env: &CaEnv, say: &dyn Say) -> Result<(), Failure> {
        self.inner.ca(ctx, env, say)
    }
    fn access(&self, ctx: &Context, env: &CaEnv, project: &str) -> Result<(), Failure> {
        self.inner.access(ctx, env, project)
    }
    fn sync(
        &self,
        ctx: &Context,
        env: &CaEnv,
        target: Target<'_>,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure> {
        self.inner.sync(ctx, env, target, say, aws, remote)
    }
    fn sync_all(
        &self,
        ctx: &Context,
        env: &CaEnv,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure> {
        self.inner.sync_all(ctx, env, say, aws, remote)
    }
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
    ) -> Result<(), Failure> {
        self.inner
            .deploy(ctx, env, target, instance_type, say, aws, remote)
    }
    fn bootstrap(
        &self,
        ctx: &Context,
        env: &CaEnv,
        target: Target<'_>,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure> {
        self.inner.bootstrap(ctx, env, target, say, aws, remote)
    }
    fn provision(
        &self,
        ctx: &Context,
        env: &CaEnv,
        project: &str,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure> {
        self.inner.provision(ctx, env, project, say, aws, remote)
    }
}

fn zoned(offset: fn(i64) -> i32) -> Zoned {
    Zoned {
        inner: FakeReach::default(),
        offset,
    }
}

fn utc(_: i64) -> i32 {
    0
}
fn minus_five_hours(_: i64) -> i32 {
    -5 * 3600
}
fn plus_five_thirty(_: i64) -> i32 {
    5 * 3600 + 1800
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// The civil date and time of a Unix instant in UTC (Howard Hinnant's algorithm), written as EC2
/// writes a launch time (`+00:00`) or as the lease state file's contract does (`Z`).
fn civil_time(unix: i64, zulu: bool) -> String {
    let days = unix.div_euclid(86_400);
    let rest = unix.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}{}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
        if zulu { "Z" } else { "+00:00" }
    )
}

fn ceil_minute(unix: i64) -> i64 {
    (unix + 59).div_euclid(60) * 60
}

fn instance_launched(state: &str, launch: i64) -> Value {
    let mut v = instance(state);
    v["Reservations"][0]["Instances"][0]["LaunchTime"] = json!(civil_time(launch, false));
    v
}

fn lease(
    id: u32,
    project: &str,
    state: &str,
    expires: Option<i64>,
    released: Option<i64>,
) -> Value {
    let mut r = json!({
        "id": format!("5eedc0de-0000-4000-8000-{id:012x}"),
        "project": project,
        "state": state,
        "platform": "linux/arm64",
        "profile": "agent-linux",
        "size": "m",
        "ttl_seconds": 3600,
        "created_at": civil_time(now_unix() - 86_400, true),
        "driver": "ec2",
    });
    if let Some(t) = expires {
        r["expires_at"] = json!(civil_time(t, true));
    }
    if let Some(t) = released {
        r["released_at"] = json!(civil_time(t, true));
    }
    if state == "ready" {
        r["ssh"] = json!({
            "host_alias": format!("agentspace-{project}-feed{id:04x}"),
            "user": "ubuntu"
        });
    }
    if state == "unsatisfiable" {
        r["reasons"] = json!([{"code": "no_capacity", "message": "No capacity."}]);
    }
    r
}

fn leases_file(home: &TempHome) -> std::path::PathBuf {
    home.ctx().home.join(".agentspace").join("leases.json")
}

fn write_leases(home: &TempHome, records: Vec<Value>) {
    let path = leases_file(home);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let doc = json!({"version": 0, "leases": records});
    std::fs::write(path, format!("{doc}\n")).unwrap();
}

impl Rig {
    /// `run`, with the `Reach` (and so the zone) a test chooses.
    fn run_in(&self, request: &Request, reach: &dyn Reach) -> Result<(), Failure> {
        done(dispatch(
            request,
            &Parts {
                ctx: &self.ctx,
                ca_env: &self.ca_env,
                say: &self.say,
                report: &self.say,
                aws: &self.aws,
                remote: &self.remote,
                clock: &self.clock,
                reach,
            },
        ))
    }
}

/// The box (stopped) and `agentspace`'s host (running since `launch`), with `records` in the lease
/// state file, reported in the zone `offset` gives: the report's `Leases` and `Guard` lines.
fn lease_lines_of_a_running_host(
    launch: i64,
    records: Vec<Value>,
    offset: fn(i64) -> i32,
) -> Vec<String> {
    let mut hosts = Hosts::seed();
    hosts.hosts[0].instance_id = Some(ID.to_string());
    hosts
        .hosts
        .push(record("agentspace", HostState::Ready, Some(OTHER_ID)));
    let home = home_with_registry("status-all-guard", &hosts);
    write_leases(&home, records);
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![
            ok(instance("stopped")),
            ok(volume(8)),
            ok(instance_launched("running", launch)),
            ok(volume(8)),
            ok(ping("Online")),
        ],
    );
    rig.run_in(&all_request(Command::Status), &zoned(offset))
        .unwrap();
    rig.say
        .lines()
        .into_iter()
        .filter(|l| l.starts_with("Guard") || l.starts_with("Leases"))
        .collect()
}

#[test]
fn status_all_prints_the_leases_line_for_every_host_and_the_guard_line_for_a_running_unexplained_one()
 {
    let now = now_unix();
    let mut hosts = Hosts::seed();
    hosts.hosts[0].instance_id = Some(ID.to_string());
    hosts
        .hosts
        .push(record("agentspace", HostState::Ready, Some(OTHER_ID)));
    hosts
        .hosts
        .push(record("example", HostState::Creating, None));
    let home = home_with_registry("status-all-example", &hosts);
    write_leases(
        &home,
        vec![
            lease(1, "swimcoach", "ready", Some(now + 3600), None),
            lease(2, "example", "pending", None, None),
        ],
    );
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![
            ok(instance("running")),
            ok(volume(8)),
            ok(ping("Online")),
            ok(instance_launched("running", now - 300)),
            ok(volume(8)),
            ok(ping("Online")),
        ],
    );
    rig.run_in(&all_request(Command::Status), &zoned(minus_five_hours))
        .unwrap();
    let stop = agentspace_zone::clock_time_at(now - 300 + 900, -5 * 3600);
    let lines = rig.say.lines();
    let at = |prefix: &str| lines.iter().position(|l| l.starts_with(prefix)).unwrap();
    let (box_at, host_at, making_at) = (
        at("Host      swimcoach"),
        at("Host      agentspace"),
        at("Host      example"),
    );
    assert_eq!(lines[box_at + 6], "Leases    1 live");
    assert_eq!(lines[host_at + 6], "Leases    0 live");
    assert_eq!(
        lines[host_at + 7],
        format!("Guard     unexplained; stops at {stop}")
    );
    assert_eq!(lines[making_at + 1], "Instance  none recorded");
    assert_eq!(lines[making_at + 2], "Leases    1 live");
    assert_eq!(lines.len(), making_at + 3, "nothing more: {lines:?}");
    assert_eq!(rig.aws.calls().len(), 6, "no call beyond the report's own");
}

#[test]
fn the_guard_line_is_in_the_zone_the_system_gives_by_whatever_offset() {
    let now = now_unix();
    for (offset, seconds) in [
        (utc as fn(i64) -> i32, 0),
        (minus_five_hours as fn(i64) -> i32, -5 * 3600),
        (plus_five_thirty as fn(i64) -> i32, 5 * 3600 + 1800),
    ] {
        let lines = lease_lines_of_a_running_host(now - 300, vec![], offset);
        assert_eq!(
            lines,
            [
                "Leases    0 live".to_string(),
                "Leases    0 live".to_string(),
                format!(
                    "Guard     unexplained; stops at {}",
                    agentspace_zone::clock_time_at(now - 300 + 900, seconds)
                ),
            ],
            "offset {seconds}"
        );
    }
}

#[test]
fn the_since_is_the_later_of_the_launch_time_and_the_last_lease_end() {
    let now = now_unix();
    let want = |since: i64| {
        vec![
            "Leases    0 live".to_string(),
            "Leases    0 live".to_string(),
            format!(
                "Guard     unexplained; stops at {}",
                agentspace_zone::clock_time_at(since + 900, 0)
            ),
        ]
    };
    // The launch time, for a project with no lease end, and when it is the later.
    assert_eq!(
        lease_lines_of_a_running_host(now - 300, vec![], utc),
        want(now - 300)
    );
    assert_eq!(
        lease_lines_of_a_running_host(
            now - 120,
            vec![lease(
                1,
                "agentspace",
                "released",
                Some(now - 3600),
                Some(now - 3700)
            )],
            utc
        ),
        want(now - 120)
    );
    // A released lease's `released_at` before its `expires_at`, when it is the later.
    assert_eq!(
        lease_lines_of_a_running_host(
            now - 7200,
            vec![lease(
                1,
                "agentspace",
                "released",
                Some(now + 3000),
                Some(now - 120)
            )],
            utc
        ),
        want(now - 120)
    );
    // A lease released while pending: its `released_at`.
    assert_eq!(
        lease_lines_of_a_running_host(
            now - 7200,
            vec![lease(1, "agentspace", "released", None, Some(now - 180))],
            utc
        ),
        want(now - 180)
    );
    // An expired lease: its `expires_at`.
    assert_eq!(
        lease_lines_of_a_running_host(
            now - 7200,
            vec![lease(1, "agentspace", "ready", Some(now - 240), None)],
            utc
        ),
        want(now - 240)
    );
    // Released after it expired: it ended when it expired (P11).
    assert_eq!(
        lease_lines_of_a_running_host(
            now - 7200,
            vec![lease(
                1,
                "agentspace",
                "released",
                Some(now - 200),
                Some(now - 60)
            )],
            utc
        ),
        want(now - 200)
    );
    // An unsatisfiable lease and another project's lease add no end.
    assert_eq!(
        lease_lines_of_a_running_host(
            now - 300,
            vec![
                lease(1, "agentspace", "unsatisfiable", None, None),
                lease(2, "swimcoach", "released", Some(now - 100), Some(now - 100)),
                lease(
                    3,
                    "agentspace-2",
                    "released",
                    Some(now - 100),
                    Some(now - 100)
                ),
            ],
            utc
        ),
        want(now - 300)
    );
}

#[test]
fn seconds_round_up_to_the_next_whole_minute() {
    let now = now_unix();
    let aligned = (now - 300).div_euclid(60) * 60;
    for launch in [aligned, aligned + 1, aligned + 59, aligned - 1] {
        let lines = lease_lines_of_a_running_host(launch, vec![], utc);
        assert_eq!(
            lines[2],
            format!(
                "Guard     unexplained; stops at {}",
                agentspace_zone::clock_time_at(launch + 900, 0)
            ),
            "launch {} seconds after a minute",
            launch - aligned
        );
    }
}

#[test]
fn a_stop_time_that_has_passed_shows_the_reports_minute_or_the_next() {
    for launch in [now_unix() - 7200, 1_577_836_800] {
        let before = now_unix();
        let lines = lease_lines_of_a_running_host(launch, vec![], utc);
        let after = now_unix();
        let mut candidates = Vec::new();
        let mut minute = ceil_minute(before);
        while minute <= ceil_minute(after) {
            candidates.push(format!(
                "Guard     unexplained; stops at {}",
                agentspace_zone::clock_time_at(minute, 0)
            ));
            minute += 60;
        }
        assert!(candidates.contains(&lines[2]), "{:?}", lines[2]);
    }
}

#[test]
fn a_host_a_live_lease_explains_a_stopped_host_and_a_pending_host_get_no_guard_line() {
    let now = now_unix();
    let lines = lease_lines_of_a_running_host(
        now - 300,
        vec![lease(1, "agentspace", "pending", None, None)],
        utc,
    );
    assert_eq!(lines, ["Leases    0 live", "Leases    1 live"]);
    for state in [
        "pending",
        "stopping",
        "stopped",
        "shutting-down",
        "terminated",
    ] {
        let mut hosts = Hosts::seed();
        hosts.hosts[0].instance_id = Some(ID.to_string());
        let home = home_with_registry("status-all-states", &hosts);
        write_leases(&home, vec![]);
        let rig = Rig::new(
            &home,
            false,
            false,
            vec![ok(instance(state)), ok(volume(8))],
        );
        rig.run_in(&all_request(Command::Status), &zoned(utc))
            .unwrap();
        let lines = rig.say.lines();
        assert_eq!(
            lines.last().map(String::as_str),
            Some("Leases    0 live"),
            "{state}"
        );
        assert!(!lines.iter().any(|l| l.starts_with("Guard")), "{state}");
    }
}

#[test]
fn a_running_unexplained_host_with_no_launch_time_fails_its_part_alone() {
    let mut hosts = Hosts::seed();
    hosts.hosts[0].instance_id = Some(ID.to_string());
    hosts
        .hosts
        .push(record("agentspace", HostState::Ready, Some(OTHER_ID)));
    let home = home_with_registry("status-all-no-launch", &hosts);
    write_leases(&home, vec![]);
    let mut running = instance("running");
    running["Reservations"][0]["Instances"][0]
        .as_object_mut()
        .unwrap()
        .remove("LaunchTime");
    // The box is stopped, so it never needs a launch time; the running host that no lease explains
    // is the second, and its part fails alone.
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![
            ok(instance("stopped")),
            ok(volume(8)),
            ok(running),
            ok(volume(8)),
            ok(ping("Online")),
        ],
    );
    let failure = rig
        .run_in(&all_request(Command::Status), &zoned(utc))
        .unwrap_err();
    assert_eq!(failure.message, "agentspace: 1 of 2 hosts failed.");
    let lines = rig.say.lines();
    assert_eq!(lines[0], "Host      swimcoach (agentbox), ready");
    assert_eq!(
        lines[6], "Leases    0 live",
        "the report before it is whole"
    );
    assert_eq!(
        lines[7],
        format!("Host      agentspace ({HOST_STACK}), ready")
    );
    assert_eq!(
        lines[8],
        "agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].LaunchTime.",
        "before its five lines, and no Leases or Guard line"
    );
    assert_eq!(lines.len(), 9, "{lines:?}");
}

#[test]
fn an_unreadable_lease_state_file_leaves_out_the_lease_lines_and_says_why_once() {
    let mut hosts = Hosts::seed();
    hosts.hosts[0].instance_id = Some(ID.to_string());
    for quiet in [false, true] {
        let home = home_with_registry("status-all-bad-leases", &hosts);
        let path = leases_file(&home);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not json").unwrap();
        let rig = Rig::new(
            &home,
            quiet,
            false,
            vec![ok(instance("running")), ok(volume(8)), ok(ping("Online"))],
        );
        rig.run_in(&all_request(Command::Status), &zoned(utc))
            .unwrap();
        let lines = rig.say.lines();
        assert_eq!(lines.len(), 7, "quiet {quiet}: {lines:?}");
        assert!(lines[6].starts_with(&format!(
            "agentspace: The lease state file {} cannot be read: ",
            path.display()
        )));
        assert!(lines[6].ends_with('.'), "{}", lines[6]);
        assert!(
            !lines
                .iter()
                .any(|l| l.starts_with("Leases") || l.starts_with("Guard"))
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"not json", "never written");
    }
}

#[test]
fn status_without_all_reads_no_lease_state_file_and_no_launch_time() {
    // The five lines alone, with a lease state file that is not valid in the home, and with an
    // answer that has no `LaunchTime`: neither is looked at.
    let home = home_with_cache("status-no-leases");
    let path = leases_file(&home);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"not json").unwrap();
    let mut running = instance("running");
    running["Reservations"][0]["Instances"][0]
        .as_object_mut()
        .unwrap()
        .remove("LaunchTime");
    let rig = Rig::new(
        &home,
        false,
        false,
        vec![ok(running), ok(volume(8)), ok(ping("Online"))],
    );
    rig.run_in(&request(Command::Status), &zoned(utc)).unwrap();
    assert_eq!(rig.say.lines().len(), 5, "{:?}", rig.say.lines());
    assert_eq!(rig.aws.calls().len(), 3);
}

#[test]
fn down_all_never_reads_the_lease_state_file() {
    let home = three_hosts("down-all-bad-leases");
    let path = leases_file(&home);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"not json").unwrap();
    let rig = Rig::new(&home, false, false, vec![ok(json!({})), ok(json!({}))]);
    rig.run(&all_request(Command::Down)).unwrap();
    assert!(
        !rig.say
            .lines()
            .iter()
            .any(|l| l.contains("lease state file"))
    );
}
