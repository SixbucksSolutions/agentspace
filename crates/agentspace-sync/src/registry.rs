//! The host registry, as `sync` and `bootstrap` use it (docs/operator.md, The host registry, as the
//! operator uses it; docs/state.md, The host registry, The instance ID; D45, D49, D55).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **A command looks its host up once, as it starts**, and a registry that exists and cannot be
//!   read or is not valid fails the command with `agentspace: The host registry <path> cannot be
//!   read: <reason>.`, before any program starts or any AWS call is made. It is never read as
//!   missing and never repaired.
//! - **A project with no record fails with `agentspace: Project '<project>' has no host; provision
//!   -Host <project> makes one.`**
//! - **Every change is made under `hosts.lock`**, held from before the read the change starts from
//!   until after the file is replaced, and never across an AWS call or an ssh. A lock not taken
//!   within its 30 seconds fails with `agentspace: The host registry's lock <path> is held by
//!   another program; try again.`, and a file that cannot be written with `agentspace: Cannot write
//!   <path>.`
//! - **`sync` writes one member only: a record's `instance_id`**, when it read none from the record
//!   and took it from the stack, changing nothing else in the record.

use std::path::{Path, PathBuf};

use agentspace_box::{CacheRead, InstanceIdCache, cache_bytes, parse_cache};
use agentspace_hosts::{
    BOX_STACK, Host, HostKey, Hosts, HostsError, Target, host_for, host_for_keyed, hosts_lock_path,
    hosts_path, is_box,
};
use agentspace_operator_api::{CallerFailure, Failure};

use crate::files::cannot_write;

fn failure(message: String) -> Failure {
    Failure {
        message,
        caller: CallerFailure::CouldNotReach,
    }
}

/// The one sentence fragment that names what is wrong, for a registry that cannot be read.
fn reason(error: &HostsError) -> String {
    match error {
        HostsError::Unreadable(why) | HostsError::CannotLock(why) | HostsError::Unwritable(why) => {
            why.to_string()
        }
        HostsError::NotValid(why) => why.to_string(),
        HostsError::Busy => "the registry is in use by another command".to_string(),
        HostsError::BadProject => "a project is not in its normalized form".to_string(),
        HostsError::CannotLockRun(why) | HostsError::CannotLockGuard(why) => why.to_string(),
    }
}

/// The failure of a registry that cannot be read or is not valid.
pub(crate) fn cannot_read(home: &Path, error: &HostsError) -> Failure {
    failure(format!(
        "agentspace: The host registry {} cannot be read: {}.",
        hosts_path(home).display(),
        reason(error).replace(['\r', '\n'], " ")
    ))
}

/// Reads the registry without a lock. A missing file reads as the box's record alone.
pub(crate) fn read(home: &Path) -> Result<Hosts, Failure> {
    agentspace_hosts::read(home).map_err(|e| cannot_read(home, &e))
}

/// The failure of a write or a lock.
fn change_failure(home: &Path, error: &HostsError) -> Failure {
    match error {
        HostsError::Busy => failure(format!(
            "agentspace: The host registry's lock {} is held by another program; try again.",
            hosts_lock_path(home).display()
        )),
        HostsError::CannotLock(_) => cannot_write(&hosts_lock_path(home)),
        HostsError::Unreadable(_) | HostsError::NotValid(_) => cannot_read(home, error),
        _ => cannot_write(&hosts_path(home)),
    }
}

/// Takes `hosts.lock`, waiting up to its 30 seconds.
pub(crate) fn lock(home: &Path) -> Result<agentspace_hosts::HostsLock, Failure> {
    agentspace_hosts::lock(home).map_err(|e| change_failure(home, &e))
}

/// The "has no host" failure for the host `key` (operator.md, From the command line): for a
/// `linux/arm64` host `agentspace: Project '<project>' has no host; provision -Host <project> makes
/// one.`, as before v0.12, and for a host of another platform the host named by its host name and
/// its command given `-Platform`: `agentspace: Project '<host name>' has no host; provision -Host
/// <project> -Platform <platform> makes one.` (Messages; pins P12 and P44 of v0.12-T01).
pub(crate) fn no_host_keyed(key: &HostKey) -> Failure {
    failure(format!(
        "agentspace: Project '{}' has no host; provision -Host {} makes one.",
        key.host_name(),
        host_flags(key)
    ))
}

/// What follows `-Host ` in a command line a message suggests for the host `key`: its project, and
/// for a host not on `linux/arm64` ` -Platform <platform>` after it (operator.md, Messages).
pub(crate) fn host_flags(key: &HostKey) -> String {
    if key.is_linux_arm64() {
        key.project.clone()
    } else {
        format!("{} -Platform {}", key.project, key.platform)
    }
}

/// The record `target` names. The box is the record whose stack is `agentbox`, and `-Host` for the
/// box's own project is the box (operator.md, From the command line). A project with no record
/// fails with the "has no host" message.
pub(crate) fn find(hosts: &Hosts, target: Target<'_>) -> Result<Host, Failure> {
    match target {
        Target::Box => hosts
            .hosts
            .iter()
            .find(|h| h.stack == BOX_STACK)
            .cloned()
            .ok_or_else(|| failure("agentspace: The host registry holds no box record.".into())),
        Target::Host(project) => host_for(hosts, project)
            .cloned()
            .ok_or_else(|| no_host_keyed(&HostKey::linux_arm64(project))),
    }
}

/// The record of the host `key` (D91): a `linux/arm64` key's is [`find`]'s for its target, so the
/// box's key finds the box's record; any other key's is the record whose project and platform are
/// the key's, and none is the "has no host" message for that host. A record of the project on
/// another platform never stands in.
pub(crate) fn find_keyed(hosts: &Hosts, key: &HostKey) -> Result<Host, Failure> {
    match crate::linux_arm64_target(key) {
        Some(target) => find(hosts, target),
        None => host_for_keyed(hosts, key)
            .cloned()
            .ok_or_else(|| no_host_keyed(key)),
    }
}

/// Sets `project`'s `linux/arm64` record's `instance_id` to `id`, or removes it for `None`:
/// [`set_instance_id_keyed`] of its key.
#[cfg(test)]
pub(crate) fn set_instance_id(home: &Path, project: &str, id: Option<&str>) -> Result<(), Failure> {
    set_instance_id_keyed(home, &HostKey::linux_arm64(project), id)
}

/// Sets the host `key`'s record's `instance_id` to `id`, or removes it for `None`, under
/// `hosts.lock`, and changes nothing else in it. A file that is missing starts from the box's
/// record, which a first change writes with the change.
pub(crate) fn set_instance_id_keyed(
    home: &Path,
    key: &HostKey,
    id: Option<&str>,
) -> Result<(), Failure> {
    let held = lock(home)?;
    let mut hosts = read(home)?;
    let Some(record) = hosts.hosts.iter_mut().find(|h| h.is_for(key)) else {
        return Err(no_host_keyed(key));
    };
    let wanted = id.map(str::to_string);
    if record.instance_id == wanted && hosts_path(home).is_file() {
        return Ok(());
    }
    record.instance_id = wanted;
    agentspace_hosts::write(home, &hosts, &held).map_err(|e| change_failure(home, &e))
}

/// Removes `project`'s `linux/arm64` record's `instance_type`, or sets it:
/// [`set_instance_type_keyed`] of its key.
#[cfg(test)]
pub(crate) fn set_instance_type(
    home: &Path,
    project: &str,
    instance_type: Option<&str>,
) -> Result<(), Failure> {
    set_instance_type_keyed(home, &HostKey::linux_arm64(project), instance_type)
}

/// Removes the host `key`'s record's `instance_type`, or sets it, under `hosts.lock`, changing
/// nothing else in the record (docs/state.md, The instance type; pins P11, P26 and P30). A missing
/// file starts from the seed document and is written, never from the implied record (pin P31).
pub(crate) fn set_instance_type_keyed(
    home: &Path,
    key: &HostKey,
    instance_type: Option<&str>,
) -> Result<(), Failure> {
    agentspace_hosts::set_instance_type_keyed(home, key, instance_type)
        .map_err(|e| record_failure(home, key, &e))
}

/// [`set_instance_keyed`] of `project`'s `linux/arm64` key.
#[cfg(test)]
pub(crate) fn set_instance(
    home: &Path,
    project: &str,
    instance_id: Option<&str>,
    instance_type: Option<&str>,
) -> Result<(), Failure> {
    set_instance_keyed(
        home,
        &HostKey::linux_arm64(project),
        instance_id,
        instance_type,
    )
}

/// Sets or removes the host `key`'s record's `instance_id` and `instance_type` together, in one
/// write, under `hosts.lock`, and nothing else of any record (docs/state.md, The instance type,
/// steps 2 and 4; pins P11, P23 and P30).
pub(crate) fn set_instance_keyed(
    home: &Path,
    key: &HostKey,
    instance_id: Option<&str>,
    instance_type: Option<&str>,
) -> Result<(), Failure> {
    agentspace_hosts::set_instance_keyed(home, key, instance_id, instance_type)
        .map_err(|e| record_failure(home, key, &e))
}

/// [`set_last_success_at_keyed`] of `project`'s `linux/arm64` key.
#[cfg(test)]
pub(crate) fn set_last_success_at(home: &Path, project: &str, at: &str) -> Result<(), Failure> {
    set_last_success_at_keyed(home, &HostKey::linux_arm64(project), at)
}

/// Sets the host `key`'s record's `last_success_at` to `at`, under `hosts.lock` alone, changing
/// nothing else of any record (docs/operator.md, provision, step 9's end; D77; pin P23). A failure
/// is the registry's message, as every other change's is.
pub(crate) fn set_last_success_at_keyed(
    home: &Path,
    key: &HostKey,
    at: &str,
) -> Result<(), Failure> {
    agentspace_hosts::set_last_success_at_keyed(home, key, at)
        .map_err(|e| record_failure(home, key, &e))
}

/// The failure of a change to one host's record: a host with no record is the "has no host"
/// message, any other error is the registry's.
fn record_failure(home: &Path, key: &HostKey, error: &HostsError) -> Failure {
    match error {
        HostsError::Unwritable(agentspace_hosts::Failure::NoSuchRecord) => no_host_keyed(key),
        _ => change_failure(home, error),
    }
}
/// The instance-ID cache over a host's record, which `agentspace-box` reads and writes through
/// (operator.md, The host registry, as the operator uses it).
///
/// The registry is read and checked by the caller first, so [`InstanceIdCache::read`] never
/// reports a file that cannot be opened: it gives only the record's ID or a missing one.
pub(crate) struct RegistryCache {
    home: PathBuf,
    key: HostKey,
    path: PathBuf,
    instance_id: Option<String>,
}

impl RegistryCache {
    pub(crate) fn new(home: &Path, host: &Host) -> RegistryCache {
        RegistryCache {
            home: home.to_path_buf(),
            key: host.key(),
            path: hosts_path(home),
            instance_id: host.instance_id.clone(),
        }
    }
}

impl InstanceIdCache for RegistryCache {
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
        match parse_cache(bytes) {
            Some(id) => set_instance_id_keyed(&self.home, &self.key, Some(&id)),
            None => Err(cannot_write(&self.path)),
        }
    }
}

/// Whether the record is the box's.
pub(crate) fn is_box_record(host: &Host) -> bool {
    is_box(host)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{Scratch, ctx_in};
    use agentspace_hosts::HostState;

    fn seeded(dir: &Path) -> Hosts {
        let mut hosts = Hosts::seed();
        hosts.hosts.push(Host {
            last_success_at: None,
            platform: None,
            project: "agentspace".into(),
            stack: "agentspace-host-agentspace".into(),
            state: HostState::Creating,
            instance_id: None,
            instance_type: None,
            last_attempt_at: None,
        });
        let held = agentspace_hosts::lock(dir).unwrap();
        agentspace_hosts::write(dir, &hosts, &held).unwrap();
        hosts
    }

    #[test]
    fn the_box_and_a_host_are_found_and_an_unknown_project_is_refused() {
        let hosts = Hosts::seed();
        assert_eq!(find(&hosts, Target::Box).unwrap().project, "swimcoach");
        assert!(is_box_record(
            &find(&hosts, Target::Host("swimcoach")).unwrap()
        ));
        assert_eq!(
            find(&hosts, Target::Host("nothing")).unwrap_err().message,
            "agentspace: Project 'nothing' has no host; provision -Host nothing makes one."
        );
    }

    #[test]
    fn an_instance_id_is_written_into_its_record_alone_and_removed_again() {
        let dir = Scratch::new("registry-id");
        let ctx = ctx_in(dir.path());
        let home = crate::files::Places::new(&ctx).home().to_path_buf();
        let before = seeded(&home);
        set_instance_id(&home, "agentspace", Some("i-1234567890abcdef0")).unwrap();
        let after = read(&home).unwrap();
        assert_eq!(after.hosts[0], before.hosts[0]);
        assert_eq!(
            after.hosts[1].instance_id.as_deref(),
            Some("i-1234567890abcdef0")
        );
        assert_eq!(after.hosts[1].state, HostState::Creating);
        set_instance_id(&home, "agentspace", None).unwrap();
        assert_eq!(read(&home).unwrap(), before);
    }

    #[test]
    fn a_missing_registry_starts_from_the_box_and_the_first_change_writes_it() {
        let dir = Scratch::new("registry-seed");
        let ctx = ctx_in(dir.path());
        let home = crate::files::Places::new(&ctx).home().to_path_buf();
        assert!(!hosts_path(&home).exists());
        set_instance_id(&home, "swimcoach", Some("i-1234567890abcdef0")).unwrap();
        assert_eq!(
            read(&home).unwrap().hosts[0].instance_id.as_deref(),
            Some("i-1234567890abcdef0")
        );
    }

    #[test]
    fn a_registry_that_is_not_valid_fails_with_its_message_and_is_not_repaired() {
        let dir = Scratch::new("registry-bad");
        let ctx = ctx_in(dir.path());
        let home = crate::files::Places::new(&ctx).home().to_path_buf();
        std::fs::create_dir_all(hosts_path(&home).parent().unwrap()).unwrap();
        std::fs::write(hosts_path(&home), "not json").unwrap();
        let message = read(&home).unwrap_err().message;
        assert!(
            message.starts_with(&format!(
                "agentspace: The host registry {} cannot be read: ",
                hosts_path(&home).display()
            )) && message.ends_with('.'),
            "{message}"
        );
        assert_eq!(std::fs::read(hosts_path(&home)).unwrap(), b"not json");
    }

    #[test]
    fn the_cache_reads_the_record_and_writes_through_the_registry() {
        let dir = Scratch::new("registry-cache");
        let ctx = ctx_in(dir.path());
        let home = crate::files::Places::new(&ctx).home().to_path_buf();
        let hosts = seeded(&home);
        let cache = RegistryCache::new(&home, &hosts.hosts[1]);
        assert_eq!(cache.read(), CacheRead::Missing);
        cache.write(&cache_bytes("i-fedcba9876543210f")).unwrap();
        assert_eq!(
            read(&home).unwrap().hosts[1].instance_id.as_deref(),
            Some("i-fedcba9876543210f")
        );
        let known = Host {
            instance_id: Some("i-1234567890abcdef0".into()),
            ..hosts.hosts[1].clone()
        };
        assert_eq!(
            RegistryCache::new(&home, &known).read(),
            CacheRead::Bytes(cache_bytes("i-1234567890abcdef0"))
        );
    }
}
