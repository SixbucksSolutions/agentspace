//! The pass's half of the launch handshake (docs/keeper.md, Starting a holder, The handshake and
//! Timings of a start), written against `Pass`, so its steps and bounds run the same with the real
//! WMI create and clocks (`WmiStarter`) and with a fake one in this module's tests.

use crate::files::{create_claim, read_whole};
use crate::time::{format_ticks, ms_to_ticks};
use crate::{LAUNCHER_ARGUMENT, launcher_command_line};
use agentspace_keeper_api::{Clock, HolderRecord, Matcher, Query, StartRequest, Started};
use agentspace_keeper_files::{
    Claim, LaunchRequest, LaunchResult, claim_file, launch_request_file, parse_launch_result,
    result_file, serialize_launch_request,
};
use agentspace_state::write_atomically;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The launcher's deadline: 30 seconds after it is taken, just before the request is written.
pub(crate) const DEADLINE: Duration = Duration::from_secs(30);
/// The pass's wait for the result when the create succeeded: until 35 seconds after the request.
pub(crate) const RESULT_WAIT: Duration = Duration::from_secs(35);
/// The wait after a launcher claimed first: 60 more seconds, from the first try to withdraw.
pub(crate) const CLAIMED_WAIT: Duration = Duration::from_secs(60);
/// How often the pass looks for the result file.
pub(crate) const POLL: Duration = Duration::from_millis(100);
/// A create call that throws or reaches its bound is tried up to 3 times, and never more.
pub(crate) const CREATE_TRIES: u32 = 3;
/// The gap between create tries.
pub(crate) const CREATE_GAP: Duration = Duration::from_secs(2);
/// The bound on one create call.
pub(crate) const CREATE_BOUND: Duration = Duration::from_secs(10);
/// The error of a create call that reached its bound.
pub(crate) const BOUND_ERROR: &str = "The call did not return within 10 seconds.";

/// The prefix of every failure of steps 1 to 8 (keeper.md, The handshake).
pub(crate) const PREFIX: &str = "The holder could not be started: ";
/// `board-holder-ssh.log`, in the state folder (docs/state.md, The keeper's logs).
pub(crate) const HOLDER_LOG_FILE: &str = "board-holder-ssh.log";

const NO_ANSWER: &str = "The launcher did not answer within 35 seconds.";
const WITHDRAWN: &str = "Its request was withdrawn, so no holder was started.";
const UNRECORDED: &str = "A launcher claimed the request, or it could not be withdrawn, and no \
                          answer came in 60 more seconds, so a holder may be running unrecorded.";
const EXITED: &str = "The holder exited at once; see board-holder-ssh.log.";
const UNREAD_LINE: &str = "The holder started, but its command line could not be read back to \
                           check it, so it runs unrecorded.";
const UNOPENED: &str =
    "The holder started, but could not be opened to check it, so it may be running unrecorded.";

/// What one create call of the launcher gave.
#[derive(Debug, Clone, PartialEq, Eq)]
// Off Windows the create always throws, so `Returned` is made only on Windows and in tests.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum Create {
    /// The call returned this code; 0 is a launcher created.
    Returned(u32),
    /// The call threw, or reached its bound: the error.
    Threw(String),
}

/// What the handshake needs from the system.
pub(crate) trait Pass {
    fn clock(&self) -> &dyn Clock;
    /// A new `<hex>` for this start's three files: 32 lowercase hex digits.
    fn hex(&self) -> String;
    /// The means that creates the launcher, as a failure names it: `Win32_Process.Create`.
    fn means(&self) -> &str;
    /// One create call of the launcher with this command line and working folder, no window, and
    /// the user's logon environment, bounded at `CREATE_BOUND`.
    fn create(&self, command_line: &str, directory: &Path) -> Create;
    /// Looks at a process once, with no wait (keeper.md, Trusting the record).
    fn query(&self, pid: u32) -> Query;
    /// Removes a launch file, if it is there, in one attempt: `true` when it is gone afterwards
    /// (removed, or not there), `false` when it could not be removed. A failure is not an error of
    /// the start, but a request that stays keeps its claim (step 8).
    fn remove(&self, path: &Path) -> bool;
}

/// The whole start: steps 1 to 8 of The handshake. `application` is the request's `application`:
/// the ssh program, or `<System32>\cmd.exe` for a batch.
pub(crate) fn start(
    pass: &dyn Pass,
    req: &StartRequest,
    matcher: &dyn Matcher,
    application: &Path,
) -> Started {
    // The starter owns the request's form (state.md, A holder's launch files; architect,
    // 2026-11-19, for E0.06-T02-S04): it MUST write every path in the request with backslashes,
    // turning each `/` of the `-SshExe` it was given into `\`, and MUST build the holder's command
    // line from that same path, so the request, the record and a later match agree. So every
    // step below, the holder command and step 7's match included, sees only the normalized paths.
    let state_dir = windows_separators(req.state_dir);
    let profile = windows_separators(req.profile);
    let ssh_exe = windows_separators(req.ssh_exe);
    let application = windows_separators(application);
    let req = &StartRequest {
        state_dir: &state_dir,
        profile: &profile,
        ssh_exe: &ssh_exe,
        ..*req
    };
    let application = application.as_path();
    let hex = pass.hex();
    let request = req.state_dir.join(launch_request_file(&hex));
    let claim = req.state_dir.join(claim_file(&hex));
    let result = req.state_dir.join(result_file(&hex));
    let outcome = handshake(pass, req, matcher, application, &request, &claim, &result);
    // Step 8: the request, then the claim, then the result, whatever happened. The request goes
    // before its claim, so a launcher that claims after the claim is gone finds no request.
    // When the request cannot be removed, the claim MUST stay (keeper.md, The handshake, step 8;
    // architect, 2026-11-19, for E0.06-T02-S04): the claim is what keeps a late launcher, from a
    // create call that was abandoned, from claiming that request and starting a holder no pass
    // records. So the pass removes the result alone, and leaves the request and its claim.
    if pass.remove(&request) {
        pass.remove(&claim);
    }
    pass.remove(&result);
    outcome
}

/// A Windows path with every `/` written as `\`, which Windows reads the same. On any other
/// system the path is left as it is.
fn windows_separators(path: &Path) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(path.to_string_lossy().replace('/', "\\"))
    } else {
        path.to_path_buf()
    }
}

fn failed(reason: &str) -> Started {
    Started::Failed(format!("{PREFIX}{reason}"))
}

/// Steps 1 to 7.
fn handshake(
    pass: &dyn Pass,
    req: &StartRequest,
    matcher: &dyn Matcher,
    application: &Path,
    request: &Path,
    claim: &Path,
    result: &Path,
) -> Started {
    let clock = pass.clock();
    // Step 1. The deadline is taken just before the request is written, and every later bound of
    // the start counts from it: `begun` is the same moment on the clock the waits use.
    let begun = clock.monotonic();
    let deadline_ms = clock
        .utc_now_ms()
        .saturating_add(DEADLINE.as_millis() as i64);
    let launch = LaunchRequest {
        application: application.to_string_lossy().into_owned(),
        command_line: matcher.holder_command(req.ssh_exe, req.alias),
        log: req
            .state_dir
            .join(HOLDER_LOG_FILE)
            .to_string_lossy()
            .into_owned(),
        directory: req.profile.to_string_lossy().into_owned(),
        not_after: format_ticks(ms_to_ticks(deadline_ms)),
        result: result.to_string_lossy().into_owned(),
    };
    // A request that cannot be written fails with `The launch request could not be written:
    // <error>`, after the usual prefix (keeper.md, The handshake, step 7, the rest of the rulings;
    // architect, 2026-11-19, for E0.06-T02-S04).
    let bytes = match serialize_launch_request(&launch) {
        Ok(b) => b,
        Err(e) => return failed(&format!("The launch request could not be written: {}", e.0)),
    };
    if let Err(e) = write_atomically(request, &bytes) {
        return failed(&format!("The launch request could not be written: {e}"));
    }

    // Step 2: up to 3 bounded tries, 2 seconds apart, for a call that throws or reaches its
    // bound; a non-zero code is not retried.
    let command_line = launcher_command_line(&windows_separators(req.own_exe), request);
    debug_assert!(command_line.contains(LAUNCHER_ARGUMENT));
    let means = pass.means();
    let mut why: Option<String> = None;
    for attempt in 0..CREATE_TRIES {
        if attempt > 0 {
            clock.sleep(CREATE_GAP);
        }
        match pass.create(&command_line, req.profile) {
            Create::Returned(0) => {
                why = None;
                break;
            }
            Create::Returned(code) => {
                why = Some(format!("{means} returned {code}."));
                break;
            }
            Create::Threw(error) => {
                why = Some(format!("{means} could not be called: {error}"));
            }
        }
    }

    // Step 5: wait for the result until 35 seconds after the request when the create succeeded;
    // when it failed, look once, at once.
    let answer = match &why {
        None => wait_for_result(clock, result, begun + RESULT_WAIT),
        Some(_) => look(result),
    };
    let answer = match answer {
        Some(a) => a,
        None => {
            let why = why.unwrap_or_else(|| NO_ANSWER.to_string());
            let first_try = clock.monotonic();
            if create_claim(claim, Claim::Abandoned).is_ok() {
                return failed(&format!("{why} {WITHDRAWN}"));
            }
            match wait_for_result(clock, result, first_try + CLAIMED_WAIT) {
                Some(a) => a,
                None => return failed(&format!("{why} {UNRECORDED}")),
            }
        }
    };

    // Step 6.
    let (pid, process_started_at) = match answer {
        LaunchResult::NotStarted { error } => return failed(&error),
        LaunchResult::Started {
            pid,
            process_started_at,
            ..
        } => (pid, process_started_at),
    };

    // Step 7: once, with no wait, by PID, creation time and command line. A process that is gone,
    // or a running one that is not the holder the result names (created at another time, or a
    // command line the matcher refuses), is "exited at once". A process that cannot be opened may
    // well be running, and a holder still running by PID and creation time whose command line
    // cannot be read back has not exited: each fails the start saying it may run unrecorded
    // (keeper.md, The handshake, step 7, and Starting a holder, item 7; architect, 2026-11-19,
    // for E0.06-T02-S04), and the pass leaves it alone, as this module never stops a process.
    let command_line = match pass.query(pid) {
        Query::Running(info) if info.process_started_at == process_started_at => info.command_line,
        Query::CannotOpen(_) => return failed(UNOPENED),
        Query::Running(_) | Query::Gone => return failed(EXITED),
    };
    match command_line {
        None => return failed(UNREAD_LINE),
        Some(line) if !matcher.is_holder(&line, req.ssh_exe, req.alias) => return failed(EXITED),
        Some(_) => {}
    }
    Started::Holder(HolderRecord {
        pid,
        process_started_at,
        lease_id: req.lease_id.to_string(),
        alias: req.alias.to_string(),
    })
}

/// One look at the result file. A file that is missing, cannot be read, or is neither of the
/// result's shapes is no answer.
fn look(result: &Path) -> Option<LaunchResult> {
    let bytes = read_whole(result).ok()?;
    parse_launch_result(&bytes).ok()
}

/// Looks every `POLL` until an answer comes or the clock reaches `until`, and looks once more
/// then.
fn wait_for_result(clock: &dyn Clock, result: &Path, until: Duration) -> Option<LaunchResult> {
    loop {
        if let Some(answer) = look(result) {
            return Some(answer);
        }
        let now = clock.monotonic();
        if now >= until {
            return None;
        }
        clock.sleep(POLL.min(until - now));
    }
}

/// Runs one create call on a thread of its own and waits for it up to `bound`. A call that has
/// not returned by then is abandoned and counts as a call that threw, with `BOUND_ERROR`; what it
/// returns later is never used (keeper.md, The handshake, step 2).
pub(crate) fn bounded<F>(call: F, bound: Duration) -> Create
where
    F: FnOnce() -> Create + Send + 'static,
{
    let (send, receive) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("agentspace-launch-create".to_string())
        .spawn(move || {
            let _ = send.send(call());
        });
    if let Err(e) = spawned {
        return Create::Threw(format!("A thread for the call could not be started: {e}"));
    }
    match receive.recv_timeout(bound) {
        Ok(answer) => answer,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Create::Threw(BOUND_ERROR.to_string()),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Create::Threw("The call ended without an answer.".to_string())
        }
    }
}

// Windows only: the request's schema takes only Windows full paths, and these tests write real
// requests in a throwaway folder.
#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::testing::Scratch;
    use agentspace_keeper_api::ProcessInfo;
    use agentspace_keeper_files::{
        HolderMatcher, parse_claim, parse_launch_request, serialize_launch_result,
    };
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;

    const UTC_MS: i64 = 1_795_061_136_456;
    const START: Duration = Duration::from_secs(1000);
    const PID: u32 = 4242;
    const STARTED_AT: &str = "2026-11-19T04:05:37.001Z";
    const ALIAS: &str = "agentspace-demo-0a1b2c3d";
    const HEX: &str = "0123456789abcdef0123456789abcdef";

    /// A clock whose time moves only when something sleeps.
    struct FakeClock {
        now: Cell<Duration>,
        sleeps: RefCell<Vec<Duration>>,
        on_sleep: RefCell<Box<dyn FnMut(Duration)>>,
    }

    impl Clock for FakeClock {
        fn utc_now_ms(&self) -> i64 {
            UTC_MS + (self.now.get() - START).as_millis() as i64
        }
        fn monotonic(&self) -> Duration {
            self.now.get()
        }
        fn sleep(&self, d: Duration) {
            self.sleeps.borrow_mut().push(d);
            self.now.set(self.now.get() + d);
            (self.on_sleep.borrow_mut())(self.now.get());
        }
    }

    /// What the fake create does when called: its answer, and what it leaves on the disk.
    type OnCreate = Box<dyn FnMut(&FakePass, &str, &Path) -> Create>;

    struct FakePass {
        clock: FakeClock,
        dir: PathBuf,
        on_create: RefCell<Option<OnCreate>>,
        creates: RefCell<Vec<(String, PathBuf)>>,
        query: RefCell<Query>,
        removed: RefCell<Vec<(String, Option<Vec<u8>>)>>,
        /// Names whose removal fails, as a file another process holds without sharing delete.
        unremovable: RefCell<Vec<String>>,
    }

    impl FakePass {
        fn new(scratch: &Scratch) -> FakePass {
            FakePass {
                clock: FakeClock {
                    now: Cell::new(START),
                    sleeps: RefCell::new(Vec::new()),
                    on_sleep: RefCell::new(Box::new(|_| {})),
                },
                dir: scratch.path().to_path_buf(),
                on_create: RefCell::new(None),
                creates: RefCell::new(Vec::new()),
                query: RefCell::new(Query::Running(ProcessInfo {
                    process_started_at: STARTED_AT.to_string(),
                    command_line: Some(matcher().holder_command(&ssh(), ALIAS)),
                })),
                removed: RefCell::new(Vec::new()),
                unremovable: RefCell::new(Vec::new()),
            }
        }

        fn on_create(&self, f: impl FnMut(&FakePass, &str, &Path) -> Create + 'static) {
            *self.on_create.borrow_mut() = Some(Box::new(f));
        }

        fn file(&self, name: String) -> PathBuf {
            self.dir.join(name)
        }

        fn elapsed(&self) -> Duration {
            self.clock.now.get() - START
        }

        fn removed_names(&self) -> Vec<String> {
            self.removed
                .borrow()
                .iter()
                .map(|(n, _)| n.clone())
                .collect()
        }

        fn removed_content(&self, name: &str) -> Option<Vec<u8>> {
            self.removed
                .borrow()
                .iter()
                .find(|(n, _)| n == name)
                .and_then(|(_, c)| c.clone())
        }
    }

    impl Pass for FakePass {
        fn clock(&self) -> &dyn Clock {
            &self.clock
        }
        fn hex(&self) -> String {
            HEX.to_string()
        }
        fn means(&self) -> &str {
            "Win32_Process.Create"
        }
        fn create(&self, command_line: &str, directory: &Path) -> Create {
            self.creates
                .borrow_mut()
                .push((command_line.to_string(), directory.to_path_buf()));
            let mut f = self.on_create.borrow_mut().take();
            let answer = match f.as_mut() {
                Some(f) => f(self, command_line, directory),
                None => Create::Returned(0),
            };
            *self.on_create.borrow_mut() = f;
            answer
        }
        fn query(&self, _: u32) -> Query {
            self.query.borrow().clone()
        }
        fn remove(&self, path: &Path) -> bool {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let content = std::fs::read(path).ok();
            let gone = !self.unremovable.borrow().contains(&name)
                && match std::fs::remove_file(path) {
                    Ok(()) => true,
                    Err(e) => e.kind() == std::io::ErrorKind::NotFound,
                };
            self.removed.borrow_mut().push((name, content));
            gone
        }
    }

    fn matcher() -> HolderMatcher {
        HolderMatcher::new(r"C:\Windows\System32")
    }

    fn ssh() -> PathBuf {
        PathBuf::from(r"C:\Tools\OpenSSH\ssh.exe")
    }

    fn own() -> PathBuf {
        PathBuf::from(r"C:\Program Files\agentspace\agentspace-keeper.exe")
    }

    fn profile() -> PathBuf {
        PathBuf::from(r"C:\Users\someone")
    }

    fn run(pass: &FakePass) -> Started {
        let ssh = ssh();
        let own = own();
        let profile = profile();
        let req = StartRequest {
            state_dir: &pass.dir,
            profile: &profile,
            ssh_exe: &ssh,
            lease_id: "0a1b2c3d-0000-4000-8000-000000000001",
            alias: ALIAS,
            own_exe: &own,
        };
        start(pass, &req, &matcher(), &ssh)
    }

    fn write_result(pass: &FakePass, result: &LaunchResult) {
        let bytes = serialize_launch_result(result).unwrap();
        std::fs::write(pass.file(result_file(HEX)), bytes).unwrap();
    }

    fn started() -> LaunchResult {
        LaunchResult::Started {
            pid: PID,
            process_started_at: STARTED_AT.to_string(),
            command_line: Some(matcher().holder_command(&ssh(), ALIAS)),
        }
    }

    /// A launcher that claims the request and answers at once.
    fn answering(pass: &FakePass, result: LaunchResult) {
        pass.on_create(move |p, _, _| {
            create_claim(&p.file(claim_file(HEX)), Claim::Launcher(77)).unwrap();
            write_result(p, &result);
            Create::Returned(0)
        });
    }

    const ORDER: [&str; 3] = [
        "board-holder-launch.0123456789abcdef0123456789abcdef.json",
        "board-holder-launch.0123456789abcdef0123456789abcdef.claim",
        "board-holder-launch.0123456789abcdef0123456789abcdef.result.json",
    ];

    #[test]
    fn a_launcher_that_answers_gives_the_holder_and_all_three_files_go_in_order() {
        let scratch = Scratch::new("answer");
        let pass = FakePass::new(&scratch);
        answering(&pass, started());
        let got = run(&pass);
        assert_eq!(
            got,
            Started::Holder(HolderRecord {
                pid: PID,
                process_started_at: STARTED_AT.to_string(),
                lease_id: "0a1b2c3d-0000-4000-8000-000000000001".to_string(),
                alias: ALIAS.to_string(),
            })
        );
        assert_eq!(pass.removed_names(), ORDER);
        assert_eq!(
            pass.removed_content(ORDER[1]).as_deref(),
            Some(&b"launcher 77\n"[..])
        );
        assert_eq!(pass.creates.borrow().len(), 1);
        assert!(scratch.names().is_empty(), "{:?}", scratch.names());
    }

    #[test]
    fn the_request_names_the_holder_its_log_its_folder_the_result_and_a_deadline_30_seconds_on() {
        let scratch = Scratch::new("request");
        let pass = FakePass::new(&scratch);
        let seen: std::rc::Rc<RefCell<Option<LaunchRequest>>> = Default::default();
        let keep = seen.clone();
        pass.on_create(move |p, _, _| {
            let bytes = std::fs::read(p.file(launch_request_file(HEX))).unwrap();
            *keep.borrow_mut() = Some(parse_launch_request(&bytes).unwrap());
            Create::Returned(0)
        });
        let _ = run(&pass);
        let request = seen.borrow().clone().unwrap();
        assert_eq!(request.application, ssh().to_string_lossy());
        assert_eq!(
            request.command_line,
            format!(r#""C:\Tools\OpenSSH\ssh.exe" -N -o ExitOnForwardFailure=yes {ALIAS}"#)
        );
        assert_eq!(
            request.log,
            scratch
                .path()
                .join("board-holder-ssh.log")
                .to_string_lossy()
        );
        assert_eq!(request.directory, profile().to_string_lossy());
        assert_eq!(
            request.result,
            pass.file(result_file(HEX)).to_string_lossy()
        );
        assert_eq!(
            request.not_after,
            format_ticks(ms_to_ticks(UTC_MS + 30_000))
        );
        let (line, directory) = pass.creates.borrow()[0].clone();
        assert_eq!(
            line,
            format!(
                "\"{}\" {LAUNCHER_ARGUMENT} \"{}\"",
                own().display(),
                pass.file(launch_request_file(HEX)).display()
            )
        );
        assert_eq!(directory, profile());
    }

    #[test]
    fn an_unanswered_request_is_withdrawn_with_abandoned_at_35_seconds() {
        let scratch = Scratch::new("unanswered");
        let pass = FakePass::new(&scratch);
        let got = run(&pass);
        assert_eq!(
            got,
            Started::Failed(
                "The holder could not be started: The launcher did not answer within 35 seconds. \
                 Its request was withdrawn, so no holder was started."
                    .to_string()
            )
        );
        assert_eq!(pass.elapsed(), RESULT_WAIT);
        assert!(pass.clock.sleeps.borrow().iter().all(|d| *d <= POLL));
        assert_eq!(pass.removed_names(), ORDER);
        let claim = pass.removed_content(ORDER[1]).unwrap();
        assert_eq!(parse_claim(&claim), Some(Claim::Abandoned));
        assert_eq!(pass.removed_content(ORDER[2]), None);
        assert!(scratch.names().is_empty());
    }

    #[test]
    fn a_launcher_that_answers_during_the_wait_is_seen_within_100_ms() {
        let scratch = Scratch::new("late");
        let pass = FakePass::new(&scratch);
        let dir = pass.dir.clone();
        *pass.clock.on_sleep.borrow_mut() = Box::new(move |now| {
            if now == Duration::from_secs(1012) {
                let bytes = serialize_launch_result(&started()).unwrap();
                std::fs::write(dir.join(result_file(HEX)), bytes).unwrap();
            }
        });
        assert!(matches!(run(&pass), Started::Holder(_)));
        assert_eq!(pass.elapsed(), Duration::from_secs(12));
    }

    #[test]
    fn a_create_that_throws_is_tried_3_times_2_seconds_apart_and_withdrawn_at_once() {
        let scratch = Scratch::new("throws");
        let pass = FakePass::new(&scratch);
        let count = std::rc::Rc::new(Cell::new(0));
        let c = count.clone();
        pass.on_create(move |_, _, _| {
            c.set(c.get() + 1);
            Create::Threw(format!("Error {}.", c.get()))
        });
        let got = run(&pass);
        assert_eq!(count.get(), 3);
        assert_eq!(*pass.clock.sleeps.borrow(), [CREATE_GAP, CREATE_GAP]);
        assert_eq!(pass.elapsed(), Duration::from_secs(4));
        assert_eq!(
            got,
            Started::Failed(
                "The holder could not be started: Win32_Process.Create could not be called: \
                 Error 3. Its request was withdrawn, so no holder was started."
                    .to_string()
            )
        );
        assert_eq!(
            pass.removed_content(ORDER[1]).as_deref(),
            Some(&b"abandoned\n"[..])
        );
    }

    #[test]
    fn a_create_that_throws_then_succeeds_is_not_tried_again() {
        let scratch = Scratch::new("thensucceeds");
        let pass = FakePass::new(&scratch);
        let count = std::rc::Rc::new(Cell::new(0));
        let c = count.clone();
        pass.on_create(move |p, _, _| {
            c.set(c.get() + 1);
            if c.get() == 1 {
                return Create::Threw(BOUND_ERROR.to_string());
            }
            create_claim(&p.file(claim_file(HEX)), Claim::Launcher(5)).unwrap();
            write_result(p, &started());
            Create::Returned(0)
        });
        assert!(matches!(run(&pass), Started::Holder(_)));
        assert_eq!(count.get(), 2);
    }

    #[test]
    fn a_non_zero_code_is_not_retried_and_the_request_is_withdrawn() {
        let scratch = Scratch::new("code");
        let pass = FakePass::new(&scratch);
        pass.on_create(|_, _, _| Create::Returned(9));
        let got = run(&pass);
        assert_eq!(pass.creates.borrow().len(), 1);
        assert_eq!(pass.elapsed(), Duration::ZERO);
        assert_eq!(
            got,
            Started::Failed(
                "The holder could not be started: Win32_Process.Create returned 9. Its request \
                 was withdrawn, so no holder was started."
                    .to_string()
            )
        );
    }

    #[test]
    fn a_failed_create_whose_launcher_answered_anyway_is_seen_by_the_one_look() {
        let scratch = Scratch::new("threwbutmade");
        let pass = FakePass::new(&scratch);
        let count = std::rc::Rc::new(Cell::new(0));
        let c = count.clone();
        pass.on_create(move |p, _, _| {
            c.set(c.get() + 1);
            if c.get() == 3 {
                create_claim(&p.file(claim_file(HEX)), Claim::Launcher(5)).unwrap();
                write_result(p, &started());
            }
            Create::Threw(BOUND_ERROR.to_string())
        });
        assert!(matches!(run(&pass), Started::Holder(_)));
    }

    #[test]
    fn a_launcher_that_claimed_first_is_waited_for_60_more_seconds_then_reported() {
        let scratch = Scratch::new("claimed");
        let pass = FakePass::new(&scratch);
        pass.on_create(|p, _, _| {
            create_claim(&p.file(claim_file(HEX)), Claim::Launcher(5)).unwrap();
            Create::Returned(0)
        });
        let got = run(&pass);
        assert_eq!(pass.elapsed(), RESULT_WAIT + CLAIMED_WAIT);
        assert_eq!(
            got,
            Started::Failed(
                "The holder could not be started: The launcher did not answer within 35 seconds. \
                 A launcher claimed the request, or it could not be withdrawn, and no answer came \
                 in 60 more seconds, so a holder may be running unrecorded."
                    .to_string()
            )
        );
        assert_eq!(
            pass.removed_content(ORDER[1]).as_deref(),
            Some(&b"launcher 5\n"[..])
        );
    }

    #[test]
    fn a_launcher_that_claimed_first_and_answers_in_the_60_seconds_gives_its_holder() {
        let scratch = Scratch::new("claimedanswers");
        let pass = FakePass::new(&scratch);
        pass.on_create(|p, _, _| {
            create_claim(&p.file(claim_file(HEX)), Claim::Launcher(5)).unwrap();
            Create::Returned(0)
        });
        let dir = pass.dir.clone();
        *pass.clock.on_sleep.borrow_mut() = Box::new(move |now| {
            if now == Duration::from_secs(1080) {
                let bytes = serialize_launch_result(&started()).unwrap();
                std::fs::write(dir.join(result_file(HEX)), bytes).unwrap();
            }
        });
        assert!(matches!(run(&pass), Started::Holder(_)));
        assert_eq!(pass.elapsed(), Duration::from_secs(80));
    }

    #[test]
    fn an_error_in_the_result_fails_the_start_with_that_error() {
        let scratch = Scratch::new("error");
        let pass = FakePass::new(&scratch);
        answering(
            &pass,
            LaunchResult::NotStarted {
                error: "NUL could not be opened.".to_string(),
            },
        );
        assert_eq!(
            run(&pass),
            Started::Failed(
                "The holder could not be started: NUL could not be opened.".to_string()
            )
        );
        assert_eq!(pass.removed_names(), ORDER);
    }

    #[test]
    fn a_result_that_is_neither_shape_is_no_answer() {
        let scratch = Scratch::new("garbled");
        let pass = FakePass::new(&scratch);
        pass.on_create(|p, _, _| {
            create_claim(&p.file(claim_file(HEX)), Claim::Launcher(5)).unwrap();
            std::fs::write(p.file(result_file(HEX)), b"{\"pid\":0}\n").unwrap();
            Create::Returned(0)
        });
        let got = run(&pass);
        assert!(
            matches!(&got, Started::Failed(r) if r.ends_with("may be running unrecorded.")),
            "{got:?}"
        );
    }

    #[test]
    fn a_holder_that_is_gone_or_not_the_one_named_fails_the_start() {
        let exited = Started::Failed(
            "The holder could not be started: The holder exited at once; see board-holder-ssh.log."
                .to_string(),
        );
        let wrong_time = Query::Running(ProcessInfo {
            process_started_at: "2026-11-19T04:05:37.002Z".to_string(),
            command_line: Some(matcher().holder_command(&ssh(), ALIAS)),
        });
        let wrong_line = Query::Running(ProcessInfo {
            process_started_at: STARTED_AT.to_string(),
            command_line: Some(matcher().holder_command(&ssh(), "agentspace-other-1")),
        });
        // Created at another time, its command line unread: still not the holder named.
        let wrong_time_unread = Query::Running(ProcessInfo {
            process_started_at: "2026-11-19T04:05:37.002Z".to_string(),
            command_line: None,
        });
        for query in [Query::Gone, wrong_time, wrong_line, wrong_time_unread] {
            let scratch = Scratch::new("exited");
            let pass = FakePass::new(&scratch);
            *pass.query.borrow_mut() = query.clone();
            answering(&pass, started());
            assert_eq!(run(&pass), exited, "{query:?}");
            assert_eq!(pass.removed_names(), ORDER);
        }
    }

    /// Step 7 (keeper.md; architect, 2026-11-19, for E0.06-T02-S04): a holder still running by
    /// PID and creation time whose command line cannot be read back has not exited. The start
    /// fails saying it runs unrecorded, and nothing stops it: the handshake only looks at it.
    #[test]
    fn a_running_holder_whose_command_line_cannot_be_read_back_runs_unrecorded() {
        let scratch = Scratch::new("unreadline");
        let pass = FakePass::new(&scratch);
        *pass.query.borrow_mut() = Query::Running(ProcessInfo {
            process_started_at: STARTED_AT.to_string(),
            command_line: None,
        });
        answering(&pass, started());
        assert_eq!(
            run(&pass),
            Started::Failed(
                "The holder could not be started: The holder started, but its command line could \
                 not be read back to check it, so it runs unrecorded."
                    .to_string()
            )
        );
        assert_eq!(pass.removed_names(), ORDER);
        assert!(scratch.names().is_empty());
    }

    /// Step 7 (keeper.md; architect, 2026-11-19, for E0.06-T02-S04): a holder that cannot be
    /// opened may well be running, so it is not "exited at once". The start fails saying it may
    /// be running unrecorded, and nothing stops it.
    #[test]
    fn a_holder_that_cannot_be_opened_may_be_running_unrecorded() {
        let scratch = Scratch::new("unopened");
        let pass = FakePass::new(&scratch);
        *pass.query.borrow_mut() = Query::CannotOpen(5);
        answering(&pass, started());
        assert_eq!(
            run(&pass),
            Started::Failed(
                "The holder could not be started: The holder started, but could not be opened to \
                 check it, so it may be running unrecorded."
                    .to_string()
            )
        );
        assert_eq!(pass.removed_names(), ORDER);
        assert!(scratch.names().is_empty());
    }

    /// Step 8 (keeper.md; architect, 2026-11-19, for E0.06-T02-S04): when the request cannot be
    /// removed, its claim stays, so no late launcher can claim it and start a holder no pass
    /// records. The result alone is removed, and the start's outcome is unchanged.
    #[test]
    fn a_request_that_cannot_be_removed_keeps_its_claim_and_only_the_result_goes() {
        let scratch = Scratch::new("kept");
        let pass = FakePass::new(&scratch);
        pass.unremovable.borrow_mut().push(ORDER[0].to_string());
        answering(&pass, started());
        assert!(matches!(run(&pass), Started::Holder(_)));
        assert_eq!(pass.removed_names(), [ORDER[0], ORDER[2]]);
        assert_eq!(scratch.names(), [ORDER[1], ORDER[0]]);
        assert_eq!(
            std::fs::read(pass.file(claim_file(HEX))).unwrap(),
            b"launcher 77\n"
        );
        // A launcher that comes now cannot claim the request, so it starts nothing.
        assert!(create_claim(&pass.file(claim_file(HEX)), Claim::Launcher(78)).is_err());
    }

    /// The same for a withdrawn request: its `abandoned` claim stays with it.
    #[test]
    fn a_withdrawn_request_that_cannot_be_removed_keeps_its_abandoned_claim() {
        let scratch = Scratch::new("keptwithdrawn");
        let pass = FakePass::new(&scratch);
        pass.unremovable.borrow_mut().push(ORDER[0].to_string());
        pass.on_create(|_, _, _| Create::Returned(9));
        assert!(matches!(run(&pass), Started::Failed(_)));
        assert_eq!(pass.removed_names(), [ORDER[0], ORDER[2]]);
        assert_eq!(
            std::fs::read(pass.file(claim_file(HEX))).unwrap(),
            b"abandoned\n"
        );
    }

    #[test]
    fn a_request_that_cannot_be_written_fails_before_any_create() {
        let scratch = Scratch::new("unwritable");
        let mut pass = FakePass::new(&scratch);
        pass.dir = scratch.path().join("missing");
        let got = run(&pass);
        assert!(
            matches!(&got, Started::Failed(r) if r.starts_with("The holder could not be started: The launch request could not be written: ")),
            "{got:?}"
        );
        assert!(pass.creates.borrow().is_empty());
    }

    #[test]
    fn a_call_past_its_bound_is_abandoned_as_a_call_that_threw() {
        let began = std::time::Instant::now();
        let got = bounded(
            || {
                std::thread::sleep(Duration::from_secs(3));
                Create::Returned(0)
            },
            Duration::from_millis(100),
        );
        assert_eq!(got, Create::Threw(BOUND_ERROR.to_string()));
        assert!(began.elapsed() < Duration::from_secs(2));
        assert_eq!(
            bounded(|| Create::Returned(7), Duration::from_secs(5)),
            Create::Returned(7)
        );
    }

    /// A matcher that knows a holder only byte for byte, so a start passes step 7 only when the
    /// command line it asked for and the path it matches with are the same text.
    struct Verbatim;

    impl Matcher for Verbatim {
        fn holder_command(&self, ssh_exe: &Path, alias: &str) -> String {
            matcher().holder_command(ssh_exe, alias)
        }
        fn is_holder(&self, command_line: &str, ssh_exe: &Path, alias: &str) -> bool {
            command_line == self.holder_command(ssh_exe, alias)
        }
        fn self_check(&self) -> Result<(), String> {
            Ok(())
        }
    }

    /// The ruling of state.md, A holder's launch files (architect, 2026-11-19, for
    /// E0.06-T02-S04): every path in the request has backslashes, and the holder's command line is
    /// built from the same normalized `-SshExe`, which step 7 matches with too.
    #[test]
    fn paths_given_with_forward_slashes_are_written_with_backslashes() {
        let scratch = Scratch::new("slashes");
        let mut pass = FakePass::new(&scratch);
        pass.dir = PathBuf::from(scratch.path().to_string_lossy().replace('\\', "/"));
        let seen: std::rc::Rc<RefCell<Option<LaunchRequest>>> = Default::default();
        let keep = seen.clone();
        // A launcher as the real one answers: the command line read back from the process it made
        // is the request's.
        pass.on_create(move |p, _, _| {
            let bytes = std::fs::read(p.file(launch_request_file(HEX))).unwrap();
            let request = parse_launch_request(&bytes).unwrap();
            create_claim(&p.file(claim_file(HEX)), Claim::Launcher(5)).unwrap();
            let answer = LaunchResult::Started {
                pid: PID,
                process_started_at: STARTED_AT.to_string(),
                command_line: Some(request.command_line.clone()),
            };
            *p.query.borrow_mut() = Query::Running(ProcessInfo {
                process_started_at: STARTED_AT.to_string(),
                command_line: Some(request.command_line.clone()),
            });
            *keep.borrow_mut() = Some(request);
            write_result(p, &answer);
            Create::Returned(0)
        });
        let ssh = PathBuf::from("C:/Tools/OpenSSH/ssh.exe");
        let own = PathBuf::from("C:/Program Files/agentspace/agentspace-keeper.exe");
        let profile = PathBuf::from("C:/Users/someone");
        let req = StartRequest {
            state_dir: &pass.dir,
            profile: &profile,
            ssh_exe: &ssh,
            lease_id: "0a1b2c3d-0000-4000-8000-000000000001",
            alias: ALIAS,
            own_exe: &own,
        };
        assert!(matches!(
            start(&pass, &req, &Verbatim, &ssh),
            Started::Holder(_)
        ));
        let request = seen.borrow().clone().unwrap();
        assert_eq!(request.application, r"C:\Tools\OpenSSH\ssh.exe");
        assert_eq!(
            request.command_line,
            format!(r#""C:\Tools\OpenSSH\ssh.exe" -N -o ExitOnForwardFailure=yes {ALIAS}"#)
        );
        assert_eq!(request.directory, r"C:\Users\someone");
        for path in [&request.log, &request.result] {
            assert!(!path.contains('/'), "{path}");
        }
        let (line, directory) = pass.creates.borrow()[0].clone();
        assert!(!line.contains('/'), "{line}");
        assert_eq!(directory, PathBuf::from(r"C:\Users\someone"));
        assert!(scratch.names().is_empty());
    }

    /// The same for a batch `-SshExe`, whose path lies inside cmd's command line.
    #[test]
    fn a_batch_given_with_forward_slashes_runs_under_cmd_with_backslashes() {
        let scratch = Scratch::new("slashesbatch");
        let pass = FakePass::new(&scratch);
        let seen: std::rc::Rc<RefCell<Option<LaunchRequest>>> = Default::default();
        let keep = seen.clone();
        pass.on_create(move |p, _, _| {
            let bytes = std::fs::read(p.file(launch_request_file(HEX))).unwrap();
            *keep.borrow_mut() = Some(parse_launch_request(&bytes).unwrap());
            Create::Returned(9)
        });
        let ssh = PathBuf::from("C:/Tools/stub holder/ssh-stub.cmd");
        let own = own();
        let profile = profile();
        let req = StartRequest {
            state_dir: &pass.dir,
            profile: &profile,
            ssh_exe: &ssh,
            lease_id: "0a1b2c3d-0000-4000-8000-000000000001",
            alias: ALIAS,
            own_exe: &own,
        };
        let _ = start(&pass, &req, &matcher(), &matcher().program(&ssh));
        let request = seen.borrow().clone().unwrap();
        assert_eq!(request.application, r"C:\Windows\System32\cmd.exe");
        assert_eq!(
            request.command_line,
            format!(
                r#""C:\Windows\System32\cmd.exe" /d /c ""C:\Tools\stub holder\ssh-stub.cmd" -N -o ExitOnForwardFailure=yes {ALIAS}""#
            )
        );
    }

    #[test]
    fn the_bounds_are_the_specs() {
        assert_eq!(DEADLINE, Duration::from_secs(30));
        assert_eq!(RESULT_WAIT, Duration::from_secs(35));
        assert_eq!(CLAIMED_WAIT, Duration::from_secs(60));
        assert_eq!(POLL, Duration::from_millis(100));
        assert_eq!(CREATE_TRIES, 3);
        assert_eq!(CREATE_GAP, Duration::from_secs(2));
        assert_eq!(CREATE_BOUND, Duration::from_secs(10));
    }
}
