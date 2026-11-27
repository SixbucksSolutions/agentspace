//! Which host a connection reaches, and the host's instance ID as a member of its record
//! (docs/operator.md, From the ProxyCommand and The host registry, as the operator uses it; D45, D49,
//! D55).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! **From v0.12 a host is its project and its platform** (D91): every lookup below is by the
//! host's key, a registry record without `platform` being `linux/arm64`'s.
//!
//! The host comes from the ProxyCommand's one argument, in this order:
//! 1. **No argument: the box.**
//! 2. **`agentspace-host.<host name>`: that host**, whatever its state. The host name is
//!    `<project>`, the project's `linux/arm64` host, or `<project>.<os>-<arch>`, its host on that
//!    platform, the name part being `linux-amd64` or `windows-amd64` exactly (pins P1 and P12 of
//!    v0.12-T01). A rest that is not a host name, and a host with no record, fail the connection.
//! 3. **Any other argument, a lease alias** ([`lease_key`]): the lease state file is read
//!    without its lock, retried on a sharing violation alone (pin P23), for the records that are the
//!    alias's: the alias built from a record's `project` and `id`, or a stored `ready` record's
//!    `ssh.host_alias` (pins P21 and P22). Their states are decided at one reading of the clock, a
//!    stored `ready` record at or after its `expires_at` being `expired`. The first `ready` one is
//!    the alias's lease; when none is `ready`, the connection is refused, naming the last one's
//!    state, **before the host registry is read** ([`choose`] comes after). No record, or no file,
//!    is the box. A file that cannot be read or is not valid refuses an argument of lease alias
//!    form ([`is_lease_alias_form`]), and sends any other argument to the box. A `ready` lease's
//!    host, its record's `project` on its record's `platform`, is then looked up in the registry:
//!    a `ready` host that is not the box is that host. Otherwise a `linux/arm64` lease reaches the
//!    box, and a lease of any other platform is refused, with no tunnel (pin P13 of v0.12-T01).
//!
//! The ProxyCommand never writes the lease state file and never takes its lock.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use agentspace_box::{CacheRead, InstanceIdCache, cache_bytes, parse_cache};
use agentspace_hosts::{
    Host, HostKey, Hosts, HostsError, Target, host_for_keyed, hosts_path, is_box,
    is_normalized_project, target_for_keyed,
};
use agentspace_operator_api::{CallerFailure, Failure, Say};
use agentspace_state::{Record, StateFile, StoreError, parse_time};

use crate::platform;

/// What a host's operator alias starts with (D47, D55). No lease alias holds a `.`, so none starts
/// with it.
pub const HOST_ALIAS_PREFIX: &str = "agentspace-host.";

/// The lease state file's name in the state folder.
const LEASE_FILE: &str = "leases.json";

fn could_not_reach(message: String) -> Failure {
    Failure {
        message,
        caller: CallerFailure::CouldNotReach,
    }
}

/// The failure of a host registry that cannot be read or is not valid (operator.md, The host
/// registry, as the operator uses it): `agentspace: The host registry <path> cannot be read:
/// <reason>.`, which a caller sees as "Could not reach the host".
pub fn registry_failure(home: &Path, error: &HostsError) -> Failure {
    let reason = match error {
        HostsError::Unreadable(why) => why.to_string(),
        HostsError::NotValid(why) => why.to_string(),
        other => other.to_string().trim_end_matches('.').to_string(),
    };
    could_not_reach(format!(
        "agentspace: The host registry {} cannot be read: {reason}.",
        hosts_path(home).display()
    ))
}

/// The state folder: `AGENTSPACE_STATE_DIR` when it is set and not empty, made full against the
/// working folder, and `<home>\.agentspace` otherwise (state.md, What moves which files).
pub fn state_dir(override_dir: Option<std::ffi::OsString>, home: &Path) -> PathBuf {
    match override_dir.filter(|v| !v.is_empty()) {
        Some(dir) => {
            let dir = PathBuf::from(dir);
            if dir.is_absolute() {
                dir
            } else {
                std::path::absolute(&dir).unwrap_or(dir)
            }
        }
        None => home.join(".agentspace"),
    }
}

/// The box's record: the one whose stack is `agentbox`. A valid registry always holds exactly one.
fn the_box(hosts: &Hosts) -> Result<&Host, Failure> {
    hosts.hosts.iter().find(|h| is_box(h)).ok_or_else(|| {
        could_not_reach("agentspace: The host registry holds no record for the box.".to_string())
    })
}

/// The diagnostic line printed before each retry of a read refused on a sharing violation (pin P27).
pub const RETRY_LINE: &str =
    "agentspace: The lease records were held by another program; trying again.";

/// At most this many attempts at the read (pin P23).
const READ_ATTEMPTS: u32 = 3;
/// The pause before each retry, so that attempts start about 0, 400 and 800 milliseconds after the
/// first (pin P27).
const RETRY_PAUSE: Duration = Duration::from_millis(400);
/// No attempt starts more than this long after the first began (pin P23).
const RETRY_WINDOW: Duration = Duration::from_secs(1);

/// Why the file could not be read when every attempt was refused on a sharing violation.
const WHY_HELD: &str = "another program held it";
/// The reasons a read of the ProxyCommand's own gives, in the words `agentspace_state::read` uses.
const WHY_NOT_A_FILE: &str = "a folder is where the state file belongs";
const WHY_READ: &str = "the state file could not be read";

/// The clocks the lease lookup reads, apart from the system's so that a test injects its own and
/// never reads the real clock.
pub trait LeaseClock {
    /// The time, UTC, in whole seconds since the Unix epoch: the one reading a lease's state is
    /// decided at.
    fn now(&self) -> i64;
    /// The time passed on a monotonic clock since a fixed point of the clock's own.
    fn elapsed(&self) -> Duration;
    /// Waits for `duration`.
    fn pause(&self, duration: Duration);
}

/// The system's clocks: the wall clock for the lease's state, a monotonic one for the retries.
pub struct SystemLeaseClock {
    start: Instant,
}

impl SystemLeaseClock {
    pub fn new() -> SystemLeaseClock {
        SystemLeaseClock {
            start: Instant::now(),
        }
    }
}

impl Default for SystemLeaseClock {
    fn default() -> Self {
        SystemLeaseClock::new()
    }
}

impl LeaseClock for SystemLeaseClock {
    fn now(&self) -> i64 {
        agentspace_state::now()
    }

    fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    fn pause(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// Whether `alias` is of lease alias form (operator.md, From the ProxyCommand, step 3; pin P21):
/// the whole of it matches `^agentspace-[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?-[0-9a-f]{8}$`,
/// case-sensitively. The tail is fixed at 9 characters, so the project is what lies between.
pub fn is_lease_alias_form(alias: &str) -> bool {
    let Some(rest) = alias.strip_prefix("agentspace-") else {
        return false;
    };
    let Some(split) = rest.len().checked_sub(9) else {
        return false;
    };
    let (Some(project), Some(tail)) = (rest.get(..split), rest.get(split..)) else {
        return false;
    };
    let Some(hex) = tail.strip_prefix('-') else {
        return false;
    };
    hex.len() == 8
        && hex.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
        && is_normalized_project(project)
}

/// A lease's state at the clock's one reading (lease-api.md, States).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseState {
    Ready,
    Expired,
    Released,
    Unsatisfiable,
    Pending,
    /// A stored state that is none of the contract's, which a valid file never holds.
    Unknown,
}

/// The state of `record` at `now`: its stored state, but a stored `ready` record at or after its
/// `expires_at`, compared as instants to the second, is `expired`. A stored `ready` record whose
/// `expires_at` does not parse is `expired`, as the lease CLI's own documents derive it; a valid
/// file never holds one.
pub fn state_at(record: &Record, now: i64) -> LeaseState {
    match record.state.as_str() {
        "ready" => match record.expires_at.as_deref() {
            Some(at) if parse_time(at).is_none_or(|at| now >= at) => LeaseState::Expired,
            _ => LeaseState::Ready,
        },
        "released" => LeaseState::Released,
        "unsatisfiable" => LeaseState::Unsatisfiable,
        "pending" => LeaseState::Pending,
        _ => LeaseState::Unknown,
    }
}

/// Whether `record` is `alias`'s (pins P21 and P22): the alias built from its `project` and `id`,
/// `agentspace-<project>-<first 8 characters of id>`, is `alias`, or it is stored `ready` and its
/// `ssh.host_alias` is `alias`. Both comparisons are exact and case-sensitive.
fn is_the_alias(record: &Record, alias: &str) -> bool {
    let built = record
        .id
        .get(..8)
        .is_some_and(|id8| alias == format!("agentspace-{}-{id8}", record.project));
    built
        || (record.state == "ready"
            && record
                .ssh
                .as_ref()
                .is_some_and(|ssh| ssh.host_alias == alias))
}

fn refusal(message: String) -> Failure {
    Failure {
        message,
        caller: CallerFailure::Same,
    }
}

/// The refusal of a lease-form argument while the lease state file cannot be read or is not valid
/// (operator.md, A lease that is not ready (#0229)).
pub fn unreadable_refusal(alias: &str) -> Failure {
    refusal(format!(
        "agentspace: The lease records could not be read; no connection was opened for {alias}. Run 'agentspace lease list' to see the error."
    ))
}

/// The refusal of an alias whose lease is `state` (operator.md, A lease that is not ready (#0229)).
/// `ready` is never refused; a stored state that is none of the contract's is refused as records
/// that could not be read, since a file that holds one is not valid.
pub fn not_ready_refusal(alias: &str, state: LeaseState) -> Failure {
    let line = match state {
        LeaseState::Released => format!(
            "agentspace: The lease for {alias} has been released; no connection was opened. Create a new lease."
        ),
        LeaseState::Expired => format!(
            "agentspace: The lease for {alias} has expired; no connection was opened. Create a new lease."
        ),
        LeaseState::Unsatisfiable => format!(
            "agentspace: The lease for {alias} is unsatisfiable; no connection was opened. Create a new lease."
        ),
        LeaseState::Pending => format!(
            "agentspace: The lease for {alias} is pending, not yet ready; no connection was opened. Connect once 'agentspace lease get' answers ready."
        ),
        LeaseState::Ready | LeaseState::Unknown => return unreadable_refusal(alias),
    };
    refusal(line)
}

/// The refusal of a `ready` lease on a platform other than `linux/arm64` whose project has no
/// `ready` record on that platform (operator.md, A lease that is not ready (#0229); pins P13 and
/// P56 of v0.12-T01). Its lease is `ready`, so it names no state.
pub fn no_ready_host_refusal(alias: &str) -> Failure {
    refusal(format!(
        "agentspace: The lease for {alias} has no ready host on its platform; no connection was opened."
    ))
}

/// Decides `alias` from the records at `now` (pin P22): the first record that is the alias's and
/// `ready` gives its host's key, the record's `project` and `platform` as stored (D91; pin P8 of
/// v0.12-T01); when records are the alias's and none is `ready`, the alias is refused naming the
/// last one's state; when none is, `None`.
pub fn decide(leases: &[Record], alias: &str, now: i64) -> Result<Option<HostKey>, Failure> {
    let mut last = None;
    for record in leases.iter().filter(|r| is_the_alias(r, alias)) {
        let state = state_at(record, now);
        if state == LeaseState::Ready {
            return Ok(Some(HostKey::new(
                record.project.clone(),
                record.platform.clone(),
            )));
        }
        last = Some(state);
    }
    match last {
        None => Ok(None),
        Some(state) => Err(not_ready_refusal(alias, state)),
    }
}

/// One attempt at the read: the file, or why it cannot be used, or `None` when the open was
/// refused on a sharing violation.
fn attempt(state_dir: &Path) -> Option<Result<StateFile, String>> {
    let path = state_dir.join(LEASE_FILE);
    match platform::open_shared(&path) {
        Ok(mut file) => {
            if file.metadata().is_ok_and(|m| m.is_dir()) {
                return Some(Err(WHY_NOT_A_FILE.to_string()));
            }
            let mut bytes = Vec::new();
            if file.read_to_end(&mut bytes).is_err() {
                return Some(Err(WHY_READ.to_string()));
            }
            Some(agentspace_state::parse(&bytes).map_err(|e| e.0))
        }
        Err(error) if platform::is_sharing_violation(&error) => None,
        // A missing file or folder, a folder at the path, a file where the state folder belongs,
        // and every other refusal: the lease records' own read decides, in its own words, and is
        // not tried again.
        Err(_) => Some(
            agentspace_state::read(state_dir).map_err(|error| match error {
                StoreError::Busy => "it is in use".to_string(),
                StoreError::CannotLock(why)
                | StoreError::Unreadable(why)
                | StoreError::NotValid(why)
                | StoreError::Unwritable(why) => why,
            }),
        ),
    }
}

/// Reads the lease state file whole, without its lock (operator.md, From the ProxyCommand, step 3).
/// A missing file is no leases. A read refused on a sharing violation is tried again, at most 3
/// attempts, with a 400 millisecond pause before each retry, and no attempt starts more than 1
/// second after the first began (pins P23 and P27); before each retry `say` gets [`RETRY_LINE`],
/// which only `AGENTSPACE_DIAGNOSTIC` shows. Every other failure is decided on the attempt that met
/// it.
fn read_leases(
    state_dir: &Path,
    say: &dyn Say,
    clock: &dyn LeaseClock,
) -> Result<StateFile, String> {
    let first = clock.elapsed();
    let since_first = || clock.elapsed().saturating_sub(first);
    let mut attempts = 1;
    loop {
        if let Some(read) = attempt(state_dir) {
            return read;
        }
        if attempts >= READ_ATTEMPTS || since_first() + RETRY_PAUSE > RETRY_WINDOW {
            return Err(WHY_HELD.to_string());
        }
        say.detail(RETRY_LINE);
        clock.pause(RETRY_PAUSE);
        if since_first() > RETRY_WINDOW {
            return Err(WHY_HELD.to_string());
        }
        attempts += 1;
    }
}

/// Step 3 of From the ProxyCommand for a lease alias, made before the host registry is read: the
/// key of `alias`'s `ready` lease's host, its project and platform, or `None` for the box, or the
/// refusal of a lease that is not `ready` (#0229; pins P21 to P23 and P27; D91).
///
/// `None` for no argument and for a host alias, which read nothing here. A file that cannot be
/// read or is not valid gives `say` a line that names the file and the problem, which only
/// `AGENTSPACE_DIAGNOSTIC` shows, then refuses an argument of lease alias form and sends any other
/// to the box. The lease's state is decided at one reading of `clock`, taken once the read is made.
pub fn lease_key(
    alias: Option<&str>,
    state_dir: &Path,
    say: &dyn Say,
    clock: &dyn LeaseClock,
) -> Result<Option<HostKey>, Failure> {
    let Some(alias) = alias else {
        return Ok(None);
    };
    if alias.starts_with(HOST_ALIAS_PREFIX) {
        return Ok(None);
    }
    match read_leases(state_dir, say, clock) {
        Ok(file) => decide(&file.leases, alias, clock.now()),
        Err(why) => {
            say.detail(&format!(
                "agentspace: The lease state file {} cannot be used: {why}.",
                state_dir.join(LEASE_FILE).display()
            ));
            if is_lease_alias_form(alias) {
                Err(unreadable_refusal(alias))
            } else {
                Ok(None)
            }
        }
    }
}

/// The host an operator alias's rest names (operator.md, From the ProxyCommand, step 2; pins P1 and
/// P12 of v0.12-T01): a host name, `<project>` for the project's `linux/arm64` host or
/// `<project>.<os>-<arch>` for its host on that platform, read by the push's own reading of a host
/// name, so the ProxyCommand and the push stamp it chooses agree on every name. `None` for a rest
/// that is not a host name: a project not in the normalized form, or a name part that is neither
/// `linux-amd64` nor `windows-amd64`, `linux-arm64` and an upper-case one included.
pub fn host_alias_key(rest: &str) -> Option<HostKey> {
    let (project, platform) = agentspace_push::parse_host_name(rest)?;
    Some(HostKey::new(project, platform))
}

/// The host the connection reaches (operator.md, From the ProxyCommand), from the registry the
/// caller has already read and validated. `alias` is the ProxyCommand's argument, `None` for no
/// argument, and `lease` is what [`lease_key`] found for it before the registry was read: the
/// lease's host key, its record's project and platform.
///
/// A `ready` lease whose host record is `ready` reaches that host; a `linux/arm64` lease whose
/// project has no `ready` record on `linux/arm64` reaches the box; and a lease of any other
/// platform with no `ready` record on its platform is refused ([`no_ready_host_refusal`]), before
/// anything is started (pin P13 of v0.12-T01).
pub fn choose<'a>(
    alias: Option<&str>,
    hosts: &'a Hosts,
    lease: Option<&HostKey>,
) -> Result<&'a Host, Failure> {
    let Some(alias) = alias else {
        return the_box(hosts);
    };
    if let Some(rest) = alias.strip_prefix(HOST_ALIAS_PREFIX) {
        let record = host_alias_key(rest).and_then(|key| host_for_keyed(hosts, &key));
        return record.ok_or_else(|| {
            could_not_reach("agentspace: The host alias names no host in the registry.".to_string())
        });
    }
    let Some(key) = lease else {
        return the_box(hosts);
    };
    match target_for_keyed(hosts, key) {
        Some(Target::Host(_)) => host_for_keyed(hosts, key).ok_or_else(|| {
            could_not_reach("agentspace: The host registry changed while it was read.".into())
        }),
        Some(Target::Box) => the_box(hosts),
        None => Err(no_ready_host_refusal(alias)),
    }
}

/// The instance ID as a member of a host's record in the registry, which replaces the instance-ID
/// cache (D45; operator.md, The host registry, as the operator uses it).
///
/// **The caller has read and validated the registry first**, so `read` gives the record's
/// `instance_id` as the cache's bytes, or missing when the record holds none, and never an
/// unopenable cache: that message tells a person to remove a file, which MUST NEVER be said of
/// `hosts.json`.
///
/// **Its `write` is the ProxyCommand's write and nothing more**: when the record still holds no
/// `instance_id`, under the registry's lock and from a fresh read, it puts the ID there and
/// changes nothing else. It never replaces an ID and never adds a record. **A write that fails, or
/// a registry that has become not valid by then, does not fail the connection**: it returns `Ok`,
/// and the connection uses the ID it read.
///
/// **The record is found by the host's key, its project and its platform** (D91), so a write for one
/// of a project's hosts never puts its ID into the project's host on another platform.
#[derive(Debug, Clone)]
pub struct HostCache {
    home: PathBuf,
    path: PathBuf,
    key: HostKey,
    instance_id: Option<String>,
}

impl HostCache {
    /// The cache of `host`'s record, as the caller read it from the registry under `home`.
    pub fn new(home: &Path, host: &Host) -> HostCache {
        HostCache {
            home: home.to_path_buf(),
            path: hosts_path(home),
            key: host.key(),
            instance_id: host.instance_id.clone(),
        }
    }

    fn record(&self, id: &str) -> Result<(), HostsError> {
        let held = agentspace_hosts::lock(&self.home)?;
        let mut hosts = agentspace_hosts::read(&self.home)?;
        let Some(record) = hosts.hosts.iter_mut().find(|h| h.is_for(&self.key)) else {
            return Ok(());
        };
        if record.instance_id.is_some() {
            return Ok(());
        }
        record.instance_id = Some(id.to_string());
        agentspace_hosts::write(&self.home, &hosts, &held)
    }
}

impl InstanceIdCache for HostCache {
    fn path(&self) -> &Path {
        &self.path
    }

    fn read(&self) -> CacheRead {
        match &self.instance_id {
            Some(id) => CacheRead::Bytes(cache_bytes(id)),
            None => CacheRead::Missing,
        }
    }

    fn write(&self, bytes: &[u8]) -> Result<(), Failure> {
        if let Some(id) = parse_cache(bytes) {
            // Architect's to confirm: a failed write is dropped without a diagnostic line, since
            // the connection goes on and the spec gives no text for it.
            let _ = self.record(&id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentspace_hosts::HostState;
    use agentspace_operator::Printer;
    use agentspace_operator_api::Mode;
    use agentspace_state::{Record, Ssh, StateFile};
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Write for Capture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Capture {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    const ID_A: &str = "i-1234567890abcdef0";
    const ID_B: &str = "i-fedcba9876543210f";

    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Dir {
            let dir = std::env::temp_dir().join(format!(
                "agentspace-ssh-proxy-host-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Dir(dir)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn host(project: &str, state: HostState) -> Host {
        Host {
            last_success_at: None,
            platform: None,
            project: project.to_string(),
            stack: agentspace_hosts::stack_name(project),
            state,
            instance_id: None,
            instance_type: None,
            last_attempt_at: None,
        }
    }

    fn registry(extra: &[Host]) -> Hosts {
        let mut hosts = Hosts::seed();
        hosts.hosts.extend(extra.iter().cloned());
        hosts
    }

    fn lease(project: &str, alias: &str) -> Record {
        Record {
            sized_for: None,
            plan: None,
            plan_source: None,
            id: format!("00000000-0000-4000-8000-{:012x}", alias.len()),
            project: project.to_string(),
            state: "ready".to_string(),
            platform: "linux/arm64".to_string(),
            profile: "agent-linux".to_string(),
            size: "medium".to_string(),
            ttl_seconds: 3600,
            created_at: "2026-11-20T10:00:00Z".to_string(),
            expires_at: Some("2026-11-20T11:00:00Z".to_string()),
            released_at: None,
            ssh: Some(Ssh {
                host_alias: alias.to_string(),
                user: "ubuntu".to_string(),
            }),
            reasons: None,
            driver: Some("ec2".to_string()),
        }
    }

    fn write_leases(dir: &Path, leases: Vec<Record>) {
        let bytes = agentspace_state::serialize(&StateFile { leases }).unwrap();
        std::fs::write(dir.join(LEASE_FILE), bytes).unwrap();
    }

    fn say(diagnostic: bool) -> (Printer, Capture) {
        let cap = Capture::default();
        let printer = Printer::new(Mode::Caller, false, diagnostic, Box::new(cap.clone()));
        (printer, cap)
    }

    fn project_of(result: Result<&Host, Failure>) -> String {
        result.unwrap().project.clone()
    }

    /// The fixture's `expires_at`, 2026-11-20T11:00:00Z, as an instant.
    fn expiry() -> i64 {
        parse_time("2026-11-20T11:00:00Z").unwrap()
    }

    /// A clock the test sets: the wall clock reads `now`, the monotonic one starts at 0 and moves
    /// only by a pause, plus `per_pause_extra`, and every pause is recorded and calls `on_pause`
    /// with its count. It never reads the real clock and never sleeps.
    struct FakeClock {
        now: i64,
        elapsed: std::cell::Cell<Duration>,
        per_pause_extra: Duration,
        pauses: std::cell::RefCell<Vec<Duration>>,
        nows: std::cell::Cell<u32>,
        on_pause: std::cell::RefCell<Box<dyn FnMut(usize)>>,
    }

    impl FakeClock {
        fn at(now: i64) -> FakeClock {
            FakeClock {
                now,
                elapsed: std::cell::Cell::new(Duration::ZERO),
                per_pause_extra: Duration::ZERO,
                pauses: std::cell::RefCell::new(Vec::new()),
                nows: std::cell::Cell::new(0),
                on_pause: std::cell::RefCell::new(Box::new(|_| {})),
            }
        }

        /// Before the fixture's expiry.
        fn before_expiry() -> FakeClock {
            FakeClock::at(expiry() - 1800)
        }

        fn pauses(&self) -> Vec<Duration> {
            self.pauses.borrow().clone()
        }
    }

    impl LeaseClock for FakeClock {
        fn now(&self) -> i64 {
            self.nows.set(self.nows.get() + 1);
            self.now
        }
        fn elapsed(&self) -> Duration {
            self.elapsed.get()
        }
        fn pause(&self, duration: Duration) {
            self.elapsed
                .set(self.elapsed.get() + duration + self.per_pause_extra);
            let count = {
                let mut pauses = self.pauses.borrow_mut();
                pauses.push(duration);
                pauses.len()
            };
            (self.on_pause.borrow_mut())(count);
        }
    }

    /// Step 3 whole, as the ProxyCommand runs it: the lease first, then the host.
    fn pick<'a>(
        alias: Option<&str>,
        hosts: &'a Hosts,
        dir: &Path,
        say: &dyn Say,
        clock: &dyn LeaseClock,
    ) -> Result<&'a Host, Failure> {
        let lease = lease_key(alias, dir, say, clock)?;
        choose(alias, hosts, lease.as_ref())
    }

    /// A record of `project` whose built alias is `agentspace-<project>-<hex8>`, in `state`, with the
    /// fields state.md gives that state: `ssh` only when `ready`.
    fn record(project: &str, hex8: &str, state: &str) -> Record {
        let mut r = lease(project, &format!("agentspace-{project}-{hex8}"));
        r.id = format!("{hex8}-0000-4000-8000-000000000001");
        r.state = state.to_string();
        if state != "ready" {
            r.ssh = None;
        }
        match state {
            "released" => r.released_at = Some("2026-11-20T10:30:00Z".to_string()),
            "pending" => r.expires_at = None,
            "unsatisfiable" => {
                r.expires_at = None;
                r.reasons = Some(vec![agentspace_state::Reason {
                    code: "no_capacity".to_string(),
                    message: "No host can hold the lease.".to_string(),
                }]);
            }
            _ => {}
        }
        r
    }

    /// The same record under another ID with the same first 8 characters, so that two records
    /// share one built alias.
    fn again(mut r: Record, n: u32) -> Record {
        r.id = format!("{}-0000-4000-8000-{n:012x}", &r.id[..8]);
        r
    }

    const LEASE_A: &str = "agentspace-agentspace-3a9c5e21";

    #[test]
    fn no_argument_is_the_box_and_reads_no_lease_file() {
        let dir = Dir::new("noarg");
        std::fs::write(dir.0.join(LEASE_FILE), b"not json").unwrap();
        let (printer, cap) = say(true);
        let hosts = registry(&[host("agentspace", HostState::Ready)]);
        let clock = FakeClock::before_expiry();
        assert_eq!(
            project_of(pick(None, &hosts, &dir.0, &printer, &clock)),
            "swimcoach"
        );
        assert_eq!(cap.text(), "");
        assert_eq!(clock.nows.get(), 0);
    }

    #[test]
    fn a_host_alias_names_its_host_whatever_its_state() {
        let dir = Dir::new("hostalias");
        // A lease state file that is not valid: a host alias never reads it.
        std::fs::write(dir.0.join(LEASE_FILE), b"not json").unwrap();
        let (printer, cap) = say(true);
        let clock = FakeClock::before_expiry();
        for state in [HostState::Creating, HostState::Ready, HostState::Failed] {
            let hosts = registry(&[host("agentspace", state)]);
            assert_eq!(
                project_of(pick(
                    Some("agentspace-host.agentspace"),
                    &hosts,
                    &dir.0,
                    &printer,
                    &clock
                )),
                "agentspace"
            );
        }
        assert_eq!(cap.text(), "");
    }

    #[test]
    fn the_box_has_a_host_alias_too() {
        let dir = Dir::new("boxalias");
        let (printer, _) = say(false);
        let hosts = registry(&[]);
        let clock = FakeClock::before_expiry();
        let chosen = pick(
            Some("agentspace-host.swimcoach"),
            &hosts,
            &dir.0,
            &printer,
            &clock,
        )
        .unwrap();
        assert!(is_box(chosen));
    }

    #[test]
    fn a_host_alias_that_names_nothing_is_refused() {
        let dir = Dir::new("badalias");
        let (printer, _) = say(false);
        let hosts = registry(&[host("agentspace", HostState::Ready)]);
        let clock = FakeClock::before_expiry();
        for alias in [
            "agentspace-host.",
            "agentspace-host.Agentspace",
            "agentspace-host.nobody",
            "agentspace-host.-x",
            "agentspace-host.a.b",
        ] {
            let failure = pick(Some(alias), &hosts, &dir.0, &printer, &clock).unwrap_err();
            assert_eq!(failure.caller, CallerFailure::CouldNotReach, "{alias}");
        }
    }

    #[test]
    fn a_lease_alias_reaches_its_projects_ready_host() {
        let dir = Dir::new("lease");
        write_leases(
            &dir.0,
            vec![
                lease("swimcoach", "agentspace-swimcoach-aaaaaaaa"),
                lease("agentspace", "agentspace-agentspace-bbbbbbbb"),
            ],
        );
        let (printer, _) = say(false);
        let hosts = registry(&[host("agentspace", HostState::Ready)]);
        // Before the fixture's expires_at, so both leases are `ready`.
        let clock = FakeClock::before_expiry();
        assert_eq!(
            project_of(pick(
                Some("agentspace-agentspace-bbbbbbbb"),
                &hosts,
                &dir.0,
                &printer,
                &clock
            )),
            "agentspace"
        );
        assert_eq!(
            project_of(pick(
                Some("agentspace-swimcoach-aaaaaaaa"),
                &hosts,
                &dir.0,
                &printer,
                &clock
            )),
            "swimcoach"
        );
    }

    #[test]
    fn the_lease_alias_is_matched_exactly_and_case_sensitively() {
        let dir = Dir::new("exact");
        write_leases(
            &dir.0,
            vec![lease("agentspace", "agentspace-agentspace-bbbbbbbb")],
        );
        let (printer, _) = say(false);
        let hosts = registry(&[host("agentspace", HostState::Ready)]);
        let clock = FakeClock::before_expiry();
        for alias in [
            "AGENTSPACE-agentspace-bbbbbbbb",
            "agentspace-agentspace-bbbbbbb",
            "agentspace-agentspace-bbbbbbbb ",
        ] {
            assert_eq!(
                project_of(pick(Some(alias), &hosts, &dir.0, &printer, &clock)),
                "swimcoach",
                "{alias}"
            );
        }
    }

    #[test]
    fn a_lease_alias_with_no_record_or_no_file_is_the_box() {
        let dir = Dir::new("norecord");
        let (printer, cap) = say(true);
        let hosts = registry(&[host("agentspace", HostState::Ready)]);
        let clock = FakeClock::before_expiry();
        // No file.
        assert_eq!(
            project_of(pick(
                Some("agentspace-x-cccccccc"),
                &hosts,
                &dir.0,
                &printer,
                &clock
            )),
            "swimcoach"
        );
        // A file without the alias.
        write_leases(
            &dir.0,
            vec![lease("agentspace", "agentspace-agentspace-bbbbbbbb")],
        );
        assert_eq!(
            project_of(pick(
                Some("agentspace-x-cccccccc"),
                &hosts,
                &dir.0,
                &printer,
                &clock
            )),
            "swimcoach"
        );
        assert_eq!(cap.text(), "");
        assert!(clock.pauses().is_empty());
    }

    #[test]
    fn a_project_with_no_ready_host_is_the_box() {
        let dir = Dir::new("notready");
        write_leases(
            &dir.0,
            vec![
                lease("agentspace", "agentspace-agentspace-bbbbbbbb"),
                lease("other", "agentspace-other-dddddddd"),
            ],
        );
        let (printer, _) = say(false);
        // Before the fixture's expires_at, so both leases are `ready`.
        let clock = FakeClock::before_expiry();
        for state in [HostState::Creating, HostState::Failed] {
            let hosts = registry(&[host("agentspace", state)]);
            assert_eq!(
                project_of(pick(
                    Some("agentspace-agentspace-bbbbbbbb"),
                    &hosts,
                    &dir.0,
                    &printer,
                    &clock
                )),
                "swimcoach"
            );
        }
        // A project with no record at all.
        let hosts = registry(&[]);
        assert_eq!(
            project_of(pick(
                Some("agentspace-other-dddddddd"),
                &hosts,
                &dir.0,
                &printer,
                &clock
            )),
            "swimcoach"
        );
    }

    /// An argument that is not of lease alias form, with 7 hex digits.
    const NOT_LEASE_FORM: &str = "agentspace-x-ccccccc";

    #[test]
    fn a_lease_file_that_is_not_valid_is_the_box_and_diagnostic_names_it() {
        let dir = Dir::new("invalid");
        std::fs::write(dir.0.join(LEASE_FILE), b"{\"version\":1}").unwrap();
        let hosts = registry(&[host("agentspace", HostState::Ready)]);
        let clock = FakeClock::before_expiry();
        let (printer, cap) = say(false);
        assert_eq!(
            project_of(pick(Some(NOT_LEASE_FORM), &hosts, &dir.0, &printer, &clock)),
            "swimcoach"
        );
        assert_eq!(cap.text(), "");
        let (printer, cap) = say(true);
        assert_eq!(
            project_of(pick(Some(NOT_LEASE_FORM), &hosts, &dir.0, &printer, &clock)),
            "swimcoach"
        );
        let text = cap.text();
        assert!(
            text.starts_with("agentspace: The lease state file "),
            "{text}"
        );
        assert!(text.contains(LEASE_FILE), "{text}");
        assert_eq!(text.lines().count(), 1);
        // Not valid is not tried again.
        assert!(clock.pauses().is_empty());
    }

    #[test]
    fn a_lease_file_that_is_not_valid_refuses_a_lease_form_argument() {
        let dir = Dir::new("invalidform");
        std::fs::write(dir.0.join(LEASE_FILE), b"{\"version\":1}").unwrap();
        let clock = FakeClock::before_expiry();
        for diagnostic in [false, true] {
            let (printer, cap) = say(diagnostic);
            let failure =
                lease_key(Some("agentspace-x-cccccccc"), &dir.0, &printer, &clock).unwrap_err();
            assert_eq!(
                failure,
                refusal(
                    "agentspace: The lease records could not be read; no connection was opened for agentspace-x-cccccccc. Run 'agentspace lease list' to see the error."
                        .to_string()
                )
            );
            // The line naming the file comes from the lookup, before the refusal is printed.
            let text = cap.text();
            if diagnostic {
                assert!(
                    text.starts_with("agentspace: The lease state file "),
                    "{text}"
                );
                assert_eq!(text.lines().count(), 1);
            } else {
                assert_eq!(text, "");
            }
        }
        assert!(clock.pauses().is_empty());
    }

    #[test]
    fn a_folder_at_the_lease_file_is_the_box() {
        let dir = Dir::new("folder");
        std::fs::create_dir_all(dir.0.join(LEASE_FILE)).unwrap();
        let hosts = registry(&[host("agentspace", HostState::Ready)]);
        let (printer, _) = say(false);
        let clock = FakeClock::before_expiry();
        assert_eq!(
            project_of(pick(Some(NOT_LEASE_FORM), &hosts, &dir.0, &printer, &clock)),
            "swimcoach"
        );
        assert!(clock.pauses().is_empty());
    }

    #[test]
    fn a_folder_at_the_lease_file_refuses_a_lease_form_argument() {
        let dir = Dir::new("folderform");
        std::fs::create_dir_all(dir.0.join(LEASE_FILE)).unwrap();
        let (printer, cap) = say(true);
        let clock = FakeClock::before_expiry();
        let failure =
            lease_key(Some("agentspace-x-cccccccc"), &dir.0, &printer, &clock).unwrap_err();
        assert_eq!(failure, unreadable_refusal("agentspace-x-cccccccc"));
        assert!(
            cap.text()
                .contains("a folder is where the state file belongs")
        );
        assert!(clock.pauses().is_empty());
    }

    #[test]
    fn lease_alias_form_is_the_contracts_pattern() {
        // A project of 32 characters, the longest, and one of 33.
        let longest = format!("agentspace-a{}b-12345678", "-".repeat(30));
        let too_long = format!("agentspace-a{}b-12345678", "-".repeat(31));
        for yes in [
            "agentspace-swimcoach-3a9c5e21",
            "agentspace-x-cccccccc",
            "agentspace-a-b-00000000",
            "agentspace-0-ffffffff",
            longest.as_str(),
        ] {
            assert!(is_lease_alias_form(yes), "{yes}");
        }
        for no in [
            "agentspace",
            "agentspace-swimcoach-3A9C5E21",
            "agentspace-swimcoach-3a9c5e2",
            "agentspace-swimcoach-3a9c5e21x",
            "agentspace--3a9c5e21",
            "agentspace-Swim-3a9c5e21",
            "agentspace--x-3a9c5e21",
            "agentspace-x--3a9c5e21",
            "agentspace-x_y-3a9c5e21",
            "Agentspace-x-3a9c5e21",
            "agentspace-x-3a9c5e2g",
            "agentspace-x 3a9c5e21",
            too_long.as_str(),
            "agentspace-x-3a9c5e21\n",
            "agentspace-é-3a9c5e21",
            "conformance-proxy-alias",
            "",
        ] {
            assert!(!is_lease_alias_form(no), "{no:?}");
        }
    }

    #[test]
    fn each_state_that_is_not_ready_is_refused_with_its_line_and_no_detail() {
        let dir = Dir::new("states");
        write_leases(
            &dir.0,
            vec![
                record("agentspace", "11111111", "released"),
                record("agentspace", "22222222", "unsatisfiable"),
                record("agentspace", "33333333", "pending"),
                record("agentspace", "44444444", "ready"),
            ],
        );
        let cases = [
            (
                "agentspace-agentspace-11111111",
                "has been released; no connection was opened. Create a new lease.",
            ),
            (
                "agentspace-agentspace-22222222",
                "is unsatisfiable; no connection was opened. Create a new lease.",
            ),
            (
                "agentspace-agentspace-33333333",
                "is pending, not yet ready; no connection was opened. Connect once 'agentspace lease get' answers ready.",
            ),
            (
                "agentspace-agentspace-44444444",
                "has expired; no connection was opened. Create a new lease.",
            ),
        ];
        // At the fixture's expires_at, so the stored `ready` lease is `expired`.
        let clock = FakeClock::at(expiry());
        for (alias, rest) in cases {
            let (printer, cap) = say(true);
            let failure = lease_key(Some(alias), &dir.0, &printer, &clock).unwrap_err();
            assert_eq!(
                failure,
                refusal(format!("agentspace: The lease for {alias} {rest}")),
                "{alias}"
            );
            assert_eq!(failure.caller, CallerFailure::Same);
            // A record was read: the diagnostic adds no line.
            assert_eq!(cap.text(), "", "{alias}");
        }
        // The clock is read once for each lookup.
        assert_eq!(clock.nows.get(), 4);
    }

    #[test]
    fn a_stored_ready_lease_expires_at_its_expires_at_to_the_second() {
        let r = record("agentspace", "44444444", "ready");
        assert_eq!(state_at(&r, expiry() - 1), LeaseState::Ready);
        assert_eq!(state_at(&r, expiry()), LeaseState::Expired);
        assert_eq!(state_at(&r, expiry() + 1), LeaseState::Expired);
        let alias = "agentspace-agentspace-44444444";
        assert_eq!(
            decide(std::slice::from_ref(&r), alias, expiry() - 1),
            Ok(Some(HostKey::linux_arm64("agentspace")))
        );
        assert_eq!(
            decide(std::slice::from_ref(&r), alias, expiry()),
            Err(not_ready_refusal(alias, LeaseState::Expired))
        );
        // Found by its `ssh.host_alias` too, and expired the same way.
        let by_ssh = lease("agentspace", "agentspace-agentspace-bbbbbbbb");
        assert_eq!(
            decide(
                std::slice::from_ref(&by_ssh),
                "agentspace-agentspace-bbbbbbbb",
                expiry()
            ),
            Err(not_ready_refusal(
                "agentspace-agentspace-bbbbbbbb",
                LeaseState::Expired
            ))
        );
    }

    #[test]
    fn a_record_is_the_aliass_by_its_built_alias_or_a_ready_records_ssh_alias() {
        let now = expiry() - 1;
        // Built from `project` and `id`, though `ssh.host_alias` says otherwise.
        let mut built = record("agentspace", "55555555", "ready");
        built.ssh.as_mut().unwrap().host_alias = "elsewhere".to_string();
        assert_eq!(
            decide(
                std::slice::from_ref(&built),
                "agentspace-agentspace-55555555",
                now
            ),
            Ok(Some(HostKey::linux_arm64("agentspace")))
        );
        // A stored `ready` record by its `ssh.host_alias`.
        assert_eq!(
            decide(std::slice::from_ref(&built), "elsewhere", now),
            Ok(Some(HostKey::linux_arm64("agentspace")))
        );
        // A record that is not stored `ready` is never found by an `ssh.host_alias`.
        let mut pending = record("agentspace", "66666666", "pending");
        pending.ssh = Some(agentspace_state::Ssh {
            host_alias: "elsewhere".to_string(),
            user: "ubuntu".to_string(),
        });
        assert_eq!(
            decide(std::slice::from_ref(&pending), "elsewhere", now),
            Ok(None)
        );
        // Case-sensitive, and on the first 8 characters of the ID only.
        assert_eq!(
            decide(
                std::slice::from_ref(&built),
                "agentspace-Agentspace-55555555",
                now
            ),
            Ok(None)
        );
        assert_eq!(
            decide(
                std::slice::from_ref(&built),
                "agentspace-agentspace-55555555-0000",
                now
            ),
            Ok(None)
        );
    }

    #[test]
    fn of_two_records_the_first_ready_connects_else_the_last_is_named() {
        let alias = "agentspace-agentspace-77777777";
        let now = expiry() - 1;
        let released = record("agentspace", "77777777", "released");
        let ready_a = again(record("agentspace", "77777777", "ready"), 2);
        // Two `ready` with different projects: the first in the file's order. The built alias
        // holds the project, so the second, of another project, is found by its `ssh.host_alias`,
        // which is the alias.
        let mut ready_b = again(record("agentspace", "77777777", "ready"), 3);
        ready_b.project = "other".to_string();
        assert_eq!(ready_b.ssh.as_ref().unwrap().host_alias, alias);
        assert_eq!(
            decide(
                &[released.clone(), ready_a.clone(), ready_b.clone()],
                alias,
                now
            ),
            Ok(Some(HostKey::linux_arm64("agentspace")))
        );
        assert_eq!(
            decide(&[ready_b.clone(), ready_a.clone()], alias, now),
            Ok(Some(HostKey::linux_arm64("other")))
        );
        // None `ready`: the last one's state names the refusal.
        let pending = again(record("agentspace", "77777777", "pending"), 4);
        assert_eq!(
            decide(&[released.clone(), pending.clone()], alias, now),
            Err(not_ready_refusal(alias, LeaseState::Pending))
        );
        assert_eq!(
            decide(&[pending.clone(), released.clone()], alias, now),
            Err(not_ready_refusal(alias, LeaseState::Released))
        );
        // A stored `ready` past its expires_at is skipped, and named `expired` when it is the last.
        let late = expiry();
        assert_eq!(
            decide(&[ready_a.clone(), released.clone()], alias, late),
            Err(not_ready_refusal(alias, LeaseState::Released))
        );
        assert_eq!(
            decide(&[released.clone(), ready_a.clone()], alias, late),
            Err(not_ready_refusal(alias, LeaseState::Expired))
        );
        // An expired one before a `ready` one: the `ready` one connects.
        let mut fresh = ready_b.clone();
        fresh.project = "agentspace".to_string();
        fresh.expires_at = Some("2026-11-20T12:00:00Z".to_string());
        assert_eq!(
            decide(&[ready_a, fresh], alias, late),
            Ok(Some(HostKey::linux_arm64("agentspace")))
        );
    }

    #[test]
    fn two_records_are_judged_at_one_clock_reading() {
        let dir = Dir::new("onereading");
        let alias = "agentspace-agentspace-88888888";
        write_leases(
            &dir.0,
            vec![
                record("agentspace", "88888888", "released"),
                again(record("agentspace", "88888888", "ready"), 2),
            ],
        );
        let (printer, _) = say(false);
        let clock = FakeClock::before_expiry();
        assert_eq!(
            lease_key(Some(alias), &dir.0, &printer, &clock),
            Ok(Some(HostKey::linux_arm64("agentspace")))
        );
        assert_eq!(clock.nows.get(), 1);
    }

    #[test]
    fn a_ready_lease_found_by_its_built_alias_reaches_its_host() {
        let dir = Dir::new("readyhost");
        // Its `ssh.host_alias` is another name, so only the built alias finds it.
        let mut ready = record("agentspace", "3a9c5e21", "ready");
        ready.ssh.as_mut().unwrap().host_alias = "agentspace-other-00000000".to_string();
        write_leases(&dir.0, vec![ready]);
        let hosts = registry(&[host("agentspace", HostState::Ready)]);
        let (printer, cap) = say(true);
        let clock = FakeClock::before_expiry();
        assert_eq!(
            project_of(pick(Some(LEASE_A), &hosts, &dir.0, &printer, &clock)),
            "agentspace"
        );
        assert_eq!(cap.text(), "");
    }

    /// Holds `path` open sharing nothing, so that every other open is refused on a sharing
    /// violation.
    #[cfg(windows)]
    fn hold(path: &Path) -> std::fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
            .unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn a_read_held_throughout_is_tried_3_times_400_ms_apart_then_refused() {
        let dir = Dir::new("heldall");
        write_leases(&dir.0, vec![record("agentspace", "3a9c5e21", "ready")]);
        let held = hold(&dir.0.join(LEASE_FILE));
        let (printer, cap) = say(true);
        let clock = FakeClock::before_expiry();
        let failure = lease_key(Some(LEASE_A), &dir.0, &printer, &clock).unwrap_err();
        drop(held);
        assert_eq!(failure, unreadable_refusal(LEASE_A));
        assert_eq!(clock.pauses(), vec![RETRY_PAUSE, RETRY_PAUSE]);
        assert_eq!(RETRY_PAUSE, Duration::from_millis(400));
        let text = cap.text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert_eq!(lines[0], RETRY_LINE);
        assert_eq!(lines[1], RETRY_LINE);
        assert_eq!(
            RETRY_LINE,
            "agentspace: The lease records were held by another program; trying again."
        );
        assert!(
            lines[2].starts_with("agentspace: The lease state file "),
            "{text}"
        );
        // The clock's one reading is never taken for a file that was not read.
        assert_eq!(clock.nows.get(), 0);

        // Without the diagnostic, the same refusal and no line.
        let held = hold(&dir.0.join(LEASE_FILE));
        let (printer, cap) = say(false);
        let clock = FakeClock::before_expiry();
        let failure = lease_key(Some(LEASE_A), &dir.0, &printer, &clock).unwrap_err();
        drop(held);
        assert_eq!(failure, unreadable_refusal(LEASE_A));
        assert_eq!(clock.pauses().len(), 2);
        assert_eq!(cap.text(), "");
    }

    #[cfg(windows)]
    #[test]
    fn a_read_held_throughout_sends_an_argument_not_of_lease_form_to_the_box() {
        let dir = Dir::new("heldbox");
        write_leases(&dir.0, vec![record("agentspace", "3a9c5e21", "ready")]);
        let held = hold(&dir.0.join(LEASE_FILE));
        let (printer, _) = say(false);
        let clock = FakeClock::before_expiry();
        let found = lease_key(Some(NOT_LEASE_FORM), &dir.0, &printer, &clock);
        drop(held);
        assert_eq!(found, Ok(None));
        assert_eq!(clock.pauses().len(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn a_read_freed_during_a_retry_reads_the_file_as_a_first_attempt_would() {
        let dir = Dir::new("heldonce");
        write_leases(&dir.0, vec![record("agentspace", "3a9c5e21", "released")]);
        let held = std::cell::RefCell::new(Some(hold(&dir.0.join(LEASE_FILE))));
        let (printer, cap) = say(true);
        let clock = FakeClock::before_expiry();
        // The first pause lets the file go.
        let held = std::rc::Rc::new(held);
        let in_pause = std::rc::Rc::clone(&held);
        *clock.on_pause.borrow_mut() = Box::new(move |_| {
            in_pause.borrow_mut().take();
        });
        let failure = lease_key(Some(LEASE_A), &dir.0, &printer, &clock).unwrap_err();
        assert_eq!(failure, not_ready_refusal(LEASE_A, LeaseState::Released));
        assert_eq!(clock.pauses(), vec![RETRY_PAUSE]);
        assert_eq!(cap.text(), format!("{RETRY_LINE}\n"));
        assert_eq!(clock.nows.get(), 1);
        assert!(held.borrow().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn a_read_freed_onto_a_file_that_is_not_valid_ends_the_retries() {
        let dir = Dir::new("heldinvalid");
        std::fs::write(dir.0.join(LEASE_FILE), b"{\"version\":1}").unwrap();
        let held = std::rc::Rc::new(std::cell::RefCell::new(Some(hold(&dir.0.join(LEASE_FILE)))));
        let (printer, cap) = say(true);
        let clock = FakeClock::before_expiry();
        let in_pause = std::rc::Rc::clone(&held);
        *clock.on_pause.borrow_mut() = Box::new(move |_| {
            in_pause.borrow_mut().take();
        });
        let failure = lease_key(Some(LEASE_A), &dir.0, &printer, &clock).unwrap_err();
        assert_eq!(failure, unreadable_refusal(LEASE_A));
        assert_eq!(clock.pauses().len(), 1);
        let text = cap.text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert_eq!(lines[0], RETRY_LINE);
        assert!(lines[1].starts_with("agentspace: The lease state file "));
    }

    #[cfg(windows)]
    #[test]
    fn no_attempt_starts_more_than_1_second_after_the_first() {
        let dir = Dir::new("heldslow");
        write_leases(&dir.0, vec![record("agentspace", "3a9c5e21", "ready")]);
        let held = hold(&dir.0.join(LEASE_FILE));
        let (printer, cap) = say(true);
        // Each pause runs 300 milliseconds long: the second attempt starts at 700, and a third
        // would start at 1400, so it is never made.
        let mut clock = FakeClock::before_expiry();
        clock.per_pause_extra = Duration::from_millis(300);
        let failure = lease_key(Some(LEASE_A), &dir.0, &printer, &clock).unwrap_err();
        drop(held);
        assert_eq!(failure, unreadable_refusal(LEASE_A));
        assert_eq!(clock.pauses().len(), 1);
        assert_eq!(cap.text().lines().filter(|l| *l == RETRY_LINE).count(), 1);
        // A pause that overruns the second itself ends the retries.
        let held = hold(&dir.0.join(LEASE_FILE));
        let (printer, _) = say(false);
        let mut clock = FakeClock::before_expiry();
        clock.per_pause_extra = Duration::from_millis(700);
        let failure = lease_key(Some(LEASE_A), &dir.0, &printer, &clock).unwrap_err();
        drop(held);
        assert_eq!(failure, unreadable_refusal(LEASE_A));
        assert_eq!(clock.pauses().len(), 1);
    }

    #[test]
    fn a_missing_file_is_not_tried_again_and_is_the_box() {
        let dir = Dir::new("missing");
        let (printer, cap) = say(true);
        let clock = FakeClock::before_expiry();
        assert_eq!(lease_key(Some(LEASE_A), &dir.0, &printer, &clock), Ok(None));
        assert!(clock.pauses().is_empty());
        assert_eq!(cap.text(), "");
    }

    #[test]
    fn the_state_folder_is_the_override_or_the_homes() {
        let home = Path::new("home");
        assert_eq!(state_dir(None, home), home.join(".agentspace"));
        assert_eq!(
            state_dir(Some(std::ffi::OsString::new()), home),
            home.join(".agentspace")
        );
        let abs = std::env::temp_dir().join("state-x");
        assert_eq!(state_dir(Some(abs.clone().into_os_string()), home), abs);
        let relative = state_dir(Some("rel-state".into()), home);
        assert!(relative.is_absolute());
        assert!(relative.ends_with("rel-state"));
    }

    #[test]
    fn a_registry_failure_names_the_file_and_the_reason_on_one_line() {
        let failure = registry_failure(
            Path::new("home"),
            &HostsError::NotValid(agentspace_hosts::Invalid::Schema),
        );
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        assert!(
            failure
                .message
                .starts_with("agentspace: The host registry "),
            "{}",
            failure.message
        );
        assert!(
            failure
                .message
                .ends_with("cannot be read: it does not match its schema."),
            "{}",
            failure.message
        );
        assert!(!failure.message.contains('\n'));
        let busy = registry_failure(Path::new("home"), &HostsError::Busy);
        assert!(
            busy.message
                .ends_with("cannot be read: The host registry is in use by another command.")
        );
    }

    fn cache_for(home: &Path, project: &str) -> HostCache {
        let hosts = agentspace_hosts::read(home).unwrap();
        let record = hosts.hosts.iter().find(|h| h.project == project).unwrap();
        HostCache::new(home, record)
    }

    #[test]
    fn the_cache_reads_the_records_instance_id_and_never_unopenable() {
        let home = Dir::new("cacheread");
        let mut record = host("agentspace", HostState::Ready);
        let cache = HostCache::new(&home.0, &record);
        assert_eq!(cache.read(), CacheRead::Missing);
        record.instance_id = Some(ID_A.to_string());
        let cache = HostCache::new(&home.0, &record);
        assert_eq!(cache.read(), CacheRead::Bytes(cache_bytes(ID_A)));
        assert_eq!(cache.path(), hosts_path(&home.0));
    }

    #[test]
    fn a_write_adds_a_missing_instance_id_to_its_record_and_nothing_else() {
        let home = Dir::new("cachewrite");
        {
            let held = agentspace_hosts::lock(&home.0).unwrap();
            let mut hosts = registry(&[host("agentspace", HostState::Creating)]);
            hosts.hosts[1].last_attempt_at = Some("2026-11-20T19:05:42Z".to_string());
            agentspace_hosts::write(&home.0, &hosts, &held).unwrap();
        }
        let before = agentspace_hosts::read(&home.0).unwrap();
        cache_for(&home.0, "agentspace")
            .write(&cache_bytes(ID_A))
            .unwrap();
        let after = agentspace_hosts::read(&home.0).unwrap();
        assert_eq!(after.hosts[0], before.hosts[0]);
        assert_eq!(after.hosts[1].instance_id.as_deref(), Some(ID_A));
        assert_eq!(after.hosts[1].state, HostState::Creating);
        assert_eq!(
            after.hosts[1].last_attempt_at,
            before.hosts[1].last_attempt_at
        );
        assert_eq!(after.hosts.len(), 2);
    }

    #[test]
    fn a_write_never_replaces_an_instance_id_the_record_holds() {
        let home = Dir::new("cachekeep");
        {
            let held = agentspace_hosts::lock(&home.0).unwrap();
            let mut hosts = registry(&[]);
            hosts.hosts[0].instance_id = Some(ID_A.to_string());
            agentspace_hosts::write(&home.0, &hosts, &held).unwrap();
        }
        // A stale reader that saw no ID writes after another has stored one.
        let stale = HostCache::new(&home.0, &host("swimcoach", HostState::Ready));
        stale.write(&cache_bytes(ID_B)).unwrap();
        let after = agentspace_hosts::read(&home.0).unwrap();
        assert_eq!(after.hosts[0].instance_id.as_deref(), Some(ID_A));
    }

    #[test]
    fn a_write_for_the_box_creates_the_registry_holding_its_record_alone() {
        let home = Dir::new("cachebox");
        let cache = cache_for(&home.0, "swimcoach");
        cache.write(&cache_bytes(ID_A)).unwrap();
        let after = agentspace_hosts::read(&home.0).unwrap();
        assert_eq!(after.hosts.len(), 1);
        assert_eq!(after.hosts[0].project, "swimcoach");
        assert_eq!(after.hosts[0].instance_id.as_deref(), Some(ID_A));
        assert!(hosts_path(&home.0).is_file());
    }

    #[test]
    fn a_write_never_adds_a_record() {
        let home = Dir::new("cachenoadd");
        let cache = HostCache::new(&home.0, &host("agentspace", HostState::Ready));
        cache.write(&cache_bytes(ID_A)).unwrap();
        assert_eq!(agentspace_hosts::read(&home.0).unwrap(), Hosts::seed());
    }

    #[test]
    fn a_write_that_fails_does_not_fail_the_connection() {
        let home = Dir::new("cachefail");
        // A folder where the registry's lock belongs: the lock cannot be taken.
        std::fs::create_dir_all(agentspace_hosts::hosts_lock_path(&home.0)).unwrap();
        let cache = HostCache::new(&home.0, &host("swimcoach", HostState::Ready));
        assert_eq!(cache.write(&cache_bytes(ID_A)), Ok(()));
        // Bytes that are not an instance ID write nothing and also succeed.
        assert_eq!(cache.write(b"nonsense"), Ok(()));
        assert!(!hosts_path(&home.0).exists());
    }

    // A host by its project and platform (D91; operator.md, From the ProxyCommand; pins P8, P12 and
    // P13 of v0.12-T01). Every registry below holds the project's host on the other platform first,
    // so a lookup by the project alone would find the wrong record.

    fn host_on(project: &str, platform: &str, state: HostState) -> Host {
        Host::new(&HostKey::new(project, platform), state)
    }

    /// A `ready` lease of `project` on `platform` whose built alias is
    /// `agentspace-<project>-<hex8>`.
    fn lease_on(project: &str, platform: &str, hex8: &str) -> Record {
        let mut r = record(project, hex8, "ready");
        r.platform = platform.to_string();
        r
    }

    fn platform_of_pick(result: Result<&Host, Failure>) -> (String, String) {
        let host = result.unwrap();
        (host.project.clone(), host.platform().to_string())
    }

    const WINDOWS_LEASE: &str = "agentspace-agentspace-0e1e2e3e";
    const LINUX_LEASE: &str = "agentspace-agentspace-0a1a2a3a";

    #[test]
    fn a_host_alias_names_the_host_of_its_project_on_its_platform() {
        let dir = Dir::new("hostaliasplatform");
        let (printer, cap) = say(true);
        let clock = FakeClock::before_expiry();
        let hosts = registry(&[
            host_on("agentspace", "windows/amd64", HostState::Ready),
            host_on("agentspace", "linux/amd64", HostState::Creating),
            host("agentspace", HostState::Ready),
        ]);
        for (alias, platform) in [
            ("agentspace-host.agentspace", "linux/arm64"),
            ("agentspace-host.agentspace.windows-amd64", "windows/amd64"),
            ("agentspace-host.agentspace.linux-amd64", "linux/amd64"),
        ] {
            assert_eq!(
                platform_of_pick(pick(Some(alias), &hosts, &dir.0, &printer, &clock)),
                ("agentspace".to_string(), platform.to_string()),
                "{alias}"
            );
        }
        assert_eq!(cap.text(), "");
        // `linux/arm64` never appears in a name; another name part, or one with no record, fails.
        let only_linux = registry(&[host("agentspace", HostState::Ready)]);
        for alias in [
            "agentspace-host.agentspace.linux-arm64",
            "agentspace-host.agentspace.Windows-amd64",
            "agentspace-host.agentspace.windows-arm64",
            "agentspace-host.agentspace.windows-amd64",
            "agentspace-host.swimcoach.linux-amd64",
        ] {
            let failure = pick(Some(alias), &only_linux, &dir.0, &printer, &clock).unwrap_err();
            assert_eq!(failure.caller, CallerFailure::CouldNotReach, "{alias}");
        }
    }

    #[test]
    fn a_lease_reaches_the_ready_host_of_its_own_platform() {
        let dir = Dir::new("leaseplatform");
        write_leases(
            &dir.0,
            vec![
                lease_on("agentspace", "windows/amd64", "0e1e2e3e"),
                lease_on("agentspace", "linux/arm64", "0a1a2a3a"),
            ],
        );
        let (printer, cap) = say(true);
        let clock = FakeClock::before_expiry();
        let hosts = registry(&[
            host_on("agentspace", "windows/amd64", HostState::Ready),
            host("agentspace", HostState::Ready),
        ]);
        assert_eq!(
            platform_of_pick(pick(Some(WINDOWS_LEASE), &hosts, &dir.0, &printer, &clock)),
            ("agentspace".to_string(), "windows/amd64".to_string())
        );
        assert_eq!(
            platform_of_pick(pick(Some(LINUX_LEASE), &hosts, &dir.0, &printer, &clock)),
            ("agentspace".to_string(), "linux/arm64".to_string())
        );
        assert_eq!(cap.text(), "");
    }

    #[test]
    fn a_ready_lease_off_linux_arm64_with_no_ready_host_there_is_refused() {
        let dir = Dir::new("noreadyplatform");
        write_leases(
            &dir.0,
            vec![lease_on("agentspace", "windows/amd64", "0e1e2e3e")],
        );
        let clock = FakeClock::before_expiry();
        let line = format!(
            "agentspace: The lease for {WINDOWS_LEASE} has no ready host on its platform; no connection was opened."
        );
        for hosts in [
            // The project's `ready` host is on `linux/arm64` alone.
            registry(&[host("agentspace", HostState::Ready)]),
            // Its host on the lease's platform is not `ready`.
            registry(&[
                host("agentspace", HostState::Ready),
                host_on("agentspace", "windows/amd64", HostState::Creating),
            ]),
            registry(&[host_on("agentspace", "windows/amd64", HostState::Failed)]),
        ] {
            for diagnostic in [false, true] {
                let (printer, cap) = say(diagnostic);
                let failure =
                    pick(Some(WINDOWS_LEASE), &hosts, &dir.0, &printer, &clock).unwrap_err();
                assert_eq!(failure.message, line);
                // The caller's line is the refusal itself, and the diagnostic adds nothing.
                assert_eq!(failure.caller, CallerFailure::Same);
                assert_eq!(cap.text(), "");
            }
        }
        // The fallback to the box is `linux/arm64`'s alone: such a lease of `linux/arm64` with no
        // `ready` host there reaches the box, though a `ready` host of its project's is on another
        // platform.
        write_leases(
            &dir.0,
            vec![lease_on("agentspace", "linux/arm64", "0a1a2a3a")],
        );
        let (printer, _) = say(false);
        let hosts = registry(&[host_on("agentspace", "windows/amd64", HostState::Ready)]);
        let chosen = pick(Some(LINUX_LEASE), &hosts, &dir.0, &printer, &clock).unwrap();
        assert!(is_box(chosen));
    }

    #[test]
    fn decide_gives_the_records_project_and_platform() {
        let r = lease_on("agentspace", "windows/amd64", "0e1e2e3e");
        assert_eq!(
            decide(std::slice::from_ref(&r), WINDOWS_LEASE, expiry() - 1),
            Ok(Some(HostKey::new("agentspace", "windows/amd64")))
        );
    }

    #[test]
    fn a_write_finds_its_record_by_project_and_platform() {
        let home = Dir::new("cacheplatform");
        {
            let held = agentspace_hosts::lock(&home.0).unwrap();
            let hosts = registry(&[
                host_on("agentspace", "windows/amd64", HostState::Ready),
                host("agentspace", HostState::Ready),
            ]);
            agentspace_hosts::write(&home.0, &hosts, &held).unwrap();
        }
        let linux = HostCache::new(&home.0, &host("agentspace", HostState::Ready));
        linux.write(&cache_bytes(ID_A)).unwrap();
        let windows = HostCache::new(
            &home.0,
            &host_on("agentspace", "windows/amd64", HostState::Ready),
        );
        windows.write(&cache_bytes(ID_B)).unwrap();
        let after = agentspace_hosts::read(&home.0).unwrap();
        assert_eq!(after.hosts[1].platform(), "windows/amd64");
        assert_eq!(after.hosts[1].instance_id.as_deref(), Some(ID_B));
        assert_eq!(after.hosts[2].platform(), "linux/arm64");
        assert_eq!(after.hosts[2].instance_id.as_deref(), Some(ID_A));
    }
}
