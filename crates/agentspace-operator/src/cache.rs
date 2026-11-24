//! A host's instance ID as the host registry holds it: the `instance_id` member of its record in
//! `%USERPROFILE%\.agentspace\hosts.json` (operator.md, The host registry, as the operator uses it;
//! docs/state.md, The host registry; D45).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The decisions about the ID's bytes are `agentspace-box`'s. This module reads the registry, and
//! writes a record's `instance_id` and nothing else of it, under `hosts.lock`.

use std::path::{Path, PathBuf};

use agentspace_box::{CacheRead, InstanceIdCache, cache_bytes, parse_cache};
use agentspace_hosts::{Hosts, HostsError, hosts_lock_path, hosts_path};
use agentspace_operator_api::{CallerFailure, Failure};

fn could_not_reach(message: String) -> Failure {
    Failure {
        message,
        caller: CallerFailure::CouldNotReach,
    }
}

/// The failure of a registry read, lock or write, in operator.md's words:
/// - a file that cannot be read or is not valid: `agentspace: The host registry <path> cannot be
///   read: <reason>.`;
/// - a lock not taken within its 30 seconds: `agentspace: The host registry's lock <path> is held
///   by another program; try again.`;
/// - a file that cannot be written: `agentspace: Cannot write <path>.`
// Architect's to confirm: a lock that cannot be opened for another reason, and a project list
// error that reaches this function, are given `Cannot write <path>.` of the lock's or the file's
// path, since operator.md gives no text for either.
pub(crate) fn registry_failure(home: &Path, error: HostsError) -> Failure {
    let registry = hosts_path(home);
    let lock = hosts_lock_path(home);
    could_not_reach(match error {
        HostsError::Unreadable(why) => format!(
            "agentspace: The host registry {} cannot be read: {why}.",
            registry.display()
        ),
        HostsError::NotValid(why) => format!(
            "agentspace: The host registry {} cannot be read: {why}.",
            registry.display()
        ),
        HostsError::Busy => format!(
            "agentspace: The host registry's lock {} is held by another program; try again.",
            lock.display()
        ),
        HostsError::CannotLock(_) => format!("agentspace: Cannot write {}.", lock.display()),
        HostsError::Unwritable(_)
        | HostsError::BadProject
        | HostsError::CannotLockRun(_)
        | HostsError::CannotLockGuard(_) => {
            format!("agentspace: Cannot write {}.", registry.display())
        }
    })
}

/// Reads the registry, without its lock. A missing file reads as the box's record alone; a file
/// that cannot be read or is not valid fails the command, and is never repaired.
pub(crate) fn read_registry(home: &Path) -> Result<Hosts, Failure> {
    agentspace_hosts::read(home).map_err(|e| registry_failure(home, e))
}

/// Changes one record's `instance_id`, and nothing else in the registry, under `hosts.lock`: the
/// lock is held from before the read until after the file is replaced, and is never held across an
/// AWS call. `None` removes the member. A project with no record is left alone: the operator never
/// adds a record outside `provision`.
pub(crate) fn set_instance_id(
    home: &Path,
    project: &str,
    instance_id: Option<&str>,
) -> Result<(), Failure> {
    let held = agentspace_hosts::lock(home).map_err(|e| registry_failure(home, e))?;
    let mut hosts = read_registry(home)?;
    let Some(record) = hosts.hosts.iter_mut().find(|h| h.project == project) else {
        return Ok(());
    };
    record.instance_id = instance_id.map(str::to_string);
    agentspace_hosts::write(home, &hosts, &held).map_err(|e| registry_failure(home, e))
}

/// One host's instance ID, as `agentspace-box` reads and writes its cache, backed by the host
/// registry. **It never reports an unopenable cache** (`agentspace_box::InstanceIdCache`'s rule for
/// a registry-backed cache): a registry that is not valid fails the command before any cache is
/// read, with the registry's own message.
#[derive(Debug, Clone)]
pub struct HostCache {
    home: PathBuf,
    project: String,
    path: PathBuf,
}

impl HostCache {
    /// The cache of `project`'s record, under `home`.
    pub fn new(home: &Path, project: &str) -> Self {
        Self {
            home: home.to_path_buf(),
            project: project.to_string(),
            path: hosts_path(home),
        }
    }
}

impl InstanceIdCache for HostCache {
    fn path(&self) -> &Path {
        &self.path
    }

    fn read(&self) -> CacheRead {
        // Architect's to confirm: a registry that cannot be read or is not valid reads as `Missing`
        // here, so the caller's own read of the registry (`read_registry`) fails the command with
        // the spec's message rather than this cache failing with its own.
        let id = agentspace_hosts::read(&self.home).ok().and_then(|hosts| {
            hosts
                .hosts
                .into_iter()
                .find(|h| h.project == self.project)
                .and_then(|h| h.instance_id)
        });
        match id {
            Some(id) => CacheRead::Bytes(cache_bytes(&id)),
            None => CacheRead::Missing,
        }
    }

    /// Writes the ID into the record, changing nothing else in it. A registry that cannot be
    /// written fails with its message (`agentspace: Cannot write <path>.`).
    fn write(&self, bytes: &[u8]) -> Result<(), Failure> {
        let Some(id) = parse_cache(bytes) else {
            return Err(could_not_reach(format!(
                "agentspace: Cannot write {}.",
                self.path.display()
            )));
        };
        set_instance_id(&self.home, &self.project, Some(&id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempHome;
    use agentspace_hosts::{Host, HostState};

    const ID: &str = "i-1234567890abcdef0";

    fn write_registry(home: &Path, hosts: &Hosts) {
        let held = agentspace_hosts::lock(home).unwrap();
        agentspace_hosts::write(home, hosts, &held).unwrap();
    }

    fn two() -> Hosts {
        let mut hosts = Hosts::seed();
        hosts.hosts.push(Host {
            project: "agentspace".to_string(),
            stack: "agentspace-host-agentspace".to_string(),
            state: HostState::Ready,
            instance_id: None,
            instance_type: None,
            last_attempt_at: Some("2026-11-20T19:05:42Z".to_string()),
        });
        hosts
    }

    #[test]
    fn the_cache_is_the_registry_and_a_missing_registry_has_no_id() {
        let home = TempHome::new("cache-path");
        let cache = HostCache::new(&home.ctx().home, "swimcoach");
        assert_eq!(cache.path(), hosts_path(&home.ctx().home));
        assert_eq!(cache.read(), CacheRead::Missing);
    }

    #[test]
    fn a_write_changes_that_records_instance_id_alone_and_makes_the_registry() {
        let home = TempHome::new("cache-write");
        let h = home.ctx().home;
        write_registry(&h, &two());
        let cache = HostCache::new(&h, "agentspace");
        cache.write(&cache_bytes(ID)).unwrap();
        let mut expected = two();
        expected.hosts[1].instance_id = Some(ID.to_string());
        assert_eq!(agentspace_hosts::read(&h).unwrap(), expected);
        assert_eq!(cache.read(), CacheRead::Bytes(cache_bytes(ID)));
        assert_eq!(
            HostCache::new(&h, "swimcoach").read(),
            CacheRead::Missing,
            "The box's record is untouched"
        );
    }

    #[test]
    fn the_first_change_to_a_missing_registry_writes_the_boxs_record_with_it() {
        let home = TempHome::new("cache-first");
        let h = home.ctx().home;
        HostCache::new(&h, "swimcoach")
            .write(&cache_bytes(ID))
            .unwrap();
        let hosts = agentspace_hosts::read(&h).unwrap();
        assert_eq!(hosts.hosts.len(), 1);
        assert_eq!(hosts.hosts[0].instance_id.as_deref(), Some(ID));
    }

    #[test]
    fn a_registry_that_is_not_valid_fails_with_its_name_and_a_reason() {
        let home = TempHome::new("cache-invalid");
        let h = home.ctx().home;
        std::fs::create_dir_all(h.join(".agentspace")).unwrap();
        std::fs::write(hosts_path(&h), b"{\"version\":0,\"hosts\":[]}").unwrap();
        let failure = read_registry(&h).unwrap_err();
        assert_eq!(
            failure.message,
            format!(
                "agentspace: The host registry {} cannot be read: it does not hold exactly one box record, for swimcoach.",
                hosts_path(&h).display()
            )
        );
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        // A write refuses it too, and writes nothing.
        let failure = HostCache::new(&h, "swimcoach")
            .write(&cache_bytes(ID))
            .unwrap_err();
        assert!(failure.message.contains("cannot be read"), "{failure:?}");
    }

    #[test]
    fn a_held_lock_is_reported_with_its_path() {
        let home = TempHome::new("cache-busy");
        let h = home.ctx().home;
        let failure = registry_failure(&h, HostsError::Busy);
        assert_eq!(
            failure.message,
            format!(
                "agentspace: The host registry's lock {} is held by another program; try again.",
                hosts_lock_path(&h).display()
            )
        );
    }

    #[test]
    fn a_write_of_bytes_that_are_no_id_is_refused() {
        let home = TempHome::new("cache-bad-bytes");
        let h = home.ctx().home;
        let cache = HostCache::new(&h, "swimcoach");
        assert!(cache.write(b"{}").is_err());
        assert!(!hosts_path(&h).exists());
    }
}
