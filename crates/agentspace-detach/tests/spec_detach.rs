//! Starting a program that outlives the call (card #0147, D48).
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! Written from the card alone. The helper program, `detach-test-helper`, is built by this crate:
//! `write <file> <var> <delay-ms>` sleeps, then writes one line `set:<value>` or `unset` for the
//! variable `<var>`; `detach <file> <var> <value> <delay-ms>` reads one line from stdin, starts
//! `write` of its own exe through `WmiDetach` with the environment `[(var, value)]` and the file's
//! folder as its working directory, prints `started` or `refused: <message>`, and sleeps;
//! `ctrlc <file>` writes `handled` or `ignored` for a Ctrl+C it sends its own console.
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
fn no_detach_start_pid_refuses_too() {
    let scratch = Scratch::new("nodetach-pid");
    let r = NoDetach.start_pid(Path::new("nothing"), &[], scratch.path(), &[]);
    assert!(
        r.is_err(),
        "NoDetach refuses start_pid as it refuses start."
    );
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
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, WAIT_OBJECT_0};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
        QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
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

    /// A relative or empty program is refused before any call into Windows (driver.md's program
    /// is always a full path).
    #[test]
    fn a_program_that_is_not_a_full_path_is_refused() {
        let name = helper()
            .file_name()
            .map(PathBuf::from)
            .expect("The helper has a file name.");
        assert_refused("progrelative", Some(&name), &[], &[]);
        let why = WmiDetach
            .start(Path::new(""), &["provision"], &std::env::temp_dir(), &[])
            .err();
        assert_eq!(
            why.as_deref(),
            Some("The program must be named by its full path.")
        );
    }

    /// A nonzero `ReturnValue` of `Win32_Process.Create` is an Err: a full path that names no
    /// file is WMI's 9, the path not found.
    #[test]
    fn a_program_that_does_not_exist_is_an_err_naming_return_value_9() {
        let scratch = Scratch::new("missing");
        let program = scratch.path().join("no such program.exe");
        let why = WmiDetach
            .start(&program, &["provision"], scratch.path(), &[])
            .expect_err("A start of a program that does not exist is an Err.");
        assert!(
            why.starts_with("Win32_Process.Create returned 9"),
            "The Err names ReturnValue 9: {why}"
        );
    }

    /// The `.cmd` stand-in: it writes the first three arguments it receives, `%1`, `%2` and `%3`
    /// as cmd.exe gives them, quotes and all, each on its own line, to `args.txt` beside itself,
    /// through a temporary file and a move, so a reader sees all three lines or none.
    const STAND_IN: &str = "@echo off\r\n\
        (\r\n\
        echo(%1\r\n\
        echo(%2\r\n\
        echo(%3\r\n\
        ) > \"%~dp0args.tmp\"\r\n\
        move /y \"%~dp0args.tmp\" \"%~dp0args.txt\" >nul\r\n";

    /// driver.md: a `.cmd` program is run as `cmd.exe /d /c "<program>" provision -Host <project>`,
    /// and receives exactly `provision`, `-Host` and the project (architect, 2026-11-20, for
    /// v0.07-T02: an argument is quoted only when it needs it).
    fn a_cmd_stand_in_receives_the_three_arguments_in(scratch: &Scratch, folder: &Path) {
        std::fs::create_dir_all(folder).expect("The stand-in's folder can be made.");
        let script = folder.join("stand-in.cmd");
        std::fs::write(&script, STAND_IN).expect("The stand-in can be written.");
        let root = std::env::var_os("SystemRoot").expect("SystemRoot is set on Windows.");
        let cmd = PathBuf::from(root).join("System32").join("cmd.exe");
        let script_arg = script.to_string_lossy().to_string();
        WmiDetach
            .start(
                &cmd,
                &["/d", "/c", &script_arg, "provision", "-Host", "proj"],
                scratch.path(),
                &[],
            )
            .expect("The start of cmd.exe is accepted.");
        let out = folder.join("args.txt");
        let text = wait_for(Duration::from_secs(60), || {
            std::fs::read_to_string(&out).ok()
        })
        .expect("The stand-in wrote the arguments it received.");
        let got: Vec<&str> = text.lines().map(str::trim_end).collect();
        assert_eq!(got, ["provision", "-Host", "proj"]);
    }

    #[test]
    fn a_cmd_stand_in_started_through_cmd_receives_exactly_its_three_arguments() {
        let scratch = Scratch::new("cmd");
        let folder = scratch.path().join("standin");
        a_cmd_stand_in_receives_the_three_arguments_in(&scratch, &folder);
    }

    #[test]
    fn a_cmd_stand_in_in_a_folder_with_a_space_receives_exactly_its_three_arguments() {
        let scratch = Scratch::new("cmd-space");
        let folder = scratch.path().join("a stand in folder");
        a_cmd_stand_in_receives_the_three_arguments_in(&scratch, &folder);
    }

    /// Starts the helper's `ctrlc` mode with `start` and reads its one line.
    fn ctrl_c_seen(tag: &str, start: impl FnOnce(&Path, &Path) -> Result<(), String>) -> String {
        let scratch = Scratch::new(tag);
        let out = scratch.path().join("ctrlc.txt");
        start(&out, scratch.path())
            .unwrap_or_else(|e| panic!("The start is accepted ({tag}): {e}"));
        wait_for(Duration::from_secs(60), || read_line_of(&out))
            .unwrap_or_else(|| panic!("The ctrlc helper wrote its file ({tag})."))
    }

    /// driver.md, The detached start: the run starts in a new process group (architect,
    /// 2026-11-20, for v0.07-T02). The observable is Windows' own: a process created with
    /// `CREATE_NEW_PROCESS_GROUP` starts with Ctrl+C disabled, so a Ctrl+C sent to its console
    /// never reaches its handler. The control is the same helper created through `wmi::create`
    /// with no flags, as agentspace-launch creates its launcher, whose handler does see it, so
    /// the difference is the flag alone.
    #[test]
    fn a_program_started_through_wmi_detach_is_in_a_new_process_group() {
        let detached = ctrl_c_seen("ctrlc-detach", |out, cwd| {
            let out = out.to_string_lossy().to_string();
            WmiDetach.start(&helper(), &["ctrlc", &out], cwd, &[])
        });
        let control = ctrl_c_seen("ctrlc-control", |out, cwd| {
            let mut line = std::ffi::OsString::from("\"");
            line.push(helper().as_os_str());
            line.push("\" ctrlc \"");
            line.push(out.as_os_str());
            line.push("\"");
            match agentspace_detach::wmi::create(&line, cwd.as_os_str(), &[], None)? {
                0 => Ok(()),
                code => Err(format!("ReturnValue {code}")),
            }
        });
        assert_eq!(
            control, "handled",
            "With no flags, the helper's handler sees the Ctrl+C, so the check can tell."
        );
        assert_eq!(
            detached, "ignored",
            "A program started through WmiDetach MUST be in a new process group, with Ctrl+C disabled."
        );
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

    /// A process this test started, owned through a handle it opened by the ID `start_pid` gave:
    /// the one process the test ever ends. Ended and waited for when dropped, so a test that fails
    /// first leaves nothing running. It is never reached by name or by an ID the test did not get
    /// from its own start.
    struct Started(HANDLE);

    impl Started {
        /// Opens `pid` with the rights to read its image name, end it and wait for it. `None` when
        /// no such process can be opened.
        fn open(pid: u32) -> Option<Started> {
            // SAFETY: A plain call; a null handle is its failure.
            let handle = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE,
                    0,
                    pid,
                )
            };
            if handle.is_null() {
                None
            } else {
                Some(Started(handle))
            }
        }

        /// The file name of the process's image.
        fn image_name(&self) -> String {
            let mut buffer = [0u16; 1024];
            let mut size = buffer.len() as u32;
            // SAFETY: The handle is open, and the buffer and its size are ours.
            let ok =
                unsafe { QueryFullProcessImageNameW(self.0, 0, buffer.as_mut_ptr(), &mut size) };
            assert!(ok != 0, "The process's image name can be read.");
            let path = String::from_utf16_lossy(&buffer[..size as usize]);
            Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        }

        fn has_ended(&self) -> bool {
            // SAFETY: The handle is open, with the right to wait.
            unsafe { WaitForSingleObject(self.0, 0) == WAIT_OBJECT_0 }
        }

        /// Ends the process through the handle, and waits up to 20 seconds for it to be gone.
        fn end(&self) -> bool {
            // SAFETY: The handle is open, with the right to end the process it names.
            unsafe {
                TerminateProcess(self.0, 1);
                WaitForSingleObject(self.0, 20_000) == WAIT_OBJECT_0
            }
        }
    }

    impl Drop for Started {
        fn drop(&mut self) {
            self.end();
            // SAFETY: The handle is this value's own and is closed once.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    /// The process ID `start_pid` answers is the created process's (architect, 2026-11-22, for
    /// v0.08-T03). The helper's `write` sleeps 2 minutes, so it is alive when it is opened by the
    /// ID; the test checks the image name, ends that one process through its handle, and
    /// confirms it is gone, by the handle's wait and by the ID no longer opening.
    #[test]
    fn start_pid_gives_the_id_of_the_process_it_created() {
        let scratch = Scratch::new("pid");
        let out = scratch.path().join("out.txt");
        let file = out.to_string_lossy().to_string();
        let answer = WmiDetach.start_pid(
            &helper(),
            &["write", &file, "AGENTSPACE_DETACH_SPEC_PID", "120000"],
            scratch.path(),
            &[],
        );
        let pid = answer
            .expect("The start is accepted.")
            .expect("WmiDetach gives the process ID.");
        assert_ne!(pid, 0, "The process ID is a real one.");
        // The ID names a process the test can open, and it is the helper's.
        let process = Started::open(pid).expect("The process with that ID can be opened.");
        assert_eq!(
            process.image_name(),
            Path::new(HELPER)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            "The process the ID names is the helper."
        );
        assert!(
            !process.has_ended(),
            "It is still running before it is ended."
        );
        assert!(process.end(), "The test ended the process it started.");
        assert!(process.has_ended(), "The process is gone.");
        drop(process);
        assert!(
            Started::open(pid).is_none_or(|again| again.has_ended()),
            "No process holds that ID any more."
        );
    }

    /// `start_pid` refuses what `start` refuses, before any call into Windows.
    #[test]
    fn start_pid_refuses_what_start_refuses() {
        let scratch = Scratch::new("pid-refused");
        let relative = WmiDetach.start_pid(Path::new("run.exe"), &[], scratch.path(), &[]);
        assert_eq!(
            relative,
            Err("The program must be named by its full path.".to_string())
        );
        let quote = WmiDetach.start_pid(&helper(), &["a\"b"], scratch.path(), &[]);
        assert!(quote.is_err_and(|e| e.starts_with("Argument 1 holds a double quote")));
    }
}
