//! Shared fakes and adapters for the agentspace-box tests (story E0.06-T01-S03).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Written from docs/operator.md and the pinned `agentspace-operator-api` alone. Every call into
//! agentspace-box's own API, which the pin does not fix, is in the `api` section below, and
//! nowhere else: when the implementation names things differently, only that section changes, and
//! no assertion does.
#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[allow(unused_imports)]
pub use agentspace_operator_api::{
    Aws, AwsError, Call, CallerFailure, CallerLine, Failure, Mode, Output, SIGN_IN_MESSAGE, Say,
};
pub use serde_json::{Value, json};

pub const ID: &str = "i-0123456789abcdef0";
pub const CMD_PREFIX: &str = "cmd-";

/// The fake clock's start: 2026-11-19T12:00:00Z, as a duration since the Unix epoch.
pub const EPOCH_START_SECS: u64 = 1_795_089_600;

pub const SECOND: Duration = Duration::from_secs(1);

// ---------------------------------------------------------------------------------------------
// The API assumed of agentspace-box. Every use of the crate is here.
// ---------------------------------------------------------------------------------------------
pub mod api {
    use super::*;

    // The crate's clock gives a monotonic `now` and, apart from it, the wall-clock `utc_now` that
    // the 20-minute rule compares a launch time with. The fake gives both from one reading.
    impl agentspace_box::Clock for FakeClock {
        fn now(&self) -> Duration {
            FakeClock::now_value(self)
        }
        fn sleep(&self, duration: Duration) {
            FakeClock::sleep_value(self, duration)
        }
        fn utc_now(&self) -> std::time::SystemTime {
            std::time::UNIX_EPOCH + FakeClock::now_value(self)
        }
    }

    /// The start-and-wait step of operator.md, "Starting the box and waiting for it", steps 1 to
    /// 4. It prints through `say` and asks AWS through `aws`.
    pub fn start_and_wait(
        aws: &FakeAws,
        say: &FakeSay,
        clock: &FakeClock,
        id: &str,
    ) -> Result<(), Failure> {
        agentspace_box::start_and_wait_for(aws, say, clock, id)
    }

    /// What the crate decides from the read of `instance-id.json`: `Ok(Some(id))` for a cache
    /// that holds an ID, `Ok(None)` for one that counts as missing (absent, empty, whitespace,
    /// unparsable, not matching its schema), and FA-16's failure for one that cannot be opened.
    // Reading the file is T03's, so the crate takes what the read gave and the file's full path
    // for the message; `ErrorKind::NotFound` is a missing file, not an unopenable one.
    pub fn read_cache(
        path: &Path,
        read: std::io::Result<Vec<u8>>,
    ) -> Result<Option<String>, Failure> {
        agentspace_box::read_cache(path, read)
    }

    /// The rebuild from `cloudformation describe-stacks`: the ID, and the exact bytes of the cache
    /// to write (T03 writes them). A failure gives no bytes, so no cache is written.
    /// The box's stack, `agentbox`.
    pub fn rebuild_cache(aws: &FakeAws) -> Result<(String, Vec<u8>), Failure> {
        rebuild_cache_for(aws, "agentbox")
    }

    /// The same rebuild from the stack named `stack`, a host's.
    pub fn rebuild_cache_for(aws: &FakeAws, stack: &str) -> Result<(String, Vec<u8>), Failure> {
        agentspace_box::instance_id_from_stack(aws, stack).map(|id| {
            let bytes = agentspace_box::cache_bytes(&id);
            (id, bytes)
        })
    }

    /// The bytes of the cache for `id`: compact JSON, no byte order mark, no line ending.
    pub fn cache_bytes(id: &str) -> Vec<u8> {
        agentspace_box::cache_bytes(id)
    }

    // The crate reads and writes the cache file through a trait the program implements (T03); the
    // fake gives what a read of the whole file would, and records each write in the shared log.
    impl agentspace_box::InstanceIdCache for FakeCache {
        fn path(&self) -> &Path {
            &self.path
        }
        fn read(&self) -> agentspace_box::CacheRead {
            self.log.lock().unwrap().push(Event::CacheRead);
            let read: std::io::Result<Vec<u8>> = match &self.file {
                CacheFile::Absent => Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
                CacheFile::Bytes(bytes) => Ok(bytes.clone()),
                CacheFile::Denied => {
                    Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
                }
            };
            read.into()
        }
        fn write(&self, bytes: &[u8]) -> Result<(), Failure> {
            self.log
                .lock()
                .unwrap()
                .push(Event::CacheWrite(bytes.to_vec()));
            match &self.write_failure {
                Some(failure) => Err(failure.clone()),
                None => Ok(()),
            }
        }
    }

    /// `up`'s steps 1 and 2 (operator.md, up): the instance ID from the cache, or from the stack
    /// when the cache counts as missing, writing the cache then; then the start-and-wait step.
    /// The box's, whose stack is `agentbox`.
    pub fn up(sim: &Sim, cache: &FakeCache) -> Result<String, Failure> {
        up_for(sim, cache, "agentbox")
    }

    /// The same two steps for the host whose stack is `stack`.
    pub fn up_for(sim: &Sim, cache: &FakeCache, stack: &str) -> Result<String, Failure> {
        agentspace_box::start_and_wait(&sim.aws, &sim.say, &*sim.clock, cache, stack)
    }
}

// ---------------------------------------------------------------------------------------------
// The shared event log.
// ---------------------------------------------------------------------------------------------

/// One AWS CLI call as the crate made it.
#[derive(Debug, Clone, PartialEq)]
pub struct Rec {
    pub service: String,
    pub operation: String,
    pub args: Vec<String>,
    pub output: Output,
}

impl Rec {
    fn from_call(call: &Call) -> Rec {
        Rec {
            service: call.service.to_string(),
            operation: call.operation.to_string(),
            args: call.args.iter().map(|a| a.to_string()).collect(),
            output: call.output,
        }
    }

    /// `service operation args...`, joined with single spaces. Tolerant of how the crate splits
    /// `ec2 wait instance-stopped` between `operation` and `args`.
    pub fn text(&self) -> String {
        let mut parts = vec![self.service.clone(), self.operation.clone()];
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }

    /// The call's name: `service operation`, and for `ec2 wait` the waiter too.
    pub fn key(&self) -> String {
        let text = self.text();
        let words: Vec<&str> = text.split(' ').collect();
        let n = if words.len() >= 3 && words[0] == "ec2" && words[1] == "wait" {
            3
        } else {
            2
        };
        words[..n.min(words.len())].join(" ")
    }

    /// The operation's own arguments after the key, as the crate gave them, joined.
    pub fn rest(&self) -> String {
        let text = self.text();
        let key = self.key();
        text[key.len()..].trim_start().to_string()
    }

    /// The value after `flag` in the operation's arguments.
    pub fn arg_after(&self, flag: &str) -> Option<String> {
        let all: Vec<String> = self.text().split(' ').map(str::to_string).collect();
        all.iter()
            .position(|a| a == flag)
            .and_then(|i| all.get(i + 1).cloned())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Call(Rec),
    Progress {
        person: String,
        caller: Option<CallerLine>,
    },
    Warning(String),
    Detail(String),
    Sleep(Duration),
    /// The crate read the cache file.
    CacheRead,
    /// The crate replaced the cache file with these bytes.
    CacheWrite(Vec<u8>),
}

pub type Log = Arc<Mutex<Vec<Event>>>;

// ---------------------------------------------------------------------------------------------
// The fake clock.
// ---------------------------------------------------------------------------------------------

pub struct FakeClock {
    start: Duration,
    elapsed: Mutex<Duration>,
    log: Log,
}

impl FakeClock {
    fn new(log: Log) -> FakeClock {
        FakeClock {
            start: Duration::from_secs(EPOCH_START_SECS),
            elapsed: Mutex::new(Duration::ZERO),
            log,
        }
    }
    fn now_value(&self) -> Duration {
        self.start + *self.elapsed.lock().unwrap()
    }
    fn sleep_value(&self, duration: Duration) {
        self.log.lock().unwrap().push(Event::Sleep(duration));
        *self.elapsed.lock().unwrap() += duration;
    }
    /// Time that passes inside a call (a long `ec2 wait`), which is not a sleep.
    pub fn advance(&self, duration: Duration) {
        *self.elapsed.lock().unwrap() += duration;
    }
    pub fn elapsed(&self) -> Duration {
        *self.elapsed.lock().unwrap()
    }
}

// ---------------------------------------------------------------------------------------------
// The fake cache file: what a read of it gives, and every write recorded.
// ---------------------------------------------------------------------------------------------

/// What is at the cache's path.
#[derive(Debug, Clone)]
pub enum CacheFile {
    /// No file.
    Absent,
    /// A file with this content.
    Bytes(Vec<u8>),
    /// A file that cannot be opened.
    Denied,
}

pub struct FakeCache {
    path: std::path::PathBuf,
    file: CacheFile,
    write_failure: Option<Failure>,
    log: Log,
}

impl FakeCache {
    /// A cache file for `sim`, at a path under the temp folder that is only ever named in a
    /// message and never opened.
    pub fn new(sim: &Sim, file: CacheFile) -> FakeCache {
        FakeCache {
            path: std::env::temp_dir()
                .join("agentspace-box-test-home")
                .join(".agentspace")
                .join("instance-id.json"),
            file,
            write_failure: None,
            log: sim.log.clone(),
        }
    }

    /// The same, with every write failing with `failure`.
    pub fn failing_writes(mut self, failure: Failure) -> FakeCache {
        self.write_failure = Some(failure);
        self
    }

    pub fn full_path(&self) -> &Path {
        &self.path
    }
}

// ---------------------------------------------------------------------------------------------
// The fake Aws: scripted answers per call name, every call recorded.
// ---------------------------------------------------------------------------------------------

type Answer = Rc<dyn Fn(&Rec, &FakeClock) -> Result<Value, AwsError>>;

pub struct FakeAws {
    log: Log,
    clock: Arc<FakeClock>,
    script: std::cell::RefCell<HashMap<String, VecDeque<Answer>>>,
    total: std::cell::Cell<usize>,
}

impl Aws for FakeAws {
    fn call(&self, call: &Call) -> Result<Value, AwsError> {
        let rec = Rec::from_call(call);
        self.log.lock().unwrap().push(Event::Call(rec.clone()));
        self.total.set(self.total.get() + 1);
        assert!(
            self.total.get() < 50_000,
            "Runaway: more than 50000 AWS calls"
        );
        let answer = {
            let mut script = self.script.borrow_mut();
            let queue = script
                .get_mut(&rec.key())
                .unwrap_or_else(|| panic!("Unscripted call: {}", rec.text()));
            if queue.len() > 1 {
                queue.pop_front().unwrap()
            } else {
                queue[0].clone()
            }
        };
        answer(&rec, &self.clock)
    }
}

// ---------------------------------------------------------------------------------------------
// The fake Say: records what it was asked, and renders what the pin says it would print.
// ---------------------------------------------------------------------------------------------

pub struct FakeSay {
    mode: Mode,
    diagnostic: bool,
    log: Log,
}

impl Say for FakeSay {
    fn mode(&self) -> Mode {
        self.mode
    }
    fn progress(&self, person: &str, caller: Option<CallerLine>) {
        self.log.lock().unwrap().push(Event::Progress {
            person: person.to_string(),
            caller,
        });
    }
    fn warning(&self, person: &str) {
        self.log
            .lock()
            .unwrap()
            .push(Event::Warning(person.to_string()));
    }
    fn detail(&self, text: &str) {
        self.log
            .lock()
            .unwrap()
            .push(Event::Detail(text.to_string()));
    }
}

/// The caller's progress line text (operator.md, What a caller sees).
pub fn caller_line_text(line: CallerLine) -> &'static str {
    match line {
        CallerLine::Starting => "Starting the host...",
        CallerLine::Waiting => "Waiting for the host to be ready...",
        CallerLine::Ready => "The host is ready.",
    }
}

/// What a command prints for a failure in `mode` (operator.md, What a caller sees).
pub fn render_failure(mode: Mode, failure: &Failure) -> String {
    match mode {
        Mode::Person => failure.message.clone(),
        Mode::Caller => match failure.caller {
            CallerFailure::Same => failure.message.clone(),
            CallerFailure::NotAvailable => {
                "agentspace: The host is not available, and could not be started.".to_string()
            }
            CallerFailure::NotReady => {
                "agentspace: The host was not ready within 15 minutes; try again later."
                    .to_string()
            }
            CallerFailure::CouldNotReach => "agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.".to_string(),
        },
    }
}

// ---------------------------------------------------------------------------------------------
// The simulation: one fake of each, sharing a log.
// ---------------------------------------------------------------------------------------------

pub struct Sim {
    pub aws: FakeAws,
    pub say: FakeSay,
    pub clock: Arc<FakeClock>,
    pub log: Log,
    pub mode: Mode,
}

impl Sim {
    pub fn new(mode: Mode) -> Sim {
        Sim::with_diagnostic(mode, false)
    }

    pub fn with_diagnostic(mode: Mode, diagnostic: bool) -> Sim {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let clock = Arc::new(FakeClock::new(log.clone()));
        Sim {
            aws: FakeAws {
                log: log.clone(),
                clock: clock.clone(),
                script: Default::default(),
                total: Default::default(),
            },
            say: FakeSay {
                mode,
                diagnostic,
                log: log.clone(),
            },
            clock,
            log,
            mode,
        }
    }

    /// Answers for the call named `key` (`ec2 describe-instances`, ...), one per call; the last
    /// one repeats.
    pub fn script(&self, key: &str, answers: Vec<Result<Value, AwsError>>) -> &Sim {
        let queue = answers
            .into_iter()
            .map(|a| -> Answer { Rc::new(move |_, _| a.clone()) })
            .collect();
        self.aws.script.borrow_mut().insert(key.to_string(), queue);
        self
    }

    /// One answer computed from the call and the clock, for every call of that name.
    pub fn script_fn(
        &self,
        key: &str,
        f: impl Fn(&Rec, &FakeClock) -> Result<Value, AwsError> + 'static,
    ) -> &Sim {
        let mut queue = VecDeque::new();
        queue.push_back(Rc::new(f) as Answer);
        self.aws.script.borrow_mut().insert(key.to_string(), queue);
        self
    }

    pub fn run(&self) -> Result<(), Failure> {
        api::start_and_wait(&self.aws, &self.say, &self.clock, ID)
    }

    pub fn events(&self) -> Vec<Event> {
        self.log.lock().unwrap().clone()
    }

    pub fn calls(&self) -> Vec<Rec> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Call(r) => Some(r),
                _ => None,
            })
            .collect()
    }

    pub fn keys(&self) -> Vec<String> {
        self.calls().iter().map(Rec::key).collect()
    }

    pub fn count(&self, key: &str) -> usize {
        self.keys().iter().filter(|k| *k == key).count()
    }

    pub fn calls_of(&self, key: &str) -> Vec<Rec> {
        self.calls()
            .into_iter()
            .filter(|r| r.key() == key)
            .collect()
    }

    /// The person's lines the crate asked `Say` to print, in order.
    pub fn person_lines(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Progress { person, .. } => Some(person),
                _ => None,
            })
            .collect()
    }

    pub fn progress_pairs(&self) -> Vec<(String, Option<CallerLine>)> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Progress { person, caller } => Some((person, caller)),
                _ => None,
            })
            .collect()
    }

    pub fn warnings(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Warning(w) => Some(w),
                _ => None,
            })
            .collect()
    }

    pub fn sleeps(&self) -> Vec<Duration> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                Event::Sleep(d) => Some(d),
                _ => None,
            })
            .collect()
    }

    pub fn elapsed(&self) -> Duration {
        self.clock.elapsed()
    }

    /// The bytes of each write of the cache file, in order.
    pub fn cache_writes(&self) -> Vec<Vec<u8>> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                Event::CacheWrite(bytes) => Some(bytes),
                _ => None,
            })
            .collect()
    }

    /// What the mode's `Say` would have put on stderr: the person's lines, or in caller mode the
    /// caller's progress line at most once per run, nothing for `None`, and detail only with
    /// `AGENTSPACE_DIAGNOSTIC`. Warnings are rewritten in caller mode.
    pub fn rendered(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen: Vec<CallerLine> = Vec::new();
        for event in self.events() {
            match (event, self.mode) {
                (Event::Progress { person, .. }, Mode::Person) => out.push(person),
                (Event::Progress { caller, .. }, Mode::Caller) => {
                    if let Some(line) = caller
                        && !seen.contains(&line)
                    {
                        seen.push(line);
                        out.push(caller_line_text(line).to_string());
                    }
                }
                (Event::Warning(w), Mode::Person) => out.push(w),
                (Event::Warning(w), Mode::Caller) => out.push(
                    w.replace("the instance", "the host")
                        .replace("The instance", "The host"),
                ),
                (Event::Detail(d), Mode::Caller) if self.say.diagnostic => out.push(d),
                _ => {}
            }
        }
        out
    }

    /// The index in the whole event log of the first event that satisfies `f`.
    pub fn index_of(&self, f: impl Fn(&Event) -> bool) -> Option<usize> {
        self.events().iter().position(f)
    }

    pub fn index_of_call(&self, text: &str) -> Option<usize> {
        self.index_of(|e| matches!(e, Event::Call(r) if r.text() == text || r.key() == text))
    }

    pub fn index_of_line(&self, line: &str) -> Option<usize> {
        self.index_of(|e| matches!(e, Event::Progress { person, .. } if person == line))
    }
}

// ---------------------------------------------------------------------------------------------
// Answer builders: the JSON the AWS CLI prints, with only the members the step reads.
// ---------------------------------------------------------------------------------------------

/// `ec2 describe-instances` as step 1 reads it: `State.Name` only.
pub fn state(name: &str) -> Result<Value, AwsError> {
    Ok(json!({"Reservations":[{"Instances":[{"State":{"Name":name}}]}]}))
}

/// `ec2 describe-instances` as step 2 reads it: `LaunchTime` only.
pub fn launched(time: &str) -> Result<Value, AwsError> {
    Ok(json!({"Reservations":[{"Instances":[{"LaunchTime":time}]}]}))
}

pub fn instance_with(members: Value) -> Result<Value, AwsError> {
    Ok(json!({"Reservations":[{"Instances":[members]}]}))
}

pub enum Style {
    Plus00,
    Z,
    ZFrac,
    Minus7,
}

/// The launch time `ago` seconds before the fake clock's start, in the AWS CLI's ISO 8601 form;
/// negative `ago` is a time in the future.
pub fn iso(ago: i64, style: Style) -> String {
    let utc = 12 * 3600 - ago;
    let (secs, suffix) = match style {
        Style::Plus00 => (utc, "+00:00"),
        Style::Z => (utc, "Z"),
        Style::ZFrac => (utc, ".000Z"),
        Style::Minus7 => (utc - 7 * 3600, "-07:00"),
    };
    assert!((0..86_400).contains(&secs), "Test time out of the day");
    format!(
        "2026-11-19T{:02}:{:02}:{:02}{}",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60,
        suffix
    )
}

pub fn ping(status: &str) -> Result<Value, AwsError> {
    Ok(json!({"InstanceInformationList":[{"PingStatus":status}]}))
}

pub fn online() -> Result<Value, AwsError> {
    ping("Online")
}

pub fn no_information() -> Result<Value, AwsError> {
    Ok(json!({"InstanceInformationList":[]}))
}

pub fn sent(command_id: &str) -> Result<Value, AwsError> {
    Ok(json!({"Command":{"CommandId":command_id}}))
}

pub fn invocation(status: &str, stdout: &str) -> Result<Value, AwsError> {
    Ok(json!({"Status":status,"StandardOutputContent":stdout}))
}

pub fn failed(text: &str) -> Result<Value, AwsError> {
    Err(AwsError::Failed(text.to_string()))
}

/// Scripts the patch question: the n-th `ssm send-command` answers `cmd-n`, and the invocation of
/// `cmd-n` ends at once with `Success` and the n-th word (the last repeats).
pub fn patch_words(sim: &Sim, words: &[&str]) {
    let counter = Rc::new(std::cell::Cell::new(0usize));
    let c = counter.clone();
    sim.script_fn("ssm send-command", move |_, _| {
        let n = c.get();
        c.set(n + 1);
        sent(&format!("{CMD_PREFIX}{n}"))
    });
    let words: Vec<String> = words.iter().map(|w| w.to_string()).collect();
    sim.script_fn("ssm get-command-invocation", move |rec, _| {
        let cid = rec.arg_after("--command-id").expect("--command-id");
        let n: usize = cid.trim_start_matches(CMD_PREFIX).parse().expect("cmd-n");
        invocation("Success", &words[n.min(words.len() - 1)])
    });
}

/// A box that is online from the first look and answers `words` to the patch question.
pub fn online_with(sim: &Sim, words: &[&str]) {
    sim.script("ssm describe-instance-information", vec![online()]);
    patch_words(sim, words);
}

/// A running, long-launched box that needs no wait.
pub fn old_running(sim: &Sim) {
    sim.script(
        "ec2 describe-instances",
        vec![state("running"), launched(&iso(3 * 3600, Style::Plus00))],
    );
}

/// A stopped box that comes up patched at once.
pub fn stopped_then_patched(sim: &Sim) {
    sim.script("ec2 describe-instances", vec![state("stopped")]);
    sim.script("ec2 start-instances", vec![Ok(json!({}))]);
    online_with(sim, &["patched"]);
}

// ---------------------------------------------------------------------------------------------
// Backend text (operator.md, What a caller sees, "Backend text is ...").
// ---------------------------------------------------------------------------------------------

/// What in `line` is backend text: an instance, volume or host ID; a region name; a machine type;
/// a state name; the words EC2, AWS, SSM, Session Manager and CloudFormation; the stack's name;
/// the host's operating system or patch tool. An empty answer means the line is clean.
pub fn backend_text(line: &str) -> Vec<String> {
    let lower = line.to_ascii_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .filter(|t| !t.is_empty())
        .collect();
    let mut found = Vec::new();
    for word in [
        "ec2",
        "aws",
        "ssm",
        "cloudformation",
        "agentbox",
        "ubuntu",
        "dist-upgrade",
        "apt",
        "snap",
        "journalctl",
        "systemd",
        "pending",
        "running",
        "stopping",
        "stopped",
        "shutting-down",
        "terminated",
    ] {
        if tokens.contains(&word) {
            found.push(word.to_string());
        }
    }
    if lower.contains("session manager") {
        found.push("session manager".to_string());
    }
    for token in &tokens {
        let hex = |s: &str| s.len() >= 8 && s.chars().all(|c| c.is_ascii_hexdigit());
        if let Some(rest) = token.strip_prefix("i-")
            && hex(rest)
        {
            found.push(format!("instance id {token}"));
        }
        if let Some(rest) = token.strip_prefix("vol-")
            && hex(rest)
        {
            found.push(format!("volume id {token}"));
        }
        // A region name: two letters, a word, a digit (us-west-2).
        let parts: Vec<&str> = token.split('-').collect();
        if parts.len() == 3
            && parts[0].len() == 2
            && parts[1].chars().all(|c| c.is_ascii_lowercase())
            && !parts[1].is_empty()
            && parts[2].chars().all(|c| c.is_ascii_digit())
            && !parts[2].is_empty()
        {
            found.push(format!("region {token}"));
        }
        if token.contains("xlarge") || token.starts_with("m9g") || token.starts_with("t4g") {
            found.push(format!("machine type {token}"));
        }
    }
    found
}

pub fn assert_no_backend_text(line: &str) {
    let found = backend_text(line);
    assert!(
        found.is_empty(),
        "Backend text {found:?} in the caller's line: {line:?}"
    );
}

/// A line must start with a capital, `agentspace: `, or a digit, an identifier or a path
/// (AGENTS.md, Writing).
pub fn assert_starts_with_capital(line: &str) {
    let first = line.chars().next().expect("Empty line");
    assert!(
        first.is_ascii_uppercase() || line.starts_with("agentspace: "),
        "Line does not start with a capital: {line:?}"
    );
}

/// No two consecutive progress lines are the same (a message prints only when it differs from the
/// last one printed).
pub fn assert_no_consecutive_repeats(lines: &[String]) {
    for pair in lines.windows(2) {
        assert_ne!(
            pair[0], pair[1],
            "A message printed twice in a row: {lines:?}"
        );
    }
}

pub fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

pub fn both_modes(f: impl Fn(Mode)) {
    f(Mode::Person);
    f(Mode::Caller);
}

/// `a` happened, `b` happened, and `a` came first. (`None < Some` would let a missing event pass.)
pub fn assert_before(a: Option<usize>, b: Option<usize>) {
    let (a, b) = (
        a.expect("The earlier event never happened"),
        b.expect("The later event never happened"),
    );
    assert!(a < b, "Event {a} is not before event {b}");
}
