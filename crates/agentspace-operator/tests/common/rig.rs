//! The rig for the operator's commands: one throwaway world per test, the stand-ins for the AWS
//! CLI, ssh and scp that docs/operator.md's test seam names, and readers for what they recorded.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! **What a rig is.** A folder under the temp folder that holds a home (`USERPROFILE`), a second
//! home (`HOME`), `LOCALAPPDATA`, `APPDATA`, a `SystemRoot` that holds nothing, a temp folder, a
//! working folder, the stand-ins (in a folder whose name has a space, so every test shows that the
//! seam's value is taken whole), and the stand-ins' control folder. A run of the operator gets
//! exactly these, never the test process's own environment.
//!
//! **What the stand-ins are.** One small program, compiled from `standin_src.rs` once per test
//! process and copied to `aws.exe`, `ssh.exe` and `scp.exe`. It records each call (its arguments,
//! its stdin, the time, whether the secret reached it) and answers from files the test wrote.
//!
//! **A test MUST NOT print a captured environment's values, nor the token.** Nothing here
//! formats an environment, and every helper reports a mismatch by name or count.
#![allow(dead_code)]
#![allow(unsafe_code)]

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
};

/// A made-up instance ID, one that the repository's identifier scan does not match.
pub const ID: &str = "i-1234567890abcdef0";
/// A second one, for a test that needs the cache to say something else than the stack does.
pub const OTHER_ID: &str = "i-fedcba9876543210f";
/// A third one, for a test with the box and two more hosts.
pub const THIRD_ID: &str = "i-abcdef1234567890a";
/// A made-up volume ID.
pub const VOL: &str = "vol-1234567890abcdef0";
/// The written text of #0034's message (operator.md, AWS failures).
pub const SIGN_IN: &str = "agentspace: AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry.";
/// What a test writes to the operator's stdin: no program the operator starts may see it.
pub const SENTINEL: &str = "SENTINEL-FOR-THE-OPERATOR-ALONE\n";
/// The credential folder's name under localswim's folder; any name works (operator.md, The receipt).
const GUID: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";
/// The base URL of a fake localswim service.
pub const BASE: &str = "http://127.0.0.1:8811";

// ---------------------------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------------------------

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    to_hex(&h.finalize())
}

/// The stamp of operator.md, The stamp: SHA-256 of `<id> LF <zone> LF <content>`, uppercase hex.
pub fn expected_stamp(instance: &str, zone: &str, content: &str) -> String {
    sha256_hex(format!("{instance}\n{zone}\n{content}").as_bytes()).to_ascii_uppercase()
}

/// A JSON string literal's body: backslashes and quotes escaped.
pub fn json_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// `CryptProtectData` on `plain` for the current user, with no entropy and
/// `CRYPTPROTECT_UI_FORBIDDEN`, as operator.md says a test writes a credential.
fn protect(plain: &[u8]) -> Vec<u8> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr() as *mut u8,
    };
    let mut out = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: `input` points at `plain`, which outlives the call; `out` is written by the call and
    // freed below with `LocalFree`, as the API requires.
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
        assert!(
            ok != 0 && !out.pbData.is_null(),
            "CryptProtectData failed for a dummy credential"
        );
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData as *mut core::ffi::c_void);
        bytes
    }
}

/// A Bot credential file as localswim writes it (localswim's windows-operations.md, "The service
/// receipt and credential files"; operator.md, The Bot credential).
fn bot_json(token_dpapi_hex: &str) -> String {
    format!(
        "{{\"format\":\"localswim-credential\",\"format_version\":1,\"role\":\"bot\",\"token_dpapi_hex\":\"{token_dpapi_hex}\"}}"
    )
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A made-up token that no other text in a run holds: 43 base64url characters.
pub fn dummy_token() -> String {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let text = format!(
        "Dummy-{:016x}-{:08x}-{:04x}-AAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        t as u64,
        std::process::id(),
        n
    );
    text.chars().take(43).collect()
}

/// The real clock's reading, in Unix seconds.
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The civil date and time of a Unix instant, in UTC: year, month, day, hour, minute, second.
fn civil(unix: i64) -> (i64, i64, i64, i64, i64, i64) {
    let days = unix.div_euclid(86_400);
    let rest = unix.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, rest / 3600, rest % 3600 / 60, rest % 60)
}

/// An ISO 8601 UTC time at the Unix instant `unix`, with an explicit offset, as EC2 prints it.
pub fn iso_at(unix: i64) -> String {
    let (y, m, d, h, mi, s) = civil(unix);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}+00:00")
}

/// A time in the lease contract's form, `YYYY-MM-DDTHH:MM:SSZ` (docs/state.md, The lease state file).
pub fn iso_z(unix: i64) -> String {
    let (y, m, d, h, mi, s) = civil(unix);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// An ISO 8601 UTC time `seconds_ago` seconds before now, with an explicit offset, as EC2 prints it.
pub fn iso_ago(seconds_ago: i64) -> String {
    iso_at(now_unix() - seconds_ago)
}

// ---------------------------------------------------------------------------------------------
// The lease state file's records (docs/state.md, The lease state file)
// ---------------------------------------------------------------------------------------------

/// A lease's ID that no other text in a run holds: `5eedc0de` begins it, so a test can look for it.
pub fn lease_id(n: u32) -> String {
    format!("5eedc0de-0000-4000-8000-{n:012x}")
}

/// The alias a lease's `ssh` names; `feed` begins its last group, so a test can look for it.
pub fn lease_alias(project: &str, n: u32) -> String {
    format!("agentspace-{project}-feed{n:04x}")
}

/// One record of the lease state file, compact, as state.md writes it. `state` is the state as
/// stored (`pending`, `ready`, `released` or `unsatisfiable`); `expires` and `released` are Unix
/// seconds, written only where the stored state allows them.
pub fn lease_rec(
    n: u32,
    project: &str,
    state: &str,
    expires: Option<i64>,
    released: Option<i64>,
) -> String {
    let mut r = format!(
        "{{\"id\":\"{}\",\"project\":\"{project}\",\"state\":\"{state}\",\"platform\":\"linux/arm64\",\"profile\":\"agent-linux\",\"size\":\"m\",\"ttl_seconds\":3600,\"created_at\":\"{}\"",
        lease_id(n),
        iso_z(now_unix() - 86_400)
    );
    if let Some(t) = expires {
        r.push_str(&format!(",\"expires_at\":\"{}\"", iso_z(t)));
    }
    if let Some(t) = released {
        r.push_str(&format!(",\"released_at\":\"{}\"", iso_z(t)));
    }
    if state == "ready" {
        r.push_str(&format!(
            ",\"ssh\":{{\"host_alias\":\"{}\",\"user\":\"ubuntu\"}}",
            lease_alias(project, n)
        ));
    }
    if state == "unsatisfiable" {
        r.push_str(",\"reasons\":[{\"code\":\"no_capacity\",\"message\":\"No capacity.\"}]");
    }
    r.push_str(",\"driver\":\"ec2\"}");
    r
}

/// A live `ready` lease of `project`: it expires in an hour.
pub fn ready_lease(n: u32, project: &str) -> String {
    lease_rec(n, project, "ready", Some(now_unix() + 3600), None)
}

/// A `pending` lease of `project`, which is live.
pub fn pending_lease(n: u32, project: &str) -> String {
    lease_rec(n, project, "pending", None, None)
}

/// The lease state file's document: `{"version":0,"leases":[...]}` and one LF.
pub fn leases_doc(records: &[String]) -> Vec<u8> {
    format!("{{\"version\":0,\"leases\":[{}]}}\n", records.join(",")).into_bytes()
}

/// An old launch time: more than 20 minutes ago, so `up` does not wait.
pub fn old_launch() -> String {
    "2020-01-01T00:00:00+00:00".to_string()
}

// ---------------------------------------------------------------------------------------------
// The host registry's records (docs/state.md, The host registry)
// ---------------------------------------------------------------------------------------------

/// A record as state.md writes it: compact, the members in its table's order.
pub fn rec(project: &str, stack: &str, state: &str, id: Option<&str>) -> String {
    let mut r = format!("{{\"project\":\"{project}\",\"stack\":\"{stack}\",\"state\":\"{state}\"");
    if let Some(id) = id {
        r.push_str(&format!(",\"instance_id\":\"{id}\""));
    }
    r.push('}');
    r
}

/// The box's record: project `swimcoach`, stack `agentbox`, `ready` (D43, D55).
pub fn box_rec(id: Option<&str>) -> String {
    rec("swimcoach", "agentbox", "ready", id)
}

/// The record of another host: its stack is `agentspace-host-<project>` (D44).
pub fn host_rec(project: &str, state: &str, id: Option<&str>) -> String {
    rec(project, &format!("agentspace-host-{project}"), state, id)
}

/// The registry's document: `{"version":0,"hosts":[...]}` and one LF.
pub fn hosts_doc(records: &[String]) -> Vec<u8> {
    format!("{{\"version\":0,\"hosts\":[{}]}}\n", records.join(",")).into_bytes()
}

// ---------------------------------------------------------------------------------------------
// The answers of the AWS CLI
// ---------------------------------------------------------------------------------------------

/// What the stack's `describe-stacks` answers: one `InstanceId` output, and an `InstanceType`
/// parameter of `m9g.xlarge`, which `deploy`'s step 5 reads back (P22, P44).
pub fn stack_json(id: &str) -> String {
    format!(
        "{{\"Stacks\":[{{\"StackName\":\"agentbox\",\"Parameters\":[{{\"ParameterKey\":\"InstanceType\",\"ParameterValue\":\"m9g.xlarge\"}}],\"Outputs\":[{{\"OutputKey\":\"Other\",\"OutputValue\":\"x\"}},{{\"OutputKey\":\"InstanceId\",\"OutputValue\":\"{id}\"}}]}}]}}"
    )
}

/// What `ec2 describe-instances` answers. A `None` leaves the member out.
pub fn instance_json(
    state: &str,
    launch: Option<&str>,
    itype: Option<&str>,
    root: Option<&str>,
    volumes: &[(&str, &str)],
) -> String {
    let mut members = vec![format!("\"State\":{{\"Name\":\"{state}\"}}")];
    if let Some(l) = launch {
        members.push(format!("\"LaunchTime\":\"{l}\""));
    }
    if let Some(t) = itype {
        members.push(format!("\"InstanceType\":\"{t}\""));
    }
    if let Some(r) = root {
        members.push(format!("\"RootDeviceName\":\"{r}\""));
    }
    let maps: Vec<String> = volumes
        .iter()
        .map(|(dev, vol)| {
            format!("{{\"DeviceName\":\"{dev}\",\"Ebs\":{{\"VolumeId\":\"{vol}\"}}}}")
        })
        .collect();
    members.push(format!("\"BlockDeviceMappings\":[{}]", maps.join(",")));
    format!(
        "{{\"Reservations\":[{{\"Instances\":[{{{}}}]}}]}}",
        members.join(",")
    )
}

/// A full answer: root device `/dev/sda1`, the type `m9g.xlarge`, and the root volume `VOL`.
pub fn full_instance(state: &str, launch: &str) -> String {
    instance_json(
        state,
        Some(launch),
        Some("m9g.xlarge"),
        Some("/dev/sda1"),
        &[("/dev/sda1", VOL)],
    )
}

pub fn ssm_json(ping: &str) -> String {
    format!("{{\"InstanceInformationList\":[{{\"PingStatus\":\"{ping}\"}}]}}")
}

pub fn invocation_json(status: &str, stdout: &str) -> String {
    format!("{{\"Status\":\"{status}\",\"StandardOutputContent\":\"{stdout}\\n\"}}")
}

// ---------------------------------------------------------------------------------------------
// The stand-in program
// ---------------------------------------------------------------------------------------------

/// The compiled stand-in, built once per test process into the target's temporary folder and
/// reused across runs by a name that holds the hash of its source.
fn standin_exe() -> &'static Path {
    static EXE: OnceLock<PathBuf> = OnceLock::new();
    EXE.get_or_init(|| {
        let src = include_str!("standin_src.rs");
        let tag = sha256_hex(src.as_bytes())[..16].to_string();
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("operator-standin");
        fs::create_dir_all(&dir).expect("the stand-in's build folder");
        let exe = dir.join(format!("standin-{tag}.exe"));
        if exe.is_file() {
            return exe;
        }
        let pid = std::process::id();
        let source = dir.join(format!("standin-{tag}-{pid}.rs"));
        let tmp = dir.join(format!("standin-{tag}-{pid}.tmp.exe"));
        fs::write(&source, src).expect("the stand-in's source");
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
        let built = Command::new(rustc)
            .args(["--edition", "2024", "--crate-name", "standin"])
            .args(["-C", "debuginfo=0", "-C", "opt-level=1"])
            .arg(&source)
            .arg("-o")
            .arg(&tmp)
            .current_dir(&dir)
            .stdin(Stdio::null())
            .output()
            .expect("rustc starts, to build the test stand-in");
        let _ = fs::remove_file(&source);
        assert!(
            built.status.success(),
            "rustc could not build the stand-in: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        if fs::rename(&tmp, &exe).is_err() {
            // Another test process built it first.
            let _ = fs::remove_file(&tmp);
            assert!(exe.is_file(), "the stand-in's build is missing");
        }
        exe
    })
}

/// One call a stand-in recorded.
#[derive(Debug, Clone)]
pub struct Call {
    pub index: u32,
    /// `aws`, `ssh` or `scp`.
    pub role: String,
    /// `<service>-<operation>` for `aws`, the role otherwise.
    pub key: String,
    /// How many times that key was called, this one included.
    pub n: u32,
    pub time_ms: u128,
    pub args: Vec<String>,
    /// The value of AGENTSPACE_NESTED the stand-in saw, or `-` when it was not set.
    pub nested: String,
    /// How many variables whose names end `_POSIX_FD_STATE` the stand-in saw.
    pub fd_state: u32,
    /// Whether the secret (`secret.txt`) was in an argument or an environment variable's value.
    pub secret: bool,
    pub stdin: Vec<u8>,
}

impl Call {
    /// The arguments of an AWS call that follow `--region us-west-2` and `--output json`.
    pub fn aws_rest(&self) -> Vec<String> {
        let mut rest: Vec<String> = self.args.iter().skip(2).cloned().collect();
        let mut i = 0;
        while i < rest.len() {
            if rest[i] == "--region" || rest[i] == "--output" {
                let end = (i + 2).min(rest.len());
                rest.drain(i..end);
            } else {
                i += 1;
            }
        }
        rest
    }
}

// ---------------------------------------------------------------------------------------------
// The rig
// ---------------------------------------------------------------------------------------------

pub struct Rig {
    root: tempfile::TempDir,
    pub profile: PathBuf,
    pub home: PathBuf,
    pub local_app_data: PathBuf,
    pub app_data: PathBuf,
    pub system_root: PathBuf,
    pub temp: PathBuf,
    pub work: PathBuf,
    pub standins: PathBuf,
    pub ctl: PathBuf,
    pub token: String,
}

impl Rig {
    pub fn new() -> Rig {
        let root = tempfile::tempdir().expect("a throwaway world");
        let at = |name: &str| {
            let p = root.path().join(name);
            fs::create_dir_all(&p).expect("a throwaway folder");
            p
        };
        let rig = Rig {
            profile: at("profile"),
            home: at("home"),
            local_app_data: at("localappdata"),
            app_data: at("appdata"),
            system_root: at("systemroot"),
            temp: at("temp"),
            work: at("work"),
            standins: at("stand ins"),
            ctl: at("ctl"),
            token: dummy_token(),
            root,
        };
        fs::create_dir_all(rig.ctl.join("resp")).expect("the answers' folder");
        for name in ["aws", "ssh", "scp"] {
            fs::copy(standin_exe(), rig.standins.join(format!("{name}.exe")))
                .expect("a stand-in's copy");
        }
        rig
    }

    /// A rig whose AWS CLI answers every call the commands make, as a healthy, old, running box.
    pub fn standard() -> Rig {
        let rig = Rig::new();
        rig.standard_answers();
        rig
    }

    pub fn root(&self) -> &Path {
        self.root.path()
    }

    pub fn aws_exe(&self) -> PathBuf {
        self.standins.join("aws.exe")
    }
    pub fn ssh_exe(&self) -> PathBuf {
        self.standins.join("ssh.exe")
    }
    pub fn scp_exe(&self) -> PathBuf {
        self.standins.join("scp.exe")
    }

    // ---- the stand-ins' answers ----

    /// Sets the answer to call `n` (or every call) of `key`: its stdout, stderr and exit code.
    pub fn answer(
        &self,
        key: &str,
        n: Option<u32>,
        out: Option<&str>,
        err: Option<&str>,
        code: Option<i32>,
    ) {
        let base = match n {
            Some(n) => format!("{key}.{n}"),
            None => key.to_string(),
        };
        let resp = self.ctl.join("resp");
        for (ext, text) in [
            ("out", out.map(str::to_string)),
            ("err", err.map(str::to_string)),
            ("code", code.map(|c| c.to_string())),
        ] {
            let path = resp.join(format!("{base}.{ext}"));
            match text {
                Some(t) => fs::write(&path, t).expect("an answer"),
                None => {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }

    /// Answers every call of `key` with `out` and exit 0.
    pub fn ok(&self, key: &str, out: &str) {
        self.answer(key, None, Some(out), None, Some(0));
    }

    /// Answers call `n` of `key` with `out` and exit 0.
    pub fn ok_nth(&self, key: &str, n: u32, out: &str) {
        self.answer(key, Some(n), Some(out), None, Some(0));
    }

    /// Makes every call of `key` fail with `err` on stderr and `code`.
    pub fn fail(&self, key: &str, err: &str, code: i32) {
        self.answer(key, None, None, Some(err), Some(code));
    }

    /// Makes call `n` of `key` fail.
    pub fn fail_nth(&self, key: &str, n: u32, err: &str, code: i32) {
        self.answer(key, Some(n), None, Some(err), Some(code));
    }

    pub fn sleep(&self, key: &str, millis: u32) {
        fs::write(
            self.ctl.join("resp").join(format!("{key}.sleep")),
            millis.to_string(),
        )
        .expect("a pause");
    }

    pub fn standard_answers(&self) {
        self.ok("cloudformation-describe-stacks", &stack_json(ID));
        self.ok(
            "ec2-describe-instances",
            &full_instance("running", &old_launch()),
        );
        self.ok("ec2-describe-volumes", "{\"Volumes\":[{\"Size\":8}]}");
        self.ok("ec2-start-instances", "{}");
        self.ok("ec2-stop-instances", "{}");
        self.ok("ec2-modify-volume", "{}");
        self.ok("ec2-wait", "");
        self.ok("ssm-describe-instance-information", &ssm_json("Online"));
        self.ok(
            "ssm-send-command",
            "{\"Command\":{\"CommandId\":\"cmd-1234\"}}",
        );
        self.ok(
            "ssm-get-command-invocation",
            &invocation_json("Success", "patched"),
        );
    }

    /// The box is in `state`, launched long ago.
    pub fn instance(&self, state: &str) {
        self.ok(
            "ec2-describe-instances",
            &full_instance(state, &old_launch()),
        );
    }

    // ---- the laptop's files ----

    pub fn state_dir(&self) -> PathBuf {
        self.profile.join(".agentspace")
    }

    /// The retired instance-ID cache (D45). No v0.07 command reads, writes or removes it; a test
    /// that leaves one shows that.
    pub fn leftover_cache_path(&self) -> PathBuf {
        self.state_dir().join("instance-id.json")
    }

    pub fn write_leftover_cache(&self, id: &str) {
        fs::create_dir_all(self.state_dir()).expect("the state folder");
        fs::write(
            self.leftover_cache_path(),
            format!("{{\"version\":0,\"instance_id\":\"{id}\"}}"),
        )
        .expect("the leftover cache");
    }

    pub fn leftover_cache_bytes(&self) -> Option<Vec<u8>> {
        fs::read(self.leftover_cache_path()).ok()
    }

    // ---- the host registry (docs/state.md, The host registry) ----

    pub fn hosts_path(&self) -> PathBuf {
        self.state_dir().join("hosts.json")
    }

    pub fn write_hosts(&self, records: &[String]) {
        self.write_hosts_bytes(&hosts_doc(records));
    }

    pub fn write_hosts_bytes(&self, bytes: &[u8]) {
        fs::create_dir_all(self.state_dir()).expect("the state folder");
        fs::write(self.hosts_path(), bytes).expect("the host registry");
    }

    pub fn hosts_bytes(&self) -> Option<Vec<u8>> {
        fs::read(self.hosts_path()).ok()
    }

    /// The registry as the box alone, with `id` as its instance ID: what v0.06's
    /// `write_cache(id)` was, after D45 (the retired cache's members are a record's).
    pub fn write_box_id(&self, id: &str) {
        self.write_hosts(&[box_rec(Some(id))]);
    }

    /// The registry of the box and one more host, each with an instance ID.
    pub fn write_box_and(&self, project: &str, state: &str, box_id: &str, id: Option<&str>) {
        self.write_hosts(&[box_rec(Some(box_id)), host_rec(project, state, id)]);
    }

    /// A host's folder, `hosts\<project>` under the state folder.
    pub fn host_dir(&self, project: &str) -> PathBuf {
        self.state_dir().join("hosts").join(project)
    }

    /// The host's operator alias file in the default ssh folder.
    pub fn host_alias_path(&self, project: &str) -> PathBuf {
        self.profile
            .join(".ssh")
            .join("agentspace.d")
            .join(format!("agentspace-host.{project}"))
    }

    /// Whether the rig's home holds any sign of the operator's key, certificate or alias for a host.
    pub fn signed_anything(&self) -> bool {
        self.state_dir().join("hosts").exists()
            || self.profile.join(".ssh").join("agentspace.d").exists()
            || self.profile.join(".ssh").join("config").exists()
    }

    /// The exact bytes of a registry that holds the box's record with `id`, as a first change
    /// writes it (state.md: compact JSON, members in the table's order, one LF).
    pub fn hosts_file_box(id: &str) -> Vec<u8> {
        hosts_doc(&[box_rec(Some(id))])
    }

    pub fn stamp_path(&self) -> PathBuf {
        self.state_dir().join("localswim-pushed.json")
    }

    pub fn stamp_bytes(&self) -> Option<Vec<u8>> {
        fs::read(self.stamp_path()).ok()
    }

    /// The stamp file's exact bytes for `zone` (docs/state.md, The push stamp).
    pub fn stamp_file(&self, zone: &str) -> Vec<u8> {
        format!(
            "{{\"version\":0,\"stamp\":\"{}\"}}",
            expected_stamp(ID, zone, &self.content())
        )
        .into_bytes()
    }

    pub fn write_stamp_for(&self, zone: &str) {
        fs::create_dir_all(self.state_dir()).expect("the state folder");
        fs::write(self.stamp_path(), self.stamp_file(zone)).expect("the stamp");
    }

    /// Whether the state folder holds a file of an atomic replace that did not finish.
    pub fn tmp_files(&self) -> Vec<String> {
        fs::read_dir(self.state_dir())
            .map(|d| {
                d.filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.ends_with(".tmp"))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The push's content for this rig's token (operator.md, The content).
    pub fn content(&self) -> String {
        format!("LOCALSWIM_URL={BASE}\nLOCALSWIM_TOKEN={}\n", self.token)
    }

    pub fn localswim_dir(&self) -> PathBuf {
        self.local_app_data.join("localswim-v2")
    }

    pub fn credential_dir(&self) -> PathBuf {
        self.localswim_dir().join("credentials").join(GUID)
    }

    /// Writes localswim's receipt and a Bot credential for this rig's token, as localswim does, and
    /// tells the stand-ins which text is secret.
    pub fn localswim(&self) {
        fs::create_dir_all(self.credential_dir()).expect("the credential folder");
        fs::write(
            self.localswim_dir().join("service.json"),
            format!(
                "{{\"base\":\"{BASE}\",\"credential_directory\":\"{}\",\"executable\":\"x.exe\"}}",
                json_escape(&self.credential_dir().to_string_lossy())
            ),
        )
        .expect("the receipt");
        let hex = to_hex(&protect(&utf16le(&self.token)));
        fs::write(self.credential_dir().join("bot.json"), bot_json(&hex)).expect("bot.json");
        fs::write(self.ctl.join("secret.txt"), &self.token).expect("the secret's name");
    }

    // ---- what the stand-ins recorded ----

    pub fn calls(&self) -> Vec<Call> {
        let Ok(text) = fs::read_to_string(self.ctl.join("calls.log")) else {
            return Vec::new();
        };
        let mut calls = Vec::new();
        let mut current: Option<Call> = None;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("CALL ") {
                let mut it = rest.split(' ');
                let index: u32 = it.next().unwrap().parse().unwrap();
                current = Some(Call {
                    index,
                    role: it.next().unwrap().to_string(),
                    key: it.next().unwrap().to_string(),
                    n: it.next().unwrap().parse().unwrap(),
                    time_ms: 0,
                    args: Vec::new(),
                    nested: String::new(),
                    fd_state: 0,
                    secret: false,
                    stdin: fs::read(self.ctl.join(format!("stdin-{index}.bin")))
                        .unwrap_or_default(),
                });
            } else if line == "END" {
                calls.push(current.take().expect("a record"));
            } else if let Some(c) = current.as_mut() {
                if let Some(v) = line.strip_prefix("TIME ") {
                    c.time_ms = v.parse().unwrap_or(0);
                } else if let Some(v) = line.strip_prefix("ARG ") {
                    c.args.push(v.to_string());
                } else if let Some(v) = line.strip_prefix("NESTED ") {
                    c.nested = v.to_string();
                } else if let Some(v) = line.strip_prefix("FDSTATE ") {
                    c.fd_state = v.parse().unwrap_or(0);
                } else if let Some(v) = line.strip_prefix("SECRET ") {
                    c.secret = v == "1";
                }
            }
        }
        calls.sort_by_key(|c| c.index);
        calls
    }

    /// The keys of the calls, in order: `ec2-describe-instances`, `ssh`, ...
    pub fn keys(&self) -> Vec<String> {
        self.calls().into_iter().map(|c| c.key).collect()
    }

    /// The calls of one key, in order.
    pub fn calls_of(&self, key: &str) -> Vec<Call> {
        self.calls().into_iter().filter(|c| c.key == key).collect()
    }

    /// Every file under the rig that holds the token, as UTF-8 or UTF-16LE, but the stand-ins' own
    /// control folder (which holds what they were sent, the push's stdin among it).
    pub fn files_holding_the_token(&self) -> Vec<PathBuf> {
        let utf8 = self.token.clone().into_bytes();
        let wide = utf16le(&self.token);
        let mut found = Vec::new();
        let mut stack = vec![self.root().to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path == self.ctl {
                    continue;
                }
                if path.is_dir() {
                    stack.push(path);
                } else if let Ok(bytes) = fs::read(&path)
                    && (contains(&bytes, &utf8) || contains(&bytes, &wide))
                {
                    found.push(path);
                }
            }
        }
        found
    }

    /// Every file and folder under the rig's home, relative, sorted: what a command left behind.
    pub fn tree(&self, under: &Path) -> Vec<String> {
        let mut found = Vec::new();
        let mut stack = vec![under.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                found.push(
                    path.strip_prefix(under)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .into_owned(),
                );
                if path.is_dir() {
                    stack.push(path);
                }
            }
        }
        found.sort();
        found
    }

    // ---- running the operator ----

    pub fn op(&self, args: &[&str]) -> Op<'_> {
        Op {
            rig: self,
            args: args.iter().map(OsString::from).collect(),
            env: Vec::new(),
            hold_stdin: false,
            real_windows: false,
            default_error_mode: false,
            exe: None,
            timeout: Duration::from_secs(90),
        }
    }

    /// A copy of the operator in a folder of the rig's own, with a file named
    /// `agentspace-ssh-proxy.exe` beside it, which is all a host's operator alias needs to name
    /// (operator.md, How each program finds the others: a program finds its siblings beside its
    /// own executable). Run the operator from it with [`Op::exe`]. The proxy file is never run.
    pub fn operator_with_proxy(&self) -> PathBuf {
        let dir = self.root().join("install");
        fs::create_dir_all(&dir).expect("an install folder");
        let operator = dir.join("agentspace-operator.exe");
        fs::copy(env!("CARGO_BIN_EXE_agentspace-operator"), &operator).expect("a copy");
        fs::write(dir.join("agentspace-ssh-proxy.exe"), b"not a program").expect("a stand-in");
        operator
    }

    /// A copy of the operator with no proxy beside it.
    pub fn operator_without_proxy(&self) -> PathBuf {
        let dir = self.root().join("install-bare");
        fs::create_dir_all(&dir).expect("an install folder");
        let operator = dir.join("agentspace-operator.exe");
        fs::copy(env!("CARGO_BIN_EXE_agentspace-operator"), &operator).expect("a copy");
        operator
    }

    /// Runs the command and returns what it did.
    pub fn run(&self, args: &[&str]) -> Out {
        self.op(args).run()
    }
}

pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// What one run of the operator gave.
#[derive(Debug)]
pub struct Out {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub elapsed: Duration,
    /// When the run was seen to end, in milliseconds since the Unix epoch: the clock a stand-in's
    /// [`Call::time_ms`] is read from, so the time from a call's start to the run's end can be
    /// bounded without counting the operator's start and its earlier calls, which a loaded
    /// machine slows most (#0189).
    pub ended_ms: u128,
}

impl Out {
    pub fn out(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
    pub fn err(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
    pub fn err_lines(&self) -> Vec<String> {
        self.err().lines().map(str::to_string).collect()
    }
}

pub struct Op<'a> {
    rig: &'a Rig,
    args: Vec<OsString>,
    env: Vec<(String, Option<OsString>)>,
    hold_stdin: bool,
    real_windows: bool,
    default_error_mode: bool,
    exe: Option<PathBuf>,
    timeout: Duration,
}

/// `CREATE_DEFAULT_ERROR_MODE`: the new process gets the default error mode, 0, and not the
/// test process's own.
const CREATE_DEFAULT_ERROR_MODE: u32 = 0x0400_0000;

impl Op<'_> {
    /// Runs this copy of the operator in place of the one cargo built.
    pub fn exe(mut self, path: impl AsRef<Path>) -> Self {
        self.exe = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn env(mut self, name: &str, value: impl AsRef<OsStr>) -> Self {
        self.env
            .push((name.to_string(), Some(value.as_ref().to_os_string())));
        self
    }

    pub fn env_remove(mut self, name: &str) -> Self {
        self.env.push((name.to_string(), None));
        self
    }

    /// Keeps the operator's stdin open and empty for the whole run: a program that reads it waits.
    pub fn hold_stdin(mut self) -> Self {
        self.hold_stdin = true;
        self
    }

    /// Gives the run the real `SystemRoot`, `USERNAME`, `USERDOMAIN` and `COMPUTERNAME` of the test
    /// process, for `ca`, which calls Windows' own programs. Everything else stays the rig's.
    pub fn real_windows(mut self) -> Self {
        self.real_windows = true;
        self
    }

    /// Starts the operator with error mode 0 rather than the test process's own (#0191): a run
    /// wrapper or a console can hand the test `SEM_FAILCRITICALERRORS`, which the operator would
    /// inherit, and which hides the 16-bit dialog without the operator's help.
    pub fn default_error_mode(mut self) -> Self {
        self.default_error_mode = true;
        self
    }

    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }

    pub fn run(self) -> Out {
        let rig = self.rig;
        let mut c = match &self.exe {
            Some(path) => Command::new(path),
            None => Command::new(env!("CARGO_BIN_EXE_agentspace-operator")),
        };
        c.args(&self.args).env_clear().current_dir(&rig.work);
        c.env("USERPROFILE", &rig.profile)
            .env("HOME", &rig.home)
            .env("LOCALAPPDATA", &rig.local_app_data)
            .env("APPDATA", &rig.app_data)
            .env("TEMP", &rig.temp)
            .env("TMP", &rig.temp)
            .env("SystemRoot", &rig.system_root)
            .env("USERNAME", "tester")
            .env("COMPUTERNAME", "COMPUTER")
            .env("AGENTSPACE_AWS_CMD", rig.aws_exe())
            .env("AGENTSPACE_SSH_CMD", rig.ssh_exe())
            .env("AGENTSPACE_SCP_CMD", rig.scp_exe())
            .env("AGENTSPACE_TIMEZONE", "America/New_York")
            .env("STANDIN_DIR", &rig.ctl);
        if self.real_windows {
            for name in ["SystemRoot", "USERNAME", "USERDOMAIN", "COMPUTERNAME"] {
                if let Some(v) = std::env::var_os(name) {
                    c.env(name, v);
                }
            }
        }
        for (name, value) in &self.env {
            match value {
                Some(v) => c.env(name, v),
                None => c.env_remove(name),
            };
        }
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if self.default_error_mode {
            use std::os::windows::process::CommandExt;
            c.creation_flags(CREATE_DEFAULT_ERROR_MODE);
        }

        let started = Instant::now();
        let mut child = c.spawn().expect("the operator starts");
        let mut stdin = child.stdin.take();
        if !self.hold_stdin
            && let Some(mut pipe) = stdin.take()
        {
            let _ = pipe.write_all(SENTINEL.as_bytes());
            drop(pipe);
        }
        let mut so = child.stdout.take().expect("stdout");
        let mut se = child.stderr.take().expect("stderr");
        let t_out = std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = so.read_to_end(&mut b);
            b
        });
        let t_err = std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = se.read_to_end(&mut b);
            b
        });
        let code = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None) => {
                    if started.elapsed() > self.timeout {
                        // The operator is this test's own child; nothing else is stopped.
                        let _ = child.kill();
                        let _ = child.wait();
                        drop(stdin.take());
                        let stdout = t_out.join().unwrap_or_default();
                        let stderr = t_err.join().unwrap_or_default();
                        panic!(
                            "the operator did not end within {:?}; stderr so far had {} bytes, stdout {}",
                            self.timeout,
                            stderr.len(),
                            stdout.len()
                        );
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => panic!("waiting for the operator failed: {e}"),
            }
        };
        let elapsed = started.elapsed();
        let ended_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        drop(stdin.take());
        Out {
            code,
            stdout: t_out.join().unwrap_or_default(),
            stderr: t_err.join().unwrap_or_default(),
            elapsed,
            ended_ms,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Expectations the tests share
// ---------------------------------------------------------------------------------------------

/// The arguments of an AWS CLI call of `json` kind as the operator MUST pass them: the service,
/// the operation, `--region us-west-2`, `--output json`, then the operation's own arguments
/// (operator.md, AWS failures).
pub fn aws_args(service: &str, operation: &str, rest: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = [
        service,
        operation,
        "--region",
        "us-west-2",
        "--output",
        "json",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    v.extend(rest.iter().map(|s| s.to_string()));
    v
}

/// Asserts that a remote command obeys FA-15's rule (operator.md, What the laptop sends): not
/// empty, no `%`, no double quote, no control character, and not ending in a backslash.
pub fn assert_remote_command_rule(command: &str, what: &str) {
    assert!(!command.is_empty(), "{what}: the remote command is empty");
    assert!(
        !command.contains('%'),
        "{what}: the remote command holds a %"
    );
    assert!(
        !command.contains('"'),
        "{what}: the remote command holds a double quote"
    );
    assert!(
        !command.chars().any(char::is_control),
        "{what}: the remote command holds a control character"
    );
    assert!(
        !command.ends_with('\\'),
        "{what}: the remote command ends in a backslash"
    );
}

/// Asserts a plain failure: one `agentspace: ` line on stderr and nothing else, nothing on
/// stdout, exit 1.
pub fn assert_fails_with(out: &Out, message: &str, what: &str) {
    assert_eq!(
        out.code,
        Some(1),
        "{what}: exit code; stderr was {:?}",
        out.err()
    );
    assert_eq!(out.out(), "", "{what}: stdout MUST be empty on a failure");
    let err = out.err();
    let last = err.lines().last().unwrap_or("");
    assert_eq!(last, message, "{what}: the last stderr line");
    assert!(
        err.ends_with('\n'),
        "{what}: the message ends in a line feed"
    );
}
