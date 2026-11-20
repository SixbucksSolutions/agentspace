//! The launcher's half of the launch handshake (docs/keeper.md, The handshake, steps 3 and 4),
//! written against `Spawn`, so it runs the same with the real CreateProcess and with a fake in
//! this module's tests.

use crate::files::{create_claim, read_whole};
use crate::time::parse_ticks;
use agentspace_keeper_files::{
    Claim, LaunchRequest, LaunchResult, parse_launch_request, serialize_launch_result,
};
use agentspace_state::write_atomically;
use std::path::{Path, PathBuf};

/// The launcher answered, or rightly started nothing: it lost the claim, or found the request
/// gone or unreadable after it claimed it.
// The launcher exits 0 when it answered or rightly started nothing (claim lost, request gone or
// unreadable), 1 when it could not write the result, and 2 when its argument names no
// `board-holder-launch.<hex>.json` (keeper.md, The handshake, step 7, the rest of the rulings;
// architect, 2026-11-19, for E0.06-T02-S04).
pub(crate) const EXIT_DONE: i32 = 0;
/// It could not write its result, so its pass sees no answer.
pub(crate) const EXIT_NO_ANSWER: i32 = 1;
/// Its argument names no launch request (`board-holder-launch.<hex>.json`): it wrote nothing.
pub(crate) const EXIT_NOT_A_REQUEST: i32 = 2;

pub(crate) const DEADLINE_PASSED: &str =
    "The launcher reached its deadline before it started the holder, so it started nothing.";

/// The holder a launcher made, as read back from the process before it ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Made {
    pub pid: u32,
    /// `YYYY-MM-DDTHH:MM:SS.fffZ`.
    pub process_started_at: String,
    pub command_line: Option<String>,
}

/// What the launcher needs from the system.
pub(crate) trait Spawn {
    /// Starts the holder the request names (keeper.md, Starting a holder, items 1 to 5), or gives
    /// the error the result carries.
    fn spawn(&self, request: &LaunchRequest) -> Result<Made, String>;
    /// The UTC clock, in 100-nanosecond ticks since the Unix epoch.
    fn utc_now_ticks(&self) -> i64;
    /// This process's own PID, for the claim.
    fn own_pid(&self) -> u32;
}

/// `board-holder-launch.<hex>.claim` beside `board-holder-launch.<hex>.json`, or `None` when the
/// path does not name a request. `<hex>` is any run of ASCII letters and digits; state.md makes it
/// 32 lowercase hex digits.
pub(crate) fn claim_for(request: &Path) -> Option<PathBuf> {
    let name = request.file_name()?.to_str()?;
    let hex = name
        .strip_prefix("board-holder-launch.")?
        .strip_suffix(".json")?;
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    Some(request.with_file_name(agentspace_keeper_files::claim_file(hex)))
}

/// Steps 3 and 4: claim the request before anything else, read it, start the holder unless the
/// deadline has passed, and answer.
pub(crate) fn launch(request: &Path, system: &dyn Spawn) -> i32 {
    let Some(claim) = claim_for(request) else {
        return EXIT_NOT_A_REQUEST;
    };
    // Step 3: a launcher that cannot claim it starts nothing, writes nothing, and exits.
    if create_claim(&claim, Claim::Launcher(system.own_pid())).is_err() {
        return EXIT_DONE;
    }
    // One it claimed but cannot read (gone, or not the request's form) removes its claim.
    let parsed = read_whole(request)
        .ok()
        .and_then(|bytes| parse_launch_request(&bytes).ok());
    let Some(parsed) = parsed else {
        let _ = std::fs::remove_file(&claim);
        return EXIT_DONE;
    };
    // Step 4. A deadline that cannot be read is taken as passed: the launcher starts nothing.
    let in_time = parse_ticks(&parsed.not_after).is_some_and(|d| system.utc_now_ticks() <= d);
    let answer = if !in_time {
        LaunchResult::NotStarted {
            error: DEADLINE_PASSED.to_string(),
        }
    } else {
        match system.spawn(&parsed) {
            Ok(made) => LaunchResult::Started {
                pid: made.pid,
                process_started_at: made.process_started_at,
                command_line: made.command_line,
            },
            Err(error) => LaunchResult::NotStarted { error },
        }
    };
    // The launcher resumes the holder, then writes the result (keeper.md, The handshake, step 8;
    // architect, 2026-11-19, for E0.06-T02-S04): `spawn` has let the holder run before the result
    // is written, and a holder whose result cannot be written runs on, which step 5's timeout
    // reason already names. The other order would leave a suspended holder that passes the trust
    // checks yet holds nothing, if the launcher were killed between the write and the resume.
    let Ok(bytes) = serialize_launch_result(&answer) else {
        return EXIT_NO_ANSWER;
    };
    if write_atomically(Path::new(&parsed.result), &bytes).is_err() {
        return EXIT_NO_ANSWER;
    }
    EXIT_DONE
}

// Windows only: the request's schema takes only Windows full paths, and these tests write real
// requests in a throwaway folder.
#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::testing::Scratch;
    use crate::time::{format_ticks, utc_now_ticks};
    use agentspace_keeper_files::{
        claim_file, launch_request_file, parse_launch_result, result_file, serialize_launch_request,
    };
    use std::cell::RefCell;

    const HEX: &str = "fedcba9876543210fedcba9876543210";

    struct FakeSpawn {
        now: i64,
        answer: Result<Made, String>,
        calls: RefCell<Vec<LaunchRequest>>,
    }

    impl Spawn for FakeSpawn {
        fn spawn(&self, request: &LaunchRequest) -> Result<Made, String> {
            self.calls.borrow_mut().push(request.clone());
            self.answer.clone()
        }
        fn utc_now_ticks(&self) -> i64 {
            self.now
        }
        fn own_pid(&self) -> u32 {
            31337
        }
    }

    fn made() -> Made {
        Made {
            pid: 999,
            process_started_at: "2026-11-19T04:05:07.456Z".to_string(),
            command_line: Some("\"C:\\x\\ssh.exe\" -N".to_string()),
        }
    }

    fn spawner(answer: Result<Made, String>) -> FakeSpawn {
        FakeSpawn {
            now: utc_now_ticks(),
            answer,
            calls: RefCell::new(Vec::new()),
        }
    }

    /// Writes a request in `dir` whose deadline is `not_after`, and gives its path.
    fn request(dir: &Path, not_after: i64, result: &Path) -> PathBuf {
        let path = dir.join(launch_request_file(HEX));
        let r = LaunchRequest {
            application: r"C:\x\ssh.exe".to_string(),
            command_line: "\"C:\\x\\ssh.exe\" -N".to_string(),
            log: r"C:\x\board-holder-ssh.log".to_string(),
            directory: r"C:\x".to_string(),
            not_after: format_ticks(not_after),
            result: result.to_string_lossy().into_owned(),
        };
        std::fs::write(&path, serialize_launch_request(&r).unwrap()).unwrap();
        path
    }

    #[test]
    fn a_claimed_request_in_time_starts_the_holder_and_answers_with_it() {
        let dir = Scratch::new("answers");
        let fake = spawner(Ok(made()));
        let result = dir.path().join(result_file(HEX));
        let path = request(dir.path(), fake.now + 300_000_000, &result);
        assert_eq!(launch(&path, &fake), EXIT_DONE);
        assert_eq!(fake.calls.borrow().len(), 1);
        assert_eq!(
            std::fs::read(dir.path().join(claim_file(HEX))).unwrap(),
            b"launcher 31337\n"
        );
        let answer = parse_launch_result(&std::fs::read(&result).unwrap()).unwrap();
        assert_eq!(
            answer,
            LaunchResult::Started {
                pid: 999,
                process_started_at: "2026-11-19T04:05:07.456Z".to_string(),
                command_line: Some("\"C:\\x\\ssh.exe\" -N".to_string()),
            }
        );
    }

    #[test]
    fn a_launcher_that_cannot_claim_starts_nothing_and_writes_nothing() {
        let dir = Scratch::new("lost");
        let fake = spawner(Ok(made()));
        let result = dir.path().join(result_file(HEX));
        let path = request(dir.path(), fake.now + 300_000_000, &result);
        std::fs::write(dir.path().join(claim_file(HEX)), b"abandoned\n").unwrap();
        assert_eq!(launch(&path, &fake), EXIT_DONE);
        assert!(fake.calls.borrow().is_empty());
        assert!(!result.exists());
        assert_eq!(
            std::fs::read(dir.path().join(claim_file(HEX))).unwrap(),
            b"abandoned\n"
        );
    }

    #[test]
    fn a_claimed_request_that_is_gone_or_unreadable_loses_its_claim_and_starts_nothing() {
        let dir = Scratch::new("gone");
        let fake = spawner(Ok(made()));
        let path = dir.path().join(launch_request_file(HEX));
        assert_eq!(launch(&path, &fake), EXIT_DONE);
        assert!(!dir.path().join(claim_file(HEX)).exists());
        std::fs::write(&path, b"{\"application\":\"ssh.exe\"}\n").unwrap();
        assert_eq!(launch(&path, &fake), EXIT_DONE);
        assert!(!dir.path().join(claim_file(HEX)).exists());
        assert!(fake.calls.borrow().is_empty());
        assert_eq!(dir.names(), [launch_request_file(HEX)]);
    }

    #[test]
    fn a_launcher_past_its_deadline_starts_nothing_and_says_so() {
        let dir = Scratch::new("deadline");
        let fake = spawner(Ok(made()));
        let result = dir.path().join(result_file(HEX));
        let path = request(dir.path(), fake.now - 1, &result);
        assert_eq!(launch(&path, &fake), EXIT_DONE);
        assert!(fake.calls.borrow().is_empty());
        assert_eq!(
            parse_launch_result(&std::fs::read(&result).unwrap()).unwrap(),
            LaunchResult::NotStarted {
                error: DEADLINE_PASSED.to_string()
            }
        );
    }

    #[test]
    fn a_failed_start_answers_with_its_error() {
        let dir = Scratch::new("spawnerror");
        let fake = spawner(Err("The holder's log could not be opened.".to_string()));
        let result = dir.path().join(result_file(HEX));
        let path = request(dir.path(), fake.now + 300_000_000, &result);
        assert_eq!(launch(&path, &fake), EXIT_DONE);
        assert_eq!(
            parse_launch_result(&std::fs::read(&result).unwrap()).unwrap(),
            LaunchResult::NotStarted {
                error: "The holder's log could not be opened.".to_string()
            }
        );
    }

    #[test]
    fn a_result_that_cannot_be_written_exits_non_zero() {
        let dir = Scratch::new("noanswer");
        let fake = spawner(Ok(made()));
        let result = dir.path().join("missing").join(result_file(HEX));
        let path = request(dir.path(), fake.now + 300_000_000, &result);
        assert_eq!(launch(&path, &fake), EXIT_NO_ANSWER);
    }

    #[test]
    fn an_argument_that_names_no_request_writes_nothing() {
        let dir = Scratch::new("notarequest");
        let fake = spawner(Ok(made()));
        for name in [
            "leases.json",
            "board-holder-launch..json",
            "board-holder-launch.ab.claim",
            "board-holder-launch.a.b.json",
        ] {
            assert_eq!(
                launch(&dir.path().join(name), &fake),
                EXIT_NOT_A_REQUEST,
                "{name}"
            );
        }
        assert!(dir.names().is_empty());
        assert_eq!(
            claim_for(&dir.path().join("board-holder-launch.ab12.json")),
            Some(dir.path().join("board-holder-launch.ab12.claim"))
        );
    }
}
