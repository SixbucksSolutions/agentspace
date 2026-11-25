//! The ProxyCommand's process error mode (#0225, #0226, D81; docs/operator.md, "The ProxyCommand",
//! "The token push" and "The test seam").
//!
//! The test starts the built ProxyCommand from a parent whose error mode is 0
//! (`CREATE_DEFAULT_ERROR_MODE`), with the ssh of its token push named, through the test seam, as a
//! file called `.exe` whose first bytes are not `MZ`. Windows treats such a file as a 16-bit
//! program, and a process at error mode 0 that starts one waits at a modal "Unsupported 16-Bit
//! Application" dialog for a person. The ProxyCommand MUST instead go on at once: the push warns,
//! as it does for any ssh that cannot be started, and the tunnel runs.
//!
//! The tunnel's own AWS CLI cannot be the non-program: the ProxyCommand's `up` steps call the same
//! program first, and a failure there ends the connection before any tunnel (operator.md, The
//! ProxyCommand, steps 3 and 6). The test ends the process it started by its own PID, under a
//! timeout, and works only in a throwaway home.
#![cfg(windows)]

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

/// How long the ProxyCommand has to finish. A dialog never ends by itself, so the margin only has
/// to cover a loaded machine.
const LIMIT: Duration = Duration::from_secs(40);

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

/// A stand-in AWS CLI. It answers what the ProxyCommand's start-and-wait asks of a box that is
/// running and was launched long ago, so that nothing waits, and its tunnel call leaves a marker
/// file and exits 0 (operator.md, Calls by command).
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
          if \"%1\"==\"ssm\" (\r\n  echo ran>\"%~dp0tunnel-ran.txt\"\r\n  exit /b 0\r\n)\r\n\
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

/// The ProxyCommand with no argument, so it reaches the box, ready to run in the home: its
/// environment is the home's, and no seam is set but the ones the test gives it.
fn proxy(home: &Home) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_agentspace-ssh-proxy"));
    cmd.current_dir(home.folder("work"))
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
        "AGENTSPACE_PROXY_COMMAND",
    ] {
        cmd.env_remove(name);
    }
    cmd
}

struct Outcome {
    code: Option<i32>,
    stdout: String,
    stderr: String,
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
    let mut child = EndOnDrop(cmd.spawn().expect("The ProxyCommand starts"));
    let stdout = drain(child.0.stdout.take().expect("A piped stdout"));
    let stderr = drain(child.0.stderr.take().expect("A piped stderr"));
    let deadline = Instant::now() + LIMIT;
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
    let text = |receiver: mpsc::Receiver<String>| {
        receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_default()
    };
    Outcome {
        code,
        stdout: text(stdout),
        stderr: text(stderr),
    }
}

/// The token push's own error for an ssh that cannot be started (operator.md, What the laptop
/// checks; in caller mode "What a caller sees" keeps the line as it is, since it names neither
/// "the instance" nor the backend): a warning, and the connection goes on.
const PUSH_WARNING: &str = "agentspace: Could not send the localswim token; the board will not work on the box this session.";

/// Whether the ProxyCommand went on as it MUST, or why not. Pure, so that a planted outcome can
/// show that it rejects. The tunnel's exit code, 0, is the ProxyCommand's (operator.md, The
/// ProxyCommand, Exit codes), and nothing but the tunnel writes to its stdout.
fn judge(outcome: &Outcome) -> Result<(), String> {
    let Some(code) = outcome.code else {
        return Err(format!(
            "No exit within {} s: a hard-error dialog likely",
            LIMIT.as_secs()
        ));
    };
    if code != 0 {
        return Err(format!("The exit code is {code}, not 0"));
    }
    if !outcome.stdout.is_empty() {
        return Err("The ProxyCommand wrote to stdout".to_string());
    }
    if !outcome.stderr.lines().any(|line| line == PUSH_WARNING) {
        return Err("The push's warning is not among the lines of stderr".to_string());
    }
    Ok(())
}

#[test]
fn a_push_ssh_that_is_not_a_program_warns_and_the_tunnel_runs_at_once() {
    let home = Home::new("proxy");
    let aws = aws_stand_in(&home);
    let ssh = not_a_program(&home, "bin/ssh.exe");
    localswim_stand_in(&home);
    let mut cmd = proxy(&home);
    cmd.env("AGENTSPACE_AWS_CMD", &aws)
        .env("AGENTSPACE_SSH_CMD", &ssh)
        .env("AGENTSPACE_TIMEZONE", "America/New_York");
    let outcome = run_at_error_mode_0(cmd);
    if let Err(reason) = judge(&outcome) {
        panic!(
            "{reason}\nexit: {:?}\nstdout: {}\nstderr: {}",
            outcome.code,
            home.mask(&outcome.stdout),
            home.mask(&outcome.stderr)
        );
    }
    // The connection went on to its tunnel after the push's warning, and the host was ready.
    assert!(
        home.path("bin/tunnel-ran.txt").is_file(),
        "The tunnel did not run:\n{}",
        home.mask(&outcome.stderr)
    );
    assert!(
        outcome
            .stderr
            .lines()
            .any(|line| line == "The host is ready."),
        "The start-and-wait did not finish:\n{}",
        home.mask(&outcome.stderr)
    );
}

// The checker is proven on planted outcomes: each one is wrong in one way, and each is rejected.

fn planted(code: Option<i32>, stdout: &str, stderr: &str) -> Outcome {
    Outcome {
        code,
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    }
}

#[test]
fn the_judge_rejects_planted_outcomes() {
    let warning = format!("The host is ready.\n{PUSH_WARNING}\n");
    assert_eq!(judge(&planted(Some(0), "", &warning)), Ok(()));
    for bad in [
        planted(None, "", &warning),
        planted(Some(1), "", &warning),
        planted(Some(0), "text", &warning),
        planted(Some(0), "", "The host is ready.\n"),
        planted(
            Some(0),
            "",
            "agentspace: Could not send the localswim token; the board will work.\n",
        ),
    ] {
        assert!(judge(&bad).is_err());
    }
}

#[test]
fn the_planted_file_is_not_a_program() {
    let home = Home::new("planted-file");
    let file = not_a_program(&home, "bin/ssh.exe");
    let bytes = fs::read(&file).unwrap();
    assert_ne!(&bytes[..2], b"MZ");
    assert_eq!(
        file.extension().and_then(|extension| extension.to_str()),
        Some("exe")
    );
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
/// returns its exit code and its error mode read through its handle.
fn exit_code_and_error_mode(mut cmd: Command) -> (Option<i32>, Result<u32, i32>) {
    use std::os::windows::io::AsRawHandle;
    cmd.creation_flags(CREATE_DEFAULT_ERROR_MODE)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = EndOnDrop(cmd.spawn().expect("The program starts"));
    let deadline = Instant::now() + LIMIT;
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
    (code, win::error_mode_of(child.0.as_raw_handle()))
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
    assert_eq!(exit_code_and_error_mode(cmd), (Some(0), Ok(0)));
}

#[test]
fn the_mode_judge_rejects_planted_values() {
    assert_eq!(judge_mode(Ok(0x8001)), Ok(()));
    for bad in [Ok(0), Ok(0x8000), Ok(1), Ok(0x8003), Err(-1)] {
        assert!(judge_mode(bad).is_err());
    }
}

#[test]
fn the_proxy_command_sets_its_error_mode_first() {
    let home = Home::new("mode");
    // A second argument is refused before anything starts or is read (operator.md, The
    // ProxyCommand, step 1): exit 1.
    let mut cmd = proxy(&home);
    cmd.args(["first", "second"]);
    let (code, mode) = exit_code_and_error_mode(cmd);
    assert_eq!(code, Some(1));
    if let Err(reason) = judge_mode(mode) {
        panic!("{reason}");
    }
}
