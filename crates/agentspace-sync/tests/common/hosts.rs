//! Helpers for the tests of a host that is not the box (E0.07-T03-S03, #0152).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Written from docs/operator.md (Which host
//! a command acts on, How the operator reaches a host, deploy, sync, bootstrap, provision, The
//! operator's folder, one per command, The test seam) and docs/state.md (The host registry, A
//! host's folder).
//!
//! **Nothing here calls the crate's hidden functions** (`hostcalls` does), so every test file that
//! includes `common` builds on `main`'s stubs too.
//!
//! **What a host test needs that a box test does not:**
//! - **A host registry** in the throwaway home (`seed_hosts`), and the project list
//!   (`list_projects`).
//! - **A copy of the test binary the child runs from** (`beside_operator`), because the operator's
//!   alias for a host names `agentspace-ssh-proxy.exe` found beside the running program
//!   (operator.md, How each program finds the others), and a test binary's own folder holds none.
//! - **What a stand-in sees of the registry and the locks** (`watch_file`, `probe_lock`): a stand-in
//!   records a watched file's content and whether a lock is held at each call, which is how a test
//!   learns what a record said while a step ran.
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use agentspace_hosts::{Host, Hosts};

use super::times::now_secs;
use super::{ID2, Log, Rig};

/// The project of the host most tests make: a made-up name, a valid normalized project.
pub const PROJECT: &str = "demo";

/// A host's stack, `agentspace-host-<project>` (D44).
pub fn stack_of(project: &str) -> String {
    format!("agentspace-host-{project}")
}

/// A host's SSH alias, `agentspace-host.<project>` (D47, D55).
pub fn alias_of(project: &str) -> String {
    format!("agentspace-host.{project}")
}

/// One record to seed into `hosts.json`, besides the box's.
#[derive(Clone, Copy)]
pub struct Seed<'a> {
    pub project: &'a str,
    /// `creating`, `ready` or `failed`.
    pub state: &'a str,
    pub instance_id: Option<&'a str>,
    pub last_attempt_at: Option<&'a str>,
}

impl<'a> Seed<'a> {
    /// A record of `project` in `state`, with no instance ID and no attempt.
    pub fn new(project: &'a str, state: &'a str) -> Seed<'a> {
        Seed {
            project,
            state,
            instance_id: None,
            last_attempt_at: None,
        }
    }

    pub fn id(mut self, id: &'a str) -> Seed<'a> {
        self.instance_id = Some(id);
        self
    }

    pub fn attempt(mut self, at: &'a str) -> Seed<'a> {
        self.last_attempt_at = Some(at);
        self
    }

    /// The record's JSON, its members in state.md's order.
    fn json(&self) -> String {
        let mut s = format!(
            "{{\"project\":\"{}\",\"stack\":\"{}\",\"state\":\"{}\"",
            self.project,
            stack_of(self.project),
            self.state
        );
        if let Some(id) = self.instance_id {
            s.push_str(&format!(",\"instance_id\":\"{id}\""));
        }
        if let Some(at) = self.last_attempt_at {
            s.push_str(&format!(",\"last_attempt_at\":\"{at}\""));
        }
        s.push('}');
        s
    }
}

/// A stack's `describe-stacks` answer, with the status the existence check reads and an
/// `InstanceId` output.
pub fn json_stack_status(status: &str, id: Option<&str>) -> String {
    let outputs = match id {
        Some(id) => {
            format!(",\"Outputs\":[{{\"OutputKey\":\"InstanceId\",\"OutputValue\":\"{id}\"}}]")
        }
        None => String::new(),
    };
    format!("{{\"Stacks\":[{{\"StackStatus\":\"{status}\"{outputs}}}]}}")
}

/// Whether `name` is 32 lowercase hexadecimal digits.
pub fn is_command_folder_name(name: &str) -> bool {
    name.len() == 32
        && name
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Rig {
    // ---- The registry and the project list (parent only)

    pub fn hosts_json_path(&self) -> PathBuf {
        self.home_path(".agentspace/hosts.json")
    }

    /// Writes `hosts.json` holding the box's record, then `extra`.
    pub fn seed_hosts(&self, extra: &[Seed]) {
        let mut records = vec![
            "{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}".to_string(),
        ];
        records.extend(extra.iter().map(Seed::json));
        let text = format!("{{\"version\":0,\"hosts\":[{}]}}\n", records.join(","));
        self.write_home(".agentspace/hosts.json", text.as_bytes());
    }

    /// Writes `projects.json` listing `names`, each approved on a made-up date.
    pub fn list_projects(&self, names: &[&str]) {
        let items: Vec<String> = names
            .iter()
            .map(|n| format!("{{\"name\":\"{n}\",\"approved\":\"2026-11-20\"}}"))
            .collect();
        let text = format!("{{\"version\":0,\"projects\":[{}]}}\n", items.join(","));
        self.write_home(".agentspace/projects.json", text.as_bytes());
    }

    // ---- Reading the registry (parent or child)

    /// The registry as `agentspace-hosts` reads it: a missing file is the box's record alone.
    pub fn read_hosts(&self) -> Hosts {
        match agentspace_hosts::read(&self.home) {
            Ok(h) => h,
            Err(_) => panic!("The host registry in the throwaway home could not be read"),
        }
    }

    /// The record of `project`.
    pub fn record(&self, project: &str) -> Host {
        match self
            .read_hosts()
            .hosts
            .into_iter()
            .find(|h| h.project == project)
        {
            Some(h) => h,
            None => panic!("The host registry has no record of '{project}'"),
        }
    }

    /// The registry file's bytes, or `None` when there is no file.
    pub fn hosts_bytes(&self) -> Option<Vec<u8>> {
        fs::read(self.hosts_json_path()).ok()
    }

    // ---- What a stand-in records (parent only)

    /// Makes every stand-in record `path`'s content at each call.
    pub fn watch_file(&self, path: PathBuf) {
        self.watched.borrow_mut().push(path);
    }

    /// Makes every stand-in record at each call whether `path`, a lock file, is held.
    pub fn probe_lock(&self, path: PathBuf) {
        self.probed.borrow_mut().push(path);
    }

    /// Makes the ssh stand-in keep each call's whole standard input.
    pub fn capture_ssh_stdin(&self) {
        self.capture_stdin.set(true);
        if !self.child {
            fs::create_dir_all(self.stdin_dir()).unwrap();
        }
    }

    pub fn stdin_dir(&self) -> PathBuf {
        self.root.join("stdin")
    }

    /// The standard input the `n`th call of `prog` received, when it was kept.
    pub fn captured_stdin(&self, prog: &str, n: u32) -> Option<Vec<u8>> {
        fs::read(self.stdin_dir().join(format!("{prog}-{n}.stdin"))).ok()
    }

    // ---- The operator's own folder of programs

    /// The folder the child's executable runs from, once [`Rig::beside_operator`] made one.
    pub fn operator_bin(&self) -> PathBuf {
        self.root.join("operator-bin")
    }

    /// Makes the child run from a copy of the test binary in [`Rig::operator_bin`], with an empty
    /// `agentspace-ssh-proxy.exe` beside it when `with_proxy` is set. The operator's alias for a
    /// host names that program, found beside the running program (operator.md, How each program
    /// finds the others), and the test binary's own folder holds none.
    pub fn beside_operator(&self, with_proxy: bool) {
        if self.child {
            return;
        }
        let dir = self.operator_bin();
        fs::create_dir_all(&dir).unwrap();
        let copy = dir.join("agentspace-operator-test.exe");
        let own = std::env::current_exe().unwrap();
        if fs::hard_link(&own, &copy).is_err() {
            fs::copy(&own, &copy).unwrap();
        }
        if with_proxy {
            fs::write(dir.join("agentspace-ssh-proxy.exe"), b"").unwrap();
        }
        *self.operator_exe.borrow_mut() = Some(copy);
    }

    // ---- Where the operator writes

    pub fn ssh_dir(&self) -> PathBuf {
        self.home.join(".ssh")
    }

    /// The host's operator alias, `<ssh folder>\agentspace.d\agentspace-host.<project>`.
    pub fn alias_file(&self, project: &str) -> PathBuf {
        self.ssh_dir().join("agentspace.d").join(alias_of(project))
    }

    /// Whether the operator wrote anything of an alias: the `agentspace.d` folder or an ssh config.
    pub fn has_alias_files(&self) -> bool {
        self.ssh_dir().join("agentspace.d").exists() || self.ssh_dir().join("config").exists()
    }

    /// The host's folder, `%USERPROFILE%\.agentspace\hosts\<project>`.
    pub fn host_folder(&self, project: &str) -> PathBuf {
        agentspace_hosts::host_dir(&self.home, project)
    }

    pub fn operator_dir(&self) -> PathBuf {
        self.home.join(".agentspace").join("operator")
    }

    /// The names in the operator's folder that are 32 lowercase hexadecimal digits, sorted.
    pub fn command_folders(&self) -> Vec<String> {
        let mut v: Vec<String> = match fs::read_dir(self.operator_dir()) {
            Ok(rd) => rd
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| is_command_folder_name(n))
                .collect(),
            Err(_) => Vec::new(),
        };
        v.sort();
        v
    }

    /// Everything in the operator's folder, by name, sorted.
    pub fn operator_entries(&self) -> Vec<String> {
        let mut v: Vec<String> = match fs::read_dir(self.operator_dir()) {
            Ok(rd) => rd
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect(),
            Err(_) => Vec::new(),
        };
        v.sort();
        v
    }

    /// Makes a folder `name` under the operator's folder with a file in it, whose creation time
    /// is `hours` hours ago (parent only), as one a killed command left.
    pub fn leftover_folder(&self, name: &str, hours: i64) {
        if self.child {
            return;
        }
        let dir = self.operator_dir().join(name);
        fs::create_dir_all(dir.join("system")).unwrap();
        fs::write(
            dir.join("system").join("left.txt"),
            b"left by a killed command",
        )
        .unwrap();
        super::win::set_dir_creation_time(&dir, now_secs() - hours * 3600);
    }

    // ---- The stand-in aws's answers for a host's stack (parent only)

    /// The stack does not exist: the existence check, the first AWS call, fails with the CLI's own
    /// text, and the output read after the deploy answers `CREATE_COMPLETE` with `ID2`.
    pub fn stack_missing(&self, stack: &str) {
        self.rule(
            "aws",
            1,
            "describe-stacks",
            255,
            "",
            &format!(
                "An error occurred (ValidationError) when calling the DescribeStacks operation: Stack with id {stack} does not exist\\n"
            ),
        );
        self.stack_status("CREATE_COMPLETE");
    }

    /// Every `describe-stacks` answers with `status` and an `InstanceId` output, `ID2`.
    pub fn stack_status(&self, status: &str) {
        self.rule(
            "aws",
            0,
            "describe-stacks",
            0,
            &json_stack_status(status, Some(ID2)),
            "",
        );
    }

    /// The first AWS call, the existence check, answers `status`, and every later `describe-stacks`
    /// answers `CREATE_COMPLETE` with `ID2`.
    pub fn stack_first_status(&self, status: &str) {
        self.rule(
            "aws",
            1,
            "describe-stacks",
            0,
            &json_stack_status(status, None),
            "",
        );
        self.stack_status("CREATE_COMPLETE");
    }
}

impl Log {
    /// The content of the watched file whose name ends `file_name`, as the `n`th call of `prog`
    /// found it, with its line ends written `\n` and `\r`.
    pub fn watched(&self, prog: &str, n: u32, file_name: &str) -> Option<&str> {
        self.watches
            .iter()
            .find(|w| w.0 == prog && w.1 == n && w.2.ends_with(file_name))
            .map(|w| w.3.as_str())
    }

    /// Every watch of the file whose name ends `file_name`: `(program, call number, content)`.
    pub fn watched_calls(&self, file_name: &str) -> Vec<(String, u32, String)> {
        self.watches
            .iter()
            .filter(|w| w.2.ends_with(file_name))
            .map(|w| (w.0.clone(), w.1, w.3.clone()))
            .collect()
    }

    /// Every probe of the lock file whose name ends `file_name`: `(program, call number, held)`.
    pub fn probed(&self, file_name: &str) -> Vec<(String, u32, bool)> {
        self.probes
            .iter()
            .filter(|p| p.2.ends_with(file_name))
            .map(|p| (p.0.clone(), p.1, p.3 == "held"))
            .collect()
    }
}
