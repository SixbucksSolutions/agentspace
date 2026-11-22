//! The lease CLI's binary with the build's real EC2 driver and a stand-in provisioning run
//! (E0.07-T03-S01, #0150; docs/driver.md, Conformance cases for v0.07).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. Every test runs the built
//! `agentspace` in a throwaway home under the temp folder, with every variable passed through
//! `Command::env` and `USERPROFILE`, `HOME`, `LOCALAPPDATA` and `APPDATA` pointed into it, and
//! with `AGENTSPACE_PROVISION_CMD` set to a stand-in inside that home, so no test can start a real
//! provisioning run. The stand-in is a `.cmd` program that records its arguments, its working
//! folder and the four variables of its environment that the detached start sets, and exits at
//! once. A test names the variables it checks and never prints an environment.
//!
//! The whole suite is run again by the isolation guard with the process's own home moved away, and
//! a test here MUST NOT read the process's environment for a home.
#![cfg(windows)]

mod common;

use common::{BIN, ID, Run};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const CREATED: &str = "2026-11-21T08:00:00Z";
const BOX_RECORD: &str = r#"{"project":"swimcoach","stack":"agentbox","state":"ready"}"#;
const HOST_READY: &str =
    r#"{"project":"agentspace","stack":"agentspace-host-agentspace","state":"ready"}"#;

/// The stand-in: what a provisioning run would be handed, written to `started.txt` once, by a
/// rename, so a test that finds the file finds all of it.
const STAND_IN: &str = "@echo off\r\n\
echo args=%* > \"%~dp0started.tmp\"\r\n\
echo cd=%CD% >> \"%~dp0started.tmp\"\r\n\
echo userprofile=%USERPROFILE% >> \"%~dp0started.tmp\"\r\n\
echo state=%AGENTSPACE_STATE_DIR% >> \"%~dp0started.tmp\"\r\n\
echo ssh=%AGENTSPACE_SSH_DIR% >> \"%~dp0started.tmp\"\r\n\
echo proxy=%AGENTSPACE_PROXY_COMMAND% >> \"%~dp0started.tmp\"\r\n\
echo seam=%AGENTSPACE_CHECK_SEAM% >> \"%~dp0started.tmp\"\r\n\
move /y \"%~dp0started.tmp\" \"%~dp0started.txt\" > nul\r\n";

/// A UUIDv4 drawn at random from the operating system's source, lowercase and hyphenated. A test
/// whose lease the built program releases uses one, so that the release's walk over the laptop's
/// process list cannot match a process of Terry's.
fn fresh_id() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("The operating system has random bytes");
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// A throwaway home, a working folder and a stand-in, all under the temp folder.
struct Throwaway {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    bin: PathBuf,
}

impl Throwaway {
    fn new() -> Throwaway {
        let tmp = tempfile::Builder::new()
            .prefix("asbin-")
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
        fs::write(bin.join("standin.cmd"), STAND_IN.as_bytes()).unwrap();
        Throwaway {
            _tmp: tmp,
            root,
            home,
            bin,
        }
    }

    fn stand_in(&self) -> PathBuf {
        self.bin.join("standin.cmd")
    }

    fn dot(&self) -> PathBuf {
        self.home.join(".agentspace")
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        fs::create_dir_all(self.dot()).unwrap();
        fs::write(self.dot().join(name), bytes).unwrap();
    }

    fn read(&self, name: &str) -> Option<Vec<u8>> {
        fs::read(self.dot().join(name)).ok()
    }

    fn write_hosts(&self, extra: &[&str]) {
        let mut records = vec![BOX_RECORD];
        records.extend_from_slice(extra);
        self.write(
            "hosts.json",
            format!("{{\"version\":0,\"hosts\":[{}]}}\n", records.join(",")).as_bytes(),
        );
    }

    fn write_projects(&self, names: &[&str]) {
        let items: Vec<String> = names
            .iter()
            .map(|n| format!("{{\"name\":\"{n}\",\"approved\":\"2026-11-20\"}}"))
            .collect();
        self.write(
            "projects.json",
            format!("{{\"version\":0,\"projects\":[{}]}}\n", items.join(",")).as_bytes(),
        );
    }

    /// The lease record of a pending lease as the CLI writes it, with this `driver` member.
    fn pending_record(id: &str, project: &str, driver: Option<&str>) -> String {
        let mut text = format!(
            "{{\"id\":\"{id}\",\"project\":\"{project}\",\"state\":\"pending\",\"platform\":\"linux/arm64\",\"profile\":\"agent-linux\",\"size\":\"m\",\"ttl_seconds\":3600,\"created_at\":\"{CREATED}\""
        );
        if let Some(d) = driver {
            text.push_str(&format!(",\"driver\":\"{d}\""));
        }
        text.push('}');
        text
    }

    fn write_leases_in(&self, dir: &Path, records: &[String]) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("leases.json"),
            format!("{{\"version\":0,\"leases\":[{}]}}\n", records.join(",")).as_bytes(),
        )
        .unwrap();
    }

    fn write_pending(&self, id: &str, project: &str, driver: Option<&str>) {
        self.write_leases_in(
            &self.dot(),
            &[Throwaway::pending_record(id, project, driver)],
        );
    }

    /// The lease document a `get` of that pending lease prints.
    fn pending_doc(id: &str, project: &str) -> Value {
        json!({
            "id": id, "project": project, "state": "pending", "platform": "linux/arm64",
            "profile": "agent-linux", "size": "m", "ttl_seconds": 3600, "created_at": CREATED
        })
    }

    fn marker(&self) -> PathBuf {
        self.bin.join("started.txt")
    }

    /// Waits for the stand-in to have run, and reads what it recorded.
    fn started(&self, limit: Duration) -> Option<Started> {
        let begun = Instant::now();
        while begun.elapsed() < limit {
            if let Ok(text) = fs::read_to_string(self.marker()) {
                let mut lines = BTreeMap::new();
                for line in text.lines() {
                    if let Some((k, v)) = line.split_once('=') {
                        lines.insert(k.trim().to_string(), v.trim().to_string());
                    }
                }
                return Some(Started(lines));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        None
    }

    /// Nothing started the stand-in within a quiet spell.
    #[track_caller]
    fn assert_not_started(&self) {
        std::thread::sleep(Duration::from_secs(2));
        assert!(!self.marker().exists(), "The stand-in was started");
    }

    /// The program, in this home, with the stand-in as the provisioning run's program and every
    /// variable of agentspace's own removed except the ones `env` sets.
    fn command(&self, line: &[&str], cwd: &Path, env: &[(&str, OsString)]) -> Command {
        let mut cmd = Command::new(BIN);
        cmd.args(line)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("USERPROFILE", &self.home)
            .env("HOME", &self.home)
            .env("LOCALAPPDATA", self.home.join("AppData").join("Local"))
            .env("APPDATA", self.home.join("AppData").join("Roaming"));
        for var in [
            "AGENTSPACE_STATE_DIR",
            "AGENTSPACE_SSH_DIR",
            "AGENTSPACE_PROXY_COMMAND",
            "AGENTSPACE_PROVISION_CMD",
            "AGENTSPACE_OPERATOR_CMD",
            "AGENTSPACE_CHECK_SEAM",
        ] {
            cmd.env_remove(var);
        }
        for (name, value) in env {
            cmd.env(name, value);
        }
        cmd
    }

    /// Runs the program in the home, with the stand-in named.
    fn run(&self, line: &[&str]) -> Run {
        self.run_with(line, &[])
    }

    fn run_with(&self, line: &[&str], env: &[(&str, OsString)]) -> Run {
        let mut all: Vec<(&str, OsString)> =
            vec![("AGENTSPACE_PROVISION_CMD", self.stand_in().into_os_string())];
        all.extend(env.iter().cloned());
        run(self.command(line, &self.home, &all))
    }
}

/// Runs a command to its end, and fails the test when it takes more than a minute.
fn run(mut cmd: Command) -> Run {
    let child = cmd.spawn().expect("The binary starts");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let out = rx
        .recv_timeout(Duration::from_secs(60))
        .expect("The binary ends within a minute")
        .expect("The binary's output can be read");
    Run {
        status: out.status.code(),
        stdout: out.stdout,
        stderr: out.stderr,
    }
}

/// What the stand-in recorded. A value is read only by the name of the variable.
struct Started(BTreeMap<String, String>);

impl Started {
    fn get(&self, key: &str) -> &str {
        self.0.get(key).map(String::as_str).unwrap_or("")
    }

    /// Whether the recorded folder is `expected`, by the disk when both exist.
    fn is_folder(&self, key: &str, expected: &Path) -> bool {
        let got = self.get(key);
        match (fs::canonicalize(got), fs::canonicalize(expected)) {
            (Ok(a), Ok(b)) => a == b,
            _ => got
                .trim_end_matches(['\\', '/'])
                .eq_ignore_ascii_case(expected.to_string_lossy().trim_end_matches(['\\', '/'])),
        }
    }
}

#[track_caller]
fn stdout_doc(run: &Run) -> Value {
    assert_eq!(
        run.status,
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(run.stderr.is_empty(), "Nothing on stderr");
    let text = std::str::from_utf8(&run.stdout).expect("Stdout is UTF-8");
    assert!(
        text.ends_with('\n') && text.matches('\n').count() == 1,
        "Stdout holds exactly one document: {text:?}"
    );
    serde_json::from_str(text.trim_end()).expect("Stdout is JSON")
}

// ---- get ---------------------------------------------------------------------------------------------------------

#[test]
fn get_of_a_pending_lease_on_a_ready_host_starts_the_run_once_and_changes_no_lease() {
    // Conformance C7, C11, C12, C19, C27 and C28.
    let t = Throwaway::new();
    t.write_hosts(&[HOST_READY]);
    t.write_pending(ID, "agentspace", Some("ec2"));
    let leases = t.read("leases.json");
    let run1 = t.run_with(
        &["lease", "get", ID],
        &[
            ("AGENTSPACE_CHECK_SEAM", OsString::from("seam-value")),
            (
                "AGENTSPACE_OPERATOR_CMD",
                t.root.join("no such operator.exe").into(),
            ),
        ],
    );
    assert_eq!(stdout_doc(&run1), Throwaway::pending_doc(ID, "agentspace"));
    assert_eq!(
        t.read("leases.json"),
        leases,
        "The lease state file is byte for byte as it was"
    );
    assert!(!t.dot().join("leases").exists(), "No lease folder is made");
    assert!(!t.home.join(".ssh").exists(), "No alias is written");

    let started = t
        .started(Duration::from_secs(20))
        .expect("The run was started");
    assert_eq!(started.get("args"), "provision -Host agentspace");
    assert!(
        started.is_folder("cd", &t.home),
        "Its working folder is the home folder"
    );
    assert!(
        started.is_folder("userprofile", &t.home),
        "USERPROFILE is the CLI's"
    );
    // A mismatch is reported by name, never by value: the run's environment is the logon one.
    assert!(
        started.get("state").is_empty(),
        "AGENTSPACE_STATE_DIR is absent: the CLI had none"
    );
    assert!(
        started.get("ssh").is_empty(),
        "AGENTSPACE_SSH_DIR is absent: the CLI had none"
    );
    assert!(
        started.get("proxy").is_empty(),
        "AGENTSPACE_PROXY_COMMAND is absent: the CLI had none"
    );
    assert!(
        started.get("seam").is_empty(),
        "A variable set only in the CLI's environment never reaches the run"
    );
    assert!(
        t.dot().join("hosts").join("agentspace").is_dir(),
        "The host folder is made"
    );
    assert!(
        !t.dot()
            .join("hosts")
            .join("agentspace")
            .join("provision.log")
            .exists(),
        "The driver never makes the log"
    );

    // The record stays ready, and only last_attempt_at is written.
    let hosts = String::from_utf8(t.read("hosts.json").unwrap()).unwrap();
    assert!(
        hosts.contains(
            r#""stack":"agentspace-host-agentspace","state":"ready","last_attempt_at":""#
        ),
        "{hosts}"
    );
    assert!(!hosts.contains("creating"), "{hosts}");

    // A second get within five minutes starts nothing and leaves last_attempt_at alone.
    fs::remove_file(t.marker()).unwrap();
    let after_first = t.read("hosts.json");
    let run2 = t.run(&["lease", "get", ID]);
    assert_eq!(stdout_doc(&run2), Throwaway::pending_doc(ID, "agentspace"));
    t.assert_not_started();
    assert_eq!(t.read("hosts.json"), after_first);
    assert_eq!(t.read("leases.json"), leases);
}

#[test]
fn the_run_gets_the_clis_folders_made_full_and_its_override_exactly() {
    // Conformance C7: AGENTSPACE_STATE_DIR and AGENTSPACE_SSH_DIR each made full, given relative;
    // AGENTSPACE_PROXY_COMMAND exactly as set.
    let t = Throwaway::new();
    t.write_hosts(&[HOST_READY]);
    let state = t.root.join("state elsewhere");
    t.write_leases_in(
        &state,
        &[Throwaway::pending_record(ID, "agentspace", Some("ec2"))],
    );
    let run = t.run_with(
        &["lease", "get", ID],
        &[
            ("AGENTSPACE_STATE_DIR", state.clone().into_os_string()),
            ("AGENTSPACE_SSH_DIR", OsString::from("ssh-rel")),
            (
                "AGENTSPACE_PROXY_COMMAND",
                OsString::from("my-transport --stdio"),
            ),
        ],
    );
    assert_eq!(stdout_doc(&run), Throwaway::pending_doc(ID, "agentspace"));
    let started = t
        .started(Duration::from_secs(20))
        .expect("The run was started");
    assert_eq!(started.get("args"), "provision -Host agentspace");
    assert!(
        started.is_folder("state", &state),
        "AGENTSPACE_STATE_DIR is the CLI's"
    );
    assert!(
        started.is_folder("ssh", &t.home.join("ssh-rel")),
        "AGENTSPACE_SSH_DIR is made full against the CLI's working folder"
    );
    assert!(
        started.get("proxy") == "my-transport --stdio",
        "AGENTSPACE_PROXY_COMMAND is the CLI's, exactly"
    );
}

#[test]
fn a_relative_userprofile_is_made_full_for_the_run() {
    let t = Throwaway::new();
    t.write_hosts(&[HOST_READY]);
    t.write_pending(ID, "agentspace", Some("ec2"));
    // The working folder is the throwaway root, and the home is given relative to it.
    let all = [
        ("AGENTSPACE_PROVISION_CMD", t.stand_in().into_os_string()),
        ("USERPROFILE", OsString::from("home")),
        ("HOME", OsString::from("home")),
    ];
    let run = run(t.command(&["lease", "get", ID], &t.root, &all));
    assert_eq!(stdout_doc(&run), Throwaway::pending_doc(ID, "agentspace"));
    let started = t
        .started(Duration::from_secs(20))
        .expect("The run was started");
    assert!(
        started.is_folder("userprofile", &t.home),
        "USERPROFILE is made full against the CLI's working folder"
    );
    assert!(
        started.is_folder("cd", &t.home),
        "The run's working folder is the home folder"
    );
}

#[test]
fn get_of_a_pending_lease_with_no_host_record_starts_the_run_for_a_listed_project_only() {
    // Conformance C25.
    let t = Throwaway::new();
    t.write_projects(&["agentspace"]);
    t.write_pending(ID, "agentspace", Some("ec2"));
    let leases = t.read("leases.json");
    let run = t.run(&["lease", "get", ID]);
    assert_eq!(stdout_doc(&run), Throwaway::pending_doc(ID, "agentspace"));
    assert!(
        t.started(Duration::from_secs(20)).is_some(),
        "The run was started"
    );
    let hosts = String::from_utf8(t.read("hosts.json").unwrap()).unwrap();
    assert!(
        hosts.contains(r#""state":"creating","last_attempt_at":""#),
        "{hosts}"
    );
    assert_eq!(t.read("leases.json"), leases);

    // A project that is not listed: nothing is written and nothing is started.
    let t = Throwaway::new();
    t.write_pending(ID, "unlisted", Some("ec2"));
    let run = t.run(&["lease", "get", ID]);
    assert_eq!(stdout_doc(&run), Throwaway::pending_doc(ID, "unlisted"));
    t.assert_not_started();
    assert!(
        t.read("hosts.json").is_none(),
        "The registry is not written"
    );
}

#[test]
fn get_succeeds_and_starts_nothing_when_the_registry_is_not_valid() {
    // Conformance C13, the first case.
    let t = Throwaway::new();
    t.write("hosts.json", b"{");
    t.write_pending(ID, "agentspace", Some("ec2"));
    let started = Instant::now();
    let run = t.run(&["lease", "get", ID]);
    assert!(started.elapsed() < Duration::from_secs(20));
    assert_eq!(stdout_doc(&run), Throwaway::pending_doc(ID, "agentspace"));
    t.assert_not_started();
    assert_eq!(
        t.read("hosts.json").as_deref(),
        Some(&b"{"[..]),
        "Never repaired"
    );
}

#[test]
fn get_succeeds_and_changes_nothing_when_the_program_named_does_not_exist() {
    // Conformance C13 (the second case), C19's second half and C31's `get`: the record stays
    // `ready`, with no last_attempt_at written.
    let t = Throwaway::new();
    t.write_hosts(&[HOST_READY]);
    t.write_pending(ID, "agentspace", Some("ec2"));
    let hosts = t.read("hosts.json");
    let leases = t.read("leases.json");
    let run = run(t.command(
        &["lease", "get", ID],
        &t.home,
        &[(
            "AGENTSPACE_PROVISION_CMD",
            t.root.join("no such program.cmd").into_os_string(),
        )],
    ));
    assert_eq!(stdout_doc(&run), Throwaway::pending_doc(ID, "agentspace"));
    t.assert_not_started();
    assert_eq!(t.read("hosts.json"), hosts, "The registry is as it was");
    assert_eq!(t.read("leases.json"), leases);
}

#[test]
fn get_succeeds_and_starts_nothing_while_the_registry_lock_is_held() {
    // Conformance C13, the third case: get tries hosts.lock once and does not wait for it.
    let t = Throwaway::new();
    t.write_hosts(&[HOST_READY]);
    t.write_pending(ID, "agentspace", Some("ec2"));
    let hosts = t.read("hosts.json");
    let held = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(t.dot().join("hosts.lock"))
        .expect("The test can take hosts.lock");
    let begun = Instant::now();
    let run = t.run(&["lease", "get", ID]);
    let took = begun.elapsed();
    drop(held);
    assert!(
        took < Duration::from_secs(20),
        "get waited for hosts.lock: {took:?}"
    );
    assert_eq!(stdout_doc(&run), Throwaway::pending_doc(ID, "agentspace"));
    t.assert_not_started();
    assert_eq!(t.read("hosts.json"), hosts);
}

#[test]
fn get_of_a_pending_record_with_no_driver_or_an_unknown_one_starts_nothing() {
    // Conformance C26.
    for driver in [None, Some("no-such-driver")] {
        let t = Throwaway::new();
        t.write_hosts(&[HOST_READY]);
        t.write_pending(ID, "agentspace", driver);
        let hosts = t.read("hosts.json");
        let run = t.run(&["lease", "get", ID]);
        assert_eq!(
            stdout_doc(&run),
            Throwaway::pending_doc(ID, "agentspace"),
            "{driver:?}"
        );
        t.assert_not_started();
        assert_eq!(t.read("hosts.json"), hosts, "{driver:?}");
    }
}

// ---- list and release --------------------------------------------------------------------------------------------

#[test]
fn list_never_starts_the_run() {
    // Conformance C14.
    let t = Throwaway::new();
    t.write_hosts(&[HOST_READY]);
    t.write_pending(ID, "agentspace", Some("ec2"));
    let hosts = t.read("hosts.json");
    let run = t.run(&["lease", "list"]);
    let doc = stdout_doc(&run);
    assert_eq!(
        doc["leases"],
        json!([Throwaway::pending_doc(ID, "agentspace")])
    );
    t.assert_not_started();
    assert_eq!(t.read("hosts.json"), hosts);
}

#[test]
fn release_of_a_pending_lease_records_it_released_and_starts_nothing() {
    // Conformance C15: the driver is asked, finds nothing to remove, and the host is kept. The
    // built program's release walks the laptop's whole process list, and no seam scopes it, so the
    // lease ID is a UUIDv4 drawn at random for this run, never a fixed one (driver.md, "How it is
    // checked"), and no process of Terry's can match its alias.
    let id = fresh_id();
    let t = Throwaway::new();
    t.write_hosts(&[HOST_READY]);
    t.write_pending(&id, "agentspace", Some("ec2"));
    let hosts = t.read("hosts.json");
    let run = t.run(&["lease", "release", &id]);
    let doc = stdout_doc(&run);
    assert_eq!(doc["id"], id.as_str());
    assert_eq!(doc["state"], "released");
    assert!(
        doc.get("expires_at").is_none(),
        "A lease released from pending has no expires_at"
    );
    assert!(doc.get("released_at").is_some());
    t.assert_not_started();
    assert_eq!(
        t.read("hosts.json"),
        hosts,
        "The host registry is as it was"
    );
    let leases: Value = serde_json::from_slice(&t.read("leases.json").unwrap()).unwrap();
    assert_eq!(leases["leases"][0]["state"], "released");
    assert_eq!(
        leases["leases"][0]["driver"], "ec2",
        "The driver member stays"
    );
}

// ---- create ------------------------------------------------------------------------------------------------------------

#[test]
fn create_for_a_project_with_no_host_that_is_not_listed_answers_no_capacity() {
    // Conformance C1 and C2: no CA is needed for an unsatisfiable answer.
    let t = Throwaway::new();
    let run = t.run(&["lease", "create", "--project", "unlisted"]);
    let doc = stdout_doc(&run);
    assert_eq!(doc["state"], "unsatisfiable");
    assert_eq!(
        doc["reasons"],
        json!([{
            "code": "no_capacity",
            "message": "No capacity can be found for project 'unlisted': it has no host and is not on agentspace's project list."
        }])
    );
    t.assert_not_started();
    assert!(
        t.read("hosts.json").is_none(),
        "The host registry is not written"
    );
    assert!(!t.dot().join("leases").exists(), "No lease folder is made");
    assert!(!t.home.join(".ssh").exists(), "No alias is written");
    let leases: Value = serde_json::from_slice(&t.read("leases.json").unwrap()).unwrap();
    assert_eq!(
        leases["leases"][0]["driver"], "ec2",
        "The record carries driver"
    );

    // An unsupported request lists its unsupported reasons alone, and writes no registry.
    let t = Throwaway::new();
    let run = t.run(&[
        "lease",
        "create",
        "--project",
        "unlisted",
        "--platform",
        "windows/amd64",
    ]);
    let doc = stdout_doc(&run);
    let codes: Vec<&str> = doc["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["unsupported_platform"]);
    assert!(t.read("hosts.json").is_none());
}

/// Makes the CA in `home` with the operator's own `ca`, run in process (architect, 2026-11-20, for
/// E0.06-T06-S01). The context names the throwaway home and nothing of the real profile.
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
fn create_for_a_listed_project_with_no_host_is_pending_and_starts_the_run() {
    // Conformance C5, C8 and C27.
    let t = Throwaway::new();
    make_ca(&t.home);
    t.write_projects(&["agentspace"]);
    let run1 = t.run(&["lease", "create", "--project", "agentspace"]);
    let doc = stdout_doc(&run1);
    assert_eq!(doc["state"], "pending");
    for absent in ["expires_at", "ssh", "reasons"] {
        assert!(
            doc.get(absent).is_none(),
            "A pending document has no {absent}"
        );
    }
    let id = doc["id"].as_str().expect("An id").to_string();

    let started = t
        .started(Duration::from_secs(20))
        .expect("The run was started");
    assert_eq!(started.get("args"), "provision -Host agentspace");
    assert!(started.is_folder("cd", &t.home));
    assert!(t.dot().join("hosts").join("agentspace").is_dir());

    let hosts = String::from_utf8(t.read("hosts.json").unwrap()).unwrap();
    assert!(
        hosts.starts_with(&format!("{{\"version\":0,\"hosts\":[{BOX_RECORD},")),
        "The box's record is first: {hosts}"
    );
    assert!(
        hosts.contains(
            r#""stack":"agentspace-host-agentspace","state":"creating","last_attempt_at":""#
        ),
        "{hosts}"
    );
    // The record is the lease's, pending, with its driver and no file.
    let leases: Value = serde_json::from_slice(&t.read("leases.json").unwrap()).unwrap();
    let record = &leases["leases"][0];
    assert_eq!(record["id"], id.as_str());
    assert_eq!(record["state"], "pending");
    assert_eq!(record["driver"], "ec2");
    for absent in ["expires_at", "ssh", "reasons", "released_at"] {
        assert!(
            record.get(absent).is_none(),
            "A pending record has no {absent}"
        );
    }
    assert!(
        !t.dot().join("leases").join(&id).exists(),
        "No lease folder is made"
    );
    assert!(
        !t.home.join(".ssh").join("agentspace.d").exists(),
        "No alias is made"
    );

    // A second create within five minutes answers pending and starts nothing.
    fs::remove_file(t.marker()).unwrap();
    let after_first = t.read("hosts.json");
    let run2 = t.run(&["lease", "create", "--project", "agentspace"]);
    assert_eq!(stdout_doc(&run2)["state"], "pending");
    t.assert_not_started();
    assert_eq!(
        t.read("hosts.json"),
        after_first,
        "last_attempt_at is as it was"
    );
}

#[test]
fn a_create_that_would_start_a_run_with_no_program_fails_and_records_nothing() {
    // Conformance C16: the message names no path.
    let t = Throwaway::new();
    make_ca(&t.home);
    t.write_projects(&["agentspace"]);
    let run = run(t.command(
        &["lease", "create", "--project", "agentspace"],
        &t.home,
        &[(
            "AGENTSPACE_PROVISION_CMD",
            t.root.join("no such program.cmd").into_os_string(),
        )],
    ));
    assert_eq!(run.status, Some(1));
    assert!(run.stdout.is_empty());
    let doc: Value = serde_json::from_slice(&run.stderr).expect("An error document");
    assert_eq!(doc["code"], "internal");
    assert_eq!(
        doc["message"],
        "agentspace lease create failed: The lease's host cannot be made: the program that AGENTSPACE_PROVISION_CMD names does not exist. Nothing was recorded."
    );
    t.assert_not_started();
    assert!(
        t.read("hosts.json").is_none(),
        "The host registry is as it was"
    );
    if let Some(bytes) = t.read("leases.json") {
        let doc: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(doc["leases"], json!([]), "No lease was recorded");
    }
}
