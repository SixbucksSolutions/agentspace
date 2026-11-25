//! The lease CLI shows no dialog when it starts a file that is no program, whatever error mode it
//! was started with (E0.10-T03-S01, #0219; #0225, D81's change during v0.10-T02).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Windows' CreateProcess, on a
//! file named `.exe` that holds no program, raises a modal "Unsupported 16-Bit Application" dialog
//! on the desktop and waits for a person, when the calling process's error mode is 0. **The CLI MUST
//! set its own error mode first (`agentspace_launch::quiet_hard_errors`, called from its main)**, so
//! that a person's shell, a task, or a runner that starts it at error mode 0 never gets that dialog
//! from a `create` or a `get` that starts a provisioning run (docs/driver.md, Starting a
//! provisioning run).
//!
//! Each test starts the built `agentspace` as a child with `CREATE_DEFAULT_ERROR_MODE`, so that the
//! child begins at error mode 0 whatever mode this test runner has (the run wrapper sets 0x8001,
//! which would hide the dialog by itself). `AGENTSPACE_PROVISION_CMD` names a file in a throwaway
//! home that holds no program, and the home is set up so that the command would start a run:
//! a listed project with no host for a `create`, and a `pending` lease on a `ready` host for a
//! `get`. A dialog shows as the child not ending within [`BOUND`]: the test then ends the child by
//! its own handle, which closes the dialog, and fails. The test ends every process it started by
//! its own handle, and touches no other.
//!
//! The answer is the contract's: the start failing changes nothing more and fails nothing, so the
//! lease is `pending` all the same (docs/driver.md, "A start that fails changes nothing more, and
//! fails nothing"). The tests also show that a run was due and was tried, by what the driver writes
//! before the start, so that a program that never reached the start cannot pass.
//!
//! Every variable the child reads is passed through `Command::env`, `USERPROFILE`, `HOME`,
//! `LOCALAPPDATA` and `APPDATA` point into the throwaway home, and a test never prints an
//! environment. The whole suite is run again by the isolation guard with the process's own home
//! moved away, and a test here MUST NOT read the process's environment for a home.
#![cfg(windows)]
#![allow(unsafe_code)]

mod common;

use common::{BIN, ID};
use serde_json::Value;
use std::ffi::c_void;
use std::fs;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// `CREATE_DEFAULT_ERROR_MODE` (winbase.h): the new process gets the default error mode, 0, and not
/// the creator's.
const CREATE_DEFAULT_ERROR_MODE: u32 = 0x0400_0000;

/// How long the child MAY take. A `create` or a `get` that starts a run takes a second or two, and
/// the bound is for a laptop under a parallel build; a child that waits on a dialog never ends.
const BOUND: Duration = Duration::from_secs(45);

const CREATED: &str = "2026-11-21T08:00:00Z";
const BOX_RECORD: &str = r#"{"project":"swimcoach","stack":"agentbox","state":"ready"}"#;
const HOST_READY: &str =
    r#"{"project":"agentspace","stack":"agentspace-host-agentspace","state":"ready"}"#;

/// A throwaway home and a file that is no program, under the temp folder.
struct Throwaway {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    not_a_program: PathBuf,
}

impl Throwaway {
    fn new() -> Throwaway {
        let tmp = tempfile::Builder::new()
            .prefix("asmode-")
            .tempdir()
            .expect("A throwaway folder can be made");
        let root = tmp.path().to_path_buf();
        assert!(
            root.starts_with(std::env::temp_dir()),
            "The throwaway folder MUST be under the temp folder"
        );
        let home = root.join("home");
        let bin = root.join("bin");
        fs::create_dir_all(home.join("AppData").join("Local")).unwrap();
        fs::create_dir_all(home.join("AppData").join("Roaming")).unwrap();
        fs::create_dir_all(&bin).unwrap();
        // A file named .exe that holds no PE image: Windows takes it for a 16-bit program.
        let not_a_program = bin.join("not-a-program.exe");
        fs::write(&not_a_program, b"This is not a program.").unwrap();
        Throwaway {
            _tmp: tmp,
            home,
            not_a_program,
        }
    }

    fn dot(&self) -> PathBuf {
        self.home.join(".agentspace")
    }

    fn write(&self, name: &str, text: &str) {
        fs::create_dir_all(self.dot()).unwrap();
        fs::write(self.dot().join(name), text.as_bytes()).unwrap();
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.dot().join(name)).unwrap_or_default()
    }

    /// The built program, as a child that begins at error mode 0, with this line.
    fn command(&self, line: &[&str]) -> Command {
        let mut cmd = Command::new(BIN);
        cmd.args(line)
            .current_dir(&self.home)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_DEFAULT_ERROR_MODE)
            .env("USERPROFILE", &self.home)
            .env("HOME", &self.home)
            .env("LOCALAPPDATA", self.home.join("AppData").join("Local"))
            .env("APPDATA", self.home.join("AppData").join("Roaming"))
            .env("AGENTSPACE_PROVISION_CMD", &self.not_a_program);
        for var in [
            "AGENTSPACE_STATE_DIR",
            "AGENTSPACE_SSH_DIR",
            "AGENTSPACE_PROXY_COMMAND",
            "AGENTSPACE_OPERATOR_CMD",
            "AGENTSPACE_CHECK_SEAM",
        ] {
            cmd.env_remove(var);
        }
        cmd
    }
}

/// What the child left.
struct Ended {
    status: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    /// The child's process error mode when it ended, as Windows keeps it (`ProcessDefaultHardErrorMode`),
    /// read through the handle this test holds for it.
    hard_error_mode: Result<u32, i32>,
}

/// `ProcessDefaultHardErrorMode`, the class of `NtQueryInformationProcess` that holds a process's
/// error mode, as Windows keeps it. Windows documents `SetErrorMode` and `GetErrorMode` for a
/// process's own mode and no call that reads another process's, so this is read through ntdll and
/// compared only with values read the same way (see the calibration in the test below).
const PROCESS_DEFAULT_HARD_ERROR_MODE: u32 = 12;

#[link(name = "ntdll", kind = "raw-dylib")]
unsafe extern "system" {
    fn NtQueryInformationProcess(
        process: *mut c_void,
        class: u32,
        info: *mut c_void,
        length: u32,
        returned: *mut u32,
    ) -> i32;
}

#[link(name = "kernel32", kind = "raw-dylib")]
unsafe extern "system" {
    fn SetErrorMode(mode: u32) -> u32;
}

/// The error mode of the process `handle` names, as Windows keeps it, or the status of the call
/// that could not read it.
fn hard_error_mode_of(handle: *mut c_void) -> Result<u32, i32> {
    let mut value: u32 = 0;
    let mut returned: u32 = 0;
    // SAFETY: `handle` is a process handle that is open for the call, and the buffer is a `u32`
    // that lives through it, with its length given.
    let status = unsafe {
        NtQueryInformationProcess(
            handle,
            PROCESS_DEFAULT_HARD_ERROR_MODE,
            (&mut value as *mut u32).cast(),
            4,
            &mut returned,
        )
    };
    if status == 0 { Ok(value) } else { Err(status) }
}

/// The error mode, as Windows keeps it, of a process that has been given `mode` by `SetErrorMode`,
/// found by giving it to this test process for the moment of the read and putting its own back.
#[track_caller]
fn kept_for(mode: u32) -> u32 {
    // The current process's pseudo handle, `(HANDLE)-1`.
    let current = (-1_isize) as *mut c_void;
    // SAFETY: sets this process's own error mode, and sets it back.
    let before = unsafe { SetErrorMode(mode) };
    let kept = hard_error_mode_of(current);
    // SAFETY: as above.
    unsafe { SetErrorMode(before) };
    kept.unwrap_or_else(|status| panic!("This process's error mode could not be read: {status:#x}"))
}

/// Runs the child to its end within [`BOUND`]. A child that has not ended by then is waiting on a
/// dialog, or on something else: it is ended by its own handle, which closes the dialog, and the
/// test fails.
#[track_caller]
fn run_bounded(mut cmd: Command, what: &str) -> Ended {
    let mut child = cmd.spawn().expect("The binary starts");
    let began = Instant::now();
    loop {
        match child.try_wait().expect("The child can be polled") {
            Some(_) => break,
            None if began.elapsed() > BOUND => {
                // Its own handle, so no other process is touched.
                let killed = child.kill();
                let _ = child.wait();
                panic!(
                    "The {what} did not end within {BOUND:?}: it is waiting on a dialog (#0225). \
                     It was ended by its own handle (killed: {}).",
                    killed.is_ok()
                );
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    // The child has ended, and this test still holds its handle, so its mode can still be read.
    let hard_error_mode = hard_error_mode_of(child.as_raw_handle());
    let out = child.wait_with_output().expect("The output can be read");
    Ended {
        status: out.status.code(),
        stdout: out.stdout,
        stderr: out.stderr,
        hard_error_mode,
    }
}

/// The one JSON document a run that succeeded printed.
#[track_caller]
fn success_doc(ended: &Ended) -> Value {
    assert_eq!(
        ended.status,
        Some(0),
        "The answer is the lease, not an error; stderr: {}",
        String::from_utf8_lossy(&ended.stderr)
    );
    assert!(ended.stderr.is_empty(), "Nothing is written to stderr");
    let text = std::str::from_utf8(&ended.stdout).expect("Stdout is UTF-8");
    assert!(
        text.ends_with('\n') && text.matches('\n').count() == 1,
        "Stdout holds exactly one document: {text:?}"
    );
    let doc: Value = serde_json::from_str(text.trim_end()).expect("Stdout is JSON");
    let schema = common::repo_root()
        .join("schemas")
        .join("lease.output.schema.json");
    let schema: Value = serde_json::from_str(&fs::read_to_string(schema).unwrap()).unwrap();
    let validator = jsonschema::draft202012::new(&schema).expect("The schema can be built");
    let problems: Vec<String> = validator.iter_errors(&doc).map(|e| e.to_string()).collect();
    assert!(
        problems.is_empty(),
        "The lease breaks its schema: {problems:?}"
    );
    doc
}

/// Makes the CA in `home` with the operator's own `ca`, run in process, as pending_binary.rs does.
/// The context names the throwaway home and nothing of the real profile.
fn make_ca(home: &Path) {
    assert!(
        home.starts_with(std::env::temp_dir()),
        "The home is not under the temp folder"
    );
    let ctx = agentspace_operator_api::Context {
        home: home.to_path_buf(),
        local_app_data: None,
        system_root: PathBuf::from(
            std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string()),
        ),
        nested: false,
    };
    let env = agentspace_ca::CaEnv {
        user_name: Some("someone".to_string()),
        computer_name: Some("EXAMPLEPC".to_string()),
        ..agentspace_ca::CaEnv::default()
    };
    if let Err(f) = agentspace_ca::ca(&ctx, &env, &NullSay) {
        panic!(
            "The operator's ca could not make the throwaway CA: {}",
            f.message
        );
    }
}

/// A `Say` that prints nothing, for the in-process `ca`.
struct NullSay;

impl agentspace_operator_api::Say for NullSay {
    fn mode(&self) -> agentspace_operator_api::Mode {
        agentspace_operator_api::Mode::Person
    }
    fn progress(&self, _: &str, _: Option<agentspace_operator_api::CallerLine>) {}
    fn warning(&self, _: &str) {}
    fn detail(&self, _: &str) {}
}

#[test]
fn a_create_that_starts_a_run_from_error_mode_0_shows_no_dialog_and_answers_pending() {
    let t = Throwaway::new();
    make_ca(&t.home);
    // The project is listed and has no host, so a `create` adds its record and starts a run.
    t.write(
        "projects.json",
        r#"{"version":0,"projects":[{"name":"agentspace","approved":"2026-11-20"}]}"#,
    );
    let ended = run_bounded(
        t.command(&["lease", "create", "--project", "agentspace"]),
        "create",
    );
    let doc = success_doc(&ended);
    assert_eq!(doc["state"], "pending", "The lease is pending all the same");
    assert_eq!(doc["project"], "agentspace");
    for absent in ["ssh", "expires_at", "reasons"] {
        assert!(doc.get(absent).is_none(), "A pending lease has no {absent}");
    }

    // The run was due and was tried: the host's record is `creating` with the time of the start,
    // and its folder is made, both before the start (docs/driver.md, Starting a provisioning run).
    let hosts = t.read("hosts.json");
    assert!(
        hosts.contains(
            r#""stack":"agentspace-host-agentspace","state":"creating","last_attempt_at":""#
        ),
        "No run was tried: {hosts}"
    );
    assert!(
        t.dot().join("hosts").join("agentspace").is_dir(),
        "The host's folder is made before the start"
    );
    let leases: Value = serde_json::from_str(&t.read("leases.json")).expect("The lease records");
    assert_eq!(leases["leases"][0]["state"], "pending");
}

#[test]
fn a_get_that_starts_a_run_from_error_mode_0_shows_no_dialog_and_prints_the_lease_unchanged() {
    let t = Throwaway::new();
    // A `pending` lease on a `ready` host: `get` asks the driver to check it, which starts a run.
    t.write(
        "hosts.json",
        &format!("{{\"version\":0,\"hosts\":[{BOX_RECORD},{HOST_READY}]}}\n"),
    );
    t.write(
        "leases.json",
        &format!(
            "{{\"version\":0,\"leases\":[{{\"id\":\"{ID}\",\"project\":\"agentspace\",\"state\":\"pending\",\"platform\":\"linux/arm64\",\"profile\":\"agent-linux\",\"size\":\"m\",\"ttl_seconds\":3600,\"created_at\":\"{CREATED}\",\"driver\":\"ec2\"}}]}}\n"
        ),
    );
    let before = t.read("leases.json");
    let ended = run_bounded(t.command(&["lease", "get", ID]), "get");
    let doc = success_doc(&ended);
    assert_eq!(doc["state"], "pending");
    assert_eq!(doc["id"], ID);
    assert_eq!(
        t.read("leases.json"),
        before,
        "The lease records are as they were"
    );
    let hosts = t.read("hosts.json");
    assert!(
        hosts.contains(r#""last_attempt_at":""#),
        "No run was tried: {hosts}"
    );
    assert!(
        hosts.contains(r#""stack":"agentspace-host-agentspace","state":"ready""#),
        "The host stays ready: {hosts}"
    );
}

#[test]
fn the_cli_sets_its_own_error_mode_before_anything_else_whatever_it_was_started_with() {
    // The two tests above show that no dialog comes of the starts that exist. They cannot show that
    // the CLI's main makes every start quiet (#0219: "calls it from the CLI's main"), since the
    // starts today are quiet for another reason, each start's own guard and a detached start made
    // by WMI. This test reads the child's error mode itself.
    //
    // The values are read the same way for the child and for this process, so that no layout of
    // the number is assumed: the mode Windows keeps for SEM_FAILCRITICALERRORS and
    // SEM_NOOPENFILEERRORBOX (what `quiet_hard_errors` sets) and the mode it keeps for 0 (what the
    // child began with, by CREATE_DEFAULT_ERROR_MODE).
    let quiet = kept_for(0x8001);
    let default = kept_for(0);
    assert_ne!(
        quiet, default,
        "The two modes are told apart, or this test shows nothing"
    );

    // A program that sets no mode of its own, started the same way, reads as `default`: this is
    // what the CLI would read as, if its main did not set its own.
    let mut unguarded = Command::new("cmd.exe");
    unguarded
        .args(["/d", "/c", "exit", "0"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_DEFAULT_ERROR_MODE);
    let ended = run_bounded(unguarded, "unguarded program");
    assert_eq!(
        ended.hard_error_mode,
        Ok(default),
        "A child started at error mode 0 that sets none of its own reads as the default mode"
    );

    let t = Throwaway::new();
    // `--help` reaches nothing of the home: it is the first thing the program does that is its own.
    let ended = run_bounded(t.command(&["--help"]), "help");
    assert_eq!(ended.status, Some(0));
    let mode = ended
        .hard_error_mode
        .unwrap_or_else(|status| panic!("The child's error mode could not be read: {status:#x}"));
    assert_ne!(
        mode, default,
        "The CLI ended at the error mode it was started with, 0: its main did not set its own"
    );
    assert_eq!(
        mode, quiet,
        "The CLI's error mode is SEM_FAILCRITICALERRORS and SEM_NOOPENFILEERRORBOX, as quiet_hard_errors sets"
    );
}
