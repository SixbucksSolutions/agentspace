//! Which host a connection reaches, and the host's instance ID as a member of its record
//! (docs/operator.md, From the ProxyCommand and The host registry, as the operator uses it; D45, D49,
//! D55).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The host comes from the ProxyCommand's one argument, in this order:
//! 1. **No argument: the box.**
//! 2. **`agentspace-host.<project>`: that project's host**, whatever its state. A rest that is not a
//!    normalized project name, and a project with no record, fail the connection.
//! 3. **Any other argument, a lease alias:** the lease state file is read without its lock, for the
//!    record whose `ssh.host_alias` is the argument, then that record's project in the registry. A
//!    `ready` host that is not the box is that host. Every other outcome is the box: no record, no
//!    file, a file that cannot be read or is not valid, a project with no record or one that is
//!    not `ready`.
//!
//! The ProxyCommand never writes the lease state file and never takes its lock.

use std::path::{Path, PathBuf};

use agentspace_box::{CacheRead, InstanceIdCache, cache_bytes, parse_cache};
use agentspace_hosts::{
    Host, Hosts, HostsError, Target, hosts_path, is_box, is_normalized_project, target_for,
};
use agentspace_operator_api::{CallerFailure, Failure, Say};
use agentspace_state::StoreError;

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

/// The project of the lease whose `ssh.host_alias` is `alias`, exactly and case-sensitively, or
/// `None`. A file that is missing holds no lease. A file that cannot be read or is not valid reads
/// no lease from it, and `say` gets a line that names the file and the problem, which only
/// `AGENTSPACE_DIAGNOSTIC` shows (operator.md, From the ProxyCommand, step 3).
fn lease_project(state_dir: &Path, alias: &str, say: &dyn Say) -> Option<String> {
    match agentspace_state::read(state_dir) {
        Ok(file) => file
            .leases
            .iter()
            .find(|r| r.ssh.as_ref().is_some_and(|s| s.host_alias == alias))
            .map(|r| r.project.clone()),
        Err(error) => {
            let why = match error {
                StoreError::Busy => "it is in use".to_string(),
                StoreError::CannotLock(why)
                | StoreError::Unreadable(why)
                | StoreError::NotValid(why)
                | StoreError::Unwritable(why) => why,
            };
            say.detail(&format!(
                "agentspace: The lease state file {} cannot be used: {why}.",
                state_dir.join(LEASE_FILE).display()
            ));
            None
        }
    }
}

/// The host the connection reaches (operator.md, From the ProxyCommand), from the registry the
/// caller has already read and validated. `alias` is the ProxyCommand's argument, `None` for no
/// argument. The lease state file is read only for a lease alias.
pub fn choose<'a>(
    alias: Option<&str>,
    hosts: &'a Hosts,
    state_dir: &Path,
    say: &dyn Say,
) -> Result<&'a Host, Failure> {
    let Some(alias) = alias else {
        return the_box(hosts);
    };
    if let Some(project) = alias.strip_prefix(HOST_ALIAS_PREFIX) {
        let record = is_normalized_project(project)
            .then(|| hosts.hosts.iter().find(|h| h.project == project))
            .flatten();
        return record.ok_or_else(|| {
            could_not_reach("agentspace: The host alias names no host in the registry.".to_string())
        });
    }
    let Some(project) = lease_project(state_dir, alias, say) else {
        return the_box(hosts);
    };
    match target_for(hosts, &project) {
        Target::Host(project) => hosts
            .hosts
            .iter()
            .find(|h| h.project == project)
            .ok_or_else(|| {
                could_not_reach("agentspace: The host registry changed while it was read.".into())
            }),
        Target::Box => the_box(hosts),
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
#[derive(Debug, Clone)]
pub struct HostCache {
    home: PathBuf,
    path: PathBuf,
    project: String,
    instance_id: Option<String>,
}

impl HostCache {
    /// The cache of `host`'s record, as the caller read it from the registry under `home`.
    pub fn new(home: &Path, host: &Host) -> HostCache {
        HostCache {
            home: home.to_path_buf(),
            path: hosts_path(home),
            project: host.project.clone(),
            instance_id: host.instance_id.clone(),
        }
    }

    fn record(&self, id: &str) -> Result<(), HostsError> {
        let held = agentspace_hosts::lock(&self.home)?;
        let mut hosts = agentspace_hosts::read(&self.home)?;
        let Some(record) = hosts.hosts.iter_mut().find(|h| h.project == self.project) else {
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

    #[test]
    fn no_argument_is_the_box_and_reads_no_lease_file() {
        let dir = Dir::new("noarg");
        std::fs::write(dir.0.join(LEASE_FILE), b"not json").unwrap();
        let (printer, cap) = say(true);
        let hosts = registry(&[host("agentspace", HostState::Ready)]);
        assert_eq!(
            project_of(choose(None, &hosts, &dir.0, &printer)),
            "swimcoach"
        );
        assert_eq!(cap.text(), "");
    }

    #[test]
    fn a_host_alias_names_its_host_whatever_its_state() {
        let dir = Dir::new("hostalias");
        let (printer, _) = say(false);
        for state in [HostState::Creating, HostState::Ready, HostState::Failed] {
            let hosts = registry(&[host("agentspace", state)]);
            assert_eq!(
                project_of(choose(
                    Some("agentspace-host.agentspace"),
                    &hosts,
                    &dir.0,
                    &printer
                )),
                "agentspace"
            );
        }
    }

    #[test]
    fn the_box_has_a_host_alias_too() {
        let dir = Dir::new("boxalias");
        let (printer, _) = say(false);
        let hosts = registry(&[]);
        let chosen = choose(Some("agentspace-host.swimcoach"), &hosts, &dir.0, &printer).unwrap();
        assert!(is_box(chosen));
    }

    #[test]
    fn a_host_alias_that_names_nothing_is_refused() {
        let dir = Dir::new("badalias");
        let (printer, _) = say(false);
        let hosts = registry(&[host("agentspace", HostState::Ready)]);
        for alias in [
            "agentspace-host.",
            "agentspace-host.Agentspace",
            "agentspace-host.nobody",
            "agentspace-host.-x",
            "agentspace-host.a.b",
        ] {
            let failure = choose(Some(alias), &hosts, &dir.0, &printer).unwrap_err();
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
        assert_eq!(
            project_of(choose(
                Some("agentspace-agentspace-bbbbbbbb"),
                &hosts,
                &dir.0,
                &printer
            )),
            "agentspace"
        );
        assert_eq!(
            project_of(choose(
                Some("agentspace-swimcoach-aaaaaaaa"),
                &hosts,
                &dir.0,
                &printer
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
        for alias in [
            "AGENTSPACE-agentspace-bbbbbbbb",
            "agentspace-agentspace-bbbbbbb",
            "agentspace-agentspace-bbbbbbbb ",
        ] {
            assert_eq!(
                project_of(choose(Some(alias), &hosts, &dir.0, &printer)),
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
        // No file.
        assert_eq!(
            project_of(choose(
                Some("agentspace-x-cccccccc"),
                &hosts,
                &dir.0,
                &printer
            )),
            "swimcoach"
        );
        // A file without the alias.
        write_leases(
            &dir.0,
            vec![lease("agentspace", "agentspace-agentspace-bbbbbbbb")],
        );
        assert_eq!(
            project_of(choose(
                Some("agentspace-x-cccccccc"),
                &hosts,
                &dir.0,
                &printer
            )),
            "swimcoach"
        );
        assert_eq!(cap.text(), "");
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
        for state in [HostState::Creating, HostState::Failed] {
            let hosts = registry(&[host("agentspace", state)]);
            assert_eq!(
                project_of(choose(
                    Some("agentspace-agentspace-bbbbbbbb"),
                    &hosts,
                    &dir.0,
                    &printer
                )),
                "swimcoach"
            );
        }
        // A project with no record at all.
        let hosts = registry(&[]);
        assert_eq!(
            project_of(choose(
                Some("agentspace-other-dddddddd"),
                &hosts,
                &dir.0,
                &printer
            )),
            "swimcoach"
        );
    }

    #[test]
    fn a_lease_file_that_is_not_valid_is_the_box_and_diagnostic_names_it() {
        let dir = Dir::new("invalid");
        std::fs::write(dir.0.join(LEASE_FILE), b"{\"version\":1}").unwrap();
        let hosts = registry(&[host("agentspace", HostState::Ready)]);
        let (printer, cap) = say(false);
        assert_eq!(
            project_of(choose(
                Some("agentspace-x-cccccccc"),
                &hosts,
                &dir.0,
                &printer
            )),
            "swimcoach"
        );
        assert_eq!(cap.text(), "");
        let (printer, cap) = say(true);
        assert_eq!(
            project_of(choose(
                Some("agentspace-x-cccccccc"),
                &hosts,
                &dir.0,
                &printer
            )),
            "swimcoach"
        );
        let text = cap.text();
        assert!(
            text.starts_with("agentspace: The lease state file "),
            "{text}"
        );
        assert!(text.contains(LEASE_FILE), "{text}");
        assert_eq!(text.lines().count(), 1);
    }

    #[test]
    fn a_folder_at_the_lease_file_is_the_box() {
        let dir = Dir::new("folder");
        std::fs::create_dir_all(dir.0.join(LEASE_FILE)).unwrap();
        let hosts = registry(&[host("agentspace", HostState::Ready)]);
        let (printer, _) = say(false);
        assert_eq!(
            project_of(choose(
                Some("agentspace-x-cccccccc"),
                &hosts,
                &dir.0,
                &printer
            )),
            "swimcoach"
        );
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
}
