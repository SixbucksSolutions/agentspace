//! `place_pending`: places every `pending` lease of one project once its host is `ready`
//! (docs/driver.md, place_pending and Lock order; docs/state.md, The lease state file and
//! Creation, in order; D48).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **It MUST read the host record holding `hosts.lock` for that read alone**, and no lease state
//!   lock, and MUST place nothing when the record is not `ready`.
//! - **It MUST take the lease state lock once per lease, and re-read the lease state file in each
//!   hold**, so a lease released or changed meanwhile is seen as it now stands and never placed.
//! - **It MUST remove a killed placement's leftover alias and lease folder before it issues**, and
//!   a removal that fails is a failure of that lease alone.
//! - **One lease's failure MUST NOT stop the others**, and leaves that lease `pending` with every
//!   file this call made for it removed.
//! - **It MUST NOT change any member of a record but `state`, `expires_at` and `ssh`**, nor any
//!   other record, nor the host registry; it starts nothing and makes no call to AWS.

use std::collections::HashSet;

use agentspace_driver_api::{Context, Failure, LeaseRef, PlaceRequest, Placed};
use agentspace_hosts::{self as hosts, HostState, HostsError};
use agentspace_state::{Record, Ssh, StoreError, format_time};

use crate::host::registry_why;
use crate::ops::{self, HostKey};
use crate::text::{failure, with_leftovers};
use crate::{NAME, PendingFailure, PendingPlaced};

/// What follows a failure of the whole call: what it placed before stays `ready`.
const REST_STAY: &str = "The leases not yet placed stay pending.";

fn whole(message: String) -> Failure {
    failure(message, Vec::new())
}

fn host_failure(e: HostsError) -> Failure {
    whole(format!(
        "The project's host could not be looked up: {}. Every pending lease stays pending.",
        registry_why(e)
    ))
}

/// A failure to lock or read the lease state file, in lease-api.md's words for the records.
fn store_failure(e: StoreError) -> Failure {
    whole(match e {
        StoreError::Busy => format!(
            "The lease records are busy: another agentspace lease command held them for 30 seconds. {REST_STAY}"
        ),
        StoreError::CannotLock(why) => {
            format!("The lease records could not be locked: {why}. {REST_STAY}")
        }
        StoreError::Unreadable(why) | StoreError::NotValid(why) => {
            format!("The lease records could not be read: {why}. {REST_STAY}")
        }
        StoreError::Unwritable(why) => {
            format!("The lease records could not be written: {why}. {REST_STAY}")
        }
    })
}

/// Whether `record` is one this call places: stored `pending`, of the project, placed by this
/// driver.
fn is_ours(record: &Record, project: &str) -> bool {
    record.state == "pending" && record.project == project && record.driver.as_deref() == Some(NAME)
}

/// Steps 4 and 5 for one lease, under the lease state lock: the leftovers of a killed placement
/// removed as release removes them, then the credential and the alias, as `create` places one.
fn place_one(
    ctx: &Context,
    record: &Record,
    host: HostKey,
) -> Result<(String, String, i64), Failure> {
    let lease = LeaseRef {
        id: &record.id,
        project: &record.project,
    };
    ops::release(ctx, &lease)?;
    let ttl_seconds = u32::try_from(record.ttl_seconds)
        .ok()
        .filter(|t| (60..=86_400).contains(t))
        .ok_or_else(|| {
            whole("The lease's TTL is not one a certificate can be issued for.".to_string())
        })?;
    let req = PlaceRequest {
        id: &record.id,
        project: &record.project,
        platform: &record.platform,
        profile: &record.profile,
        size: &record.size,
        ttl_seconds,
    };
    match ops::issue_ready(ctx, &req, host)? {
        Placed::Ready {
            host_alias,
            user,
            expires_at,
        } => Ok((host_alias, user, expires_at)),
        // `issue_ready` answers `ready` or fails; anything else is never written.
        _ => Err(whole(
            "The lease could not be placed on its host.".to_string(),
        )),
    }
}

/// What a lease that could not be placed reports for the run's log. Place's own messages end
/// `No lease was recorded.`, which is `create`'s ending: here the lease is recorded and stays
/// `pending`, so that ending is said so.
fn lease_failure(lease_id: String, f: Failure) -> PendingFailure {
    const CREATE_END: &str = "No lease was recorded.";
    const PENDING_END: &str = "The lease stays pending.";
    let message = match f.message.rfind(CREATE_END) {
        Some(at) => format!(
            "{}{PENDING_END}{}",
            &f.message[..at],
            &f.message[at + CREATE_END.len()..]
        ),
        None => f.message,
    };
    PendingFailure { lease_id, message }
}

/// driver.md's place_pending, steps 1 and 2. The call as a whole fails only at step 1, or at a
/// hold's lock or read; a lease that cannot be placed stays `pending`, is named in `failed`, and
/// the others go on.
pub fn place_pending(ctx: &Context, project: &str) -> Result<PendingPlaced, Failure> {
    if !hosts::is_normalized_project(project) {
        return Err(host_failure(HostsError::BadProject));
    }
    // Step 1: the record, read holding `hosts.lock` for that read alone.
    let record = {
        let held = hosts::lock(&ctx.home).map_err(host_failure)?;
        let registry = hosts::read(&ctx.home).map_err(host_failure)?;
        drop(held);
        hosts::host_for(&registry, project).cloned()
    };
    let Some(record) = record.filter(|h| h.state == HostState::Ready) else {
        return Ok(PendingPlaced::default());
    };
    let host = if hosts::is_box(&record) {
        HostKey::Box
    } else {
        HostKey::Other(project)
    };

    // Step 2: one lease per hold of the lease state lock.
    let mut tried = HashSet::new();
    let mut result = PendingPlaced::default();
    loop {
        let guard = agentspace_state::lock(&ctx.state_dir).map_err(store_failure)?;
        let mut file = agentspace_state::read(&ctx.state_dir).map_err(store_failure)?;
        let Some(index) = file
            .leases
            .iter()
            .position(|r| is_ours(r, project) && !tried.contains(&r.id))
        else {
            break;
        };
        let id = file.leases[index].id.clone();
        tried.insert(id.clone());
        // A lease that fails here has had every file this call made for it removed already.
        match place_one(ctx, &file.leases[index], host) {
            Err(f) => result.failed.push(lease_failure(id, f)),
            Ok((host_alias, user, expires_at)) => {
                let lease = &mut file.leases[index];
                lease.state = "ready".to_string();
                lease.expires_at = Some(format_time(expires_at));
                lease.ssh = Some(Ssh { host_alias, user });
                match agentspace_state::write(&ctx.state_dir, &file, &guard) {
                    Ok(()) => result.placed += 1,
                    Err(e) => {
                        // The lease stays `pending`, so its files go as release removes them; what
                        // cannot be removed is named, and goes with the next placement's leftovers
                        // or the lease's release.
                        let left = ops::release(ctx, &LeaseRef { id: &id, project })
                            .err()
                            .map(|f| f.leftovers)
                            .unwrap_or_default();
                        result.failed.push(write_failure(id, &e, &left));
                    }
                }
            }
        }
        drop(guard);
    }
    Ok(result)
}

/// What a lease whose `ready` record could not be written reports: the records' why, then what
/// the release of its files could not remove, then that it stays `pending`.
fn write_failure(lease_id: String, e: &StoreError, left: &[String]) -> PendingFailure {
    let why = match e {
        StoreError::Busy => "the lease records are busy",
        StoreError::CannotLock(why)
        | StoreError::Unreadable(why)
        | StoreError::NotValid(why)
        | StoreError::Unwritable(why) => why.as_str(),
    };
    let message = with_leftovers(
        format!("The lease records could not be written: {why}."),
        left,
    );
    PendingFailure {
        lease_id,
        message: format!("{message} The lease stays pending."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    const A: &str = "3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90";
    const B: &str = "4f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90";

    fn ctx(root: &Path) -> Context {
        let home = root.join("home");
        Context {
            home: home.clone(),
            ssh_dir: home.join(".ssh"),
            proxy_override: Some("proxy %h".to_string()),
            system_root: None,
            own_exe: root.join("bin").join("agentspace-operator.exe"),
            state_dir: home.join(".agentspace"),
            operator_program: None,
            state_dir_env: None,
            ssh_dir_env: None,
        }
    }

    fn pending(id: &str, project: &str, driver: Option<&str>) -> String {
        let driver = driver
            .map(|d| format!(r#","driver":"{d}""#))
            .unwrap_or_default();
        format!(
            r#"{{"id":"{id}","project":"{project}","state":"pending","platform":"linux/arm64","profile":"agent-linux","size":"m","ttl_seconds":3600,"created_at":"2026-11-20T00:00:00Z"{driver}}}"#
        )
    }

    fn write_state(c: &Context, records: &[String]) -> PathBuf {
        fs::create_dir_all(&c.state_dir).unwrap();
        let path = c.state_dir.join("leases.json");
        fs::write(
            &path,
            format!(r#"{{"version":0,"leases":[{}]}}"#, records.join(",")),
        )
        .unwrap();
        path
    }

    #[test]
    fn a_places_ending_becomes_pendings_even_before_its_leftovers() {
        let f = failure(
            "It failed. No lease was recorded.".to_string(),
            vec!["~/x".to_string()],
        );
        let p = lease_failure(A.to_string(), f);
        assert_eq!(
            p.message,
            "It failed. The lease stays pending. Still there: ~/x."
        );
        let f = failure("It failed. No lease was recorded.".to_string(), Vec::new());
        assert_eq!(
            lease_failure(A.to_string(), f).message,
            "It failed. The lease stays pending."
        );
    }

    #[test]
    fn a_write_failure_names_the_why_and_the_leftovers_and_ends_pending() {
        let e = StoreError::Unwritable("the state folder is a file".to_string());
        let p = write_failure(A.to_string(), &e, &["~/x".to_string()]);
        assert_eq!(p.lease_id, A);
        assert_eq!(
            p.message,
            "The lease records could not be written: the state folder is a file. Still there: ~/x. The lease stays pending."
        );
    }

    #[test]
    fn a_host_that_is_not_ready_or_has_no_record_places_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let path = write_state(&c, &[pending(A, "other", Some("ec2"))]);
        let before = fs::read(&path).unwrap();
        assert_eq!(place_pending(&c, "other"), Ok(PendingPlaced::default()));
        fs::write(
            c.home.join(".agentspace").join("hosts.json"),
            r#"{"version":0,"hosts":[{"project":"swimcoach","stack":"agentbox","state":"ready"},{"project":"other","stack":"agentspace-host-other","state":"creating"}]}"#,
        )
        .unwrap();
        assert_eq!(place_pending(&c, "other"), Ok(PendingPlaced::default()));
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn a_registry_that_is_not_valid_fails_the_call() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        fs::create_dir_all(c.home.join(".agentspace")).unwrap();
        fs::write(c.home.join(".agentspace").join("hosts.json"), "{}").unwrap();
        let f = place_pending(&c, "swimcoach").unwrap_err();
        assert_eq!(
            f.message,
            "The project's host could not be looked up: agentspace's host records are not valid. Every pending lease stays pending."
        );
    }

    #[test]
    fn a_state_file_that_is_not_valid_fails_the_call_and_is_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        fs::create_dir_all(&c.state_dir).unwrap();
        fs::write(c.state_dir.join("leases.json"), "{").unwrap();
        let f = place_pending(&c, "swimcoach").unwrap_err();
        assert!(
            f.message
                .starts_with("The lease records could not be read: ")
        );
        assert_eq!(fs::read(c.state_dir.join("leases.json")).unwrap(), b"{");
    }

    #[test]
    fn every_lease_is_tried_once_and_a_failure_leaves_it_pending() {
        // No CA: each of the project's two leases fails, and the call goes on and succeeds with
        // none placed; the other project's and another driver's are never tried, and the file is
        // never written. The loop ends, so each lease was tried once.
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let path = write_state(
            &c,
            &[
                pending(A, "swimcoach", Some("ec2")),
                pending("5f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90", "other", Some("ec2")),
                pending(
                    "6f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90",
                    "swimcoach",
                    Some("x"),
                ),
                pending("7f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90", "swimcoach", None),
                pending(B, "swimcoach", Some("ec2")),
            ],
        );
        let before = fs::read(&path).unwrap();
        let done = place_pending(&c, "swimcoach").unwrap();
        assert_eq!(done.placed, 0);
        let ids: Vec<&str> = done.failed.iter().map(|f| f.lease_id.as_str()).collect();
        assert_eq!(ids, [A, B]);
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn a_killed_placements_leftovers_are_removed_before_issuing() {
        // No CA, so the lease fails at issue; its leftovers went first all the same.
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        write_state(&c, &[pending(A, "swimcoach", Some("ec2"))]);
        let folder = c.home.join(".agentspace").join("leases").join(A);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("id_ed25519"), b"k").unwrap();
        let aliases = c.ssh_dir.join("agentspace.d");
        fs::create_dir_all(&aliases).unwrap();
        let alias = aliases.join("agentspace-swimcoach-3f2a9c1e");
        fs::write(&alias, format!("# agentspace lease {A}; x\nHost y\n")).unwrap();
        let done = place_pending(&c, "swimcoach").unwrap();
        assert_eq!(done.placed, 0);
        assert_eq!(done.failed.len(), 1);
        assert_eq!(done.failed[0].lease_id, A);
        assert!(!folder.exists());
        assert!(!alias.exists());
    }

    #[test]
    fn a_foreign_alias_at_the_leases_place_is_a_failure_of_that_lease_and_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        write_state(&c, &[pending(A, "swimcoach", Some("ec2"))]);
        let aliases = c.ssh_dir.join("agentspace.d");
        fs::create_dir_all(&aliases).unwrap();
        let alias = aliases.join("agentspace-swimcoach-3f2a9c1e");
        fs::write(&alias, b"# a person's file\n").unwrap();
        let done = place_pending(&c, "swimcoach").unwrap();
        assert_eq!(done.placed, 0);
        assert_eq!(done.failed.len(), 1);
        assert_eq!(done.failed[0].lease_id, A);
        assert_eq!(done.failed[0].message, crate::text::FOREIGN_ALIAS);
        assert!(alias.exists());
    }
}
