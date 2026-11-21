//! Helpers for the EC2 driver's v0.07 tests (E0.07-T03-S01, #0150): a fake detached start, seeds
//! for the host registry, the project list and the lease state file, and the alias bytes a lease
//! has on the box and on another host.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every file lives in a throwaway folder
//! under the temp folder. A test MUST NOT start a real process, and MUST NOT print, log or format
//! the values of an environment a fake start captured: `Start` has no `Debug`, and an assertion
//! about a variable names it and never shows its value.
#![allow(dead_code)]

use crate::common::{Fx, ID, now, parse_time};
use agentspace_detach::Detach;
use agentspace_driver_api::{Context, Failure, PlaceRequest, Placed};
use agentspace_driver_ec2::Ec2Driver;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

// ---- Times --------------------------------------------------------------------------------------

/// A time in the contract's form, `YYYY-MM-DDTHH:MM:SSZ`.
pub fn iso(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// The Unix seconds of a time in the contract's form.
pub fn epoch_of(text: &str) -> i64 {
    assert!(text.ends_with('Z'), "A time ends in Z: {text}");
    parse_time(text.trim_end_matches('Z'))
}

// ---- The fake detached start ----------------------------------------------------------------------

/// What a test sees of the world at the moment a start is made.
#[derive(Clone, Debug, Default)]
pub struct Observed {
    /// Whether `hosts.lock` could be taken from outside: the driver MUST have let it go.
    pub hosts_lock_free: bool,
    pub host_dir_exists: bool,
    pub provision_lock_exists: bool,
    pub provision_log_exists: bool,
    /// `hosts.json` as it was at the start, or none.
    pub hosts_json: Option<Vec<u8>>,
}

/// Looks at the throwaway home as a start would find it.
pub fn observe(home: &Path, project: &str) -> Observed {
    Observed {
        hosts_lock_free: agentspace_hosts::try_lock(home)
            .map(|l| l.is_some())
            .unwrap_or(false),
        host_dir_exists: agentspace_hosts::host_dir(home, project).is_dir(),
        provision_lock_exists: agentspace_hosts::host_run_lock_path(home, project).exists(),
        provision_log_exists: agentspace_hosts::host_log_path(home, project).exists(),
        hosts_json: fs::read(agentspace_hosts::hosts_path(home)).ok(),
    }
}

/// One start the driver made. It has no `Debug`: its environment is never printed.
#[derive(Clone)]
pub struct Start {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    env: Vec<(String, OsString)>,
    pub seen: Observed,
}

impl Start {
    /// The names of the variables the start was given, in upper case.
    pub fn env_names(&self) -> Vec<String> {
        self.env.iter().map(|(n, _)| n.to_uppercase()).collect()
    }

    /// What the process would have for `name`: the last pair of that name wins, and an empty value
    /// makes the variable absent.
    pub fn effective(&self, name: &str) -> Option<OsString> {
        let mut out = None;
        for (n, v) in &self.env {
            if n.eq_ignore_ascii_case(name) {
                out = if v.is_empty() { None } else { Some(v.clone()) };
            }
        }
        out
    }
}

type Probe = Arc<dyn Fn() -> Observed + Send + Sync>;

/// A `Detach` that starts nothing: it records each start, and can be told to look at the home
/// when one is made, to fail, or to take its time.
pub struct FakeDetach {
    starts: Mutex<Vec<Start>>,
    fail: Mutex<Option<String>>,
    probe: Mutex<Option<Probe>>,
    block: Mutex<Duration>,
}

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl FakeDetach {
    pub fn new() -> Arc<FakeDetach> {
        Arc::new(FakeDetach {
            starts: Mutex::new(Vec::new()),
            fail: Mutex::new(None),
            probe: Mutex::new(None),
            block: Mutex::new(Duration::ZERO),
        })
    }

    /// A fake that looks at `home` and `project`'s folder at the moment of each start.
    pub fn observing(home: &Path, project: &str) -> Arc<FakeDetach> {
        let d = FakeDetach::new();
        let (home, project) = (home.to_path_buf(), project.to_string());
        *locked(&d.probe) = Some(Arc::new(move || observe(&home, &project)));
        d
    }

    /// A fake whose every start fails with this reason, after it is recorded.
    pub fn failing(reason: &str) -> Arc<FakeDetach> {
        let d = FakeDetach::new();
        *locked(&d.fail) = Some(reason.to_string());
        d
    }

    /// A fake whose every start takes this long to return, after it is recorded.
    pub fn blocking(time: Duration) -> Arc<FakeDetach> {
        let d = FakeDetach::new();
        *locked(&d.block) = time;
        d
    }

    /// How many starts were made.
    pub fn count(&self) -> usize {
        locked(&self.starts).len()
    }

    /// Every start, in order.
    pub fn all(&self) -> Vec<Start> {
        locked(&self.starts).clone()
    }

    /// The one start, with a failure that says how many there were when there was not one.
    #[track_caller]
    pub fn only(&self) -> Start {
        let all = self.all();
        assert_eq!(
            all.len(),
            1,
            "Expected exactly one start, got {}",
            all.len()
        );
        all.into_iter().next().expect("One start")
    }
}

impl Detach for FakeDetach {
    fn start(
        &self,
        program: &Path,
        args: &[&str],
        cwd: &Path,
        env: &[(&str, &OsStr)],
    ) -> Result<(), String> {
        let probe = locked(&self.probe).clone();
        let seen = probe.map(|p| p()).unwrap_or_default();
        locked(&self.starts).push(Start {
            program: program.to_path_buf(),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: cwd.to_path_buf(),
            env: env
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_os_string()))
                .collect(),
            seen,
        });
        let wait = *locked(&self.block);
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
        match locked(&self.fail).clone() {
            Some(reason) => Err(reason),
            None => Ok(()),
        }
    }
}

/// What a run's environment MUST be for a context (driver.md, The detached start; the architect's
/// amended pin of 2026-11-21): `USERPROFILE` is the context's home; `AGENTSPACE_STATE_DIR` is
/// present, and the context's `state_dir_env`, if and only if that is `Some`; `AGENTSPACE_SSH_DIR`
/// likewise with `ssh_dir_env`; `AGENTSPACE_PROXY_COMMAND` is the override exactly when there is
/// one; and no other variable is given. A failure names a variable and never shows a value.
#[track_caller]
pub fn assert_run_env(s: &Start, ctx: &Context) {
    let allowed = [
        "USERPROFILE",
        "AGENTSPACE_STATE_DIR",
        "AGENTSPACE_SSH_DIR",
        "AGENTSPACE_PROXY_COMMAND",
    ];
    for name in s.env_names() {
        assert!(
            allowed.contains(&name.as_str()),
            "The run was given a variable that is not one of the four: {name}"
        );
    }
    assert!(
        s.effective("USERPROFILE").as_deref() == Some(ctx.home.as_os_str()),
        "USERPROFILE is not the context's home"
    );
    assert!(
        s.effective("AGENTSPACE_STATE_DIR").as_deref()
            == ctx.state_dir_env.as_deref().map(Path::as_os_str),
        "AGENTSPACE_STATE_DIR is not present exactly when the CLI had it, with its value"
    );
    assert!(
        s.effective("AGENTSPACE_SSH_DIR").as_deref()
            == ctx.ssh_dir_env.as_deref().map(Path::as_os_str),
        "AGENTSPACE_SSH_DIR is not present exactly when the CLI had it, with its value"
    );
    assert!(
        s.effective("AGENTSPACE_PROXY_COMMAND").as_deref()
            == ctx.proxy_override.as_deref().map(OsStr::new),
        "AGENTSPACE_PROXY_COMMAND is not the context's override, exactly"
    );
}

// ---- The fixture ----------------------------------------------------------------------------------

impl Fx {
    /// A driver that starts through `detach`.
    pub fn driver_with(&self, detach: &Arc<FakeDetach>) -> Ec2Driver {
        Ec2Driver::with_detach(detach.clone())
    }

    /// Puts an empty `agentspace-operator.exe` beside the program and names it as the context's
    /// operator program. It is never run: the start is a fake's.
    pub fn install_operator(&mut self) -> PathBuf {
        let program = self.bin.join("agentspace-operator.exe");
        fs::write(&program, b"").unwrap();
        self.ctx.operator_program = Some(program.clone());
        program
    }

    /// An existing stand-in file with this extension, named as the context's operator program.
    pub fn install_stand_in(&mut self, ext: &str) -> PathBuf {
        let program = self.bin.join(format!("stand in.{ext}"));
        fs::write(&program, b"@exit /b 0\r\n").unwrap();
        self.ctx.operator_program = Some(program.clone());
        program
    }

    /// A state folder that is not the default, which does not exist yet.
    pub fn state_elsewhere(&mut self) -> PathBuf {
        let state = self.root.join("state dir");
        self.ctx.state_dir = state.clone();
        state
    }

    /// A supported request.
    pub fn req<'a>(&self, project: &'a str, id: &'a str, ttl: u32) -> PlaceRequest<'a> {
        self.request_for(project, id, "linux/arm64", "agent-linux", "m", ttl)
    }

    pub fn request_for<'a>(
        &self,
        project: &'a str,
        id: &'a str,
        platform: &'a str,
        profile: &'a str,
        size: &'a str,
        ttl: u32,
    ) -> PlaceRequest<'a> {
        PlaceRequest {
            id,
            project,
            platform,
            profile,
            size,
            ttl_seconds: ttl,
        }
    }

    /// Places a supported lease for `project` with a TTL of an hour.
    pub fn place_project(
        &self,
        driver: &Ec2Driver,
        project: &str,
        id: &str,
    ) -> Result<Placed, Failure> {
        use agentspace_driver_api::Driver;
        driver.place(&self.ctx, &self.req(project, id, 3600))
    }

    /// The same, for the ID `ID`.
    pub fn place_for(&self, driver: &Ec2Driver, project: &str) -> Result<Placed, Failure> {
        self.place_project(driver, project, ID)
    }

    /// `hosts.json`'s bytes, or none.
    pub fn hosts_bytes(&self) -> Option<Vec<u8>> {
        fs::read(agentspace_hosts::hosts_path(&self.home)).ok()
    }

    /// The registry's record of `project`, read through agentspace-hosts.
    pub fn host(&self, project: &str) -> Option<agentspace_hosts::Host> {
        let hosts = agentspace_hosts::read(&self.home).expect("The registry reads");
        agentspace_hosts::host_for(&hosts, project).cloned()
    }

    /// Writes `hosts.json` with the box's record first and these after it.
    pub fn write_hosts(&self, extra: &[H]) {
        let dir = agentspace_hosts::agentspace_dir(&self.home);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            agentspace_hosts::hosts_path(&self.home),
            hosts_text(extra).as_bytes(),
        )
        .unwrap();
    }

    /// Writes `hosts.json` with these exact bytes.
    pub fn write_hosts_raw(&self, bytes: &[u8]) {
        fs::create_dir_all(agentspace_hosts::agentspace_dir(&self.home)).unwrap();
        fs::write(agentspace_hosts::hosts_path(&self.home), bytes).unwrap();
    }

    /// Writes `projects.json` listing these names.
    pub fn write_projects(&self, names: &[&str]) {
        let items: Vec<String> = names
            .iter()
            .map(|n| format!("{{\"name\":\"{n}\",\"approved\":\"2026-11-20\"}}"))
            .collect();
        fs::create_dir_all(agentspace_hosts::agentspace_dir(&self.home)).unwrap();
        fs::write(
            agentspace_hosts::projects_path(&self.home),
            format!("{{\"version\":0,\"projects\":[{}]}}\n", items.join(",")).as_bytes(),
        )
        .unwrap();
    }

    pub fn write_projects_raw(&self, bytes: &[u8]) {
        fs::create_dir_all(agentspace_hosts::agentspace_dir(&self.home)).unwrap();
        fs::write(agentspace_hosts::projects_path(&self.home), bytes).unwrap();
    }

    /// Writes the lease state file in `ctx.state_dir`.
    pub fn write_leases(&self, leases: &[L]) {
        write_leases(&self.ctx.state_dir, leases);
    }

    pub fn leases_bytes(&self) -> Option<Vec<u8>> {
        fs::read(self.ctx.state_dir.join("leases.json")).ok()
    }

    /// Every record of the lease state file, in order.
    pub fn lease_values(&self) -> Vec<serde_json::Value> {
        let bytes = self.leases_bytes().expect("The lease state file exists");
        let doc: serde_json::Value =
            serde_json::from_slice(&bytes).expect("The lease state file is JSON");
        doc["leases"]
            .as_array()
            .expect("The file has leases")
            .clone()
    }

    pub fn lease_value(&self, id: &str) -> serde_json::Value {
        self.lease_values()
            .into_iter()
            .find(|r| r["id"] == id)
            .unwrap_or_else(|| panic!("No record has the ID {id}"))
    }

    /// The names in the OpenSSH folder, or none when it does not exist.
    pub fn ssh_names(&self) -> Vec<String> {
        if self.ssh_dir.is_dir() {
            Fx::names(&self.ssh_dir)
        } else {
            Vec::new()
        }
    }

    /// The alias file of the lease `id` of `project`.
    pub fn alias_file_of(&self, project: &str, id: &str) -> PathBuf {
        self.alias_dir().join(alias_for(project, id))
    }

    pub fn lease_files(&self, id: &str) -> Vec<String> {
        let dir = self.lease_dir(id);
        if dir.is_dir() {
            Fx::names(&dir)
        } else {
            Vec::new()
        }
    }

    /// Whether anything of lease `id` of `project` is on the disk: its folder or its alias file.
    pub fn has_files_of(&self, project: &str, id: &str) -> bool {
        self.lease_dir(id).exists() || self.alias_file_of(project, id).exists()
    }
}

/// The alias of a lease: `agentspace-<project>-<first 8 hex digits of the id>`.
pub fn alias_for(project: &str, id: &str) -> String {
    format!("agentspace-{project}-{}", &id[..8])
}

// ---- The host registry and the project list ---------------------------------------------------------

/// One record a test writes into `hosts.json`, after the box's.
#[derive(Clone)]
pub struct H {
    pub project: String,
    pub state: &'static str,
    pub instance_id: Option<String>,
    pub attempt: Option<i64>,
}

impl H {
    fn of(project: &str, state: &'static str) -> H {
        H {
            project: project.to_string(),
            state,
            instance_id: None,
            attempt: None,
        }
    }
    pub fn ready(project: &str) -> H {
        H::of(project, "ready")
    }
    pub fn creating(project: &str) -> H {
        H::of(project, "creating")
    }
    pub fn failed(project: &str) -> H {
        H::of(project, "failed")
    }
    /// With `last_attempt_at` this many Unix seconds.
    pub fn attempt(mut self, secs: i64) -> H {
        self.attempt = Some(secs);
        self
    }
    /// With `last_attempt_at` this many seconds before now.
    pub fn attempted_ago(self, secs: i64) -> H {
        let at = now() - secs;
        self.attempt(at)
    }
    pub fn instance(mut self, id: &str) -> H {
        self.instance_id = Some(id.to_string());
        self
    }
}

/// An instance ID that is made up, and matches no real one.
pub const INSTANCE: &str = "i-1234567890abcdef0";

/// The registry's bytes as state.md writes them: the box's record first, then these.
pub fn hosts_text(extra: &[H]) -> String {
    let mut out = String::from(
        "{\"version\":0,\"hosts\":[{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\"}",
    );
    for h in extra {
        out.push_str(&format!(
            ",{{\"project\":\"{p}\",\"stack\":\"agentspace-host-{p}\",\"state\":\"{s}\"",
            p = h.project,
            s = h.state
        ));
        if let Some(id) = &h.instance_id {
            out.push_str(&format!(",\"instance_id\":\"{id}\""));
        }
        if let Some(at) = h.attempt {
            out.push_str(&format!(",\"last_attempt_at\":\"{}\"", iso(at)));
        }
        out.push('}');
    }
    out.push_str("]}\n");
    out
}

// ---- The lease state file -----------------------------------------------------------------------------

/// One record a test writes into the lease state file: `pending`, or `released` from pending.
#[derive(Clone)]
pub struct L {
    pub id: String,
    pub project: String,
    state: &'static str,
    pub ttl: u32,
    pub created_at: i64,
    pub driver: Option<String>,
}

impl L {
    fn of(id: &str, project: &str, state: &'static str) -> L {
        L {
            id: id.to_string(),
            project: project.to_string(),
            state,
            ttl: 3600,
            created_at: now() - 7200,
            driver: Some("ec2".to_string()),
        }
    }
    /// A `pending` lease placed by the `ec2` driver, created two hours ago.
    pub fn pending(id: &str, project: &str) -> L {
        L::of(id, project, "pending")
    }
    /// A lease released while it was pending.
    pub fn released(id: &str, project: &str) -> L {
        L::of(id, project, "released")
    }
    pub fn ttl(mut self, secs: u32) -> L {
        self.ttl = secs;
        self
    }
    pub fn driver(mut self, name: Option<&str>) -> L {
        self.driver = name.map(str::to_string);
        self
    }
    pub fn json(&self) -> String {
        let mut out = format!(
            "{{\"id\":\"{}\",\"project\":\"{}\",\"state\":\"{}\",\"platform\":\"linux/arm64\",\"profile\":\"agent-linux\",\"size\":\"m\",\"ttl_seconds\":{},\"created_at\":\"{}\"",
            self.id,
            self.project,
            self.state,
            self.ttl,
            iso(self.created_at)
        );
        if self.state == "released" {
            out.push_str(&format!(
                ",\"released_at\":\"{}\"",
                iso(self.created_at + 60)
            ));
        }
        if let Some(d) = &self.driver {
            out.push_str(&format!(",\"driver\":\"{d}\""));
        }
        out.push('}');
        out
    }
}

/// Writes `<state dir>\leases.json` with these records, as the CLI writes it.
pub fn write_leases(state_dir: &Path, leases: &[L]) {
    fs::create_dir_all(state_dir).unwrap();
    let items: Vec<String> = leases.iter().map(L::json).collect();
    fs::write(
        state_dir.join("leases.json"),
        format!("{{\"version\":0,\"leases\":[{}]}}\n", items.join(",")).as_bytes(),
    )
    .unwrap();
}

/// Holds the lease state lock the way state.md says it is held: `leases.lock` open for reading and
/// writing with share mode 0. It is let go when the file is dropped.
pub fn hold_lease_lock(state_dir: &Path) -> fs::File {
    fs::create_dir_all(state_dir).unwrap();
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(state_dir.join("leases.lock"))
        .expect("The test can take the lease state lock")
}

// ---- The alias file's bytes ---------------------------------------------------------------------------

/// A path as an alias file holds it: every `\` as `/` and every `%` as `%%`.
fn alias_path(text: &str) -> String {
    text.replace('%', "%%").replace('\\', "/")
}

/// What an alias file holds after its first line, exactly (state.md, The alias file; A lease alias
/// for a host that is not the box). The box's form is v0.06's bytes.
pub fn expected_alias_tail(fx: &Fx, id: &str, project: &str, other_host: bool) -> String {
    let home = fx.ctx.home.display().to_string();
    let key = alias_path(&format!("{home}\\.agentspace\\leases\\{id}\\id_ed25519"));
    let cert = alias_path(&format!(
        "{home}\\.agentspace\\leases\\{id}\\id_ed25519-cert.pub"
    ));
    let alias = alias_for(project, id);
    let proxy = match &fx.ctx.proxy_override {
        Some(line) => line.replace('%', "%%"),
        None => format!(
            "\"{}\" {alias}",
            fx.bin.join("agentspace-ssh-proxy.exe").display()
        )
        .replace('%', "%%"),
    };
    let host_key = if other_host {
        let known = alias_path(&format!("{home}\\.agentspace\\known_hosts"));
        format!("  HostKeyAlias agentspace-host.{project}\n  UserKnownHostsFile \"{known}\"\n")
    } else {
        "  HostKeyAlias agentspace\n".to_string()
    };
    format!(
        "Host {alias}\n  User ubuntu\n  IdentityFile \"{key}\"\n  CertificateFile \"{cert}\"\n  IdentitiesOnly yes\n{host_key}  StrictHostKeyChecking yes\n  ProxyCommand {proxy}\n  ServerAliveInterval 30\n  ServerAliveCountMax 4\n  RemoteForward 127.0.0.1:8811 127.0.0.1:8811\n  RemoteForward 127.0.0.1:53147 127.0.0.1:53147\n"
    )
}

/// The lease's alias file holds the owner mark, then exactly the form for the box or for another
/// host, byte for byte.
#[track_caller]
pub fn assert_alias_bytes(fx: &Fx, id: &str, project: &str, other_host: bool) {
    let path = fx.alias_file_of(project, id);
    let bytes = fs::read(&path).expect("The alias file exists");
    assert!(
        !bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "The alias file has a byte order mark"
    );
    let text = String::from_utf8(bytes).expect("The alias file is UTF-8");
    let (first, rest) = text.split_once('\n').expect("The alias file has lines");
    assert!(
        first.starts_with(&format!("# agentspace lease {id}; ")) && !first.contains('\r'),
        "The first line is not the owner mark"
    );
    assert_eq!(
        rest,
        expected_alias_tail(fx, id, project, other_host),
        "The alias file's bytes after the owner mark"
    );
}

/// A foreign file at a lease's alias place: no owner mark, so it is nobody's.
pub fn plant_foreign_alias(fx: &Fx, project: &str, id: &str) -> Vec<u8> {
    fs::create_dir_all(fx.alias_dir()).unwrap();
    let bytes = format!("Host {}\n  User someone\n", alias_for(project, id)).into_bytes();
    fs::write(fx.alias_file_of(project, id), &bytes).unwrap();
    bytes
}

/// The Unix seconds of a record's `expires_at`.
#[track_caller]
pub fn expires_of(record: &serde_json::Value) -> i64 {
    epoch_of(record["expires_at"].as_str().expect("expires_at is text"))
}
