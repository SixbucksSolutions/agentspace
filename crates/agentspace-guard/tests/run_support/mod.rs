//! Shared fakes and builders for the guard's run tests (story E0.08-T03-S01, card #0177).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Written from docs/operator.md (guard, Calls by command, AWS failures) and docs/state.md alone,
//! against the pinned API of `agentspace_guard` and `agentspace_operator_api`. Every run goes
//! through a stand-in `Aws` that records each call and answers from a script, and through a
//! throwaway home under `%TEMP%`, which `Context` is pointed at as a value and never through
//! the process environment. No test calls real AWS or touches Terry's real files.
//!
//! A failure message of a test never prints a path under the throwaway home: `World::mask`
//! replaces it, because the temporary folder's path holds the Windows user name.
#![allow(dead_code)]

use std::collections::HashMap;
use std::fs;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

pub use agentspace_guard::{GRACE_SECONDS, Input, Report, run};
pub use agentspace_operator_api::{
    Aws, AwsError, Call, CallerLine, Context, Failure, Mode, Output, SIGN_IN_MESSAGE, Say,
};
pub use serde_json::{Value, json};

// ---------------------------------------------------------------------------------------------
// Obviously fake identifiers.
// ---------------------------------------------------------------------------------------------

/// `i-0123456789abcdef<n>`, `n` from 0 to 15.
pub fn id(n: u32) -> String {
    format!("i-0123456789abcdef{n:x}")
}

/// A 17-digit ID made of zeros.
pub const ZEROS: &str = "i-00000000000000001";

pub const SIGN_IN_REASON: &str = "AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry.";

pub fn stack_of(project: &str) -> String {
    format!("agentspace-host-{project}")
}

// ---------------------------------------------------------------------------------------------
// Times, as Unix seconds and as `YYYY-MM-DDTHH:MM:SSZ`.
// ---------------------------------------------------------------------------------------------

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The Unix seconds of `YYYY-MM-DDTHH:MM:SSZ`.
pub fn t(text: &str) -> i64 {
    let n = |a: usize, b: usize| -> i64 { text[a..b].parse().expect("a number in a test time") };
    assert!(
        text.len() == 20 && text.ends_with('Z'),
        "a test time: {text}"
    );
    days_from_civil(n(0, 4), n(5, 7), n(8, 10)) * 86_400
        + n(11, 13) * 3600
        + n(14, 16) * 60
        + n(17, 19)
}

/// `YYYY-MM-DDTHH:MM:SSZ` of Unix seconds.
pub fn z(secs: i64) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

/// The run's time of most tests: 2026-11-22T20:00:00Z.
pub fn now() -> i64 {
    t("2026-11-22T20:00:00Z")
}

/// A time on the day of `now()`, `HH:MM:SS` as written, as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn today(hms: &str) -> String {
    format!("2026-11-22T{hms}Z")
}

/// A time on the day of `now()` as an instance's `LaunchTime`, in the AWS CLI's form.
pub fn launch(hms: &str) -> String {
    format!("2026-11-22T{hms}+00:00")
}

// ---------------------------------------------------------------------------------------------
// Records of the files.
// ---------------------------------------------------------------------------------------------

pub fn host(project: &str, stack: &str, state: &str, instance_id: Option<&str>) -> Value {
    let mut record = json!({"project": project, "stack": stack, "state": state});
    if let Some(instance_id) = instance_id {
        record["instance_id"] = json!(instance_id);
    }
    record
}

/// The box's record, which every valid registry holds exactly once.
pub fn box_host(instance_id: Option<&str>) -> Value {
    host("swimcoach", "agentbox", "ready", instance_id)
}

/// A `ready` record of the host of `project`.
pub fn ready_host(project: &str, instance_id: Option<&str>) -> Value {
    host(project, &stack_of(project), "ready", instance_id)
}

static LEASE_NUMBER: AtomicU32 = AtomicU32::new(1);

fn lease_base(project: &str, state: &str) -> Value {
    let n = LEASE_NUMBER.fetch_add(1, Ordering::SeqCst);
    json!({
        "id": format!("00000000-0000-4000-8000-{n:012x}"),
        "project": project,
        "state": state,
        "platform": "linux/arm64",
        "profile": "agent-linux",
        "size": "m",
        "ttl_seconds": 28800,
        "created_at": "2026-11-20T00:00:00Z",
    })
}

/// A stored `ready` lease; it is live while the run's time is before `expires_at`.
pub fn lease_ready(project: &str, expires_at: &str) -> Value {
    let mut lease = lease_base(project, "ready");
    lease["expires_at"] = json!(expires_at);
    lease["ssh"] =
        json!({"host_alias": format!("agentspace-{project}-00000000"), "user": "ubuntu"});
    lease
}

pub fn lease_pending(project: &str) -> Value {
    lease_base(project, "pending")
}

/// A `released` lease; `expires_at` is present when the lease was ever `ready`.
pub fn lease_released(project: &str, released_at: &str, expires_at: Option<&str>) -> Value {
    let mut lease = lease_base(project, "released");
    lease["released_at"] = json!(released_at);
    if let Some(expires_at) = expires_at {
        lease["expires_at"] = json!(expires_at);
    }
    lease
}

pub fn lease_unsatisfiable(project: &str) -> Value {
    let mut lease = lease_base(project, "unsatisfiable");
    lease["reasons"] = json!([{"code": "no_capacity", "message": "No capacity."}]);
    lease
}

// ---------------------------------------------------------------------------------------------
// The stand-in AWS CLI.
// ---------------------------------------------------------------------------------------------

/// One AWS CLI call as the guard made it.
#[derive(Debug, Clone, PartialEq)]
pub struct Rec {
    pub service: String,
    pub operation: String,
    pub args: Vec<String>,
    pub output: Output,
}

impl Rec {
    /// `service operation`.
    pub fn key(&self) -> String {
        format!("{} {}", self.service, self.operation)
    }

    /// The instance ID of a stop or a send: the argument after `--instance-ids`.
    pub fn instance(&self) -> Option<String> {
        let at = self.args.iter().position(|a| a == "--instance-ids")?;
        self.args.get(at + 1).cloned()
    }
}

#[derive(Default)]
pub struct FakeAws {
    calls: Mutex<Vec<Rec>>,
    /// The elements `ec2 describe-instances` can answer with.
    instances: Mutex<Vec<Value>>,
    /// Replaces the whole answer of `ec2 describe-instances`.
    describe_answer: Mutex<Option<Result<Value, AwsError>>>,
    stacks: Mutex<HashMap<String, Result<Value, AwsError>>>,
    stops: Mutex<HashMap<String, AwsError>>,
    sends: Mutex<HashMap<String, AwsError>>,
    /// Files whose lock is probed at each call: whether another program holds each.
    probe: Mutex<Vec<PathBuf>>,
    probed: Mutex<Vec<Probed>>,
}

/// A call's key, its instance ID, and whether each probed file was held then.
pub type Probed = (String, Option<String>, Vec<bool>);

/// Whether a file is held by another handle, by one try with no sharing and no creating.
pub fn is_held(path: &Path) -> bool {
    match fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)
    {
        Ok(_) => false,
        Err(error) => error.raw_os_error() == Some(32),
    }
}

impl FakeAws {
    /// Probes these files at every call from now on.
    pub fn probe_locks(&self, paths: Vec<PathBuf>) {
        *self.probe.lock().unwrap() = paths;
    }

    /// For every call: its key, its instance ID, and whether each probed file was held then.
    pub fn probed(&self) -> Vec<Probed> {
        self.probed.lock().unwrap().clone()
    }

    /// An instance of the fleet, as `Reservations[].Instances[]` holds it.
    pub fn add_instance(&self, instance_id: &str, state: &str, launch_time: Option<&str>) {
        let mut element = json!({"InstanceId": instance_id, "State": {"Name": state}});
        if let Some(launch_time) = launch_time {
            element["LaunchTime"] = json!(launch_time);
        }
        self.instances.lock().unwrap().push(element);
    }

    /// An element of the answer as the test writes it.
    pub fn add_raw_instance(&self, element: Value) {
        self.instances.lock().unwrap().push(element);
    }

    pub fn remove_instance(&self, instance_id: &str) {
        self.instances
            .lock()
            .unwrap()
            .retain(|e| e["InstanceId"] != json!(instance_id));
    }

    pub fn set_launch_time(&self, instance_id: &str, launch_time: Option<&str>) {
        for element in self.instances.lock().unwrap().iter_mut() {
            if element["InstanceId"] == json!(instance_id) {
                match launch_time {
                    Some(text) => element["LaunchTime"] = json!(text),
                    None => {
                        element.as_object_mut().unwrap().remove("LaunchTime");
                    }
                }
            }
        }
    }

    pub fn answer_describe_with(&self, answer: Result<Value, AwsError>) {
        *self.describe_answer.lock().unwrap() = Some(answer);
    }

    /// The `InstanceId` output of a stack.
    pub fn set_stack_id(&self, stack: &str, instance_id: &str) {
        self.set_stack(
            stack,
            Ok(json!({"Stacks": [{"Outputs": [
                {"OutputKey": "Other", "OutputValue": "x"},
                {"OutputKey": "InstanceId", "OutputValue": instance_id},
            ]}]})),
        );
    }

    pub fn set_stack(&self, stack: &str, answer: Result<Value, AwsError>) {
        self.stacks
            .lock()
            .unwrap()
            .insert(stack.to_string(), answer);
    }

    pub fn fail_stop(&self, instance_id: &str, error: AwsError) {
        self.stops
            .lock()
            .unwrap()
            .insert(instance_id.to_string(), error);
    }

    pub fn fail_send(&self, instance_id: &str, error: AwsError) {
        self.sends
            .lock()
            .unwrap()
            .insert(instance_id.to_string(), error);
    }

    pub fn calls(&self) -> Vec<Rec> {
        self.calls.lock().unwrap().clone()
    }
}

impl Aws for FakeAws {
    fn call(&self, call: &Call) -> Result<Value, AwsError> {
        let rec = Rec {
            service: call.service.to_string(),
            operation: call.operation.to_string(),
            args: call.args.iter().map(|a| a.to_string()).collect(),
            output: call.output,
        };
        self.calls.lock().unwrap().push(rec.clone());
        let probes = self.probe.lock().unwrap().clone();
        if !probes.is_empty() {
            let held = probes.iter().map(|p| is_held(p)).collect();
            self.probed
                .lock()
                .unwrap()
                .push((rec.key(), rec.instance(), held));
        }
        let arg = |n: usize| rec.args.get(n).cloned().unwrap_or_default();
        match rec.key().as_str() {
            "cloudformation describe-stacks" => self
                .stacks
                .lock()
                .unwrap()
                .get(&arg(1))
                .cloned()
                .unwrap_or_else(|| Err(AwsError::Failed("The stand-in has no such stack.".into()))),
            "ec2 describe-instances" => {
                if let Some(answer) = self.describe_answer.lock().unwrap().clone() {
                    return answer;
                }
                let filter = arg(1);
                let Some(values) = filter.strip_prefix("Name=instance-id,Values=") else {
                    return Err(AwsError::Failed(
                        "The stand-in wants Name=instance-id,Values=<ids>.".into(),
                    ));
                };
                let wanted: Vec<&str> = values.split(',').collect();
                // Newest first, so a reader that goes by position and not by InstanceId fails.
                let reservations: Vec<Value> = self
                    .instances
                    .lock()
                    .unwrap()
                    .iter()
                    .rev()
                    .filter(|e| e["InstanceId"].as_str().is_none_or(|i| wanted.contains(&i)))
                    .map(|e| json!({"Instances": [e]}))
                    .collect();
                Ok(json!({"Reservations": reservations}))
            }
            "ec2 stop-instances" => match self.stops.lock().unwrap().get(&arg(1)) {
                Some(error) => Err(error.clone()),
                None => Ok(json!({"StoppingInstances": []})),
            },
            "ssm send-command" => match self.sends.lock().unwrap().get(&arg(1)) {
                Some(error) => Err(error.clone()),
                None => {
                    Ok(json!({"Command": {"CommandId": "00000000-0000-0000-0000-000000000000"}}))
                }
            },
            other => Err(AwsError::Failed(format!(
                "The stand-in has no answer for {other}."
            ))),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The stand-in for `Say`.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Progress {
        person: String,
        caller: Option<CallerLine>,
    },
    Warning(String),
    Detail(String),
}

#[derive(Default)]
pub struct FakeSay {
    events: Mutex<Vec<Event>>,
}

impl Say for FakeSay {
    fn mode(&self) -> Mode {
        Mode::Person
    }
    fn progress(&self, person: &str, caller: Option<CallerLine>) {
        self.events.lock().unwrap().push(Event::Progress {
            person: person.to_string(),
            caller,
        });
    }
    fn warning(&self, person: &str) {
        self.events
            .lock()
            .unwrap()
            .push(Event::Warning(person.to_string()));
    }
    fn detail(&self, text: &str) {
        self.events
            .lock()
            .unwrap()
            .push(Event::Detail(text.to_string()));
    }
}

// ---------------------------------------------------------------------------------------------
// The throwaway home.
// ---------------------------------------------------------------------------------------------

static WORLD_NUMBER: AtomicU32 = AtomicU32::new(0);

pub struct World {
    pub root: PathBuf,
    pub home: PathBuf,
    /// The state folder of the run: `<home>\.agentspace` until a test moves it.
    pub state_dir: PathBuf,
    pub ctx: Context,
    pub aws: FakeAws,
    pub say: FakeSay,
}

impl World {
    /// A fresh folder under `%TEMP%`, with an empty `home` folder in it.
    pub fn new(name: &str) -> World {
        let n = WORLD_NUMBER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir()
            .join("agentspace-guard-run-tests")
            .join(format!("{name}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        let ctx = Context {
            home: home.clone(),
            local_app_data: None,
            system_root: root.join("system"),
            nested: false,
        };
        let state_dir = home.join(".agentspace");
        World {
            root,
            home,
            state_dir,
            ctx,
            aws: FakeAws::default(),
            say: FakeSay::default(),
        }
    }

    /// `<home>\.agentspace`.
    pub fn agentspace(&self) -> PathBuf {
        self.home.join(".agentspace")
    }

    pub fn guard_lock(&self) -> PathBuf {
        self.agentspace().join("guard.lock")
    }

    pub fn guard_log(&self) -> PathBuf {
        self.agentspace().join("guard.log")
    }

    pub fn hosts_path(&self) -> PathBuf {
        self.agentspace().join("hosts.json")
    }

    pub fn leases_path(&self) -> PathBuf {
        self.state_dir.join("leases.json")
    }

    pub fn provision_lock(&self, project: &str) -> PathBuf {
        self.agentspace()
            .join("hosts")
            .join(project)
            .join("provision.lock")
    }

    pub fn write_hosts(&self, records: &[Value]) {
        fs::create_dir_all(self.agentspace()).unwrap();
        let text = json!({"version": 0, "hosts": records}).to_string();
        fs::write(self.hosts_path(), text).unwrap();
    }

    pub fn write_leases(&self, leases: &[Value]) {
        fs::create_dir_all(&self.state_dir).unwrap();
        let text = json!({"version": 0, "leases": leases}).to_string();
        fs::write(self.leases_path(), text).unwrap();
    }

    pub fn write_leases_raw(&self, bytes: &[u8]) {
        fs::create_dir_all(&self.state_dir).unwrap();
        fs::write(self.leases_path(), bytes).unwrap();
    }

    pub fn run(&self, at: i64) -> Result<Report, Failure> {
        let input = Input {
            ctx: &self.ctx,
            state_dir: &self.state_dir,
            now: at,
        };
        run(&input, &self.say, &self.aws)
    }

    /// A run that is expected to go to its end.
    pub fn run_ok(&self, at: i64) -> Report {
        match self.run(at) {
            Ok(report) => report,
            Err(failure) => panic!("The run failed: {}", self.mask(&failure.message)),
        }
    }

    /// A run that is expected to fail; the failure.
    pub fn run_err(&self, at: i64) -> Failure {
        match self.run(at) {
            Err(failure) => failure,
            Ok(report) => panic!("The run went to its end: {report:?}"),
        }
    }

    /// Text with the throwaway folder's path written as `<tmp>`.
    pub fn mask(&self, text: &str) -> String {
        text.replace(&self.root.display().to_string(), "<tmp>")
    }

    /// Equality that never prints a path under the throwaway folder.
    pub fn expect_eq<T: std::fmt::Debug + PartialEq>(&self, actual: &T, expected: &T, what: &str) {
        if actual != expected {
            panic!(
                "{what}\n  actual:   {}\n  expected: {}",
                self.mask(&format!("{actual:?}")),
                self.mask(&format!("{expected:?}"))
            );
        }
    }

    /// A condition that never prints a path under the throwaway folder.
    pub fn expect(&self, condition: bool, what: &str, detail: &str) {
        if !condition {
            panic!("{what}: {}", self.mask(detail));
        }
    }

    pub fn calls(&self) -> Vec<Rec> {
        self.aws.calls()
    }

    /// `service operation` of every call, in order.
    pub fn call_keys(&self) -> Vec<String> {
        self.calls().iter().map(Rec::key).collect()
    }

    /// The calls with the given key, in order.
    pub fn calls_of(&self, key: &str) -> Vec<Rec> {
        self.calls()
            .into_iter()
            .filter(|c| c.key() == key)
            .collect()
    }

    /// The instance IDs of the stops, in order.
    pub fn stopped_ids(&self) -> Vec<String> {
        self.calls_of("ec2 stop-instances")
            .iter()
            .filter_map(Rec::instance)
            .collect()
    }

    /// The instance IDs of the horizon sends, in order.
    pub fn sent_ids(&self) -> Vec<String> {
        self.calls_of("ssm send-command")
            .iter()
            .filter_map(Rec::instance)
            .collect()
    }

    /// The horizon each send carried, by instance ID, in Unix seconds.
    pub fn horizons(&self) -> Vec<(String, i64)> {
        self.calls_of("ssm send-command")
            .iter()
            .map(|c| (c.instance().unwrap(), horizon_of(c)))
            .collect()
    }

    pub fn events(&self) -> Vec<Event> {
        self.say.events.lock().unwrap().clone()
    }

    /// Every progress line, in order.
    pub fn progress(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Progress { person, caller } => {
                    assert_eq!(caller, None, "A guard line has no caller form");
                    Some(person)
                }
                _ => None,
            })
            .collect()
    }

    /// Every warning, in order: the guard's failure channel, which `-Quiet` never suppresses.
    pub fn warnings(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Warning(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    /// A host's part that failed is said exactly once, through `Say::warning` and through no
    /// other channel, as `agentspace: Host '<project>': <message>`.
    pub fn expect_host_failure_warned(&self, project: &str, message: &str) {
        let line = format!("agentspace: Host '{project}': {message}");
        let warned = self.warnings().iter().filter(|w| **w == line).count();
        assert_eq!(warned, 1, "The failure is warned once: {line}");
        let elsewhere = self
            .events()
            .iter()
            .filter(|e| match e {
                Event::Warning(_) => false,
                Event::Progress { person, .. } => person.contains(message),
                Event::Detail(text) => text.contains(message),
            })
            .count();
        assert_eq!(elsewhere, 0, "The failure is said through warning alone");
    }

    /// Every text the run said, of any kind.
    pub fn said(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .map(|e| match e {
                Event::Progress { person, .. } => person,
                Event::Warning(text) | Event::Detail(text) => text,
            })
            .collect()
    }

    pub fn log_exists(&self) -> bool {
        self.guard_log().exists()
    }

    /// The log's lines, each after its checks: UTF-8, no byte order mark, no CR, every line ended
    /// by one LF, and each line starting with a `YYYY-MM-DDTHH:MM:SSZ` time and one space. A
    /// missing log is no lines. Each is returned as its time and its text.
    pub fn log(&self) -> Vec<(String, String)> {
        let Ok(bytes) = fs::read(self.guard_log()) else {
            return Vec::new();
        };
        assert!(
            !bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
            "The log has a byte order mark"
        );
        let text = String::from_utf8(bytes).expect("The log is UTF-8");
        assert!(!text.contains('\r'), "The log has a CR");
        assert!(
            text.is_empty() || text.ends_with('\n'),
            "The log's last line has no LF"
        );
        text.lines()
            .map(|line| {
                let (time, rest) = line.split_once(' ').expect("A line has a time and a text");
                assert!(is_stamp(time), "A line starts with a time, not {time:?}");
                (time.to_string(), rest.to_string())
            })
            .collect()
    }

    /// The log's lines without their times.
    pub fn rests(&self) -> Vec<String> {
        self.log().into_iter().map(|(_, rest)| rest).collect()
    }

    /// The names a folder holds, sorted; none for a folder that is missing.
    pub fn listing(&self, dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ`, by its shape.
pub fn is_stamp(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
}

/// The stop time that the progress line of an unexplained host ends with: `<h:mm><am|pm>.`
pub fn is_clock_ending(text: &str) -> bool {
    let Some(body) = text.strip_suffix('.') else {
        return false;
    };
    let Some(body) = body.strip_suffix("am").or_else(|| body.strip_suffix("pm")) else {
        return false;
    };
    let Some((hour, minute)) = body.split_once(':') else {
        return false;
    };
    (1..=12).contains(&hour.parse::<u32>().unwrap_or(0))
        && !hour.starts_with('0')
        && minute.len() == 2
        && minute.parse::<u32>().is_ok_and(|m| m < 60)
}

/// The line that an unexplained host prints, with its time in the form `h:mm` plus `am` or `pm`.
pub fn is_unexplained_progress(line: &str, project: &str, instance_id: &str) -> bool {
    let prefix = format!("The host of '{project}' ({instance_id}) has no live lease; it stops at ");
    line.strip_prefix(&prefix).is_some_and(is_clock_ending)
}

/// The horizon that a send carries: the one decimal word of 9 or more digits in the command.
pub fn horizon_of(rec: &Rec) -> i64 {
    let command = command_of(rec);
    let words: Vec<&str> = command
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() >= 9 && w.bytes().all(|b| b.is_ascii_digit()))
        .collect();
    assert_eq!(
        words.len(),
        1,
        "The command holds the horizon as one word of its own"
    );
    words[0].parse().unwrap()
}

/// The command of a send: what follows `commands=` in the last argument.
pub fn command_of(rec: &Rec) -> String {
    rec.args
        .last()
        .and_then(|a| a.strip_prefix("commands="))
        .expect("The last argument is commands=<command>")
        .to_string()
}

// ---------------------------------------------------------------------------------------------
// Locks held by a test.
// ---------------------------------------------------------------------------------------------

/// The file open for reading and writing with no sharing, created if missing, as a lock is held.
pub fn try_hold(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(path)
}

/// A lock held by the test until the handle is dropped; its folder is made first.
pub fn hold(path: &Path) -> fs::File {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    try_hold(path).expect("The test takes the lock")
}
