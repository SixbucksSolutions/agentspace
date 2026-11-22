//! Black-box tests of what the ProxyCommand's token push does with its ssh's stderr, and of when
//! the push is due, written from docs/operator.md alone: "The token push" (When it runs, The
//! stamp, What the laptop sends, step 2), "Every ssh runs the ProxyCommand", "What a caller sees"
//! and the `AGENTSPACE_DIAGNOSTIC` row of "Environment variables" (#0167; P14, architect,
//! 2026-11-21, for v0.08-T01; built by E0.08-T02-S04, #0176).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Windows only.
//!
//! Every run happens in a throwaway home under the temp folder. The AWS CLI and ssh are stand-in
//! `.cmd` scripts named through the test seam. The ssh stand-in reads its stdin to the end, copies
//! a file of chosen bytes to its stderr as they are, and answers as the box does or does not. Every
//! value is a dummy, nothing reaches a real host or real AWS, and a variable's value is never
//! printed.

#![cfg(windows)]

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

const ID_A: &str = "i-1234567890abcdef0";
const ID_OTHER: &str = "i-fedcba9876543210f";

const STARTING: &str = "Starting the host...";
const READY: &str = "The host is ready.";
const HOST_NOT_CONFIRMED: &str = "agentspace: The host did not confirm the localswim token; the board will not work on the box this session.";

const PUSH_TOKEN: &str = "Dummy_proxy_test_token_0123456789abcdefABCD";
const PUSH_URL: &str = "http://127.0.0.1:1";
const PUSH_ZONE: &str = "America/New_York";

// ---------------------------------------------------------------------------------------------
// The throwaway home and the stand-ins
// ---------------------------------------------------------------------------------------------

struct Sandbox {
    root: PathBuf,
    home: PathBuf,
    localapp: PathBuf,
    roaming: PathBuf,
    tmp: PathBuf,
    work: PathBuf,
    standins: PathBuf,
}

impl Sandbox {
    fn new() -> Sandbox {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "agentspace-ssh-proxy-callerstderr-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        let sb = Sandbox {
            home: root.join("home"),
            localapp: root.join("localapp"),
            roaming: root.join("roaming"),
            tmp: root.join("tmp"),
            work: root.join("work"),
            standins: root.join("stand ins"),
            root,
        };
        for d in [
            &sb.home,
            &sb.localapp,
            &sb.roaming,
            &sb.tmp,
            &sb.work,
            &sb.standins,
        ] {
            fs::create_dir_all(d).expect("A throwaway folder");
        }
        sb
    }

    fn state_dir(&self) -> PathBuf {
        self.home.join(".agentspace")
    }

    fn stamp_path(&self) -> PathBuf {
        self.state_dir().join("localswim-pushed.json")
    }

    fn ssh_log(&self) -> PathBuf {
        self.standins.join("ssh.log")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn line(s: &mut String, t: &str) {
    s.push_str(t);
    s.push_str("\r\n");
}

/// A stand-in AWS CLI that answers the calls `up`'s steps make for a host in `state`, and ends the
/// tunnel at once.
fn aws_script(state: &str) -> String {
    let mut s = String::new();
    line(&mut s, "@echo off");
    line(&mut s, "setlocal EnableExtensions DisableDelayedExpansion");
    line(&mut s, "set \"D=%~dp0\"");
    line(&mut s, ">>\"%D%calls.log\" echo %*");
    line(
        &mut s,
        "if \"%~1\"==\"ssm\" if \"%~2\"==\"start-session\" exit /b 0",
    );
    for (svc, op, label) in [
        ("ec2", "describe-instances", "describe"),
        ("ec2", "start-instances", "start"),
        ("ec2", "wait", "wait"),
        ("ssm", "describe-instance-information", "ssminfo"),
        ("ssm", "send-command", "send"),
        ("ssm", "get-command-invocation", "invocation"),
    ] {
        line(
            &mut s,
            &format!("if \"%~1\"==\"{svc}\" if \"%~2\"==\"{op}\" goto {label}"),
        );
    }
    line(&mut s, "1>&2 echo UNEXPECTED-CALL");
    line(&mut s, "exit /b 9");
    line(&mut s, ":describe");
    line(
        &mut s,
        &format!(
            "echo {{\"Reservations\":[{{\"Instances\":[{{\"State\":{{\"Name\":\"{state}\"}},\"LaunchTime\":\"2020-01-01T00:00:00+00:00\"}}]}}]}}"
        ),
    );
    line(&mut s, "exit /b 0");
    line(&mut s, ":start");
    line(
        &mut s,
        "echo {\"StartingInstances\":[{\"InstanceId\":\"STDOUT-MARKER-START\"}]}",
    );
    line(&mut s, "exit /b 0");
    line(&mut s, ":wait");
    line(&mut s, "exit /b 0");
    line(&mut s, ":ssminfo");
    line(
        &mut s,
        "echo {\"InstanceInformationList\":[{\"PingStatus\":\"Online\"}]}",
    );
    line(&mut s, "exit /b 0");
    line(&mut s, ":send");
    line(
        &mut s,
        "echo {\"Command\":{\"CommandId\":\"11111111-1111-4111-8111-111111111111\"}}",
    );
    line(&mut s, "exit /b 0");
    line(&mut s, ":invocation");
    line(
        &mut s,
        "echo {\"Status\":\"Success\",\"StandardOutputContent\":\"patched\\n\"}",
    );
    line(&mut s, "exit /b 0");
    s
}

fn push_content() -> String {
    format!("LOCALSWIM_URL={PUSH_URL}\nLOCALSWIM_TOKEN={PUSH_TOKEN}\n")
}

fn hex(bytes: &[u8], upper: bool) -> String {
    bytes
        .iter()
        .map(|b| {
            if upper {
                format!("{b:02X}")
            } else {
                format!("{b:02x}")
            }
        })
        .collect()
}

/// What the box prints for a confirmed push: the SHA-256 of the content it installed.
fn content_hash() -> String {
    hex(&Sha256::digest(push_content().as_bytes()), false)
}

/// A stamp file over `<instance ID> LF <zone> LF <content>` (operator.md, The stamp).
fn stamp_file(instance_id: &str) -> String {
    let input = format!("{instance_id}\n{PUSH_ZONE}\n{}", push_content());
    format!(
        "{{\"version\":0,\"stamp\":\"{}\"}}",
        hex(&Sha256::digest(input.as_bytes()), true)
    )
}

/// The one module of this file that calls Windows: a dummy credential protected for the current
/// user with `CryptProtectData`, as operator.md, The test seam, says a test writes one.
mod win_calls {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
    };

    pub fn protect(plain: &[u8]) -> Vec<u8> {
        let input = CRYPT_INTEGER_BLOB {
            cbData: plain.len() as u32,
            pbData: plain.as_ptr() as *mut u8,
        };
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: `input` points at `plain`, which outlives the call; `out` is written by the call
        // and freed with `LocalFree`, as the API requires.
        unsafe {
            let ok = CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            );
            assert!(ok != 0 && !out.pbData.is_null(), "CryptProtectData failed");
            let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData.cast());
            bytes
        }
    }
}

/// A receipt and a dummy Bot credential in the throwaway `LOCALAPPDATA`.
fn write_localswim(sb: &Sandbox) {
    let swim = sb.localapp.join("localswim-v2");
    let creds = swim.join("credentials").join("dummy");
    fs::create_dir_all(&creds).unwrap();
    let dir = creds.to_string_lossy().replace('\\', "\\\\");
    fs::write(
        swim.join("service.json"),
        format!("{{\"base\":\"{PUSH_URL}\",\"credential_directory\":\"{dir}\"}}"),
    )
    .unwrap();
    let token: Vec<u8> = PUSH_TOKEN
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let blob = hex(&win_calls::protect(&token), false);
    fs::write(
        creds.join("bot.json"),
        format!(
            "{{\"format\":\"localswim-credential\",\"format_version\":1,\"role\":\"bot\",\"token_dpapi_hex\":\"{blob}\"}}"
        ),
    )
    .unwrap();
}

/// The ssh stand-in: logs its arguments, reads its stdin to the end, copies `stderr.bin` beside it
/// to its stderr byte for byte, and then answers as the box does, or exits with a failure code
/// and no hash line.
///
/// `type` reads a file that begins with the bytes `FF FE` as UTF-16, so no case starts its bytes so.
fn write_ssh(sb: &Sandbox, stderr_bytes: &[u8], confirms: bool) {
    fs::write(sb.standins.join("stderr.bin"), stderr_bytes).unwrap();
    let mut s = String::new();
    line(&mut s, "@echo off");
    line(&mut s, ">>\"%~dp0ssh.log\" echo %*");
    line(&mut s, "findstr \"^\" >nul 2>nul");
    line(&mut s, "type \"%~dp0stderr.bin\" 1>&2");
    if confirms {
        line(&mut s, &format!("echo {}  -", content_hash()));
        line(&mut s, &format!("echo zone {PUSH_ZONE}"));
        line(&mut s, "exit /b 0");
    } else {
        line(&mut s, "exit /b 255");
    }
    fs::write(sb.standins.join("ssh.cmd"), s).unwrap();
}

// ---------------------------------------------------------------------------------------------
// A run
// ---------------------------------------------------------------------------------------------

/// What is in the stamp file as a run starts.
#[derive(Clone)]
enum Stamp {
    Absent,
    Bytes(Vec<u8>),
}

struct Case {
    /// The state the host's instance is in.
    state: &'static str,
    stderr_bytes: Vec<u8>,
    confirms: bool,
    stamp: Stamp,
    diagnostic: Option<&'static str>,
    nested: bool,
}

impl Case {
    /// A running host, a push that is due (no stamp) and that the box confirms, and an ssh that
    /// writes nothing to its stderr.
    fn new() -> Case {
        Case {
            state: "running",
            stderr_bytes: Vec::new(),
            confirms: true,
            stamp: Stamp::Absent,
            diagnostic: None,
            nested: false,
        }
    }
    fn state(mut self, state: &'static str) -> Case {
        self.state = state;
        self
    }
    fn stderr(mut self, bytes: &[u8]) -> Case {
        self.stderr_bytes = bytes.to_vec();
        self
    }
    fn fails(mut self) -> Case {
        self.confirms = false;
        self
    }
    fn stamp(mut self, stamp: Stamp) -> Case {
        self.stamp = stamp;
        self
    }
    fn current_stamp(self) -> Case {
        self.stamp(Stamp::Bytes(stamp_file(ID_A).into_bytes()))
    }
    fn diagnostic(mut self, value: &'static str) -> Case {
        self.diagnostic = Some(value);
        self
    }
    fn nested(mut self) -> Case {
        self.nested = true;
        self
    }
}

struct Out {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    sb: Sandbox,
}

impl Out {
    fn ssh_calls(&self) -> usize {
        fs::read_to_string(self.sb.ssh_log())
            .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0)
    }

    fn aws_calls(&self) -> Vec<String> {
        fs::read_to_string(self.sb.standins.join("calls.log"))
            .map(|t| {
                t.lines()
                    .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
                    .filter(|l| !l.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn stamp(&self) -> Option<Vec<u8>> {
        fs::read(self.sb.stamp_path()).ok()
    }

    /// The stderr as text, for a message only.
    fn shown(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

fn run(case: Case) -> Out {
    let sb = Sandbox::new();
    fs::write(sb.standins.join("aws.cmd"), aws_script(case.state)).unwrap();
    write_ssh(&sb, &case.stderr_bytes, case.confirms);
    write_localswim(&sb);
    fs::create_dir_all(sb.state_dir()).unwrap();
    fs::write(
        sb.state_dir().join("hosts.json"),
        format!(
            "{{\"version\":0,\"hosts\":[{{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":\"{ID_A}\"}}]}}\n"
        ),
    )
    .unwrap();
    if let Stamp::Bytes(bytes) = &case.stamp {
        fs::write(sb.stamp_path(), bytes).unwrap();
    }

    let root = PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot is set"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_agentspace-ssh-proxy"));
    cmd.env_clear()
        .current_dir(&sb.work)
        .env("SystemRoot", &root)
        .env("windir", &root)
        .env("ComSpec", root.join("System32").join("cmd.exe"))
        .env(
            "PATH",
            format!("{};{}", root.join("System32").display(), root.display()),
        )
        .env("TEMP", &sb.tmp)
        .env("TMP", &sb.tmp)
        .env("USERPROFILE", &sb.home)
        .env("HOME", &sb.home)
        .env("LOCALAPPDATA", &sb.localapp)
        .env("APPDATA", &sb.roaming)
        .env("AGENTSPACE_AWS_CMD", sb.standins.join("aws.cmd"))
        .env("AGENTSPACE_SSH_CMD", sb.standins.join("ssh.cmd"))
        .env("AGENTSPACE_SCP_CMD", sb.standins.join("scp.cmd"))
        .env("AGENTSPACE_TIMEZONE", PUSH_ZONE)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(value) = case.diagnostic {
        cmd.env("AGENTSPACE_DIAGNOSTIC", value);
    }
    if case.nested {
        cmd.env("AGENTSPACE_NESTED", "1");
    }
    let mut child = cmd.spawn().expect("Start agentspace-ssh-proxy");

    let out_buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let err_buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let readers: Vec<_> = [
        (
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
            out_buf.clone(),
        ),
        (
            Box::new(child.stderr.take().unwrap()) as Box<dyn Read + Send>,
            err_buf.clone(),
        ),
    ]
    .into_iter()
    .map(|(mut r, buf)| {
        thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match r.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => buf.lock().unwrap().extend_from_slice(&chunk[..n]),
                }
            }
        })
    })
    .collect();

    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(st) = child.try_wait().expect("Wait") {
            break Some(st);
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(Duration::from_millis(20));
    };
    for r in readers {
        let _ = r.join();
    }
    assert!(
        status.is_some(),
        "The program did not finish within 120 seconds"
    );
    let stdout = out_buf.lock().unwrap().clone();
    let stderr = err_buf.lock().unwrap().clone();
    Out {
        code: status.and_then(|s| s.code()),
        stdout,
        stderr,
        sb,
    }
}

/// The ProxyCommand's own `The host is ready.`, with its line feed.
fn own_ready() -> Vec<u8> {
    format!("{READY}\n").into_bytes()
}

fn concat(parts: &[&[u8]]) -> Vec<u8> {
    parts.iter().flat_map(|p| p.iter().copied()).collect()
}

/// Whether `needle` is in `haystack` as one run of bytes.
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || haystack.windows(needle.len()).any(|w| w == needle)
}

/// How many times `needle` is in `haystack`, not overlapping.
fn count_bytes(haystack: &[u8], needle: &[u8]) -> usize {
    let (mut n, mut i) = (0, 0);
    while i + needle.len() <= haystack.len() {
        if &haystack[i..i + needle.len()] == needle {
            n += 1;
            i += needle.len();
        } else {
            i += 1;
        }
    }
    n
}

/// A good run: exit 0, nothing on stdout, and exactly one push ssh.
fn assert_good(o: &Out) {
    assert_eq!(o.code, Some(0), "The exit code; stderr:\n{}", o.shown());
    assert!(
        o.stdout.is_empty(),
        "stdout MUST hold nothing but the tunnel's bytes, and held {} bytes",
        o.stdout.len()
    );
}

/// Stderr is exactly these bytes.
fn assert_stderr_bytes(o: &Out, want: &[u8]) {
    assert!(
        o.stderr == want,
        "Stderr was not exactly {want:?}; it was {:?}",
        o.stderr
    );
}

// ---------------------------------------------------------------------------------------------
// The nested connection's ready line is dropped, and every other line passes on
// ---------------------------------------------------------------------------------------------

#[test]
fn the_ready_line_of_the_pushs_ssh_is_dropped_and_the_other_lines_pass_on_in_order() {
    let ssh_err = b"Warning: Permanently added 'host' (ED25519) to the list of known hosts.\nThe host is ready.\nA banner line\n";
    let o = run(Case::new().stderr(ssh_err));
    assert_good(&o);
    assert_eq!(o.ssh_calls(), 1, "The push ran");
    assert_eq!(
        o.stamp(),
        Some(stamp_file(ID_A).into_bytes()),
        "The push was confirmed"
    );
    // The connection's own `The host is ready.` first, then the ssh's other lines, in order.
    assert_stderr_bytes(
        &o,
        &concat(&[
            &own_ready(),
            b"Warning: Permanently added 'host' (ED25519) to the list of known hosts.\nA banner line\n",
        ]),
    );
}

#[test]
fn a_connection_prints_the_ready_line_once_whether_or_not_it_pushes() {
    let pushing = run(Case::new().stderr(b"The host is ready.\n"));
    assert_good(&pushing);
    assert_eq!(pushing.ssh_calls(), 1);
    assert_stderr_bytes(&pushing, &own_ready());

    let idle = run(Case::new().current_stamp());
    assert_good(&idle);
    assert_eq!(idle.ssh_calls(), 0);
    assert_stderr_bytes(&idle, &own_ready());
}

#[test]
fn every_line_that_is_exactly_the_ready_line_is_dropped() {
    let o = run(Case::new()
        .stderr(b"The host is ready.\nkept\nThe host is ready.\r\nThe host is ready.\nkept too\n"));
    assert_good(&o);
    assert_stderr_bytes(&o, &concat(&[&own_ready(), b"kept\nkept too\n"]));
}

#[test]
fn lines_pass_on_with_the_line_ending_they_came_with() {
    // LF, CR LF, an empty line of each kind, and a last line with no ending at all.
    let ssh_err = b"one\r\ntwo\n\r\n\nThe host is ready.\r\nthree\r\nlast with none";
    let o = run(Case::new().stderr(ssh_err));
    assert_good(&o);
    assert_stderr_bytes(
        &o,
        &concat(&[&own_ready(), b"one\r\ntwo\n\r\n\nthree\r\nlast with none"]),
    );
}

#[test]
fn a_dropped_ready_line_takes_its_own_line_ending_with_it() {
    for ending in ["\n", "\r\n"] {
        let ssh_err = format!("before\n{READY}{ending}after\n");
        let o = run(Case::new().stderr(ssh_err.as_bytes()));
        assert_good(&o);
        assert_stderr_bytes(&o, &concat(&[&own_ready(), b"before\nafter\n"]));
    }
}

#[test]
fn a_last_line_that_is_exactly_the_ready_line_with_no_ending_is_dropped() {
    let o = run(Case::new().stderr(b"before\nThe host is ready."));
    assert_good(&o);
    assert_stderr_bytes(&o, &concat(&[&own_ready(), b"before\n"]));
}

#[test]
fn a_last_line_with_no_ending_gets_none_added() {
    let o = run(Case::new().stderr(b"first\nlast line, no ending"));
    assert_good(&o);
    assert_stderr_bytes(&o, &concat(&[&own_ready(), b"first\nlast line, no ending"]));
}

#[test]
fn a_line_that_is_not_exactly_the_ready_line_is_kept() {
    // Each of these differs from `The host is ready.` by one byte, or is one more line.
    let near: [&[u8]; 12] = [
        b"The host is ready. \n",
        b"The host is ready. \r\n",
        b" The host is ready.\n",
        b"\tThe host is ready.\n",
        b"the host is ready.\n",
        b"The Host is ready.\n",
        b"The host is ready\n",
        b"The host is ready..\n",
        b"The host is ready.x\n",
        b"x The host is ready.\n",
        // A CR that is not part of a CR LF is not a line ending, so it is part of the line.
        b"The host is ready.\r\r\n",
        b"The host is ready.\xff\n",
    ];
    for kept in near {
        let ssh_err = concat(&[b"top\n", kept, b"The host is ready.\n", b"bottom\n"]);
        let o = run(Case::new().stderr(&ssh_err));
        assert_good(&o);
        assert_stderr_bytes(&o, &concat(&[&own_ready(), b"top\n", kept, b"bottom\n"]));
    }
}

#[test]
fn a_last_line_that_ends_in_a_lone_carriage_return_is_not_the_ready_line() {
    let o = run(Case::new().stderr(b"top\nThe host is ready.\r"));
    assert_good(&o);
    assert_stderr_bytes(&o, &concat(&[&own_ready(), b"top\nThe host is ready.\r"]));
}

#[test]
fn bytes_that_are_not_utf8_pass_on_unchanged() {
    // Invalid UTF-8 in several forms, a NUL, high bytes, and a line whose lossy decoding would
    // differ from its bytes; none of it may be decoded, replaced or dropped.
    let ssh_err: &[u8] =
        b"first line\r\n\xff\xfe\xc3\x28 raw\r\nThe host is ready.\n\xc0\xaf\x80\x81\r\nnul\x00inside\n\xed\xa0\x80\nend\xe2\x82";
    let o = run(Case::new().stderr(ssh_err));
    assert_good(&o);
    assert_stderr_bytes(
        &o,
        &concat(&[
            &own_ready(),
            b"first line\r\n\xff\xfe\xc3\x28 raw\r\n\xc0\xaf\x80\x81\r\nnul\x00inside\n\xed\xa0\x80\nend\xe2\x82",
        ]),
    );
}

#[test]
fn a_lot_of_stderr_never_stops_the_ssh_and_arrives_whole_with_its_last_lines() {
    // Far more than a pipe holds, so a reader that waits for the ssh's exit would stop the ssh;
    // the last lines are written just before the ssh exits, so a reader that stops at the exit
    // would lose them (operator.md: "a failed push's last error lines are not lost to a race with
    // its exit").
    let mut ssh_err = Vec::new();
    let mut want = own_ready();
    for n in 0..3000 {
        let text = format!("filler line {n:05} {}\r\n", "x".repeat(70));
        ssh_err.extend_from_slice(text.as_bytes());
        want.extend_from_slice(text.as_bytes());
        if n % 500 == 0 {
            ssh_err.extend_from_slice(b"The host is ready.\r\n");
        }
    }
    ssh_err.extend_from_slice(b"The very last line, no ending");
    want.extend_from_slice(b"The very last line, no ending");
    let o = run(Case::new().stderr(&ssh_err));
    assert_good(&o);
    assert_eq!(o.ssh_calls(), 1);
    assert_eq!(
        o.stamp(),
        Some(stamp_file(ID_A).into_bytes()),
        "The push was confirmed, so the ssh was not stopped by a full pipe"
    );
    assert_eq!(o.stderr.len(), want.len(), "The count of bytes on stderr");
    assert_stderr_bytes(&o, &want);
}

// ---------------------------------------------------------------------------------------------
// AGENTSPACE_DIAGNOSTIC: nothing is dropped
// ---------------------------------------------------------------------------------------------

#[test]
fn with_diagnostic_set_every_line_of_the_ssh_is_kept_the_ready_line_included() {
    let ssh_err: &[u8] = b"Warning: Permanently added 'host' (ED25519) to the list of known hosts.\nThe host is ready.\r\nA banner line\nThe host is ready.";
    let o = run(Case::new().stderr(ssh_err).diagnostic("1"));
    assert_good(&o);
    assert_eq!(o.ssh_calls(), 1);
    // The detail MAY add lines, so the ssh's bytes are looked for whole, once, in order.
    assert!(
        contains_bytes(&o.stderr, ssh_err),
        "The ssh's stderr was not kept whole; stderr:\n{}",
        o.shown()
    );
    // The connection's own line, and the ssh's two: three in all.
    assert_eq!(
        count_bytes(&o.stderr, READY.as_bytes()),
        3,
        "Every `The host is ready.` is kept; stderr:\n{}",
        o.shown()
    );
}

#[test]
fn with_diagnostic_set_the_bytes_and_endings_still_pass_unchanged() {
    let ssh_err: &[u8] = b"first\n\xff\xfe raw\r\nThe host is ready. \nno ending";
    let o = run(Case::new().stderr(ssh_err).diagnostic("yes"));
    assert_good(&o);
    assert!(
        contains_bytes(&o.stderr, ssh_err),
        "The ssh's stderr was not kept whole"
    );
}

#[test]
fn an_empty_diagnostic_variable_drops_the_ready_line_like_an_unset_one() {
    let o = run(Case::new()
        .stderr(b"keep\nThe host is ready.\nkeep too\n")
        .diagnostic(""));
    assert_good(&o);
    assert_stderr_bytes(&o, &concat(&[&own_ready(), b"keep\nkeep too\n"]));
}

// ---------------------------------------------------------------------------------------------
// A push that fails still shows its ssh's error lines
// ---------------------------------------------------------------------------------------------

#[test]
fn a_failing_push_still_shows_its_ssh_error_lines_and_its_warning() {
    let ssh_err: &[u8] = b"ssh: connect to host dummy port 22: Connection refused\r\nThe host is ready.\r\nssh: a second error line\r\n";
    let o = run(Case::new().stderr(ssh_err).fails());
    assert_eq!(
        o.code,
        Some(0),
        "The push never fails the ProxyCommand; stderr:\n{}",
        o.shown()
    );
    assert!(o.stdout.is_empty());
    assert_eq!(o.ssh_calls(), 1);
    assert_stderr_bytes(
        &o,
        &concat(&[
            &own_ready(),
            b"ssh: connect to host dummy port 22: Connection refused\r\nssh: a second error line\r\n",
            format!("{HOST_NOT_CONFIRMED}\n").as_bytes(),
        ]),
    );
    assert_eq!(
        o.stamp(),
        None,
        "A push that is not confirmed writes no stamp"
    );
}

#[test]
fn a_failing_push_that_ends_on_a_line_with_no_ending_loses_none_of_it() {
    let o = run(Case::new()
        .stderr(b"first error\nsecond error, no ending")
        .fails());
    assert_eq!(o.code, Some(0));
    assert!(
        contains_bytes(&o.stderr, b"first error\nsecond error, no ending"),
        "The error lines were lost; stderr:\n{}",
        o.shown()
    );
    assert!(contains_bytes(&o.stderr, HOST_NOT_CONFIRMED.as_bytes()));
}

#[test]
fn a_failing_push_that_writes_only_the_ready_line_shows_no_second_ready_line() {
    let o = run(Case::new().stderr(b"The host is ready.\n").fails());
    assert_eq!(o.code, Some(0));
    assert_stderr_bytes(
        &o,
        &concat(&[&own_ready(), format!("{HOST_NOT_CONFIRMED}\n").as_bytes()]),
    );
}

// ---------------------------------------------------------------------------------------------
// When the push is due, by the stamp's rule alone; a start of the host adds nothing
// ---------------------------------------------------------------------------------------------

fn wrong_stamps() -> Vec<(&'static str, Stamp)> {
    let good = stamp_file(ID_A);
    let lower = good.to_lowercase();
    vec![
        ("no stamp", Stamp::Absent),
        ("an empty stamp file", Stamp::Bytes(Vec::new())),
        ("text that is not JSON", Stamp::Bytes(b"not json".to_vec())),
        (
            "a truncated stamp file",
            Stamp::Bytes(good.as_bytes()[..good.len() - 4].to_vec()),
        ),
        (
            "a stamp that is not in uppercase hex",
            Stamp::Bytes(lower.into_bytes()),
        ),
        (
            "a stamp over another instance",
            Stamp::Bytes(stamp_file(ID_OTHER).into_bytes()),
        ),
        (
            "a stamp file of the wrong version",
            Stamp::Bytes(good.replace("\"version\":0", "\"version\":1").into_bytes()),
        ),
    ]
}

#[test]
fn the_push_is_due_for_each_case_of_the_stamps_rule() {
    for (what, stamp) in wrong_stamps() {
        let o = run(Case::new().stderr(b"The host is ready.\n").stamp(stamp));
        assert_good(&o);
        assert_eq!(o.ssh_calls(), 1, "{what}: a push is due");
        assert_eq!(
            o.stamp(),
            Some(stamp_file(ID_A).into_bytes()),
            "{what}: the stamp comes back"
        );
        assert_stderr_bytes(&o, &own_ready());
    }
}

#[test]
fn the_push_is_not_due_when_the_stamp_is_the_computed_one() {
    let current = stamp_file(ID_A).into_bytes();
    let o = run(Case::new().stamp(Stamp::Bytes(current.clone())));
    assert_good(&o);
    assert_eq!(o.ssh_calls(), 0, "Nothing is sent");
    assert_eq!(o.stamp(), Some(current), "The stamp is as it was");
    assert_stderr_bytes(&o, &own_ready());
}

#[test]
fn a_start_of_the_host_with_a_current_stamp_is_not_due() {
    // A stopped host is started and waited for; its stamp covers the instance, the zone and the
    // content, and a start changes none of them, so nothing is sent.
    let current = stamp_file(ID_A).into_bytes();
    let o = run(Case::new()
        .state("stopped")
        .stamp(Stamp::Bytes(current.clone())));
    assert_good(&o);
    assert!(
        o.aws_calls()
            .iter()
            .any(|c| c.starts_with("ec2 start-instances")),
        "The host was started"
    );
    assert_eq!(o.ssh_calls(), 0, "A start makes no push due by itself");
    assert_eq!(o.stamp(), Some(current), "The stamp is as it was");
    assert_stderr_bytes(&o, format!("{STARTING}\n{READY}\n").as_bytes());
}

#[test]
fn a_start_of_the_host_pushes_only_when_the_stamp_says_so() {
    for (what, stamp) in wrong_stamps() {
        let o = run(Case::new()
            .state("stopped")
            .stderr(b"The host is ready.\n")
            .stamp(stamp));
        assert_good(&o);
        assert!(
            o.aws_calls()
                .iter()
                .any(|c| c.starts_with("ec2 start-instances")),
            "{what}: the host was started"
        );
        assert_eq!(o.ssh_calls(), 1, "{what}: due by the stamp, once");
        assert_stderr_bytes(&o, format!("{STARTING}\n{READY}\n").as_bytes());
    }
}

#[test]
fn a_host_that_is_already_running_is_pushed_to_by_the_same_rule() {
    let current = stamp_file(ID_A).into_bytes();
    let idle = run(Case::new().stamp(Stamp::Bytes(current)));
    assert_eq!(idle.ssh_calls(), 0);
    assert!(
        !idle
            .aws_calls()
            .iter()
            .any(|c| c.starts_with("ec2 start-instances")),
        "A running host is not started"
    );
}

#[test]
fn a_nested_proxycommand_never_pushes_so_it_passes_on_nothing() {
    let o = run(Case::new().stderr(b"The host is ready.\nnoise\n").nested());
    assert_good(&o);
    assert_eq!(o.ssh_calls(), 0, "A nested ProxyCommand starts no push");
    assert_stderr_bytes(&o, &own_ready());
}
