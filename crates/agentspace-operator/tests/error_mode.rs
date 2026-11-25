//! The operator's process error mode (#0225, #0226, D81; docs/keeper.md, "Streams and environment"
//! gives the rule for the keeper's programs, and docs/operator.md, "The test seam", the variables
//! used here).
//!
//! Each test starts the built operator from a parent whose error mode is 0
//! (`CREATE_DEFAULT_ERROR_MODE`), with a configured program path that names a file called `.exe`
//! whose first bytes are not `MZ`. Windows treats such a file as a 16-bit program, and a process
//! at error mode 0 that starts one waits at a modal "Unsupported 16-Bit Application" dialog for a
//! person. The operator MUST instead fail at once with its own error. Every test ends the process
//! it started by its own PID, under a timeout, and works only in a throwaway home.
#![cfg(windows)]

use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// The process creation flag that gives the child error mode 0, whatever the parent's is.
const CREATE_DEFAULT_ERROR_MODE: u32 = 0x0400_0000;

/// How long the operator has to fail. A dialog never ends by itself, so the margin only has to
/// cover a loaded machine.
const LIMIT: Duration = Duration::from_secs(40);

/// The longest a start that fails at once may take from the child's start to its exit, and the
/// bound that makes a test notice a dialog that a person dismissed or a wait that ended by itself.
///
/// The push waits `STDOUT_WAIT`, 10 s (agentspace-push `system.rs`), on the ssh's stdout when its
/// start is not guarded, so an operator whose `up` runs at the mercy of the dialog still exits, after
/// that wait, with the same code and the same warning: without a bound the test passes. On main
/// (the 9:17pm, 2026-11-24 evidence run of this test at error mode 0x8001, which finished in
/// 10.63 s, with the "Unsupported 16-Bit Application" dialog raised on the desktop) the operator took
/// that wait; on the fixed build the same test takes about 0.2 s. 5 s is about half of the one and
/// more than twenty times the other: it fails a build that waits, and leaves a loaded machine's
/// fixed build a wide margin.
const WAIT_BOUND: Duration = Duration::from_secs(5);

/// A throwaway home that is removed when it goes out of scope.
struct Home {
    root: PathBuf,
}

impl Home {
    fn new(name: &str) -> Home {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("error-mode-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("The throwaway home is made");
        Home { root }
    }

    /// A path inside the home, written with `\` between its parts as Windows and the program
    /// under test write one.
    fn path(&self, relative: &str) -> PathBuf {
        relative
            .split('/')
            .fold(self.root.clone(), |path, part| path.join(part))
    }

    /// Makes a folder inside the home and returns it.
    fn folder(&self, relative: &str) -> PathBuf {
        let folder = self.path(relative);
        fs::create_dir_all(&folder).expect("A folder of the throwaway home is made");
        folder
    }

    /// Writes a file inside the home and returns its path.
    fn file(&self, relative: &str, bytes: &[u8]) -> PathBuf {
        let file = self.path(relative);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).expect("A folder of the throwaway home is made");
        }
        fs::write(&file, bytes).expect("A file of the throwaway home is written");
        file
    }

    /// Hides the home's path in text that is shown.
    fn mask(&self, text: &str) -> String {
        text.replace(self.root.to_string_lossy().as_ref(), "<home>")
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// A file named `.exe` whose first bytes are not `MZ`.
fn not_a_program(home: &Home, relative: &str) -> PathBuf {
    home.file(relative, b"This is text, not a Windows program.\r\n")
}

/// A stand-in AWS CLI that answers what a start-and-wait asks of a box that is running and was
/// launched long ago, so that nothing waits (docs/operator.md, Starting the box and waiting for
/// it).
fn aws_stand_in(home: &Home) -> PathBuf {
    home.file(
        "bin/stack.json",
        br#"{"Stacks":[{"StackStatus":"CREATE_COMPLETE","Outputs":[{"OutputKey":"InstanceId","OutputValue":"i-0123456789abcdef0"}]}]}"#,
    );
    home.file(
        "bin/instance.json",
        br#"{"Reservations":[{"Instances":[{"State":{"Name":"running"},"LaunchTime":"2020-01-01T00:00:00+00:00"}]}]}"#,
    );
    home.file(
        "bin/aws.cmd",
        b"@echo off\r\n\
          if \"%1\"==\"cloudformation\" (\r\n  type \"%~dp0stack.json\"\r\n  exit /b 0\r\n)\r\n\
          if \"%1\"==\"ec2\" (\r\n  type \"%~dp0instance.json\"\r\n  exit /b 0\r\n)\r\n\
          exit /b 0\r\n",
    )
}

/// The dummy token: 43 characters from `A-Z`, `a-z`, `0-9`, `-` and `_`, the form the push reads
/// (operator.md, The Bot credential). It is made up and opens nothing.
const DUMMY_TOKEN: &str = "DummyTokenForTestsOnly0123456789abcdefghijk";

/// `CryptProtectData` of `plain`, in user scope with no entropy, as lowercase hex: the way a test
/// writes a credential for itself (operator.md, The Bot credential).
fn protect_to_hex(plain: &[u8]) -> String {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(plain.len()).expect("A short input"),
        pbData: plain.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: `input` points at `plain`, which outlives the call; `output` receives a buffer that
    // is copied and then freed with `LocalFree`, as the call requires.
    let bytes = unsafe {
        let ok = CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        );
        assert_ne!(ok, 0, "CryptProtectData made a blob");
        let copy = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData.cast());
        copy
    };
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Writes a fake localswim receipt and a fake Bot credential into the home, so that the token
/// push is due and reaches its ssh (operator.md, The receipt; The Bot credential).
fn localswim_stand_in(home: &Home) {
    assert_eq!(DUMMY_TOKEN.len(), 43);
    let utf16: Vec<u8> = DUMMY_TOKEN
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let credentials = home.folder("credentials");
    let bot = format!(
        r#"{{"format":"localswim-credential","format_version":1,"role":"bot","token_dpapi_hex":"{}"}}"#,
        protect_to_hex(&utf16)
    );
    home.file("credentials/bot.json", bot.as_bytes());
    let folder = credentials.to_string_lossy().replace('\\', "\\\\");
    let receipt =
        format!(r#"{{"base":"http://127.0.0.1:8811","credential_directory":"{folder}"}}"#);
    home.file("local/localswim-v2/service.json", receipt.as_bytes());
}

/// The operator, ready to run in the home: its environment is the home's, and no seam is set but
/// the ones the test gives it.
fn operator(home: &Home, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_agentspace-operator"));
    cmd.args(args)
        .current_dir(home.folder("work"))
        .env("USERPROFILE", &home.root)
        .env("HOME", &home.root)
        .env("LOCALAPPDATA", home.folder("local"))
        .env("APPDATA", home.folder("roaming"))
        .env("AGENTSPACE_STATE_DIR", home.folder("state"))
        .env("AGENTSPACE_SSH_DIR", home.folder("ssh"));
    for name in [
        "AGENTSPACE_NESTED",
        "AGENTSPACE_DIAGNOSTIC",
        "AGENTSPACE_SCP_CMD",
        "AGENTSPACE_TIMEZONE",
        "AGENTSPACE_PROXY_COMMAND",
    ] {
        cmd.env_remove(name);
    }
    cmd
}

struct Outcome {
    /// The exit code, or `None` when the child had not exited within `LIMIT` and the test ended it.
    code: Option<i32>,
    /// From just before the start to the exit the test saw (or to the end the test gave it).
    elapsed: Duration,
    stdout: String,
    stderr: String,
}

/// Whether the child ended on its own, and soon enough that no dialog can have held it, or why not.
/// Pure, so that planted outcomes can show that it rejects.
///
/// A dialog is not seen by the exit code or the text: a person who presses OK, or a wait that times
/// out, lets the same exit and the same lines follow. The time is what shows it (`WAIT_BOUND`).
fn judge_timing(code: Option<i32>, elapsed: Duration) -> Result<(), String> {
    if code.is_none() {
        return Err(format!(
            "No exit within {} s, and the test ended the child: a hard-error dialog likely",
            LIMIT.as_secs()
        ));
    }
    if elapsed > WAIT_BOUND {
        return Err(format!(
            "The child took {:.2} s to exit, over the {} s bound: it waited, at a dialog or on a stream",
            elapsed.as_secs_f64(),
            WAIT_BOUND.as_secs()
        ));
    }
    Ok(())
}

/// Ends the child by its own handle when the test leaves, whatever the outcome.
struct EndOnDrop(Child);

impl Drop for EndOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn drain(mut pipe: impl Read + Send + 'static) -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        let _ = sender.send(String::from_utf8_lossy(&bytes).into_owned());
    });
    receiver
}

/// Runs `cmd` at error mode 0 and returns how it ended. `None` for the code means that the child
/// had not exited within `LIMIT`, and was ended by its own handle.
fn run_at_error_mode_0(mut cmd: Command) -> Outcome {
    cmd.creation_flags(CREATE_DEFAULT_ERROR_MODE)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let started = Instant::now();
    let mut child = EndOnDrop(cmd.spawn().expect("The operator starts"));
    let stdout = drain(child.0.stdout.take().expect("A piped stdout"));
    let stderr = drain(child.0.stderr.take().expect("A piped stderr"));
    let deadline = started + LIMIT;
    let code = loop {
        match child.0.try_wait().expect("The child can be waited for") {
            Some(status) => break Some(status.code().unwrap_or(-1)),
            None if Instant::now() >= deadline => {
                let _ = child.0.kill();
                let _ = child.0.wait();
                break None;
            }
            None => thread::sleep(Duration::from_millis(50)),
        }
    };
    let elapsed = started.elapsed();
    let text = |receiver: mpsc::Receiver<String>| {
        receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_default()
    };
    Outcome {
        code,
        elapsed,
        stdout: text(stdout),
        stderr: text(stderr),
    }
}

/// What a start that fails at once must look like, as a reason, or `Ok`. Pure, so that a planted
/// outcome can show that it rejects.
///
/// The program here is one a seam variable names, so its error is the seam's line only (operator.md,
/// The test seam): `agentspace: <variable> names <value>, which cannot be run.`, since a file the
/// system cannot start is one that names nothing runnable. `agentspace: Could not run <path>.` is
/// the line for a default program (Programs it starts), and for a seam's program it is #0227's
/// defect, so it is rejected (the architect's ruling P6 for E0.10-T03-S03).
fn judge(outcome: &Outcome, variable: &str, program: &Path) -> Result<(), String> {
    judge_timing(outcome.code, outcome.elapsed)?;
    let Some(code) = outcome.code else {
        return Err("The child did not exit".to_string());
    };
    if code != 1 {
        return Err(format!("The exit code is {code}, not 1"));
    }
    if !outcome.stdout.is_empty() {
        return Err("The operator wrote to stdout".to_string());
    }
    let last = outcome.stderr.trim_end().lines().last().unwrap_or("");
    let path = program.display();
    let seam = format!("agentspace: {variable} names {path}, which cannot be run.");
    if last == seam {
        Ok(())
    } else {
        Err(
            "The last line of stderr is not the seam's line for a program that cannot be run"
                .to_string(),
        )
    }
}

/// The token push's own error for an ssh that cannot be started (operator.md, What the laptop
/// checks): a warning, and `up` goes on, exit 0, as a push never fails the command.
const PUSH_WARNING: &str = "agentspace: Could not send the localswim token; the board will not work on the box this session.";

fn judge_push(outcome: &Outcome) -> Result<(), String> {
    judge_timing(outcome.code, outcome.elapsed)?;
    let Some(code) = outcome.code else {
        return Err("The child did not exit".to_string());
    };
    if code != 0 {
        return Err(format!("The exit code is {code}, not 0"));
    }
    if !outcome.stdout.is_empty() {
        return Err("The operator wrote to stdout".to_string());
    }
    if !outcome.stderr.lines().any(|line| line == PUSH_WARNING) {
        return Err("The push's warning is not among the lines of stderr".to_string());
    }
    Ok(())
}

#[test]
fn up_with_an_ssh_that_is_not_a_program_warns_and_goes_on_at_once() {
    let home = Home::new("up");
    let aws = aws_stand_in(&home);
    let ssh = not_a_program(&home, "bin/ssh.exe");
    localswim_stand_in(&home);
    let mut cmd = operator(&home, &["up"]);
    cmd.env("AGENTSPACE_AWS_CMD", &aws)
        .env("AGENTSPACE_SSH_CMD", &ssh)
        .env("AGENTSPACE_TIMEZONE", "America/New_York");
    let outcome = run_at_error_mode_0(cmd);
    if let Err(reason) = judge_push(&outcome) {
        panic!(
            "{reason}\nexit: {:?}\nstdout: {}\nstderr: {}",
            outcome.code,
            home.mask(&outcome.stdout),
            home.mask(&outcome.stderr)
        );
    }
    // The push was due and reached its ssh: the box was ready first.
    assert!(
        outcome.stderr.contains("Ready: i-0123456789abcdef0"),
        "The start-and-wait did not finish:\n{}",
        home.mask(&outcome.stderr)
    );
    assert!(
        outcome
            .stderr
            .contains("Sending the localswim Bot token to the instance..."),
        "The push did not begin:\n{}",
        home.mask(&outcome.stderr)
    );
}

#[test]
fn bootstrap_with_an_ssh_that_is_not_a_program_fails_at_once() {
    let home = Home::new("bootstrap");
    let aws = aws_stand_in(&home);
    let ssh = not_a_program(&home, "bin/ssh.exe");
    let mut cmd = operator(&home, &["bootstrap"]);
    cmd.env("AGENTSPACE_AWS_CMD", &aws)
        .env("AGENTSPACE_SSH_CMD", &ssh);
    let outcome = run_at_error_mode_0(cmd);
    if let Err(reason) = judge(&outcome, "AGENTSPACE_SSH_CMD", &ssh) {
        panic!(
            "{reason}\nexit: {:?}\nstdout: {}\nstderr: {}",
            outcome.code,
            home.mask(&outcome.stdout),
            home.mask(&outcome.stderr)
        );
    }
    // The bootstrap got as far as its ssh, so the failure is that start's and no earlier step's.
    assert!(
        outcome
            .stderr
            .contains("Running the toolchain bootstrap on the instance..."),
        "The bootstrap did not reach its ssh:\n{}",
        home.mask(&outcome.stderr)
    );
}

// The checker is proven on planted outcomes: each one is wrong in one way, and each is rejected.

/// What a start that failed at once looks like in time: well under `WAIT_BOUND`.
const PROMPT: Duration = Duration::from_millis(200);

/// What the evidence run on main took: the push's 10 s wait on the ssh's stdout, 10.63 s, with a
/// dialog on the desktop and the same exit code and lines as a good run.
const MAIN_EVIDENCE: Duration = Duration::from_millis(10_630);

fn planted_after(code: Option<i32>, elapsed: Duration, stdout: &str, stderr: &str) -> Outcome {
    Outcome {
        code,
        elapsed,
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    }
}

fn planted(code: Option<i32>, stdout: &str, stderr: &str) -> Outcome {
    planted_after(code, PROMPT, stdout, stderr)
}

#[test]
fn the_timing_judge_rejects_planted_waits() {
    assert_eq!(judge_timing(Some(1), PROMPT), Ok(()));
    assert_eq!(judge_timing(Some(0), WAIT_BOUND), Ok(()));
    for (code, elapsed) in [
        // Main's evidence: it exited by itself after its wait, with a good code.
        (Some(0), MAIN_EVIDENCE),
        (Some(1), MAIN_EVIDENCE),
        // Just over the bound.
        (Some(0), WAIT_BOUND + Duration::from_millis(1)),
        // Ended by the test, however soon the test's clock says.
        (None, PROMPT),
        (None, LIMIT),
    ] {
        assert!(
            judge_timing(code, elapsed).is_err(),
            "{code:?} after {elapsed:?} was accepted"
        );
    }
    // The wait is what is named, and the end by the test is what is named.
    let slow = judge_timing(Some(0), MAIN_EVIDENCE).unwrap_err();
    assert!(slow.contains("10.63 s"), "{slow}");
    assert!(
        judge_timing(None, LIMIT)
            .unwrap_err()
            .contains("ended the child")
    );
}

#[test]
fn the_judge_accepts_only_the_seams_line_and_rejects_planted_outcomes() {
    let program = Path::new(r"C:\throwaway\bin\ssh.exe");
    let seam =
        "agentspace: AGENTSPACE_SSH_CMD names C:\\throwaway\\bin\\ssh.exe, which cannot be run.\n";
    let could_not = "agentspace: Could not run C:\\throwaway\\bin\\ssh.exe.\n";
    assert_eq!(
        judge(&planted(Some(1), "", seam), "AGENTSPACE_SSH_CMD", program),
        Ok(())
    );
    assert_eq!(
        judge(
            &planted(Some(1), "", &format!("Running...\n{seam}")),
            "AGENTSPACE_SSH_CMD",
            program
        ),
        Ok(())
    );
    // #0227's defect: the default's line for a program the seam variable names, alone or last.
    for stderr in [could_not.to_string(), format!("Running...\n{could_not}")] {
        let reason = judge(
            &planted(Some(1), "", &stderr),
            "AGENTSPACE_SSH_CMD",
            program,
        )
        .unwrap_err();
        assert!(reason.contains("not the seam's line"), "{reason}");
    }
    for bad in [
        planted(None, "", seam),
        planted(Some(0), "", seam),
        planted(Some(2), "", seam),
        planted(Some(1), "text", seam),
        planted(
            Some(1),
            "",
            "agentspace: AGENTSPACE_SSH_CMD names C:\\other\\ssh.exe, which cannot be run.\n",
        ),
        planted(Some(1), "", "agentspace: The toolchain bootstrap failed.\n"),
        planted(Some(1), "", ""),
        // The right code and the right line, after a wait.
        planted_after(Some(1), MAIN_EVIDENCE, "", seam),
        planted_after(Some(1), WAIT_BOUND + Duration::from_millis(1), "", seam),
    ] {
        assert!(judge(&bad, "AGENTSPACE_SSH_CMD", program).is_err());
    }
    // A variable that is not the one in the message is rejected too.
    assert!(judge(&planted(Some(1), "", seam), "AGENTSPACE_AWS_CMD", program).is_err());
}

#[test]
fn the_push_judge_rejects_planted_outcomes() {
    let warning = format!("Ready: i-0\n{PUSH_WARNING}\n");
    assert_eq!(judge_push(&planted(Some(0), "", &warning)), Ok(()));
    for bad in [
        planted(None, "", &warning),
        planted(Some(1), "", &warning),
        planted(Some(0), "text", &warning),
        planted(Some(0), "", "Ready: i-0\n"),
        planted(
            Some(0),
            "",
            "agentspace: Could not send the localswim token; the board will work.\n",
        ),
        // Main's evidence: the push waited 10 s on the ssh's stdout, and still warned and went on.
        planted_after(Some(0), MAIN_EVIDENCE, "", &warning),
        planted_after(Some(0), WAIT_BOUND + Duration::from_millis(1), "", &warning),
    ] {
        assert!(judge_push(&bad).is_err());
    }
}

#[test]
fn the_planted_file_is_not_a_program() {
    let home = Home::new("planted-file");
    let file = not_a_program(&home, "bin/ssh.exe");
    let bytes = fs::read(&file).unwrap();
    assert_ne!(&bytes[..2], b"MZ");
    assert_eq!(file.extension(), Some(OsStr::new("exe")));
}

// ---------------------------------------------------------------------------------------------
// The process's own error mode: the program's `main` MUST set `SEM_FAILCRITICALERRORS |
// SEM_NOOPENFILEERRORBOX` before it starts or opens anything (#0225, D81; the rule is stated in
// docs/operator.md, Programs it starts, and docs/keeper.md, Streams and environment). A test of a
// start above can pass because a person pressed OK at the dialog, or because that start is one that
// another guard already covers; the mode is read from outside, from the finished process's handle,
// and so depends on neither.
// ---------------------------------------------------------------------------------------------

/// `SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX`.
const WANTED_MODE: u32 = 0x8001;

/// `NtQueryInformationProcess`'s class `ProcessDefaultHardErrorMode`.
const PROCESS_DEFAULT_HARD_ERROR_MODE: u32 = 12;

// This module is the only code of the file that calls Windows beside the credential's, if any.
mod win {
    use super::PROCESS_DEFAULT_HARD_ERROR_MODE;
    use std::ffi::c_void;

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtQueryInformationProcess(
            process: *mut c_void,
            class: u32,
            info: *mut c_void,
            length: u32,
            returned: *mut u32,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetErrorMode(mode: u32) -> u32;
        fn GetCurrentProcess() -> *mut c_void;
    }

    /// The error mode of the process `handle` names, in `SetErrorMode`'s form, or the failing
    /// status.
    ///
    /// The kernel keeps `SEM_FAILCRITICALERRORS` inverted in the value it reports
    /// (`ProcessDefaultHardErrorMode`: bit 0 set means that a hard error is shown), as `kernel32`
    /// does when it reads and sets the mode, so the first bit is flipped here.
    pub fn error_mode_of(handle: *mut c_void) -> Result<u32, i32> {
        let mut raw = 0u32;
        let mut returned = 0u32;
        // SAFETY: `raw` is a `u32`, the size the class gives, and outlives the call; `handle` is a
        // process handle that the caller holds open.
        let status = unsafe {
            NtQueryInformationProcess(
                handle,
                PROCESS_DEFAULT_HARD_ERROR_MODE,
                (&raw mut raw).cast(),
                4,
                &raw mut returned,
            )
        };
        if status < 0 { Err(status) } else { Ok(raw ^ 1) }
    }

    /// This process's own error mode, read through its pseudo handle.
    pub fn own_error_mode() -> Result<u32, i32> {
        // SAFETY: `GetCurrentProcess` takes nothing and returns a pseudo handle that needs no close.
        error_mode_of(unsafe { GetCurrentProcess() })
    }

    /// Sets this process's error mode and returns the previous one.
    pub fn set_own_error_mode(mode: u32) -> u32 {
        // SAFETY: `SetErrorMode` takes a bit set and has no other effect than the mode.
        unsafe { SetErrorMode(mode) }
    }
}

/// Runs `cmd` at error mode 0 to its end (or `LIMIT`, when it is ended by its own handle), and
/// returns its exit code, how long it ran, and its error mode read through its handle.
fn exit_code_and_error_mode(mut cmd: Command) -> (Option<i32>, Duration, Result<u32, i32>) {
    use std::os::windows::io::AsRawHandle;
    cmd.creation_flags(CREATE_DEFAULT_ERROR_MODE)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let started = Instant::now();
    let mut child = EndOnDrop(cmd.spawn().expect("The program starts"));
    let deadline = started + LIMIT;
    let code = loop {
        match child.0.try_wait().expect("The child can be waited for") {
            Some(status) => break Some(status.code().unwrap_or(-1)),
            None if Instant::now() >= deadline => {
                let _ = child.0.kill();
                let _ = child.0.wait();
                break None;
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    };
    let elapsed = started.elapsed();
    (code, elapsed, win::error_mode_of(child.0.as_raw_handle()))
}

/// Whether the error mode read is the wanted one, or why not. Pure, so that planted values can show
/// that it rejects.
fn judge_mode(mode: Result<u32, i32>) -> Result<(), String> {
    match mode {
        Ok(WANTED_MODE) => Ok(()),
        Ok(mode) => Err(format!(
            "The error mode is {mode:#x}, not {WANTED_MODE:#x}: a start of a file that is not a program would wait at a dialog"
        )),
        Err(status) => Err(format!(
            "The error mode could not be read: status {status:#x}"
        )),
    }
}

#[test]
fn the_probe_reads_the_mode_of_a_process_that_sets_it_and_of_one_that_does_not() {
    // This process's own mode: set to the wanted one, read back, and put back as it was.
    let before = win::set_own_error_mode(WANTED_MODE);
    let own = win::own_error_mode();
    win::set_own_error_mode(before);
    assert_eq!(own, Ok(WANTED_MODE));

    // A process that sets nothing, started at error mode 0, reads as 0.
    let home = Home::new("calibration");
    let root = std::env::var_os("SystemRoot").expect("SystemRoot is set");
    let mut cmd = Command::new(Path::new(&root).join("System32").join("cmd.exe"));
    cmd.args(["/d", "/c", "exit 0"])
        .current_dir(home.folder("work"));
    let (code, _, mode) = exit_code_and_error_mode(cmd);
    assert_eq!((code, mode), (Some(0), Ok(0)));
}

#[test]
fn the_mode_judge_rejects_planted_values() {
    assert_eq!(judge_mode(Ok(0x8001)), Ok(()));
    for bad in [Ok(0), Ok(0x8000), Ok(1), Ok(0x8003), Err(-1)] {
        assert!(judge_mode(bad).is_err());
    }
}

#[test]
fn the_operator_sets_its_error_mode_first() {
    let home = Home::new("mode");
    // No command is a usage error: it starts nothing and reads nothing (operator.md, Command line).
    let (code, elapsed, mode) = exit_code_and_error_mode(operator(&home, &[]));
    if let Err(reason) = judge_timing(code, elapsed) {
        panic!("{reason}");
    }
    assert_eq!(code, Some(1));
    if let Err(reason) = judge_mode(mode) {
        panic!("{reason}");
    }
}
