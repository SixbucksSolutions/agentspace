//! Black-box tests of `agentspace-ssh-proxy`, written from docs/operator.md alone
//! ("The ProxyCommand", "What a caller sees", "The test seam", "Streams", "Exit codes").
//!
//! Every run happens in a throwaway home under the temp folder. The AWS CLI, ssh and scp are
//! stand-in `.cmd` scripts named through the test seam, and nothing reaches a real box or
//! real AWS. Environment values the stand-ins capture are asserted by name and never printed.

#![cfg(windows)]

use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

const ID_A: &str = "i-1234567890abcdef0";
const ID_B: &str = "i-fedcba9876543210f";

const SIGN_IN: &str = "agentspace: AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry.";
const COULD_NOT_REACH: &str = "agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.";
const NOT_AVAILABLE: &str = "agentspace: The host is not available, and could not be started.";
const CONN_FAILED: &str = "agentspace: The connection to the host failed. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.";
const BAD_ALIAS: &str = "agentspace: -Alias MUST be an SSH host alias of letters, digits, '.', '_' and '-' that does not start with '-'.";
const TAKES_ONE: &str =
    "agentspace: agentspace-ssh-proxy.exe takes at most one argument, the SSH host alias.";

const STARTING: &str = "Starting the host...";
const WAITING: &str = "Waiting for the host to be ready...";
const READY: &str = "The host is ready.";
const PROGRESS: [&str; 3] = [STARTING, WAITING, READY];

const SM_PREFIX: &str =
    "C:\\Program Files\\Amazon\\SessionManagerPlugin\\bin;C:\\Program Files\\Amazon\\AWSCLIV2;";

const LONG_AGO: &str = "2020-01-01T00:00:00+00:00";
const FUTURE: &str = "2999-01-01T00:00:00+00:00";
/// A time in the lease state file's form (state.md, The lease state file).
const LEASE_FUTURE: &str = "2999-01-01T00:00:00Z";

const POSIX_A: &str = "1A2B3C4D-5E6F-4A8B-9C0D-1E2F3A4B5C6D_POSIX_FD_STATE";
const POSIX_B: &str = "FFEEDDCC-BBAA-4099-8877-665544332211_POSIX_FD_STATE";

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
    fn new(name: &str, spaced: bool) -> Sandbox {
        let root = std::env::temp_dir().join(format!(
            "agentspace-ssh-proxy-tests-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let sb = Sandbox {
            home: root.join("home"),
            localapp: root.join("localapp"),
            roaming: root.join("roaming"),
            tmp: root.join("tmp"),
            work: root.join("work"),
            standins: root.join(if spaced { "stand ins" } else { "standins" }),
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
            fs::create_dir_all(d).expect("create a throwaway folder");
        }
        sb
    }

    fn state_dir(&self) -> PathBuf {
        self.home.join(".agentspace")
    }

    /// The host registry, `hosts.json`, which holds each host's instance ID from v0.07 on
    /// (operator.md, The host registry, as the operator uses it; state.md, The instance ID).
    fn cache_path(&self) -> PathBuf {
        self.hosts_path()
    }

    fn hosts_path(&self) -> PathBuf {
        self.state_dir().join("hosts.json")
    }

    /// The v0.06 instance-ID cache, retired by v0.07: nothing reads it (state.md).
    fn retired_cache_path(&self) -> PathBuf {
        self.state_dir().join("instance-id.json")
    }

    fn leases_path(&self) -> PathBuf {
        self.state_dir().join("leases.json")
    }

    /// A host's folder, `hosts\<project>` (state.md, A host's push stamp).
    fn host_folder(&self, project: &str) -> PathBuf {
        self.state_dir().join("hosts").join(project)
    }

    fn box_stamp_path(&self) -> PathBuf {
        self.state_dir().join("localswim-pushed.json")
    }

    fn host_stamp_path(&self, project: &str) -> PathBuf {
        self.host_folder(project).join("localswim-pushed.json")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // A test may have made the registry read-only.
        if let Ok(meta) = fs::metadata(self.hosts_path()) {
            let mut perms = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            let _ = fs::set_permissions(self.hosts_path(), perms);
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Tunnel {
    /// Exits with the code and writes nothing to stdout.
    Silent,
    /// Copies its stdin to its stdout, then exits with the code.
    Echo,
    /// Writes one line to stdout, then exits with the code.
    Payload,
}

#[derive(Clone)]
struct Fail {
    service: &'static str,
    op: &'static str,
    code: i32,
    stderr: &'static str,
}

#[derive(Clone)]
struct Aws {
    state: &'static str,
    launch: &'static str,
    fail: Option<Fail>,
    /// How many `ssm describe-instance-information` answers say the agent is not online.
    ssm_offline: u32,
    /// How many `ssm get-command-invocation` answers say `pending`.
    patch_pending: u32,
    stack_json: Option<String>,
    tunnel: Tunnel,
    tunnel_code: i32,
    tunnel_stderr: bool,
    noise: bool,
    wait_code: i32,
}

impl Default for Aws {
    fn default() -> Aws {
        Aws {
            state: "running",
            launch: LONG_AGO,
            fail: None,
            ssm_offline: 0,
            patch_pending: 0,
            stack_json: None,
            tunnel: Tunnel::Silent,
            tunnel_code: 0,
            tunnel_stderr: false,
            noise: false,
            wait_code: 0,
        }
    }
}

const ERR_TEXT: &str = "An error occurred (AccessDenied) when calling the DescribeInstances operation: User is not authorized in us-west-2 for EC2 Session Manager CloudFormation agentbox i-1234567890abcdef0";

fn fail(service: &'static str, op: &'static str, code: i32, stderr: &'static str) -> Option<Fail> {
    Some(Fail {
        service,
        op,
        code,
        stderr,
    })
}

fn line(s: &mut String, t: &str) {
    s.push_str(t);
    s.push_str("\r\n");
}

fn aws_script(c: &Aws) -> String {
    let mut s = String::new();
    line(&mut s, "@echo off");
    line(&mut s, "setlocal EnableExtensions DisableDelayedExpansion");
    line(&mut s, "set \"D=%~dp0\"");
    line(&mut s, "set \"N=%~1-%~2\"");
    line(&mut s, ">>\"%D%calls.log\" echo %*");
    line(
        &mut s,
        ">\"%D%env-%N%.txt\" echo NESTED=%AGENTSPACE_NESTED%",
    );
    line(&mut s, ">>\"%D%env-%N%.txt\" echo PATH=%PATH%");
    line(
        &mut s,
        "for /f \"delims==\" %%V in ('set') do >>\"%D%env-%N%.txt\" echo NAME=%%V",
    );
    line(
        &mut s,
        "if \"%~1\"==\"ssm\" if \"%~2\"==\"start-session\" goto tunnel",
    );
    line(&mut s, "findstr \"^\" >\"%D%stdin-%N%.txt\" 2>nul");
    if c.noise {
        line(
            &mut s,
            "1>&2 echo NOISE EC2 notice about i-1234567890abcdef0 in us-west-2 via Session Manager",
        );
    }
    if let Some(f) = &c.fail {
        line(
            &mut s,
            &format!(
                "if \"%~1\"==\"{}\" if \"%~2\"==\"{}\" goto injected",
                f.service, f.op
            ),
        );
    }
    for (svc, op, label) in [
        ("ec2", "describe-instances", "describe"),
        ("ec2", "start-instances", "start"),
        ("ec2", "wait", "wait"),
        ("ssm", "describe-instance-information", "ssminfo"),
        ("ssm", "send-command", "send"),
        ("ssm", "get-command-invocation", "invocation"),
        ("cloudformation", "describe-stacks", "stacks"),
    ] {
        line(
            &mut s,
            &format!("if \"%~1\"==\"{svc}\" if \"%~2\"==\"{op}\" goto {label}"),
        );
    }
    line(&mut s, "1>&2 echo UNEXPECTED-CALL");
    line(&mut s, "exit /b 9");

    if let Some(f) = &c.fail {
        line(&mut s, ":injected");
        line(&mut s, &format!("1>&2 echo {}", f.stderr));
        line(&mut s, &format!("exit /b {}", f.code));
    }

    line(&mut s, ":describe");
    line(
        &mut s,
        &format!(
            "echo {{\"Reservations\":[{{\"Instances\":[{{\"State\":{{\"Name\":\"{}\"}},\"LaunchTime\":\"{}\"}}]}}]}}",
            c.state, c.launch
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
    line(&mut s, "echo WAIT-STDOUT-MARKER");
    line(&mut s, "1>&2 echo WAIT-STDERR-MARKER");
    line(&mut s, &format!("exit /b {}", c.wait_code));

    line(&mut s, ":ssminfo");
    line(&mut s, ">>\"%D%n-ssminfo.txt\" echo x");
    line(&mut s, "set K=0");
    line(
        &mut s,
        "for /f %%L in ('type \"%D%n-ssminfo.txt\"') do set /a K+=1",
    );
    line(&mut s, &format!("if %K% LEQ {} goto ssmoff", c.ssm_offline));
    line(
        &mut s,
        "echo {\"InstanceInformationList\":[{\"PingStatus\":\"Online\"}]}",
    );
    line(&mut s, "exit /b 0");
    line(&mut s, ":ssmoff");
    line(
        &mut s,
        "echo {\"InstanceInformationList\":[{\"PingStatus\":\"ConnectionLost\"}]}",
    );
    line(&mut s, "exit /b 0");

    line(&mut s, ":send");
    line(
        &mut s,
        "echo {\"Command\":{\"CommandId\":\"11111111-1111-4111-8111-111111111111\"}}",
    );
    line(&mut s, "exit /b 0");

    line(&mut s, ":invocation");
    line(&mut s, ">>\"%D%n-invocation.txt\" echo x");
    line(&mut s, "set K=0");
    line(
        &mut s,
        "for /f %%L in ('type \"%D%n-invocation.txt\"') do set /a K+=1",
    );
    line(
        &mut s,
        &format!("if %K% LEQ {} goto pending", c.patch_pending),
    );
    line(
        &mut s,
        "echo {\"Status\":\"Success\",\"StandardOutputContent\":\"patched\\n\"}",
    );
    line(&mut s, "exit /b 0");
    line(&mut s, ":pending");
    line(
        &mut s,
        "echo {\"Status\":\"Success\",\"StandardOutputContent\":\"pending\\n\"}",
    );
    line(&mut s, "exit /b 0");

    line(&mut s, ":stacks");
    match &c.stack_json {
        Some(j) => line(&mut s, &format!("echo {j}")),
        None => line(
            &mut s,
            &format!(
                "echo {{\"Stacks\":[{{\"Outputs\":[{{\"OutputKey\":\"InstanceId\",\"OutputValue\":\"{ID_B}\"}}]}}]}}"
            ),
        ),
    }
    line(&mut s, "exit /b 0");

    line(&mut s, ":tunnel");
    if c.tunnel_stderr {
        line(
            &mut s,
            "1>&2 echo TUNNEL-STDERR-MARKER i-1234567890abcdef0 SessionManagerPlugin us-west-2",
        );
    }
    match c.tunnel {
        Tunnel::Echo => {
            line(
                &mut s,
                "\"%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -NoProfile -NonInteractive -Command \"[Console]::OpenStandardInput().CopyTo([Console]::OpenStandardOutput())\"",
            );
            line(&mut s, &format!("exit /b {}", c.tunnel_code));
        }
        Tunnel::Payload => {
            line(&mut s, "echo PAYLOAD-BYTES");
            line(&mut s, &format!("exit /b {}", c.tunnel_code));
        }
        Tunnel::Silent => line(&mut s, &format!("exit /b {}", c.tunnel_code)),
    }
    s
}

fn plain_script(log: &str) -> String {
    let mut s = String::new();
    line(&mut s, "@echo off");
    line(&mut s, &format!(">>\"%~dp0{log}\" echo %*"));
    line(&mut s, "exit /b 0");
    s
}

// ---------------------------------------------------------------------------------------------
// A run
// ---------------------------------------------------------------------------------------------

/// What the host registry, `hosts.json`, holds as a case starts (the name is the v0.06 cache's).
enum Cache {
    /// A well-formed registry whose box record names the id.
    Valid(&'static str),
    /// These raw bytes as the registry file.
    Raw(Vec<u8>),
    /// A state folder with no registry file.
    Missing,
    /// No state folder at all.
    NoDir,
    /// A folder where the registry file should be.
    Folder,
}

// ---------------------------------------------------------------------------------------------
// The host registry and the lease state file, as a test writes them (state.md)
// ---------------------------------------------------------------------------------------------

/// A project that is not the box's, and the instance IDs that tell the hosts apart. None matches
/// the pattern of a real one.
const PROJ_A: &str = "orchard";
const PROJ_B: &str = "quarry";
const ID_C: &str = "i-abcdef12345678901";
const ID_D: &str = "i-1111111122222222a";

/// One host record that is not the box's: its project, its state, and its instance ID if known.
#[derive(Clone, Copy)]
struct HostRec {
    project: &'static str,
    state: &'static str,
    instance_id: Option<&'static str>,
}

fn host(project: &'static str, state: &'static str, id: Option<&'static str>) -> HostRec {
    HostRec {
        project,
        state,
        instance_id: id,
    }
}

/// The registry as agentspace writes it: compact JSON, the box's record first, each record's
/// members in the order project, stack, state, instance_id, then one line feed (state.md).
fn registry(others: &[HostRec], box_id: Option<&str>) -> String {
    let mut s = String::from("{\"version\":0,\"hosts\":[");
    s.push_str("{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"");
    if let Some(id) = box_id {
        s.push_str(&format!(",\"instance_id\":\"{id}\""));
    }
    s.push('}');
    for h in others {
        s.push_str(&format!(
            ",{{\"project\":\"{p}\",\"stack\":\"agentspace-host-{p}\",\"state\":\"{st}\"",
            p = h.project,
            st = h.state
        ));
        if let Some(id) = h.instance_id {
            s.push_str(&format!(",\"instance_id\":\"{id}\""));
        }
        s.push('}');
    }
    s.push_str("]}\n");
    s
}

/// The first lease ID's first eight digits, which end the alias (lease-api.md).
const LEASE_PREFIX: &str = "aaaaaaaa";

/// A lease alias, `agentspace-<project>-<first 8 hex digits of the id>`.
fn lease_alias(project: &str, prefix: &str) -> String {
    format!("agentspace-{project}-{prefix}")
}

/// A stored `ready` lease record, in a form the state file's schema accepts.
fn ready_lease(project: &str, prefix: &str, expires_at: &str) -> String {
    format!(
        "{{\"id\":\"{prefix}-1111-4111-8111-111111111111\",\"project\":\"{project}\",\"state\":\"ready\",\"platform\":\"linux/arm64\",\"profile\":\"base\",\"size\":\"small\",\"ttl_seconds\":3600,\"created_at\":\"2026-01-01T00:00:00Z\",\"expires_at\":\"{expires_at}\",\"ssh\":{{\"host_alias\":\"{alias}\",\"user\":\"ubuntu\"}},\"driver\":\"ec2\"}}",
        alias = lease_alias(project, prefix)
    )
}

/// A stored `released` lease record: it carries no `ssh`, so it never matches an alias.
fn released_lease(project: &str, prefix: &str) -> String {
    format!(
        "{{\"id\":\"{prefix}-1111-4111-8111-111111111111\",\"project\":\"{project}\",\"state\":\"released\",\"platform\":\"linux/arm64\",\"profile\":\"base\",\"size\":\"small\",\"ttl_seconds\":3600,\"created_at\":\"2026-01-01T00:00:00Z\",\"expires_at\":\"2026-01-01T01:00:00Z\",\"released_at\":\"2026-01-01T00:30:00Z\",\"driver\":\"ec2\"}}"
    )
}

fn lease_file(records: &[String]) -> String {
    format!("{{\"version\":0,\"leases\":[{}]}}\n", records.join(","))
}

/// A setup that writes the registry and, when given, the lease state file, in the default state
/// folder, `%USERPROFILE%\.agentspace`.
fn seed(registry_text: String, leases_text: Option<String>) -> Setup {
    Box::new(move |sb: &Sandbox| {
        fs::create_dir_all(sb.state_dir()).unwrap();
        fs::write(sb.hosts_path(), &registry_text).unwrap();
        if let Some(text) = &leases_text {
            fs::write(sb.leases_path(), text).unwrap();
        }
    })
}

enum Stdin {
    Null,
    Bytes(Vec<u8>),
    /// Writes the bytes, keeps stdin open until they come back on stdout, then closes it.
    HeldOpen(Vec<u8>),
}

type Setup = Box<dyn FnOnce(&Sandbox)>;

struct Case {
    name: String,
    args: Vec<OsString>,
    aws: Aws,
    cache: Cache,
    env: Vec<(String, String)>,
    env_remove: Vec<&'static str>,
    /// Variables set to a folder under the sandbox root, once it exists.
    env_roots: Vec<(String, String)>,
    path: Option<String>,
    stdin: Stdin,
    spaced: bool,
    aws_cmd: Option<OsString>,
    setup: Option<Setup>,
}

impl Case {
    fn new(name: &str) -> Case {
        Case {
            name: name.to_string(),
            args: Vec::new(),
            aws: Aws::default(),
            cache: Cache::Valid(ID_A),
            env: Vec::new(),
            env_remove: Vec::new(),
            env_roots: Vec::new(),
            path: None,
            stdin: Stdin::Null,
            spaced: false,
            aws_cmd: None,
            setup: None,
        }
    }
    fn args(mut self, a: &[&str]) -> Case {
        self.args = a.iter().map(OsString::from).collect();
        self
    }
    fn aws(mut self, a: Aws) -> Case {
        self.aws = a;
        self
    }
    fn cache(mut self, c: Cache) -> Case {
        self.cache = c;
        self
    }
    fn env(mut self, k: &str, v: &str) -> Case {
        self.env.push((k.to_string(), v.to_string()));
        self
    }
    fn remove(mut self, k: &'static str) -> Case {
        self.env_remove.push(k);
        self
    }
    fn stdin(mut self, s: Stdin) -> Case {
        self.stdin = s;
        self
    }
}

struct EnvRec {
    nested: String,
    path: String,
    names: Vec<String>,
}

struct Out {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    path_in: String,
    streamed_before_close: bool,
    sb: Sandbox,
}

fn sysroot() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot is set on Windows"))
}

fn write_cache(sb: &Sandbox, cache: &Cache) {
    match cache {
        Cache::NoDir => {}
        Cache::Missing => fs::create_dir_all(sb.state_dir()).unwrap(),
        Cache::Valid(id) => {
            fs::create_dir_all(sb.state_dir()).unwrap();
            fs::write(sb.hosts_path(), registry(&[], Some(id))).unwrap();
            // The v0.06 cache is retired and MUST be ignored, so it is seeded with the same ID
            // only to keep the box's path comparable with v0.06's.
            fs::write(
                sb.retired_cache_path(),
                format!("{{\"version\":0,\"instance_id\":\"{id}\"}}"),
            )
            .unwrap();
        }
        Cache::Raw(b) => {
            fs::create_dir_all(sb.state_dir()).unwrap();
            fs::write(sb.cache_path(), b).unwrap();
        }
        Cache::Folder => fs::create_dir_all(sb.cache_path()).unwrap(),
    }
}

fn run(mut c: Case) -> Out {
    let sb = Sandbox::new(&c.name, c.spaced);
    fs::write(sb.standins.join("aws.cmd"), aws_script(&c.aws)).unwrap();
    fs::write(sb.standins.join("ssh.cmd"), plain_script("ssh.log")).unwrap();
    fs::write(sb.standins.join("scp.cmd"), plain_script("scp.log")).unwrap();
    write_cache(&sb, &c.cache);
    if let Some(setup) = c.setup.take() {
        setup(&sb);
    }

    let root = sysroot();
    let path_in = c.path.clone().unwrap_or_else(|| {
        format!(
            "{};{};{}",
            root.join("System32").display(),
            root.display(),
            sb.root.join("pathmarker").display()
        )
    });

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_agentspace-ssh-proxy"));
    cmd.args(&c.args)
        .env_clear()
        .current_dir(&sb.work)
        .env("SystemRoot", &root)
        .env("windir", &root)
        .env("ComSpec", root.join("System32").join("cmd.exe"))
        .env("PATH", &path_in)
        .env("TEMP", &sb.tmp)
        .env("TMP", &sb.tmp)
        .env("USERPROFILE", &sb.home)
        .env("HOME", &sb.home)
        .env("LOCALAPPDATA", &sb.localapp)
        .env("APPDATA", &sb.roaming)
        .env(
            "AGENTSPACE_AWS_CMD",
            c.aws_cmd
                .clone()
                .unwrap_or_else(|| sb.standins.join("aws.cmd").into_os_string()),
        )
        .env("AGENTSPACE_SSH_CMD", sb.standins.join("ssh.cmd"))
        .env("AGENTSPACE_SCP_CMD", sb.standins.join("scp.cmd"))
        .env("AGENTSPACE_TIMEZONE", "America/New_York")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for r in &c.env_remove {
        cmd.env_remove(r);
    }
    for (k, v) in &c.env {
        cmd.env(k, v);
    }
    for (k, folder) in &c.env_roots {
        cmd.env(k, sb.root.join(folder));
    }
    cmd.stdin(if matches!(c.stdin, Stdin::Null) {
        Stdio::null()
    } else {
        Stdio::piped()
    });

    let mut child = cmd.spawn().expect("start agentspace-ssh-proxy");

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

    let streamed = Arc::new(Mutex::new(false));
    let writer = match c.stdin {
        Stdin::Null => None,
        Stdin::Bytes(bytes) => {
            let mut si = child.stdin.take().unwrap();
            Some(thread::spawn(move || {
                let _ = si.write_all(&bytes);
                // Dropping closes the pipe.
            }))
        }
        Stdin::HeldOpen(bytes) => {
            let mut si = child.stdin.take().unwrap();
            let out_buf = out_buf.clone();
            let streamed = streamed.clone();
            Some(thread::spawn(move || {
                let _ = si.write_all(&bytes);
                let _ = si.flush();
                let deadline = Instant::now() + Duration::from_secs(90);
                while Instant::now() < deadline {
                    if out_buf.lock().unwrap().len() >= bytes.len() {
                        *streamed.lock().unwrap() = true;
                        break;
                    }
                    thread::sleep(Duration::from_millis(25));
                }
            }))
        }
    };

    let deadline = Instant::now() + Duration::from_secs(150);
    let status = loop {
        if let Some(st) = child.try_wait().expect("wait") {
            break Some(st);
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(Duration::from_millis(20));
    };
    if let Some(w) = writer {
        let _ = w.join();
    }
    for r in readers {
        let _ = r.join();
    }
    assert!(
        status.is_some(),
        "the program did not finish within 150 seconds"
    );

    let stdout = out_buf.lock().unwrap().clone();
    let stderr = err_buf.lock().unwrap().clone();
    let streamed_before_close = *streamed.lock().unwrap();
    Out {
        code: status.and_then(|s| s.code()),
        stdout,
        stderr,
        path_in,
        streamed_before_close,
        sb,
    }
}

fn norm(s: &str) -> String {
    s.split_whitespace()
        .map(|t| t.trim_matches('"'))
        .collect::<Vec<_>>()
        .join(" ")
}

fn read_text(p: &Path) -> Option<String> {
    fs::read(p)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

impl Out {
    fn err_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
    fn err_lines(&self) -> Vec<String> {
        self.err_text().lines().map(str::to_string).collect()
    }
    fn calls(&self) -> Vec<String> {
        read_text(&self.sb.standins.join("calls.log"))
            .map(|t| t.lines().map(norm).filter(|l| !l.is_empty()).collect())
            .unwrap_or_default()
    }
    fn count(&self, prefix: &str) -> usize {
        self.calls()
            .iter()
            .filter(|c| c.starts_with(prefix))
            .count()
    }
    fn tunnel_calls(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| c.starts_with("ssm start-session"))
            .collect()
    }
    fn env_recs(&self) -> Vec<(String, EnvRec)> {
        let mut v = Vec::new();
        for e in fs::read_dir(&self.sb.standins).unwrap().flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if let Some(rest) = n.strip_prefix("env-") {
                let text = read_text(&e.path()).unwrap_or_default();
                let mut rec = EnvRec {
                    nested: String::new(),
                    path: String::new(),
                    names: Vec::new(),
                };
                for l in text.lines() {
                    let l = l.trim_end_matches('\r');
                    if let Some(x) = l.strip_prefix("NESTED=") {
                        rec.nested = x.to_string();
                    } else if let Some(x) = l.strip_prefix("PATH=") {
                        rec.path = x.to_string();
                    } else if let Some(x) = l.strip_prefix("NAME=") {
                        rec.names.push(x.to_string());
                    }
                }
                v.push((rest.trim_end_matches(".txt").to_string(), rec));
            }
        }
        v
    }
    fn stdin_files_empty(&self) -> bool {
        fs::read_dir(&self.sb.standins)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("stdin-"))
            .all(|e| fs::metadata(e.path()).map(|m| m.len() == 0).unwrap_or(true))
    }
    fn no_ssh_or_scp(&self) -> bool {
        !self.sb.standins.join("ssh.log").exists() && !self.sb.standins.join("scp.log").exists()
    }
    fn target(&self) -> Option<String> {
        let t = self.tunnel_calls();
        let toks: Vec<&str> = t.first()?.split(' ').collect();
        let i = toks.iter().position(|x| *x == "--target")?;
        toks.get(i + 1).map(|s| s.to_string())
    }
}

// ---------------------------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------------------------

/// Backend text (operator.md, "What a caller sees"). Returns the names of what was found.
fn backend_hits(sb: &Sandbox, text: &str) -> Vec<&'static str> {
    let low = text.to_lowercase();
    let mut hits = Vec::new();
    // An instance, volume or host ID.
    let b = low.as_bytes();
    for i in 0..b.len().saturating_sub(10) {
        if (b[i] == b'i' || b[i] == b'v') && b[i + 1] == b'-' {
            let n = b[i + 2..]
                .iter()
                .take_while(|c| c.is_ascii_hexdigit())
                .count();
            if n >= 8 {
                hits.push("an id");
                break;
            }
        }
    }
    for (needle, name) in [
        (ID_A, "instance id A"),
        (ID_B, "instance id B"),
        ("us-west-2", "a region"),
        ("ec2", "EC2"),
        ("aws", "AWS"),
        ("ssm", "SSM"),
        ("session manager", "Session Manager"),
        ("sessionmanager", "Session Manager plugin"),
        ("cloudformation", "CloudFormation"),
        ("agentbox", "the stack's name"),
        ("m9g", "a machine type"),
        ("ubuntu", "the host's operating system"),
        ("patch", "the patch tool"),
        ("amazon", "Amazon"),
        ("marker", "a stand-in's text"),
        ("noise", "a stand-in's text"),
        ("terminated", "a state name"),
        ("shutting-down", "a state name"),
        ("stopping", "a state name"),
        ("stopped", "a state name"),
        ("pending", "a state name"),
        ("running", "a state name"),
        ("agentspace-ssh-proxy-tests", "a path"),
    ] {
        if low.contains(needle) {
            hits.push(name);
        }
    }
    let root = sb.root.to_string_lossy().to_lowercase();
    if low.contains(&root) {
        hits.push("the throwaway root path");
    }
    let tmp = std::env::temp_dir().to_string_lossy().to_lowercase();
    if low.contains(&tmp) {
        hits.push("the temp folder's path");
    }
    hits
}

fn assert_no_backend(o: &Out, text: &str) {
    let hits = backend_hits(&o.sb, text);
    assert!(
        hits.is_empty(),
        "backend text reached the caller ({hits:?}) in:\n{text}"
    );
}

fn assert_stdout_empty(o: &Out) {
    assert!(
        o.stdout.is_empty(),
        "stdout MUST hold nothing but the tunnel's bytes, and held {} bytes",
        o.stdout.len()
    );
}

/// Stderr is exactly these lines, each ended by a line feed alone, with no backend text.
fn assert_stderr_is(o: &Out, lines: &[&str]) {
    let want: String = lines.iter().map(|l| format!("{l}\n")).collect();
    let got = o.err_text();
    assert!(
        got == want,
        "stderr was not exactly {lines:?}; it was:\n{got:?}"
    );
    if !lines.contains(&SIGN_IN) {
        assert_no_backend(o, &got);
    }
}

/// A failure: exit 1, nothing on stdout, no tunnel, the failure line last, and before it
/// only progress lines.
fn assert_failure(o: &Out, last: &str) {
    assert_eq!(
        o.code,
        Some(1),
        "a failure exits 1; stderr:\n{}",
        o.err_text()
    );
    assert_stdout_empty(o);
    assert!(
        o.tunnel_calls().is_empty(),
        "a failure MUST start no tunnel"
    );
    let lines = o.err_lines();
    assert_eq!(
        lines.last().map(String::as_str),
        Some(last),
        "the last line is the failure's; stderr:\n{}",
        o.err_text()
    );
    for l in &lines[..lines.len() - 1] {
        assert!(
            PROGRESS.contains(&l.as_str()),
            "only progress lines may come before the failure line, but found {l:?}"
        );
    }
    assert!(
        !o.err_text().contains('\r'),
        "lines end in a line feed alone"
    );
    if last != SIGN_IN {
        assert_no_backend(o, &o.err_text());
    }
}

/// The caller's three progress lines, each at most once, in order.
fn assert_progress_in_order(lines: &[String]) {
    let mut at = 0;
    for l in lines {
        let p = PROGRESS
            .iter()
            .position(|x| x == l)
            .unwrap_or_else(|| panic!("{l:?} is not one of the caller's progress lines"));
        assert!(p >= at, "progress lines are out of order or repeated");
        at = p + 1;
    }
}

// ---------------------------------------------------------------------------------------------
// Step 1: the second-argument refusal
// ---------------------------------------------------------------------------------------------

#[test]
fn a_second_argument_is_refused_before_anything_starts() {
    let cases: [&[&str]; 7] = [
        &["a", "b"],
        &["", "x"],
        &["a", ""],
        &["", ""],
        &["a", "b", "c"],
        &["-h", "--help"],
        &["bad alias!", "x"],
    ];
    for (i, args) in cases.iter().enumerate() {
        // No cache, no USERPROFILE: nothing at all may run before the refusal.
        let o = run(Case::new(&format!("second-arg-{i}"))
            .args(args)
            .cache(Cache::NoDir)
            .remove("USERPROFILE"));
        assert_eq!(o.code, Some(1), "a second argument exits 1 ({args:?})");
        assert_stdout_empty(&o);
        assert_stderr_is(&o, &[TAKES_ONE]);
        assert!(
            o.calls().is_empty(),
            "nothing is started before the refusal"
        );
        assert!(o.no_ssh_or_scp());
    }
}

#[test]
fn the_second_argument_message_wins_even_with_diagnostic_set() {
    let o = run(Case::new("second-arg-diag")
        .args(&["a", "b"])
        .env("AGENTSPACE_DIAGNOSTIC", "1"));
    assert_eq!(o.code, Some(1));
    assert_stdout_empty(&o);
    assert_eq!(o.err_lines().first().map(String::as_str), Some(TAKES_ONE));
    assert!(o.calls().is_empty());
}

#[test]
fn an_empty_first_argument_is_an_argument_that_reaches_the_alias_check() {
    let o = run(Case::new("empty-alias").args(&[""]));
    assert_failure(&o, BAD_ALIAS);
    assert_stderr_is(&o, &[BAD_ALIAS]);
    assert!(
        o.calls().is_empty(),
        "the alias check runs before any AWS call"
    );
}

// ---------------------------------------------------------------------------------------------
// Step 3: the alias check is the gate
// ---------------------------------------------------------------------------------------------

#[test]
fn a_bad_alias_fails_with_the_alias_message_before_any_program_starts() {
    let bad: [&str; 19] = [
        "-h",
        "--help",
        "-Fx",
        "-",
        "a b",
        "a;b",
        "a,b",
        "a=b",
        "a/b",
        "a\\b",
        "a$b",
        "a:b",
        "@x",
        "x@y",
        "a'b",
        "a\"b",
        "\u{fc}n\u{ef}",
        "good\n",
        "good\r\n",
    ];
    for (i, alias) in bad.iter().enumerate() {
        // No cache either: the check runs before the cache is read or written.
        let o = run(Case::new(&format!("bad-alias-{i}"))
            .args(&[alias])
            .cache(Cache::NoDir));
        assert_failure(&o, BAD_ALIAS);
        assert_stderr_is(&o, &[BAD_ALIAS]);
        assert!(
            o.calls().is_empty(),
            "alias {alias:?} MUST fail before any AWS call"
        );
        assert!(o.no_ssh_or_scp());
        assert!(
            !o.sb.cache_path().exists(),
            "a bad alias MUST NOT write the cache"
        );
    }
}

#[test]
fn the_alias_check_comes_before_the_home_folder_check() {
    let o = run(Case::new("alias-before-home")
        .args(&["-h"])
        .remove("USERPROFILE"));
    assert_failure(&o, BAD_ALIAS);
    assert_stderr_is(&o, &[BAD_ALIAS]);
}

#[test]
fn good_aliases_reach_the_tunnel() {
    for (i, alias) in ["agentspace", "lease-ab_1.x", "A.Z-9", "x"]
        .iter()
        .enumerate()
    {
        let o = run(Case::new(&format!("good-alias-{i}")).args(&[alias]));
        assert_eq!(
            o.code,
            Some(0),
            "alias {alias:?}; stderr:\n{}",
            o.err_text()
        );
        assert_stdout_empty(&o);
        assert_stderr_is(&o, &[READY]);
        assert_eq!(o.tunnel_calls().len(), 1);
    }
}

#[test]
fn a_missing_home_folder_is_the_could_not_reach_line() {
    let o = run(Case::new("no-userprofile")
        .args(&["agentspace"])
        .remove("USERPROFILE"));
    assert_failure(&o, COULD_NOT_REACH);
    assert_stderr_is(&o, &[COULD_NOT_REACH]);
    assert!(o.calls().is_empty());
}

// ---------------------------------------------------------------------------------------------
// The happy path, the calls, PATH, the environment
// ---------------------------------------------------------------------------------------------

#[test]
fn a_ready_box_prints_one_line_and_runs_the_tunnel_for_the_cached_id() {
    let o = run(Case::new("happy"));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_stdout_empty(&o);
    assert_stderr_is(&o, &[READY]);
    let tunnel = format!(
        "ssm start-session --region us-west-2 --target {ID_A} --document-name AWS-StartSSHSession --parameters portNumber=22"
    );
    let describe =
        format!("ec2 describe-instances --region us-west-2 --output json --instance-ids {ID_A}");
    assert_eq!(
        o.calls(),
        vec![describe.clone(), describe, tunnel],
        "the AWS calls, in order (FB-07, FB-09)"
    );
    assert!(
        o.stdin_files_empty(),
        "no program but the tunnel reads stdin"
    );
    assert!(
        o.no_ssh_or_scp(),
        "no push without a receipt, and no ssh at all"
    );
}

#[test]
fn a_seam_path_with_a_space_is_taken_whole() {
    let mut c = Case::new("spaced");
    c.spaced = true;
    let o = run(c);
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_stderr_is(&o, &[READY]);
    assert_eq!(o.tunnel_calls().len(), 1, "the seam's path is taken whole");
}

#[test]
fn path_gets_the_two_amazon_folders_first_in_fb10s_order() {
    for (i, with_amazon_later) in [false, true].into_iter().enumerate() {
        let root = sysroot();
        let original = with_amazon_later.then(|| {
            format!(
                "{};C:\\Program Files\\Amazon\\AWSCLIV2;{}",
                root.join("System32").display(),
                root.display()
            )
        });
        let mut c = Case::new(&format!("path-{i}"));
        c.path = original;
        let o = run(c);
        assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
        let want = format!("{SM_PREFIX}{}", o.path_in);
        let recs = o.env_recs();
        for call in ["ec2-describe-instances", "ssm-start-session"] {
            let rec = recs
                .iter()
                .find(|(n, _)| n == call)
                .unwrap_or_else(|| panic!("no record for {call}"));
            assert!(
                rec.1.path == want,
                "{call} MUST see PATH as the Session Manager plugin's folder, the AWS CLI's folder, then PATH as it was"
            );
        }
    }
}

#[test]
fn every_started_program_sees_nested_and_no_posix_fd_state() {
    for (i, nested) in [None, Some("0"), Some("yes")].into_iter().enumerate() {
        let mut c = Case::new(&format!("env-{i}"))
            .aws(Aws {
                state: "stopped",
                tunnel_stderr: false,
                ..Aws::default()
            })
            .env(POSIX_A, "1")
            .env(POSIX_B, "x");
        if let Some(n) = nested {
            c = c.env("AGENTSPACE_NESTED", n);
        }
        let o = run(c);
        assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
        let recs = o.env_recs();
        // A stopped box makes up, start, ssm info, send, invocation and the tunnel all run.
        assert!(recs.len() >= 5, "expected a record per distinct call");
        assert!(recs.iter().any(|(n, _)| n == "ssm-start-session"));
        for (call, rec) in &recs {
            assert!(
                rec.nested == "1",
                "{call} MUST see AGENTSPACE_NESTED set to 1"
            );
            assert!(
                !rec.names
                    .iter()
                    .any(|n| n.to_uppercase().ends_with("_POSIX_FD_STATE")),
                "{call} MUST see no *_POSIX_FD_STATE variable"
            );
            assert!(
                rec.names
                    .iter()
                    .any(|n| n.eq_ignore_ascii_case("USERPROFILE")),
                "{call} still gets the rest of the environment"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The tunnel: stdin, stdout, stderr, exit codes
// ---------------------------------------------------------------------------------------------

fn pattern(n: usize) -> Vec<u8> {
    (0..n).map(|i| ((i * 31 + i / 256) % 256) as u8).collect()
}

#[test]
fn the_tunnel_gets_the_callers_stdin_and_stdout_byte_for_byte() {
    let mut bytes = pattern(300_000);
    bytes.extend_from_slice(b"\r\n\x00\x1a\xff\r\n");
    let o = run(Case::new("echo")
        .aws(Aws {
            tunnel: Tunnel::Echo,
            ..Aws::default()
        })
        .stdin(Stdin::Bytes(bytes.clone())));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert!(
        o.stdout == bytes,
        "stdout MUST be the tunnel's bytes and nothing else (got {} bytes, wanted {})",
        o.stdout.len(),
        bytes.len()
    );
    assert_stderr_is(&o, &[READY]);
    assert!(
        o.stdin_files_empty(),
        "the ProxyCommand MUST NOT read its stdin before the tunnel, nor let another program read it"
    );
}

#[test]
fn the_tunnel_streams_while_stdin_stays_open() {
    let o = run(Case::new("streaming")
        .aws(Aws {
            tunnel: Tunnel::Echo,
            ..Aws::default()
        })
        .stdin(Stdin::HeldOpen(b"ping".to_vec())));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert!(
        o.streamed_before_close,
        "the tunnel's output MUST reach stdout while stdin is still open"
    );
    assert_eq!(o.stdout, b"ping");
}

#[test]
fn nothing_but_the_tunnel_writes_to_stdout_even_when_other_programs_do() {
    // Every up step prints to stdout in this scenario: start-instances (JSON with a marker),
    // and the wait for a stopping box.
    for (i, state) in ["stopped", "stopping", "pending"].into_iter().enumerate() {
        let o = run(Case::new(&format!("stdout-{i}")).aws(Aws {
            state,
            noise: true,
            ..Aws::default()
        }));
        assert_eq!(o.code, Some(0), "{state}; stderr:\n{}", o.err_text());
        assert_stdout_empty(&o);
        let e = o.err_text();
        assert!(
            !e.contains("MARKER"),
            "a stand-in's output MUST NOT reach the caller without AGENTSPACE_DIAGNOSTIC"
        );
    }
}

#[test]
fn the_tunnels_stderr_is_withheld_unless_diagnostic_is_set() {
    for (i, diag) in [None, Some("")].into_iter().enumerate() {
        let mut c = Case::new(&format!("tunnel-stderr-{i}")).aws(Aws {
            tunnel_stderr: true,
            ..Aws::default()
        });
        if let Some(d) = diag {
            c = c.env("AGENTSPACE_DIAGNOSTIC", d);
        }
        let o = run(c);
        assert_eq!(o.code, Some(0));
        assert_stdout_empty(&o);
        assert_stderr_is(&o, &[READY]);
    }
}

#[test]
fn diagnostic_changes_neither_stdout_nor_the_exit_code_of_a_good_run() {
    let o = run(Case::new("tunnel-stderr-diag")
        .aws(Aws {
            tunnel: Tunnel::Payload,
            tunnel_stderr: true,
            ..Aws::default()
        })
        .env("AGENTSPACE_DIAGNOSTIC", "1"));
    assert_eq!(o.code, Some(0));
    assert_eq!(o.stdout, b"PAYLOAD-BYTES\r\n");
    assert!(o.err_lines().iter().any(|l| l == READY));
}

#[test]
fn a_tunnel_that_exits_non_zero_gets_the_connection_failed_line_and_its_own_exit_code() {
    for code in [1, 7, 255] {
        let o = run(Case::new(&format!("tunnel-fail-{code}")).aws(Aws {
            tunnel: Tunnel::Payload,
            tunnel_code: code,
            tunnel_stderr: true,
            noise: true,
            ..Aws::default()
        }));
        assert_eq!(
            o.code,
            Some(code),
            "the tunnel's exit code is the program's"
        );
        assert_eq!(
            o.stdout, b"PAYLOAD-BYTES\r\n",
            "stdout holds the tunnel's bytes and nothing else"
        );
        assert_stderr_is(&o, &[READY, CONN_FAILED]);
    }
}

#[test]
fn diagnostic_does_not_change_a_failed_tunnels_exit_code_or_stdout() {
    let o = run(Case::new("tunnel-fail-diag")
        .aws(Aws {
            tunnel: Tunnel::Payload,
            tunnel_code: 7,
            tunnel_stderr: true,
            ..Aws::default()
        })
        .env("AGENTSPACE_DIAGNOSTIC", "1"));
    assert_eq!(o.code, Some(7));
    assert_eq!(o.stdout, b"PAYLOAD-BYTES\r\n");
    assert!(o.err_lines().iter().any(|l| l == CONN_FAILED));
}

#[test]
fn a_tunnel_that_exits_zero_exits_zero_with_nothing_added() {
    let o = run(Case::new("tunnel-zero").aws(Aws {
        tunnel: Tunnel::Payload,
        ..Aws::default()
    }));
    assert_eq!(o.code, Some(0));
    assert_eq!(o.stdout, b"PAYLOAD-BYTES\r\n");
    assert_stderr_is(&o, &[READY]);
}

// ---------------------------------------------------------------------------------------------
// Caller mode: progress lines
// ---------------------------------------------------------------------------------------------

fn up_calls_for_stopped(o: &Out) {
    assert_eq!(
        o.count("ec2 describe-instances"),
        1,
        "FB-09: no second describe"
    );
    assert_eq!(
        o.count(&format!(
            "ec2 start-instances --region us-west-2 --output json --instance-ids {ID_A}"
        )),
        1
    );
}

#[test]
fn a_stopped_box_prints_starting_then_ready() {
    let o = run(Case::new("stopped").aws(Aws {
        state: "stopped",
        noise: true,
        ..Aws::default()
    }));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_stdout_empty(&o);
    assert_stderr_is(&o, &[STARTING, READY]);
    up_calls_for_stopped(&o);
    assert!(o.count("ssm describe-instance-information --region us-west-2 --output json") >= 1);
    assert!(o.count("ssm send-command --region us-west-2 --output json") >= 1);
    assert_eq!(o.tunnel_calls().len(), 1);
}

#[test]
fn a_stopping_box_waits_ignores_the_waits_exit_code_and_prints_starting_once() {
    for (i, wait_code) in [0, 255].into_iter().enumerate() {
        let o = run(Case::new(&format!("stopping-{i}")).aws(Aws {
            state: "stopping",
            wait_code,
            noise: true,
            ..Aws::default()
        }));
        assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
        assert_stdout_empty(&o);
        assert_stderr_is(&o, &[STARTING, READY]);
        // FB-07 gives the order service, operation, region; whether `instance-stopped` is the
        // operation or its argument is not pinned, so only the parts are checked.
        let waits: Vec<String> = o
            .calls()
            .into_iter()
            .filter(|c| c.starts_with("ec2 wait"))
            .collect();
        assert_eq!(waits.len(), 1, "one wait");
        assert!(waits[0].contains("instance-stopped"));
        assert!(waits[0].contains("--region us-west-2"));
        assert!(waits[0].contains(&format!("--instance-ids {ID_A}")));
        assert!(
            !waits[0].contains("--output"),
            "the wait passes no --output"
        );
        up_calls_for_stopped(&o);
    }
}

#[test]
fn a_pending_box_is_waited_for_without_a_start_request() {
    let o = run(Case::new("pending").aws(Aws {
        state: "pending",
        ..Aws::default()
    }));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_stderr_is(&o, &[READY]);
    assert_eq!(o.count("ec2 start-instances"), 0);
    assert_eq!(o.count("ec2 describe-instances"), 1);
    assert!(o.count("ssm describe-instance-information") >= 1);
}

#[test]
fn a_box_running_for_over_twenty_minutes_is_not_waited_for() {
    let o = run(Case::new("running-old"));
    assert_eq!(o.code, Some(0));
    assert_eq!(o.count("ssm "), 1, "only the tunnel's ssm call");
    assert_eq!(o.count("ec2 describe-instances"), 2);
}

#[test]
fn a_recently_started_box_is_waited_for_and_each_waiting_line_prints_once() {
    // Check 1: the agent is not online. Check 2: online, patch run pending. Check 3: patched.
    // That is two 5-second pauses, so this test takes about ten seconds.
    let o = run(Case::new("waiting").aws(Aws {
        launch: FUTURE,
        ssm_offline: 1,
        patch_pending: 1,
        noise: true,
        ..Aws::default()
    }));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_stdout_empty(&o);
    assert_stderr_is(&o, &[WAITING, READY]);
    assert_eq!(o.count("ssm describe-instance-information"), 3);
    assert_progress_in_order(&o.err_lines());
}

// ---------------------------------------------------------------------------------------------
// Caller mode: failures
// ---------------------------------------------------------------------------------------------

#[test]
fn a_box_in_another_state_is_not_available() {
    for (i, state) in ["terminated", "shutting-down"].into_iter().enumerate() {
        let o = run(Case::new(&format!("unavailable-{i}")).aws(Aws {
            state,
            noise: true,
            ..Aws::default()
        }));
        assert_failure(&o, NOT_AVAILABLE);
        assert_stderr_is(&o, &[NOT_AVAILABLE]);
        assert_eq!(o.count("ec2 start-instances"), 0);
    }
}

#[test]
fn any_other_failed_call_is_the_could_not_reach_line_with_no_backend_text() {
    let steps: Vec<(&str, Aws)> = vec![
        (
            "describe",
            Aws {
                fail: fail("ec2", "describe-instances", 254, ERR_TEXT),
                noise: true,
                ..Aws::default()
            },
        ),
        (
            "start",
            Aws {
                state: "stopped",
                fail: fail("ec2", "start-instances", 254, ERR_TEXT),
                noise: true,
                ..Aws::default()
            },
        ),
        (
            "ssminfo",
            Aws {
                launch: FUTURE,
                fail: fail("ssm", "describe-instance-information", 254, ERR_TEXT),
                noise: true,
                ..Aws::default()
            },
        ),
        (
            "send",
            Aws {
                launch: FUTURE,
                fail: fail("ssm", "send-command", 254, ERR_TEXT),
                noise: true,
                ..Aws::default()
            },
        ),
    ];
    for (name, aws) in steps {
        let o = run(Case::new(&format!("generic-{name}")).aws(aws));
        assert_failure(&o, COULD_NOT_REACH);
        assert_eq!(
            o.err_lines().last().map(String::as_str),
            Some(COULD_NOT_REACH)
        );
        assert_eq!(o.tunnel_calls().len(), 0, "{name}: no tunnel");
        assert!(
            !o.err_text().contains("authorized"),
            "{name}: the CLI's text MUST NOT reach the caller"
        );
    }
}

#[test]
fn diagnostic_adds_detail_after_the_failure_line_and_changes_nothing_else() {
    let o = run(Case::new("generic-diag")
        .aws(Aws {
            fail: fail("ec2", "describe-instances", 254, ERR_TEXT),
            ..Aws::default()
        })
        .env("AGENTSPACE_DIAGNOSTIC", "1"));
    assert_eq!(o.code, Some(1));
    assert_stdout_empty(&o);
    assert!(o.tunnel_calls().is_empty());
    assert_eq!(
        o.err_lines().first().map(String::as_str),
        Some(COULD_NOT_REACH),
        "the failure line comes first and detail after it"
    );
}

#[test]
fn an_empty_diagnostic_variable_withholds_detail_like_an_unset_one() {
    let o = run(Case::new("generic-diag-empty")
        .aws(Aws {
            fail: fail("ec2", "describe-instances", 254, ERR_TEXT),
            ..Aws::default()
        })
        .env("AGENTSPACE_DIAGNOSTIC", ""));
    assert_failure(&o, COULD_NOT_REACH);
    assert_stderr_is(&o, &[COULD_NOT_REACH]);
}

#[test]
fn a_sign_in_failure_prints_the_sign_in_message_exactly() {
    let texts = [
        "The SSO session associated with this profile has expired or is invalid. To refresh this SSO session run aws login.",
        "Unable to locate credentials. You can configure credentials by running aws configure.",
        "UNABLE TO LOCATE CREDENTIALS",
        "Your session has EXPIRED.",
        "Please run AWS LOGIN again.",
    ];
    for (i, t) in texts.iter().enumerate() {
        let o = run(Case::new(&format!("signin-{i}")).aws(Aws {
            fail: fail("ec2", "describe-instances", 255, t),
            ..Aws::default()
        }));
        assert_failure(&o, SIGN_IN);
        assert_stderr_is(&o, &[SIGN_IN]);
    }
}

#[test]
fn the_sign_in_message_is_the_first_line_even_with_diagnostic() {
    let o = run(Case::new("signin-diag")
        .aws(Aws {
            fail: fail(
                "ec2",
                "describe-instances",
                255,
                "Your session has expired. Run aws login.",
            ),
            ..Aws::default()
        })
        .env("AGENTSPACE_DIAGNOSTIC", "1"));
    assert_eq!(o.code, Some(1));
    assert_stdout_empty(&o);
    assert_eq!(o.err_lines().first().map(String::as_str), Some(SIGN_IN));
}

#[test]
fn a_sign_in_failure_while_rebuilding_the_cache_prints_the_sign_in_message() {
    let o = run(Case::new("signin-stacks").cache(Cache::Missing).aws(Aws {
        fail: fail(
            "cloudformation",
            "describe-stacks",
            255,
            "Unable to locate credentials.",
        ),
        ..Aws::default()
    }));
    assert_failure(&o, SIGN_IN);
    assert_stderr_is(&o, &[SIGN_IN]);
}
// ---------------------------------------------------------------------------------------------
// The instance ID: from the host registry, `hosts.json` (operator.md, The host registry, as the
// operator uses it; state.md, The instance ID). The v0.06 cache, `instance-id.json`, is retired.
// ---------------------------------------------------------------------------------------------

impl Case {
    /// A case whose state folder holds exactly this registry and, when given, this lease state
    /// file, in the default state folder.
    fn seeded(name: &str, registry_text: String, leases_text: Option<String>) -> Case {
        let mut c = Case::new(name).cache(Cache::NoDir);
        c.setup = Some(seed(registry_text, leases_text));
        c
    }
}

/// The names of the files in the state folder.
fn state_names(o: &Out) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(o.sb.state_dir())
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn the_tunnel_targets_the_cached_id_and_never_another() {
    let o = run(Case::new("cache-valid").cache(Cache::Valid(ID_B)));
    assert_eq!(o.code, Some(0));
    assert_eq!(o.target().as_deref(), Some(ID_B));
    assert_eq!(o.tunnel_calls().len(), 1);
    assert_eq!(
        o.count("cloudformation"),
        0,
        "a registry that holds the ID needs no stack call"
    );
    assert!(
        o.calls().iter().all(|c| !c.contains(ID_A)),
        "no call names an id other than the registry's"
    );
}

#[test]
fn a_registry_with_a_byte_order_mark_and_whitespace_is_read() {
    let mut raw = vec![0xEF, 0xBB, 0xBF];
    raw.extend_from_slice(
        format!(
            "  \r\n {{ \"version\" : 0 ,\r\n \"hosts\" : [ {{ \"state\":\"ready\", \"stack\":\"agentbox\", \"project\":\"swimcoach\", \"instance_id\" : \"{ID_B}\" }} ] }}\r\n\r\n"
        )
        .as_bytes(),
    );
    let o = run(Case::new("registry-bom").cache(Cache::Raw(raw)));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_eq!(o.target().as_deref(), Some(ID_B));
    assert_eq!(o.count("cloudformation"), 0);
}

/// Registries that are not valid (state.md, The host registry). A reader never reads one as
/// missing and never repairs it, so each fails the connection and changes nothing.
fn invalid_registries() -> Vec<(&'static str, Vec<u8>)> {
    let box_rec = "{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}";
    let wrap = |records: &str| format!("{{\"version\":0,\"hosts\":[{records}]}}").into_bytes();
    vec![
        ("empty", Vec::new()),
        ("whitespace", b"  \r\n".to_vec()),
        ("not-json", b"not json".to_vec()),
        ("array", b"[]".to_vec()),
        ("version", b"{\"version\":1,\"hosts\":[]}".to_vec()),
        (
            "top-extra",
            format!("{{\"version\":0,\"hosts\":[{box_rec}],\"extra\":true}}").into_bytes(),
        ),
        (
            "record-extra",
            wrap(
                "{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"extra\":1}",
            ),
        ),
        (
            "null-id",
            wrap(
                "{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":null}",
            ),
        ),
        (
            "short-id",
            wrap(
                "{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":\"i-123\"}",
            ),
        ),
        (
            "upper-id",
            wrap(
                "{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":\"I-1234567890ABCDEF0\"}",
            ),
        ),
        (
            "newline-id",
            wrap(&format!(
                "{{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":\"{ID_A}\\n\"}}"
            )),
        ),
        (
            "bad-state",
            wrap("{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"stopped\"}"),
        ),
        (
            "duplicate-member",
            wrap(
                "{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"state\":\"ready\"}",
            ),
        ),
        (
            "two-for-one-project",
            wrap(&format!(
                "{box_rec},{{\"project\":\"{PROJ_A}\",\"stack\":\"agentspace-host-{PROJ_A}\",\"state\":\"ready\"}},{{\"project\":\"{PROJ_A}\",\"stack\":\"agentspace-host-{PROJ_A}\",\"state\":\"creating\"}}"
            )),
        ),
        (
            "no-box-record",
            wrap(&format!(
                "{{\"project\":\"{PROJ_A}\",\"stack\":\"agentspace-host-{PROJ_A}\",\"state\":\"ready\"}}"
            )),
        ),
        ("two-box-records", wrap(&format!("{box_rec},{box_rec}"))),
        (
            "box-for-another-project",
            wrap("{\"project\":\"other\",\"stack\":\"agentbox\",\"state\":\"ready\"}"),
        ),
        (
            "stack-for-another-project",
            wrap(&format!(
                "{box_rec},{{\"project\":\"{PROJ_A}\",\"stack\":\"agentspace-host-{PROJ_B}\",\"state\":\"ready\"}}"
            )),
        ),
        (
            "project-not-normalized",
            wrap(&format!(
                "{box_rec},{{\"project\":\"Orchard\",\"stack\":\"agentspace-host-Orchard\",\"state\":\"ready\"}}"
            )),
        ),
        ("not-utf8", vec![0xFF, 0xFE, 0x7B, 0x00, 0x7D, 0x00]),
        (
            "the-retired-caches-form",
            format!("{{\"version\":0,\"instance_id\":\"{ID_A}\"}}").into_bytes(),
        ),
    ]
}

#[test]
fn a_registry_that_is_not_valid_fails_the_connection_and_is_never_repaired() {
    for (name, raw) in invalid_registries() {
        // The box's own path, a host alias and a lease alias: every connection reads the
        // registry, so every one fails.
        for (i, args) in [
            vec![],
            vec![format!("agentspace-host.{PROJ_A}")],
            vec![lease_alias(PROJ_A, LEASE_PREFIX)],
        ]
        .into_iter()
        .enumerate()
        {
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let o = run(Case::new(&format!("registry-bad-{name}-{i}"))
                .args(&arg_refs)
                .cache(Cache::Raw(raw.clone())));
            assert_failure(&o, COULD_NOT_REACH);
            assert_stderr_is(&o, &[COULD_NOT_REACH]);
            assert!(
                o.calls().is_empty(),
                "{name}/{i}: no AWS call is made over a registry that is not valid"
            );
            assert_eq!(
                fs::read(o.sb.hosts_path()).unwrap(),
                raw,
                "{name}/{i}: the registry is never rewritten"
            );
        }
    }
}

#[test]
fn a_missing_registry_is_the_boxs_record_alone_and_the_id_is_written_atomically() {
    for (i, cache) in [Cache::Missing, Cache::NoDir].into_iter().enumerate() {
        let o = run(Case::new(&format!("registry-missing-{i}")).cache(cache));
        assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
        assert_stderr_is(&o, &[READY]);
        assert_eq!(
            o.calls().first().map(String::as_str),
            Some(
                "cloudformation describe-stacks --region us-west-2 --output json --stack-name agentbox"
            ),
            "the box's record has no ID, so the box's stack is read"
        );
        assert_eq!(o.target().as_deref(), Some(ID_B));
        assert_eq!(
            fs::read(o.sb.hosts_path()).unwrap(),
            registry(&[], Some(ID_B)).into_bytes(),
            "the first change writes the box's record with the ID, compact, then one line feed"
        );
        assert!(
            state_names(&o).iter().all(|n| !n.contains(".tmp")),
            "no temporary file is left"
        );
    }
}

#[test]
fn the_retired_caches_and_the_prototypes_are_never_read() {
    let mut c = Case::new("cache-retired").cache(Cache::Missing);
    c.setup = Some(Box::new(|sb: &Sandbox| {
        fs::write(
            sb.retired_cache_path(),
            format!("{{\"version\":0,\"instance_id\":\"{ID_A}\"}}"),
        )
        .unwrap();
        fs::write(sb.state_dir().join("instance-id"), ID_A).unwrap();
        fs::write(sb.work.join(".instance-id"), ID_A).unwrap();
        fs::write(sb.home.join(".instance-id"), ID_A).unwrap();
    }));
    let o = run(c);
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_eq!(
        o.target().as_deref(),
        Some(ID_B),
        "nothing migrates: a record without an ID reads the stack again"
    );
    assert_eq!(o.count("cloudformation describe-stacks"), 1);
}

#[test]
fn a_missing_stack_output_is_the_could_not_reach_line_and_writes_nothing() {
    let outputs = [
        "{\"Stacks\":[{\"Outputs\":[]}]}",
        "{\"Stacks\":[{}]}",
        "{\"Stacks\":[]}",
        "{\"Stacks\":[{\"Outputs\":[{\"OutputKey\":\"Other\",\"OutputValue\":\"i-1234567890abcdef0\"}]}]}",
        "{\"Stacks\":[{\"Outputs\":[{\"OutputKey\":\"InstanceId\",\"OutputValue\":\"i-nothex\"}]}]}",
        "{\"Stacks\":[{\"Outputs\":[{\"OutputKey\":\"InstanceId\",\"OutputValue\":5}]}]}",
    ];
    for (i, j) in outputs.iter().enumerate() {
        let o = run(Case::new(&format!("stack-bad-{i}"))
            .cache(Cache::Missing)
            .aws(Aws {
                stack_json: Some(j.to_string()),
                ..Aws::default()
            }));
        assert_failure(&o, COULD_NOT_REACH);
        assert_stderr_is(&o, &[COULD_NOT_REACH]);
        assert!(!o.sb.hosts_path().exists(), "no registry is written");
    }
}

#[test]
fn a_failing_stack_call_is_the_could_not_reach_line() {
    let o = run(Case::new("stack-fail").cache(Cache::Missing).aws(Aws {
        fail: fail("cloudformation", "describe-stacks", 254, ERR_TEXT),
        noise: true,
        ..Aws::default()
    }));
    assert_failure(&o, COULD_NOT_REACH);
    assert_stderr_is(&o, &[COULD_NOT_REACH]);
}

#[test]
fn a_registry_that_cannot_be_opened_is_the_could_not_reach_line_with_no_path() {
    let o = run(Case::new("registry-folder").cache(Cache::Folder));
    assert_failure(&o, COULD_NOT_REACH);
    assert_stderr_is(&o, &[COULD_NOT_REACH]);
    assert!(
        o.calls().is_empty(),
        "no AWS call is made over a registry it cannot read"
    );
}

#[test]
fn a_hosts_missing_instance_id_is_read_from_its_stack_and_written_into_its_record_alone() {
    let before = format!(
        "{{\"version\":0,\"hosts\":[{{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}},{{\"project\":\"{PROJ_A}\",\"stack\":\"agentspace-host-{PROJ_A}\",\"state\":\"ready\"}},{{\"project\":\"{PROJ_B}\",\"stack\":\"agentspace-host-{PROJ_B}\",\"state\":\"creating\",\"last_attempt_at\":\"2026-01-01T00:00:00Z\"}}]}}\n"
    );
    let after = format!(
        "{{\"version\":0,\"hosts\":[{{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}},{{\"project\":\"{PROJ_A}\",\"stack\":\"agentspace-host-{PROJ_A}\",\"state\":\"ready\",\"instance_id\":\"{ID_B}\"}},{{\"project\":\"{PROJ_B}\",\"stack\":\"agentspace-host-{PROJ_B}\",\"state\":\"creating\",\"last_attempt_at\":\"2026-01-01T00:00:00Z\"}}]}}\n"
    );
    let o = run(Case::seeded("host-id-from-stack", before, None)
        .args(&[&format!("agentspace-host.{PROJ_A}")]));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_stdout_empty(&o);
    assert_stderr_is(&o, &[READY]);
    assert_names_no_host(&o);
    assert_eq!(
        o.calls().first().map(String::as_str),
        Some(
            format!(
                "cloudformation describe-stacks --region us-west-2 --output json --stack-name agentspace-host-{PROJ_A}"
            )
            .as_str()
        ),
        "the host's own stack is read, never the box's"
    );
    assert_eq!(o.count("cloudformation"), 1);
    assert_eq!(o.target().as_deref(), Some(ID_B));
    assert_eq!(
        fs::read(o.sb.hosts_path()).unwrap(),
        after.into_bytes(),
        "only that record's instance_id is written, and no other member or record changes"
    );
    assert!(state_names(&o).iter().all(|n| !n.contains(".tmp")));
}

#[test]
fn a_hosts_instance_id_from_its_record_needs_no_stack_call_and_is_never_replaced() {
    let reg = registry(&[host(PROJ_A, "ready", Some(ID_C))], Some(ID_A));
    let o = run(Case::seeded("host-id-held", reg.clone(), None)
        .args(&[&format!("agentspace-host.{PROJ_A}")]));
    assert_reached(&o, ID_C, &[ID_A, ID_B]);
    assert_eq!(o.count("cloudformation"), 0);
    assert_eq!(fs::read(o.sb.hosts_path()).unwrap(), reg.into_bytes());
}

#[test]
fn a_registry_write_that_fails_does_not_fail_the_connection() {
    // A read-only registry cannot be replaced, so the missing ID cannot be written; the
    // connection uses the ID it read (operator.md, The host registry, as the operator uses it).
    let reg = registry(&[host(PROJ_A, "ready", None)], Some(ID_A));
    let mut c = Case::seeded("registry-readonly", reg.clone(), None)
        .args(&[&format!("agentspace-host.{PROJ_A}")]);
    let seeding = c.setup.take().unwrap();
    c.setup = Some(Box::new(move |sb: &Sandbox| {
        seeding(sb);
        let mut perms = fs::metadata(sb.hosts_path()).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(sb.hosts_path(), perms).unwrap();
    }));
    let o = run(c);
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_stdout_empty(&o);
    assert_stderr_is(&o, &[READY]);
    assert_eq!(o.target().as_deref(), Some(ID_B));
    assert_eq!(fs::read(o.sb.hosts_path()).unwrap(), reg.into_bytes());
}

// ---------------------------------------------------------------------------------------------
// Which host a connection reaches (operator.md, From the ProxyCommand; D49, D55)
// ---------------------------------------------------------------------------------------------

/// Backend and host names the caller MUST NOT see: no project, stack or instance (D1).
fn assert_names_no_host(o: &Out) {
    let text = o.err_text();
    for needle in [
        PROJ_A,
        PROJ_B,
        "swimcoach",
        "agentspace-host",
        "agentbox",
        ID_A,
        ID_B,
        ID_C,
        ID_D,
    ] {
        assert!(
            !text.contains(needle),
            "caller-mode output names {needle:?}:\n{text}"
        );
    }
}

/// The connection reached the host with this instance ID: one ready line, the tunnel to its ID,
/// and no call that names any of `others`.
fn assert_reached(o: &Out, id: &str, others: &[&str]) {
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_stdout_empty(o);
    assert_stderr_is(o, &[READY]);
    assert_names_no_host(o);
    assert_eq!(o.target().as_deref(), Some(id));
    assert_eq!(o.tunnel_calls().len(), 1);
    for other in others {
        assert!(
            o.calls().iter().all(|c| !c.contains(other)),
            "a call names another host's instance"
        );
    }
}

fn two_hosts() -> String {
    registry(
        &[
            host(PROJ_A, "ready", Some(ID_C)),
            host(PROJ_B, "ready", Some(ID_D)),
        ],
        Some(ID_A),
    )
}

#[test]
fn a_host_alias_reaches_that_projects_host() {
    let describe =
        format!("ec2 describe-instances --region us-west-2 --output json --instance-ids {ID_C}");
    let tunnel = format!(
        "ssm start-session --region us-west-2 --target {ID_C} --document-name AWS-StartSSHSession --parameters portNumber=22"
    );
    let o = run(Case::seeded("host-alias-a", two_hosts(), None)
        .args(&[&format!("agentspace-host.{PROJ_A}")]));
    assert_reached(&o, ID_C, &[ID_A, ID_B, ID_D]);
    assert_eq!(
        o.calls(),
        vec![describe.clone(), describe, tunnel],
        "up and the tunnel act on that host's instance"
    );
    assert!(o.no_ssh_or_scp());

    let o = run(Case::seeded("host-alias-b", two_hosts(), None)
        .args(&[&format!("agentspace-host.{PROJ_B}")]));
    assert_reached(&o, ID_D, &[ID_A, ID_B, ID_C]);
}

#[test]
fn a_host_alias_reaches_its_host_whatever_the_records_state() {
    for state in ["creating", "failed", "ready"] {
        let reg = registry(&[host(PROJ_A, state, Some(ID_C))], Some(ID_A));
        let o = run(Case::seeded(&format!("host-alias-{state}"), reg, None)
            .args(&[&format!("agentspace-host.{PROJ_A}")]));
        assert_reached(&o, ID_C, &[ID_A]);
    }
}

#[test]
fn a_host_alias_never_reads_the_lease_state_file() {
    let o = run(Case::seeded(
        "host-alias-leases",
        two_hosts(),
        Some("garbage, not json".to_string()),
    )
    .args(&[&format!("agentspace-host.{PROJ_A}")]));
    assert_reached(&o, ID_C, &[ID_A]);
}

#[test]
fn a_host_alias_for_a_project_with_no_record_fails_and_starts_nothing() {
    for (i, alias) in [
        "agentspace-host.nosuch".to_string(),
        format!("agentspace-host.{PROJ_A}x"),
    ]
    .iter()
    .enumerate()
    {
        let o =
            run(Case::seeded(&format!("host-alias-none-{i}"), two_hosts(), None).args(&[alias]));
        assert_failure(&o, COULD_NOT_REACH);
        assert_stderr_is(&o, &[COULD_NOT_REACH]);
        assert!(o.calls().is_empty(), "no AWS call, and so no tunnel");
        assert!(o.no_ssh_or_scp());
    }
    // A missing registry holds the box's record alone.
    let o = run(Case::new("host-alias-no-registry")
        .cache(Cache::NoDir)
        .args(&[&format!("agentspace-host.{PROJ_A}")]));
    assert_failure(&o, COULD_NOT_REACH);
    assert_stderr_is(&o, &[COULD_NOT_REACH]);
    assert!(o.calls().is_empty());
    assert!(
        !o.sb.hosts_path().exists(),
        "a failed lookup never adds a record or writes the registry"
    );
}

#[test]
fn a_host_alias_whose_project_is_not_normalized_fails_before_any_call() {
    for (i, rest) in [
        "", "Orchard", "ORCHARD", "orchard-", "-orchard", "or_chard", "or.chard",
    ]
    .iter()
    .enumerate()
    {
        let o = run(
            Case::seeded(&format!("host-alias-form-{i}"), two_hosts(), None)
                .args(&[&format!("agentspace-host.{rest}")]),
        );
        assert_failure(&o, COULD_NOT_REACH);
        assert_stderr_is(&o, &[COULD_NOT_REACH]);
        assert!(o.calls().is_empty(), "{rest:?}: no AWS call");
    }
}

#[test]
fn a_lease_alias_reaches_its_leases_ready_host() {
    let leases = lease_file(&[
        ready_lease(PROJ_A, "aaaaaaaa", LEASE_FUTURE),
        ready_lease(PROJ_B, "bbbbbbbb", LEASE_FUTURE),
    ]);
    let o = run(Case::seeded("lease-a", two_hosts(), Some(leases.clone()))
        .args(&[&lease_alias(PROJ_A, "aaaaaaaa")]));
    assert_reached(&o, ID_C, &[ID_A, ID_B, ID_D]);
    assert!(o.no_ssh_or_scp());

    let o = run(Case::seeded("lease-b", two_hosts(), Some(leases))
        .args(&[&lease_alias(PROJ_B, "bbbbbbbb")]));
    assert_reached(&o, ID_D, &[ID_A, ID_B, ID_C]);
}

#[test]
fn a_lease_alias_does_not_check_the_leases_expiry() {
    let leases = lease_file(&[ready_lease(PROJ_A, "aaaaaaaa", "2020-01-01T00:00:00Z")]);
    let o = run(Case::seeded("lease-expired", two_hosts(), Some(leases))
        .args(&[&lease_alias(PROJ_A, "aaaaaaaa")]));
    assert_reached(&o, ID_C, &[ID_A]);
}

#[test]
fn a_lease_alias_is_matched_exactly_and_case_sensitively() {
    let leases = lease_file(&[ready_lease(PROJ_A, "aaaaaaaa", LEASE_FUTURE)]);
    for (i, alias) in [
        lease_alias(PROJ_A, "aaaaaaaa").to_uppercase(),
        lease_alias(PROJ_A, "aaaaaaab"),
        format!("{}x", lease_alias(PROJ_A, "aaaaaaaa")),
        lease_alias(PROJ_A, "aaaaaaaa")
            .trim_start_matches("agentspace-")
            .to_string(),
    ]
    .iter()
    .enumerate()
    {
        let o = run(Case::seeded(
            &format!("lease-exact-{i}"),
            two_hosts(),
            Some(leases.clone()),
        )
        .args(&[alias]));
        assert_reached(&o, ID_A, &[ID_B, ID_C, ID_D]);
    }
}

#[test]
fn a_lease_alias_with_no_record_reaches_the_box() {
    // No lease state file at all, an empty one, and one that holds other leases.
    for (i, leases) in [
        None,
        Some(lease_file(&[])),
        Some(lease_file(&[ready_lease(PROJ_B, "bbbbbbbb", LEASE_FUTURE)])),
    ]
    .into_iter()
    .enumerate()
    {
        let o = run(
            Case::seeded(&format!("lease-norecord-{i}"), two_hosts(), leases)
                .args(&[&lease_alias(PROJ_A, "aaaaaaaa")]),
        );
        assert_reached(&o, ID_A, &[ID_B, ID_C, ID_D]);
    }
}

#[test]
fn a_leases_alias_that_is_not_stored_ready_never_matches() {
    // A released record keeps no `ssh`, so its alias matches nothing.
    let leases = lease_file(&[released_lease(PROJ_A, "aaaaaaaa")]);
    let o = run(Case::seeded("lease-released", two_hosts(), Some(leases))
        .args(&[&lease_alias(PROJ_A, "aaaaaaaa")]));
    assert_reached(&o, ID_A, &[ID_C]);
}

#[test]
fn a_lease_alias_whose_project_has_no_ready_host_reaches_the_box() {
    let leases = lease_file(&[ready_lease(PROJ_A, "aaaaaaaa", LEASE_FUTURE)]);
    let registries = [
        // The project has no record.
        registry(&[host(PROJ_B, "ready", Some(ID_D))], Some(ID_A)),
        // Its record is creating, or failed.
        registry(&[host(PROJ_A, "creating", Some(ID_C))], Some(ID_A)),
        registry(&[host(PROJ_A, "failed", Some(ID_C))], Some(ID_A)),
    ];
    for (i, reg) in registries.into_iter().enumerate() {
        let o = run(
            Case::seeded(&format!("lease-nohost-{i}"), reg, Some(leases.clone()))
                .args(&[&lease_alias(PROJ_A, "aaaaaaaa")]),
        );
        assert_reached(&o, ID_A, &[ID_C, ID_D]);
    }
}

#[test]
fn a_lease_state_file_that_is_not_readable_or_not_valid_sends_the_connection_to_the_box() {
    let record = ready_lease(PROJ_A, "aaaaaaaa", LEASE_FUTURE);
    let bad: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("not-json", b"garbage, not json".to_vec()),
        ("array", b"[]".to_vec()),
        (
            "version",
            format!("{{\"version\":1,\"leases\":[{record}]}}").into_bytes(),
        ),
        (
            "extra-member",
            format!("{{\"version\":0,\"leases\":[{record}],\"extra\":1}}").into_bytes(),
        ),
        // Two records with one ID are not valid, though the schema cannot refuse them.
        (
            "duplicate-id",
            lease_file(&[record.clone(), record.clone()]).into_bytes(),
        ),
        ("not-utf8", vec![0xFF, 0xFE, 0x00, 0x7B, 0x00, 0x7D, 0x00]),
    ];
    for (name, raw) in bad {
        let mut c = Case::seeded(&format!("lease-bad-{name}"), two_hosts(), None)
            .args(&[&lease_alias(PROJ_A, "aaaaaaaa")]);
        let seeding = c.setup.take().unwrap();
        c.setup = Some(Box::new(move |sb: &Sandbox| {
            seeding(sb);
            fs::write(sb.leases_path(), &raw).unwrap();
        }));
        let o = run(c);
        assert_reached(&o, ID_A, &[ID_C, ID_D]);
    }
    // Something that is not a file, where the file belongs.
    let mut c = Case::seeded("lease-bad-folder", two_hosts(), None)
        .args(&[&lease_alias(PROJ_A, "aaaaaaaa")]);
    let seeding = c.setup.take().unwrap();
    c.setup = Some(Box::new(move |sb: &Sandbox| {
        seeding(sb);
        fs::create_dir_all(sb.leases_path()).unwrap();
    }));
    let o = run(c);
    assert_reached(&o, ID_A, &[ID_C, ID_D]);
}

#[test]
fn a_lease_state_file_that_is_not_valid_is_named_only_under_diagnostic() {
    let mut c = Case::seeded("lease-bad-diag", two_hosts(), None)
        .args(&[&lease_alias(PROJ_A, "aaaaaaaa")])
        .env("AGENTSPACE_DIAGNOSTIC", "1");
    let seeding = c.setup.take().unwrap();
    c.setup = Some(Box::new(move |sb: &Sandbox| {
        seeding(sb);
        fs::write(sb.leases_path(), b"garbage, not json").unwrap();
    }));
    let o = run(c);
    // The connection still goes to the box; the extra line names the file and the problem.
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_stdout_empty(&o);
    assert_eq!(o.target().as_deref(), Some(ID_A));
    assert!(
        o.err_text().contains("leases.json"),
        "with AGENTSPACE_DIAGNOSTIC set, a line names the lease state file; stderr:\n{}",
        o.err_text()
    );
}

#[test]
fn the_lease_state_file_is_never_written_and_never_locked() {
    let leases = lease_file(&[ready_lease(PROJ_A, "aaaaaaaa", LEASE_FUTURE)]);
    let reg = two_hosts();
    let o = run(
        Case::seeded("lease-readonly-use", reg.clone(), Some(leases.clone()))
            .args(&[&lease_alias(PROJ_A, "aaaaaaaa")]),
    );
    assert_reached(&o, ID_C, &[ID_A]);
    assert_eq!(fs::read(o.sb.leases_path()).unwrap(), leases.into_bytes());
    assert_eq!(fs::read(o.sb.hosts_path()).unwrap(), reg.into_bytes());
    assert_eq!(
        state_names(&o),
        vec!["hosts.json".to_string(), "leases.json".to_string()],
        "no lock file, temporary file or any other file is made"
    );
}

#[test]
fn agentspace_state_dir_moves_the_lease_state_file_and_nothing_else() {
    let leases = lease_file(&[ready_lease(PROJ_A, "aaaaaaaa", LEASE_FUTURE)]);
    let alias = lease_alias(PROJ_A, "aaaaaaaa");

    // The lease is in the moved folder: the registry stays under USERPROFILE.
    let mut c = Case::seeded("state-dir-moved", two_hosts(), None).args(&[&alias]);
    let seeding = c.setup.take().unwrap();
    let text = leases.clone();
    c.setup = Some(Box::new(move |sb: &Sandbox| {
        seeding(sb);
        let moved = sb.root.join("moved-state");
        fs::create_dir_all(&moved).unwrap();
        fs::write(moved.join("leases.json"), &text).unwrap();
    }));
    let c = c.env_path("AGENTSPACE_STATE_DIR", "moved-state");
    let o = run(c);
    assert_reached(&o, ID_C, &[ID_A]);

    // The lease is only in the default folder: the moved folder holds none, so the box answers.
    let mut c = Case::seeded("state-dir-default", two_hosts(), Some(leases)).args(&[&alias]);
    let seeding = c.setup.take().unwrap();
    c.setup = Some(Box::new(move |sb: &Sandbox| {
        seeding(sb);
        fs::create_dir_all(sb.root.join("moved-state")).unwrap();
    }));
    let c = c.env_path("AGENTSPACE_STATE_DIR", "moved-state");
    let o = run(c);
    assert_reached(&o, ID_A, &[ID_C]);
}

impl Case {
    /// Sets a variable to a folder under the case's throwaway root, once the sandbox exists.
    fn env_path(mut self, name: &str, folder: &str) -> Case {
        self.env_roots.push((name.to_string(), folder.to_string()));
        self
    }
}

#[test]
fn no_argument_and_the_agentspace_alias_reach_the_box_whatever_the_leases() {
    let leases = lease_file(&[ready_lease(PROJ_A, "aaaaaaaa", LEASE_FUTURE)]);
    let o = run(Case::seeded("box-noarg", two_hosts(), Some(leases.clone())));
    assert_reached(&o, ID_A, &[ID_B, ID_C, ID_D]);
    let o = run(Case::seeded("box-agentspace", two_hosts(), Some(leases)).args(&["agentspace"]));
    assert_reached(&o, ID_A, &[ID_B, ID_C, ID_D]);
}

#[test]
fn the_boxs_missing_instance_id_is_read_from_the_box_stack_not_a_hosts() {
    let reg = registry(&[host(PROJ_A, "ready", Some(ID_C))], None);
    let o = run(Case::seeded("box-id-from-stack", reg, None));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_eq!(
        o.calls().first().map(String::as_str),
        Some(
            "cloudformation describe-stacks --region us-west-2 --output json --stack-name agentbox"
        )
    );
    assert_eq!(o.target().as_deref(), Some(ID_B));
    assert_eq!(
        fs::read(o.sb.hosts_path()).unwrap(),
        registry(&[host(PROJ_A, "ready", Some(ID_C))], Some(ID_B)).into_bytes(),
        "only the box's record gains an ID"
    );
}

#[test]
fn a_stopped_host_is_started_and_waited_for_by_its_own_instance_and_its_calls_name_no_host() {
    let o = run(Case::seeded("host-stopped", two_hosts(), None)
        .args(&[&format!("agentspace-host.{PROJ_A}")])
        .aws(Aws {
            state: "stopped",
            noise: true,
            ..Aws::default()
        }));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_stdout_empty(&o);
    assert_stderr_is(&o, &[STARTING, READY]);
    assert_names_no_host(&o);
    assert_eq!(
        o.count(&format!(
            "ec2 start-instances --region us-west-2 --output json --instance-ids {ID_C}"
        )),
        1
    );
    assert!(
        o.calls()
            .iter()
            .all(|c| !c.contains(ID_A) && !c.contains(ID_B) && !c.contains(ID_D)),
        "no call names another host's instance"
    );
    assert_eq!(o.target().as_deref(), Some(ID_C));
}

#[test]
fn a_host_that_is_not_available_says_so_without_naming_it() {
    let o = run(Case::seeded("host-terminated", two_hosts(), None)
        .args(&[&format!("agentspace-host.{PROJ_A}")])
        .aws(Aws {
            state: "terminated",
            noise: true,
            ..Aws::default()
        }));
    assert_failure(&o, NOT_AVAILABLE);
    assert_stderr_is(&o, &[NOT_AVAILABLE]);
    assert_names_no_host(&o);
}

#[test]
fn every_failure_of_a_hosts_connection_names_no_project_stack_or_instance() {
    let cases: Vec<(&str, Aws, &str)> = vec![
        (
            "describe",
            Aws {
                fail: fail("ec2", "describe-instances", 254, ERR_TEXT),
                noise: true,
                ..Aws::default()
            },
            COULD_NOT_REACH,
        ),
        (
            "wait",
            Aws {
                state: "stopping",
                wait_code: 255,
                fail: fail("ec2", "start-instances", 254, ERR_TEXT),
                noise: true,
                ..Aws::default()
            },
            COULD_NOT_REACH,
        ),
    ];
    for (name, aws, last) in cases {
        let o = run(
            Case::seeded(&format!("host-fail-{name}"), two_hosts(), None)
                .args(&[&format!("agentspace-host.{PROJ_A}")])
                .aws(aws),
        );
        assert_failure(&o, last);
        assert_names_no_host(&o);
    }
    // A tunnel that fails, whose stderr is withheld.
    let o = run(Case::seeded("host-tunnel-fail", two_hosts(), None)
        .args(&[&format!("agentspace-host.{PROJ_A}")])
        .aws(Aws {
            tunnel_code: 3,
            tunnel_stderr: true,
            ..Aws::default()
        }));
    assert_eq!(o.code, Some(3));
    assert_stdout_empty(&o);
    assert_stderr_is(&o, &[READY, CONN_FAILED]);
    assert_names_no_host(&o);
}

// ---------------------------------------------------------------------------------------------
// The test seam, and what a caller never sees
// ---------------------------------------------------------------------------------------------

#[test]
fn a_seam_value_that_names_nothing_runnable_never_falls_back() {
    let missing = std::env::temp_dir()
        .join("agentspace-ssh-proxy-tests-no-such-program")
        .join("aws.cmd");
    let folder = std::env::temp_dir();
    for (i, v) in [missing.into_os_string(), folder.into_os_string()]
        .into_iter()
        .enumerate()
    {
        let mut c = Case::new(&format!("seam-{i}"));
        c.aws_cmd = Some(v);
        let o = run(c);
        assert_failure(&o, COULD_NOT_REACH);
        assert_stderr_is(&o, &[COULD_NOT_REACH]);
        assert!(o.calls().is_empty());
        assert!(o.no_ssh_or_scp());
    }
}

#[test]
fn no_scenario_shows_backend_text_to_the_caller() {
    let scenarios: Vec<(&str, Aws, Vec<&str>)> = vec![
        ("sweep-ready", Aws::default(), vec![READY]),
        (
            "sweep-stopped",
            Aws {
                state: "stopped",
                ..Aws::default()
            },
            vec![STARTING, READY],
        ),
        (
            "sweep-stopping",
            Aws {
                state: "stopping",
                ..Aws::default()
            },
            vec![STARTING, READY],
        ),
        (
            "sweep-unavailable",
            Aws {
                state: "terminated",
                ..Aws::default()
            },
            vec![NOT_AVAILABLE],
        ),
        (
            "sweep-generic",
            Aws {
                fail: fail("ec2", "describe-instances", 254, ERR_TEXT),
                ..Aws::default()
            },
            vec![COULD_NOT_REACH],
        ),
        (
            "sweep-tunnel",
            Aws {
                tunnel_code: 3,
                ..Aws::default()
            },
            vec![READY, CONN_FAILED],
        ),
    ];
    for (name, mut aws, expect) in scenarios {
        aws.noise = true;
        aws.tunnel_stderr = true;
        let o = run(Case::new(name).aws(aws));
        assert_stderr_is(&o, &expect);
        assert_no_backend(&o, &o.err_text());
        assert!(
            !o.stdout.windows(6).any(|w| w == b"MARKER"),
            "{name}: no stand-in text on stdout"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The host registry's messages, which only AGENTSPACE_DIAGNOSTIC shows (operator.md, The host
// registry, as the operator uses it)
// ---------------------------------------------------------------------------------------------

#[test]
fn a_registry_that_cannot_be_read_gives_its_message_as_detail_only_under_diagnostic() {
    for (i, cache) in [Cache::Folder, Cache::Raw(b"not json".to_vec())]
        .into_iter()
        .enumerate()
    {
        let o = run(Case::new(&format!("registry-diag-{i}"))
            .cache(cache)
            .env("AGENTSPACE_DIAGNOSTIC", "1"));
        assert_eq!(o.code, Some(1));
        assert_stdout_empty(&o);
        assert!(o.tunnel_calls().is_empty());
        let lines = o.err_lines();
        assert_eq!(
            lines.len(),
            2,
            "the caller's line, then the detail: {lines:?}"
        );
        assert_eq!(lines[0], COULD_NOT_REACH);
        let prefix = format!(
            "agentspace: The host registry {} cannot be read: ",
            o.sb.hosts_path().display()
        );
        assert!(
            lines[1].starts_with(&prefix) && lines[1].ends_with('.'),
            "the detail is operator.md's message naming the file: {:?}",
            lines[1]
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The token push: over the alias, in the caller's words, and never under AGENTSPACE_NESTED
// ---------------------------------------------------------------------------------------------

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

/// A receipt and a dummy Bot credential in the throwaway `LOCALAPPDATA`, and an ssh stand-in that
/// records its arguments and the names of its environment, and answers nothing.
fn push_setup(sb: &Sandbox) {
    let swim = sb.localapp.join("localswim-v2");
    let creds = swim.join("credentials").join("dummy");
    fs::create_dir_all(&creds).unwrap();
    let dir = creds.to_string_lossy().replace('\\', "\\\\");
    fs::write(
        swim.join("service.json"),
        format!(
            "{{\"base\":\"http://127.0.0.1:1\",\"credential_directory\":\"{dir}\",\"executable\":\"not-used.exe\"}}"
        ),
    )
    .unwrap();
    let token: Vec<u8> = "Dummy_proxy_test_token_0123456789abcdefABCD"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let hex: String = win_calls::protect(&token)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    fs::write(
        creds.join("bot.json"),
        format!(
            "{{\"format\":\"localswim-credential\",\"format_version\":1,\"role\":\"bot\",\"token_dpapi_hex\":\"{hex}\"}}"
        ),
    )
    .unwrap();
    let mut s = String::new();
    line(&mut s, "@echo off");
    line(&mut s, ">>\"%~dp0ssh.log\" echo %*");
    line(
        &mut s,
        ">\"%~dp0ssh-env.txt\" echo NESTED=%AGENTSPACE_NESTED%",
    );
    line(
        &mut s,
        "for /f \"delims==\" %%V in ('set') do >>\"%~dp0ssh-env.txt\" echo NAME=%%V",
    );
    line(&mut s, "exit /b 0");
    fs::write(sb.standins.join("ssh.cmd"), s).unwrap();
}

const HOST_NOT_CONFIRMED: &str = "agentspace: The host did not confirm the localswim token; the board will not work on the box this session.";

fn ssh_calls(o: &Out) -> Vec<String> {
    read_text(&o.sb.standins.join("ssh.log"))
        .map(|t| t.lines().map(norm).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

#[test]
fn the_push_runs_over_the_alias_and_warns_in_the_callers_words() {
    for (i, (args, alias)) in [(vec!["lease-x"], "lease-x"), (vec![], "agentspace")]
        .into_iter()
        .enumerate()
    {
        let mut c = Case::new(&format!("push-{i}"))
            .args(&args)
            .env(POSIX_A, "1");
        c.setup = Some(Box::new(push_setup));
        let o = run(c);
        assert_eq!(o.code, Some(0), "the push never fails the ProxyCommand");
        assert_stdout_empty(&o);
        // No "Sending" line: its caller line is nothing.
        assert_stderr_is(&o, &[READY, HOST_NOT_CONFIRMED]);
        let calls = ssh_calls(&o);
        assert_eq!(calls.len(), 1, "one push ssh");
        assert!(
            calls[0].starts_with(&format!(
                "-o ClearAllForwardings=yes -o BatchMode=yes {alias} "
            )),
            "the push's ssh is not over the alias"
        );
        let env = read_text(&o.sb.standins.join("ssh-env.txt")).unwrap_or_default();
        assert!(
            env.lines().any(|l| l.trim_end() == "NESTED=1"),
            "the push's ssh MUST see AGENTSPACE_NESTED set to 1"
        );
        assert!(
            !env.lines()
                .any(|l| l.trim_end().to_uppercase().ends_with("_POSIX_FD_STATE")),
            "the push's ssh MUST see no *_POSIX_FD_STATE variable"
        );
        assert_eq!(o.tunnel_calls().len(), 1);
    }
}

#[test]
fn a_nested_proxycommand_does_not_push() {
    for (i, nested) in ["1", "yes"].into_iter().enumerate() {
        let mut c = Case::new(&format!("push-nested-{i}"))
            .args(&["lease-x"])
            .env("AGENTSPACE_NESTED", nested);
        c.setup = Some(Box::new(push_setup));
        let o = run(c);
        assert_eq!(o.code, Some(0));
        assert_stdout_empty(&o);
        assert_stderr_is(&o, &[READY]);
        assert!(o.no_ssh_or_scp(), "a nested ProxyCommand starts no push");
        assert_eq!(o.tunnel_calls().len(), 1);
    }
}

#[test]
fn an_empty_nested_variable_is_not_nested() {
    let mut c = Case::new("push-nested-empty")
        .args(&["lease-x"])
        .env("AGENTSPACE_NESTED", "");
    c.setup = Some(Box::new(push_setup));
    let o = run(c);
    assert_eq!(o.code, Some(0));
    assert_stderr_is(&o, &[READY, HOST_NOT_CONFIRMED]);
    assert_eq!(ssh_calls(&o).len(), 1, "the push ran");
}

// ---------------------------------------------------------------------------------------------
// The push goes to the host the connection reaches, with that host's own stamp (D50; operator.md,
// The token push, When it runs and The stamp; state.md, A host's push stamp)
// ---------------------------------------------------------------------------------------------

const PUSH_TOKEN: &str = "Dummy_proxy_test_token_0123456789abcdefABCD";
const PUSH_URL: &str = "http://127.0.0.1:1";
const PUSH_ZONE: &str = "America/New_York";

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

/// The stamp over `<instance ID> LF <zone> LF <content>`, as a stamp file holds it.
fn stamp_file(instance_id: &str) -> String {
    let input = format!("{instance_id}\n{PUSH_ZONE}\n{}", push_content());
    format!(
        "{{\"version\":0,\"stamp\":\"{}\"}}",
        hex(&Sha256::digest(input.as_bytes()), true)
    )
}

/// An ssh stand-in that logs its arguments, reads its stdin and confirms the token and the zone
/// as the box does (operator.md, What the box does).
fn confirming_ssh(sb: &Sandbox) {
    let mut s = String::new();
    line(&mut s, "@echo off");
    line(&mut s, ">>\"%~dp0ssh.log\" echo %*");
    line(&mut s, "findstr \"^\" >nul 2>nul");
    line(&mut s, &format!("echo {}  -", content_hash()));
    line(&mut s, &format!("echo zone {PUSH_ZONE}"));
    line(&mut s, "exit /b 0");
    fs::write(sb.standins.join("ssh.cmd"), s).unwrap();
}

#[derive(Clone, Copy)]
enum StampAt {
    TheBox,
    Host(&'static str),
}

fn stamp_path(sb: &Sandbox, at: StampAt) -> PathBuf {
    match at {
        StampAt::TheBox => sb.box_stamp_path(),
        StampAt::Host(p) => sb.host_stamp_path(p),
    }
}

/// A case with a receipt and a Bot credential, an ssh stand-in that does or does not confirm the
/// push, and these stamps already on disk.
fn push_case(
    name: &str,
    registry_text: String,
    leases_text: Option<String>,
    args: &[&str],
    confirms: bool,
    stamps: Vec<(StampAt, String)>,
) -> Case {
    let mut c = Case::seeded(name, registry_text, leases_text).args(args);
    let seeding = c.setup.take().unwrap();
    c.setup = Some(Box::new(move |sb: &Sandbox| {
        seeding(sb);
        push_setup(sb);
        if confirms {
            confirming_ssh(sb);
        }
        for (at, text) in &stamps {
            let path = stamp_path(sb, *at);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
    }));
    c
}

fn assert_pushed_over(o: &Out, alias: &str) {
    let calls = ssh_calls(o);
    assert_eq!(calls.len(), 1, "one push ssh");
    assert!(
        calls[0].starts_with(&format!(
            "-o ClearAllForwardings=yes -o BatchMode=yes {alias} "
        )),
        "the push's ssh is not over the alias {alias}"
    );
}

fn assert_stamp(o: &Out, at: StampAt, want: Option<String>) {
    let got = fs::read(stamp_path(&o.sb, at)).ok();
    assert_eq!(
        got,
        want.map(String::into_bytes),
        "a stamp's bytes: written only after the host confirms, and compact"
    );
}

#[test]
fn a_push_to_a_host_is_over_its_operator_alias_and_writes_that_hosts_stamp() {
    let alias = format!("agentspace-host.{PROJ_A}");
    let o = run(push_case(
        "push-host",
        two_hosts(),
        None,
        &[&alias],
        true,
        vec![],
    ));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_stdout_empty(&o);
    assert_stderr_is(&o, &[READY]);
    assert_names_no_host(&o);
    assert_eq!(o.target().as_deref(), Some(ID_C));
    assert_pushed_over(&o, &alias);
    assert_stamp(&o, StampAt::Host(PROJ_A), Some(stamp_file(ID_C)));
    assert_stamp(&o, StampAt::TheBox, None);
    assert_stamp(&o, StampAt::Host(PROJ_B), None);
    assert!(
        fs::read_dir(o.sb.host_folder(PROJ_A))
            .unwrap()
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().contains(".tmp")),
        "no temporary file is left in the host's folder"
    );
}

#[test]
fn a_host_whose_stamp_matches_is_not_pushed_to() {
    let o = run(push_case(
        "push-host-current",
        two_hosts(),
        None,
        &[&format!("agentspace-host.{PROJ_A}")],
        true,
        vec![(StampAt::Host(PROJ_A), stamp_file(ID_C))],
    ));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_stderr_is(&o, &[READY]);
    assert_eq!(o.target().as_deref(), Some(ID_C));
    assert!(o.no_ssh_or_scp(), "a current stamp means nothing is sent");
    assert_stamp(&o, StampAt::Host(PROJ_A), Some(stamp_file(ID_C)));
}

#[test]
fn the_stamp_covers_the_hosts_own_instance_id_from_the_registry() {
    // A stamp over another instance ID, the box's, does not match this host's.
    let alias = format!("agentspace-host.{PROJ_A}");
    let o = run(push_case(
        "push-host-other-id",
        two_hosts(),
        None,
        &[&alias],
        true,
        vec![(StampAt::Host(PROJ_A), stamp_file(ID_A))],
    ));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_pushed_over(&o, &alias);
    assert_stamp(&o, StampAt::Host(PROJ_A), Some(stamp_file(ID_C)));
}

#[test]
fn a_connection_to_one_host_never_reads_or_writes_another_hosts_or_the_boxs_stamp() {
    let alias = format!("agentspace-host.{PROJ_A}");
    // The box's and the other host's stamps are over this host's instance ID, so either would
    // match if it were consulted.
    let o = run(push_case(
        "push-host-isolated",
        two_hosts(),
        None,
        &[&alias],
        true,
        vec![
            (StampAt::TheBox, stamp_file(ID_C)),
            (StampAt::Host(PROJ_B), stamp_file(ID_C)),
        ],
    ));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_pushed_over(&o, &alias);
    assert_stamp(&o, StampAt::Host(PROJ_A), Some(stamp_file(ID_C)));
    assert_stamp(&o, StampAt::TheBox, Some(stamp_file(ID_C)));
    assert_stamp(&o, StampAt::Host(PROJ_B), Some(stamp_file(ID_C)));
}

#[test]
fn the_boxs_connection_reads_and_writes_the_boxs_stamp_alone() {
    // A host's stamp that would match the box's instance ID is not consulted.
    let o = run(push_case(
        "push-box",
        two_hosts(),
        None,
        &[],
        true,
        vec![(StampAt::Host(PROJ_A), stamp_file(ID_A))],
    ));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_stderr_is(&o, &[READY]);
    assert_eq!(o.target().as_deref(), Some(ID_A));
    assert_pushed_over(&o, "agentspace");
    assert_stamp(&o, StampAt::TheBox, Some(stamp_file(ID_A)));
    assert_stamp(&o, StampAt::Host(PROJ_A), Some(stamp_file(ID_A)));

    // With the box's own stamp current, nothing is sent.
    let o = run(push_case(
        "push-box-current",
        two_hosts(),
        None,
        &[],
        true,
        vec![(StampAt::TheBox, stamp_file(ID_A))],
    ));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert!(o.no_ssh_or_scp());
    assert_stamp(&o, StampAt::TheBox, Some(stamp_file(ID_A)));
}

#[test]
fn a_lease_alias_pushes_over_itself_with_its_hosts_stamp() {
    let alias = lease_alias(PROJ_A, "aaaaaaaa");
    let leases = lease_file(&[ready_lease(PROJ_A, "aaaaaaaa", LEASE_FUTURE)]);
    let o = run(push_case(
        "push-lease",
        two_hosts(),
        Some(leases.clone()),
        &[&alias],
        true,
        vec![],
    ));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_stderr_is(&o, &[READY]);
    assert_names_no_host(&o);
    assert_eq!(o.target().as_deref(), Some(ID_C));
    assert_pushed_over(&o, &alias);
    assert_stamp(&o, StampAt::Host(PROJ_A), Some(stamp_file(ID_C)));
    assert_stamp(&o, StampAt::TheBox, None);

    let o = run(push_case(
        "push-lease-current",
        two_hosts(),
        Some(leases),
        &[&alias],
        true,
        vec![(StampAt::Host(PROJ_A), stamp_file(ID_C))],
    ));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_eq!(o.target().as_deref(), Some(ID_C));
    assert!(o.no_ssh_or_scp(), "the host's current stamp means no push");
}

#[test]
fn a_lease_alias_that_falls_back_to_the_box_pushes_with_the_boxs_stamp() {
    let alias = lease_alias(PROJ_A, "aaaaaaaa");
    // No record of the lease: the box answers, and the box's stamp is the one used.
    let o = run(push_case(
        "push-lease-box",
        two_hosts(),
        None,
        &[&alias],
        true,
        vec![],
    ));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_eq!(o.target().as_deref(), Some(ID_A));
    assert_pushed_over(&o, &alias);
    assert_stamp(&o, StampAt::TheBox, Some(stamp_file(ID_A)));
    assert_stamp(&o, StampAt::Host(PROJ_A), None);
}

#[test]
fn a_host_that_does_not_confirm_keeps_its_stamp_and_the_warning_names_nothing() {
    let stale = stamp_file(ID_A);
    let o = run(push_case(
        "push-host-unconfirmed",
        two_hosts(),
        None,
        &[&format!("agentspace-host.{PROJ_A}")],
        false,
        vec![(StampAt::Host(PROJ_A), stale.clone())],
    ));
    assert_eq!(o.code, Some(0), "the push never fails the ProxyCommand");
    assert_stdout_empty(&o);
    assert_stderr_is(&o, &[READY, HOST_NOT_CONFIRMED]);
    assert_names_no_host(&o);
    assert_eq!(ssh_calls(&o).len(), 1);
    assert_stamp(&o, StampAt::Host(PROJ_A), Some(stale));
    assert_stamp(&o, StampAt::TheBox, None);
}

#[test]
fn a_nested_proxycommand_to_a_host_does_not_push() {
    let o = run(push_case(
        "push-host-nested",
        two_hosts(),
        None,
        &[&format!("agentspace-host.{PROJ_A}")],
        true,
        vec![],
    )
    .env("AGENTSPACE_NESTED", "1"));
    assert_eq!(o.code, Some(0), "stderr:\n{}", o.err_text());
    assert_stderr_is(&o, &[READY]);
    assert_eq!(o.target().as_deref(), Some(ID_C));
    assert!(o.no_ssh_or_scp());
    assert_stamp(&o, StampAt::Host(PROJ_A), None);
}
