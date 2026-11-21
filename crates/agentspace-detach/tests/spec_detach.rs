//! Starting a program that outlives the call (card #0147, D48).
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! Written from the card alone. The helper program, `detach-test-helper`, is built by this crate:
//! `write <file> <var> <delay-ms>` sleeps, then writes one line `set:<value>` or `unset` for the
//! variable `<var>`; `detach <file> <var> <value> <delay-ms>` reads one line from stdin, starts
//! `write` of its own exe through `WmiDetach` with the environment `[(var, value)]` and the file's
//! folder as its working directory, prints `started` or `refused: <message>`, and sleeps.
//!
//! Every file a test makes is in a throwaway folder under the temp folder, removed at the end. A
//! test MUST NOT print a captured environment's values, so a mismatch is reported by name only.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;
#[cfg(windows)]
use std::time::Instant;

use agentspace_detach::{Detach, NoDetach};

const HELPER: &str = env!("CARGO_BIN_EXE_detach-test-helper");

/// A throwaway folder under the temp folder, removed (best effort, with retries, because a
/// helper that was started in it may still hold it as its working directory) when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "agentspace-detach-spec-{tag}-{}-{}-{nanos}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("The scratch folder can be made.");
        Scratch(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for _ in 0..20 {
            if std::fs::remove_dir_all(&self.0).is_ok() || !self.0.exists() {
                return;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}

/// Polls until the closure gives a value, or the deadline passes.
#[cfg(windows)]
fn wait_for<T>(limit: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let end = Instant::now() + limit;
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if Instant::now() >= end {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The file's one line, once it holds a whole line.
#[cfg(windows)]
fn read_line_of(file: &Path) -> Option<String> {
    let text = std::fs::read_to_string(file).ok()?;
    if text.ends_with('\n') {
        Some(text.trim_end().to_string())
    } else {
        None
    }
}

fn helper() -> PathBuf {
    PathBuf::from(HELPER)
}

// ---------------------------------------------------------------------------------------------
// NoDetach, on every system
// ---------------------------------------------------------------------------------------------

#[test]
fn no_detach_always_refuses_with_an_err_and_starts_nothing() {
    let scratch = Scratch::new("nodetach");
    let out = scratch.path().join("out.txt");
    let out_arg = out.to_string_lossy().to_string();
    let r = NoDetach.start(
        &helper(),
        &["write", &out_arg, "SOME_VAR", "0"],
        scratch.path(),
        &[],
    );
    assert!(r.is_err(), "NoDetach refuses.");
    assert!(
        !r.err().unwrap_or_default().is_empty(),
        "The refusal says something."
    );
    std::thread::sleep(Duration::from_millis(1500));
    assert!(!out.exists(), "NoDetach started nothing.");
}

#[test]
fn no_detach_refuses_with_an_env_too() {
    let scratch = Scratch::new("nodetach-env");
    let r = NoDetach.start(
        Path::new("nothing"),
        &[],
        scratch.path(),
        &[("A_NAME", OsStr::new("a value"))],
    );
    assert!(r.is_err(), "NoDetach refuses whatever it is given.");
}

// ---------------------------------------------------------------------------------------------
// WmiDetach, Windows only
// ---------------------------------------------------------------------------------------------

#[cfg(windows)]
mod windows {
    use super::*;
    use agentspace_detach::WmiDetach;
    use std::io::{BufRead, BufReader, Write};
    use std::os::windows::io::AsRawHandle;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    /// A job object that ends what is in it when its handle is closed.
    struct KillOnCloseJob(HANDLE);

    impl KillOnCloseJob {
        fn new() -> KillOnCloseJob {
            // SAFETY: Plain Win32 calls; the information struct is all zero but the limit flag.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                assert!(!job.is_null(), "A job object can be made.");
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                assert!(ok != 0, "The job can be set to end its processes on close.");
                KillOnCloseJob(job)
            }
        }

        fn add(&self, child: &std::process::Child) {
            // SAFETY: Both handles are alive.
            let ok = unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as HANDLE) };
            assert!(
                ok != 0,
                "The child can be put in the job (Windows error {}).",
                // SAFETY: Reads the calling thread's last error.
                unsafe { GetLastError() }
            );
        }
    }

    impl Drop for KillOnCloseJob {
        fn drop(&mut self) {
            // SAFETY: The handle is the job's own and is closed once.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    /// A started helper that is killed by its handle, never by name, if a test fails before it
    /// ends.
    struct Reap(std::process::Child);

    impl Drop for Reap {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn unique(prefix: &str) -> (String, String) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tag = format!("{}_{nanos}", std::process::id());
        (
            format!("AGENTSPACE_DETACH_SPEC_{prefix}_{tag}"),
            format!("planted-{prefix}-{tag}"),
        )
    }

    /// Starts `write <file> <var> <delay>` of a helper copy directly through WmiDetach.
    fn start_write(
        program: &Path,
        file: &Path,
        var: &str,
        delay_ms: u32,
        extra_args: &[&str],
        cwd: &Path,
        env: &[(&str, &OsStr)],
    ) -> Result<(), String> {
        let file = file.to_string_lossy().to_string();
        let delay = delay_ms.to_string();
        let mut args: Vec<&str> = vec!["write", &file, var, &delay];
        args.extend_from_slice(extra_args);
        WmiDetach.start(program, &args, cwd, env)
    }

    /// The done-when: a helper started through WmiDetach from inside a kill-on-close job outlives
    /// that job's closing, and the variable passed to it arrives.
    #[test]
    fn a_program_started_through_wmi_outlives_the_job_and_gets_the_passed_variable() {
        let scratch = Scratch::new("job");
        let out = scratch.path().join("out.txt");
        let (var, value) = unique("JOB");

        let job = KillOnCloseJob::new();
        let mut child = Command::new(HELPER)
            .arg("detach")
            .arg(&out)
            .arg(&var)
            .arg(&value)
            // The write helper's delay: long enough that it finishes after the job is closed.
            .arg("2000")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("The detach helper can be started.");
        job.add(&child);
        let mut stdin = child.stdin.take().expect("Piped stdin.");
        let stdout = child.stdout.take().expect("Piped stdout.");
        let mut child = Reap(child);

        let (tx, rx) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });

        stdin
            .write_all(b"go\n")
            .and_then(|()| stdin.flush())
            .expect("The go line can be sent.");
        let line = rx
            .recv_timeout(Duration::from_secs(60))
            .expect("The detach helper said something before the deadline.");
        assert_eq!(
            line, "started",
            "The detach helper reported that WmiDetach started the program."
        );

        // Close the job, which ends the detach helper.
        drop(stdin);
        drop(job);
        let ended = wait_for(Duration::from_secs(20), || {
            child.0.try_wait().ok().flatten()
        });
        assert!(
            ended.is_some(),
            "The job ended the detach helper, so it did what it was made to do."
        );

        // The started program was not in the job: it still finishes and writes its file.
        let got = wait_for(Duration::from_secs(60), || read_line_of(&out))
            .expect("The program started through WMI outlived the job and wrote its file.");
        assert!(
            got == format!("set:{value}"),
            "The file did not say that the planted variable {var} reached the program."
        );
    }

    #[test]
    fn the_passed_variables_are_set_on_top_of_the_logon_environment() {
        let scratch = Scratch::new("onto");
        let out = scratch.path().join("out.txt");
        let out2 = scratch.path().join("out2.txt");
        let (var, value) = unique("TOP");
        start_write(
            &helper(),
            &out,
            &var,
            0,
            &[],
            scratch.path(),
            &[(&var, OsStr::new(&value))],
        )
        .expect("The start is accepted.");
        // OS is in every Windows logon environment, so a program that holds it while it also got
        // the planted variable shows that the passed ones are added to what was there.
        start_write(
            &helper(),
            &out2,
            "OS",
            0,
            &[],
            scratch.path(),
            &[(&var, OsStr::new(&value))],
        )
        .expect("The second start is accepted.");
        let got = wait_for(Duration::from_secs(60), || read_line_of(&out))
            .expect("The first program wrote its file.");
        assert!(
            got == format!("set:{value}"),
            "The planted variable {var} did not arrive."
        );
        let got2 = wait_for(Duration::from_secs(60), || read_line_of(&out2))
            .expect("The second program wrote its file.");
        assert!(
            got2.starts_with("set:"),
            "The logon environment's variable OS did not reach the program."
        );
    }

    /// What the helper's one line says about a variable. A value is never kept, so a check can
    /// report only a name and one of these.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Seen {
        Set,
        SetEmpty,
        Unset,
    }

    impl Seen {
        fn of(line: &str) -> Option<Seen> {
            match line {
                "unset" => Some(Seen::Unset),
                "set:" => Some(Seen::SetEmpty),
                other if other.starts_with("set:") => Some(Seen::Set),
                _ => None,
            }
        }

        fn label(self) -> &'static str {
            match self {
                Seen::Set => "set",
                Seen::SetEmpty => "set-empty",
                Seen::Unset => "unset",
            }
        }
    }

    /// Starts `write` of `var` through WmiDetach with `env` and says what the started program saw.
    fn seen_in_started(tag: &str, var: &str, env: &[(&str, &OsStr)]) -> Seen {
        let scratch = Scratch::new(tag);
        let out = scratch.path().join("out.txt");
        start_write(&helper(), &out, var, 0, &[], scratch.path(), env)
            .unwrap_or_else(|_| panic!("The start for {var} is accepted ({tag})."));
        let line = wait_for(Duration::from_secs(60), || read_line_of(&out))
            .unwrap_or_else(|| panic!("The program checking {var} wrote its file ({tag})."));
        Seen::of(&line)
            .unwrap_or_else(|| panic!("The program checking {var} wrote an unknown line ({tag})."))
    }

    /// Architect, 2026-11-20, for v0.07-T02: when a pair is passed, WmiDetach passes the whole
    /// logon environment with it, so the variables every program needs are still there.
    #[test]
    fn path_and_systemroot_are_present_when_a_pair_is_passed() {
        let (var, value) = unique("WHOLE");
        for name in ["PATH", "SystemRoot"] {
            let seen = seen_in_started(
                &format!("whole-{name}"),
                name,
                &[(&var, OsStr::new(&value))],
            );
            assert!(
                seen == Seen::Set,
                "The variable {name} MUST be set and not empty in a program started with a pair, and was {}.",
                seen.label()
            );
        }
    }

    /// Architect, 2026-11-20, for v0.07-T02: nothing of the starting process's own environment
    /// reaches the started program (CreateEnvironmentBlock with bInherit FALSE).
    ///
    /// The started program checks one variable, and `detach` passes that same name as its pair, so
    /// a planted variable set on a starting helper through Command::env cannot be checked absent
    /// beside another pair. The starting process here is this test process itself, which MUST NOT
    /// change its environment; it holds the variables cargo sets for a test run, and a logon
    /// environment never holds them. Each is shown present here first, so the test cannot pass
    /// because the variable was never there.
    #[test]
    fn a_variable_set_only_in_the_starting_process_is_absent_in_the_started_one() {
        let (var, value) = unique("INHERIT");
        for name in ["CARGO_PKG_NAME", "CARGO_MANIFEST_DIR"] {
            assert!(
                std::env::var_os(name).is_some(),
                "The test process MUST hold the variable {name} for this test to show anything."
            );
            let with_pair = seen_in_started(
                &format!("inherit-pair-{name}"),
                name,
                &[(&var, OsStr::new(&value))],
            );
            assert!(
                with_pair == Seen::Unset,
                "The starting process's variable {name} reached a program started with a pair (it was {}).",
                with_pair.label()
            );
            let no_pair = seen_in_started(&format!("inherit-none-{name}"), name, &[]);
            assert!(
                no_pair == Seen::Unset,
                "The starting process's variable {name} reached a program started with no pair (it was {}).",
                no_pair.label()
            );
        }
    }

    /// Architect, 2026-11-20, for v0.07-T02: a pair with an empty value removes the variable the
    /// logon environment holds (docs/state.md, The provisioning run's start). Removed means
    /// unset, never set and empty. PATH cannot be planted in the logon environment by a test, so
    /// the logon variable OS is used, with a control that it is there when another pair is passed.
    #[test]
    fn a_pair_with_an_empty_value_removes_the_variable_from_the_started_program() {
        let (var, value) = unique("EMPTY");
        let control = seen_in_started("empty-control", "OS", &[(&var, OsStr::new(&value))]);
        assert!(
            control == Seen::Set,
            "The control needs the logon variable OS set and not empty with another pair passed, and it was {}.",
            control.label()
        );
        let removed = seen_in_started("empty-removed", "OS", &[("OS", OsStr::new(""))]);
        assert!(
            removed == Seen::Unset,
            "The variable OS MUST be unset in a program started with an empty pair for it, and was {}.",
            removed.label()
        );
        // Only the named variable goes: the rest of the logon environment stays.
        let other = seen_in_started("empty-other", "PATH", &[("OS", OsStr::new(""))]);
        assert!(
            other == Seen::Set,
            "The variable PATH MUST stay set when only OS is removed, and was {}.",
            other.label()
        );
    }

    #[test]
    fn an_argument_with_spaces_arrives_intact() {
        let scratch = Scratch::new("spaces");
        let dir = scratch.path().join("a folder with spaces");
        std::fs::create_dir_all(&dir).expect("The folder can be made.");
        let out = dir.join("an output file.txt");
        let (var, value) = unique("SP");
        start_write(
            &helper(),
            &out,
            &var,
            0,
            &[],
            &dir,
            &[(&var, OsStr::new(&value))],
        )
        .expect("The start is accepted.");
        let got = wait_for(Duration::from_secs(60), || read_line_of(&out))
            .expect("The file whose path has spaces was written, so the argument arrived intact.");
        assert!(
            got == format!("set:{value}"),
            "The variable {var} did not arrive."
        );
    }

    #[test]
    fn an_argument_ending_in_backslashes_arrives_intact() {
        let scratch = Scratch::new("backslash");
        let out = scratch.path().join("out.txt");
        // The variable's name holds spaces and ends in backslashes: it needs quotes, and the
        // backslashes before the closing quote MUST be doubled, or the next argument is swallowed.
        let (base, value) = unique("BS");
        let var = format!("{base} WITH SPACES\\\\");
        start_write(
            &helper(),
            &out,
            &var,
            0,
            &[],
            scratch.path(),
            &[(&var, OsStr::new(&value))],
        )
        .expect("The start is accepted.");
        let got = wait_for(Duration::from_secs(60), || read_line_of(&out))
            .expect("The program wrote its file, so the arguments after the backslashes arrived.");
        assert!(
            got == format!("set:{value}"),
            "The argument ending in backslashes did not arrive intact (variable {base})."
        );
    }

    #[test]
    fn a_program_whose_path_has_spaces_is_started() {
        let scratch = Scratch::new("progspace");
        let dir = scratch.path().join("a program folder");
        std::fs::create_dir_all(&dir).expect("The folder can be made.");
        let program = dir.join("helper copy.exe");
        std::fs::copy(HELPER, &program).expect("The helper can be copied.");
        let out = scratch.path().join("out.txt");
        let (var, value) = unique("PS");
        start_write(
            &program,
            &out,
            &var,
            0,
            &[],
            scratch.path(),
            &[(&var, OsStr::new(&value))],
        )
        .expect("The start is accepted.");
        let got = wait_for(Duration::from_secs(60), || read_line_of(&out))
            .expect("The program whose path has spaces was started.");
        assert!(
            got == format!("set:{value}"),
            "The variable {var} did not arrive."
        );
    }

    /// Every refusal: an Err, never a panic, and nothing started. A valid start (the control) is
    /// shown to write its file, so a missing file after a refusal means nothing was started.
    fn assert_refused(
        tag: &str,
        program: Option<&Path>,
        extra_args: &[&str],
        env: &[(&str, &OsStr)],
    ) {
        let scratch = Scratch::new(tag);
        let out = scratch.path().join("out.txt");
        let program = program.map(Path::to_path_buf).unwrap_or_else(helper);
        let r = start_write(
            &program,
            &out,
            "AGENTSPACE_DETACH_SPEC_REFUSED",
            0,
            extra_args,
            scratch.path(),
            env,
        );
        assert!(r.is_err(), "The start was refused ({tag}).");
        assert!(
            !r.err().unwrap_or_default().is_empty(),
            "The refusal says something ({tag})."
        );
        // Nothing started, even after a program would have had time to write.
        std::thread::sleep(Duration::from_millis(2500));
        assert!(
            !out.exists(),
            "The refused start started something ({tag})."
        );
        // The control: the same call without the bad part starts and writes.
        let ok_out = scratch.path().join("ok.txt");
        start_write(
            &helper(),
            &ok_out,
            "AGENTSPACE_DETACH_SPEC_REFUSED",
            0,
            &[],
            scratch.path(),
            &[],
        )
        .expect("The control start is accepted.");
        assert!(
            wait_for(Duration::from_secs(60), || read_line_of(&ok_out)).is_some(),
            "The control started and wrote its file ({tag})."
        );
    }

    #[test]
    fn an_argument_with_a_double_quote_is_refused() {
        assert_refused("argquote", None, &["a\"b"], &[]);
    }

    #[test]
    fn an_argument_with_a_carriage_return_is_refused() {
        assert_refused("argcr", None, &["a\rb"], &[]);
    }

    #[test]
    fn an_argument_with_a_line_feed_is_refused() {
        assert_refused("arglf", None, &["a\nb"], &[]);
    }

    #[test]
    fn a_program_with_a_double_quote_is_refused() {
        let program = std::env::temp_dir().join("pro\"gram.exe");
        assert_refused("progquote", Some(&program), &[], &[]);
    }

    #[test]
    fn a_program_with_a_line_break_is_refused() {
        let cr = std::env::temp_dir().join("pro\rgram.exe");
        let lf = std::env::temp_dir().join("pro\ngram.exe");
        assert_refused("progcr", Some(&cr), &[], &[]);
        assert_refused("proglf", Some(&lf), &[], &[]);
    }

    #[test]
    fn an_env_name_that_is_empty_is_refused() {
        assert_refused("envempty", None, &[], &[("", OsStr::new("value"))]);
    }

    #[test]
    fn an_env_name_with_a_nul_is_refused() {
        assert_refused("envnamenul", None, &[], &[("A\0B", OsStr::new("value"))]);
    }

    #[test]
    fn an_env_name_with_an_equals_sign_is_refused() {
        assert_refused("envnameeq", None, &[], &[("A=B", OsStr::new("value"))]);
    }

    #[test]
    fn an_env_value_with_a_nul_is_refused() {
        assert_refused(
            "envvaluenul",
            None,
            &[],
            &[("A_NAME", OsStr::new("va\0lue"))],
        );
    }
}
