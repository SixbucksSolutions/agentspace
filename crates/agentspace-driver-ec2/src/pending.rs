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
//! - **It MUST NOT change any member of a record but `state`, `expires_at` and `ssh`** when it
//!   places a lease, **and `state` and `reasons`** when it answers one `unsatisfiable`, nor any
//!   other record, nor the host registry; it starts nothing and makes no call to AWS.
//! - **From v0.09 it MUST place only the leases the host meets**, by the record's `instance_type`
//!   read in step 1, and kept for the whole call (D68; pin P1).
//! - **A `pending` lease the host does not meet MUST become `unsatisfiable`, with D66's
//!   `no_capacity`, only when a live `ready` lease of the project holds the host**, judged in a
//!   hold of its own of the lease state lock after every lease the host meets has been placed, and
//!   MUST stay `pending` when none does. A lease whose size is not in `SIZES` is neither placed nor
//!   answered (pin P20).
//! - **Its result MUST name every lease it did not place `ready` in one list, in the order the call
//!   tried them** (D71's pin, PR #139).

use std::collections::HashSet;

use agentspace_driver_api::{Context, Failure, LeaseRef, PlaceRequest, Placed};
use agentspace_hosts::{self as hosts, HostState, HostsError};
use agentspace_state::{Reason, Record, Ssh, StoreError, format_time, now};

use crate::host::{holds_host, registry_why};
use crate::ops::{self, HostKey};
use crate::sizes::{SIZES, meets};
use crate::text::{failure, no_capacity_at_size, with_leftovers};
use crate::{NAME, PendingFailure, PendingPlaced, Unplaced};

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
    ops::remove_files(ctx, &lease)?;
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

/// driver.md's place_pending, steps 1 to 3. The call as a whole fails only at step 1, or at a
/// hold's lock or read; a lease that cannot be placed or answered stays `pending`, is named in
/// `unplaced`, and the others go on.
pub fn place_pending(ctx: &Context, project: &str) -> Result<PendingPlaced, Failure> {
    if !hosts::is_normalized_project(project) {
        return Err(host_failure(HostsError::BadProject));
    }
    // Step 1: the record, read holding `hosts.lock` for that read alone. It is read as a placer
    // reads it, so a missing file is the box's record at the `m9g.xlarge` it implies (pin P2c); the
    // call never writes the registry.
    let record = {
        let held = hosts::lock(&ctx.home).map_err(host_failure)?;
        let registry = hosts::read_for_placing(&ctx.home).map_err(host_failure)?;
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
    // The type step 1 read, kept for the whole call. A record with none meets nothing.
    let instance_type = record.instance_type;
    let met = |r: &Record| instance_type.as_deref().is_some_and(|t| meets(t, &r.size));

    // Step 2: the leases the host meets, one per hold of the lease state lock.
    let mut tried = HashSet::new();
    let mut result = PendingPlaced::default();
    loop {
        let guard = agentspace_state::lock(&ctx.state_dir).map_err(store_failure)?;
        let mut file = agentspace_state::read(&ctx.state_dir).map_err(store_failure)?;
        let Some(index) = file
            .leases
            .iter()
            .position(|r| is_ours(r, project) && met(r) && !tried.contains(&r.id))
        else {
            break;
        };
        let id = file.leases[index].id.clone();
        tried.insert(id.clone());
        // A lease that fails here has had every file this call made for it removed already.
        match place_one(ctx, &file.leases[index], host) {
            Err(f) => result.unplaced.push(Unplaced::Failed(lease_failure(id, f))),
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
                        let left = ops::remove_files(ctx, &LeaseRef { id: &id, project })
                            .err()
                            .map(|f| f.leftovers)
                            .unwrap_or_default();
                        result
                            .unplaced
                            .push(Unplaced::Failed(write_failure(id, &e, &left)));
                    }
                }
            }
        }
        drop(guard);
    }

    // Step 3: the leases whose size is in `SIZES` and that the host does not meet, one per hold of
    // the lease state lock, each judged by the file read in its own hold (pin P1).
    let mut judged = HashSet::new();
    loop {
        let guard = agentspace_state::lock(&ctx.state_dir).map_err(store_failure)?;
        let mut file = agentspace_state::read(&ctx.state_dir).map_err(store_failure)?;
        let at = now();
        let Some(index) = file.leases.iter().position(|r| {
            is_ours(r, project)
                && SIZES.contains(&r.size.as_str())
                && !met(r)
                && !judged.contains(&r.id)
        }) else {
            break;
        };
        let id = file.leases[index].id.clone();
        judged.insert(id.clone());
        if !holds_host(&file.leases, project, at) {
            // Nothing holds the host: the next run resizes it to the largest `pending` lease.
            result.unplaced.push(Unplaced::Left { lease_id: id });
            drop(guard);
            continue;
        }
        result
            .unplaced
            .push(refuse(ctx, project, &mut file, index, &guard));
        drop(guard);
    }
    Ok(result)
}

/// Step 3.4's refusal of one lease, under the hold `guard` proves: its leftover files removed as
/// release removes them (pin P21), then its record written `unsatisfiable`, with exactly one
/// reason, `no_capacity`, and D66's message for its project and its size. Only `state` and
/// `reasons` change. A removal or a write that fails leaves it `pending`, named as a failure.
fn refuse(
    ctx: &Context,
    project: &str,
    file: &mut agentspace_state::StateFile,
    index: usize,
    guard: &agentspace_state::LockGuard,
) -> Unplaced {
    let id = file.leases[index].id.clone();
    if let Err(f) = ops::remove_files(ctx, &LeaseRef { id: &id, project }) {
        return Unplaced::Failed(lease_failure(id, f));
    }
    let lease = &mut file.leases[index];
    lease.state = "unsatisfiable".to_string();
    lease.reasons = Some(vec![Reason {
        code: "no_capacity".to_string(),
        message: no_capacity_at_size(&lease.project, &lease.size),
    }]);
    match agentspace_state::write(&ctx.state_dir, file, guard) {
        Ok(()) => Unplaced::Refused { lease_id: id },
        Err(e) => Unplaced::Failed(write_failure(id, &e, &[])),
    }
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
        pending_sized(id, project, driver, "m")
    }

    fn pending_sized(id: &str, project: &str, driver: Option<&str>, size: &str) -> String {
        let driver = driver
            .map(|d| format!(r#","driver":"{d}""#))
            .unwrap_or_default();
        format!(
            r#"{{"id":"{id}","project":"{project}","state":"pending","platform":"linux/arm64","profile":"agent-linux","size":"{size}","ttl_seconds":3600,"created_at":"2026-11-20T00:00:00Z"{driver}}}"#
        )
    }

    /// A `ready` lease of `project` whose `expires_at` is `expires_at` seconds from now.
    fn ready(id: &str, project: &str, expires_in: i64) -> String {
        format!(
            r#"{{"id":"{id}","project":"{project}","state":"ready","platform":"linux/arm64","profile":"agent-linux","size":"m","ttl_seconds":3600,"created_at":"2026-11-20T00:00:00Z","expires_at":"{}","ssh":{{"host_alias":"agentspace-{project}-{}","user":"ubuntu"}},"driver":"ec2"}}"#,
            format_time(now() + expires_in),
            &id[..8]
        )
    }

    const C: &str = "6f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90";
    const HELD: &str = "8f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90";

    /// The registry with `other`'s host `ready`, with `instance_type` when one is given.
    fn other_ready(c: &Context, instance_type: Option<&str>) {
        let member = instance_type
            .map(|t| format!(r#","instance_type":"{t}""#))
            .unwrap_or_default();
        fs::create_dir_all(c.home.join(".agentspace")).unwrap();
        fs::write(
            c.home.join(".agentspace").join("hosts.json"),
            format!(
                r#"{{"version":0,"hosts":[{{"project":"swimcoach","stack":"agentbox","state":"ready"}},{{"project":"other","stack":"agentspace-host-other","state":"ready"{member}}}]}}"#
            ),
        )
        .unwrap();
    }

    fn records(path: &Path) -> Vec<Record> {
        agentspace_state::parse(&fs::read(path).unwrap())
            .unwrap()
            .leases
    }

    fn kinds(done: &PendingPlaced) -> Vec<(&str, &str)> {
        done.unplaced
            .iter()
            .map(|u| {
                let kind = match u {
                    Unplaced::Failed(_) => "failed",
                    Unplaced::Refused { .. } => "refused",
                    Unplaced::Left { .. } => "left",
                };
                (kind, u.lease_id())
            })
            .collect()
    }

    #[test]
    fn a_lease_the_host_does_not_meet_is_refused_while_a_live_ready_lease_holds_it() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        other_ready(&c, Some("m9g.xlarge"));
        let path = write_state(
            &c,
            &[
                pending_sized(A, "other", Some("ec2"), "l"),
                ready(HELD, "other", 3600),
            ],
        );
        let before = records(&path);
        let done = place_pending(&c, "other").unwrap();
        assert_eq!(done.placed, 0);
        assert_eq!(kinds(&done), [("refused", A)]);
        let after = records(&path);
        // Only `state` and `reasons` change, and the other record not at all.
        let mut expected = before[0].clone();
        expected.state = "unsatisfiable".to_string();
        expected.reasons = Some(vec![Reason {
            code: "no_capacity".to_string(),
            message: "No capacity can be found for project 'other' at size 'l': its host is serving other leases at a smaller size. Release them, or ask for a size they fit.".to_string(),
        }]);
        assert_eq!(after[0], expected);
        assert_eq!(after[1], before[1]);
    }

    #[test]
    fn a_lease_the_host_does_not_meet_is_left_pending_while_nothing_holds_the_host() {
        // An expired `ready` lease, another project's and a released one hold nothing.
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        other_ready(&c, Some("m9g.xlarge"));
        let path = write_state(
            &c,
            &[
                pending_sized(A, "other", Some("ec2"), "xl"),
                ready(HELD, "other", -1),
                ready(C, "swimcoach", 3600),
            ],
        );
        let before = fs::read(&path).unwrap();
        let done = place_pending(&c, "other").unwrap();
        assert_eq!(kinds(&done), [("left", A)]);
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn a_size_outside_sizes_is_neither_placed_nor_answered() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        other_ready(&c, Some("m9g.4xlarge"));
        let path = write_state(
            &c,
            &[
                pending_sized(A, "other", Some("ec2"), "xxl"),
                ready(HELD, "other", 3600),
            ],
        );
        let before = fs::read(&path).unwrap();
        assert_eq!(place_pending(&c, "other"), Ok(PendingPlaced::default()));
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn the_leases_the_host_meets_are_tried_first_and_the_result_keeps_that_order() {
        // C comes first in the file but the host does not meet it, so step 2 tries A (which
        // fails here for want of a CA) before step 3 answers C. Another driver's lease is neither.
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        other_ready(&c, Some("m9g.xlarge"));
        let path = write_state(
            &c,
            &[
                pending_sized(C, "other", Some("ec2"), "l"),
                pending_sized(B, "other", Some("x"), "l"),
                pending_sized(A, "other", Some("ec2"), "s"),
                ready(HELD, "other", 3600),
            ],
        );
        let done = place_pending(&c, "other").unwrap();
        assert_eq!(kinds(&done), [("failed", A), ("refused", C)]);
        let after = records(&path);
        assert_eq!(after[0].state, "unsatisfiable");
        assert_eq!(after[1].state, "pending");
        assert_eq!(after[2].state, "pending");
    }

    #[test]
    fn a_record_with_no_type_meets_nothing_so_nothing_is_tried() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        other_ready(&c, None);
        let path = write_state(&c, &[pending_sized(A, "other", Some("ec2"), "s")]);
        let before = fs::read(&path).unwrap();
        let done = place_pending(&c, "other").unwrap();
        // Tried, it would have failed for want of a CA; it is left for the next run instead.
        assert_eq!(kinds(&done), [("left", A)]);
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn a_refusal_removes_a_killed_placements_leftovers_first() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        other_ready(&c, Some("m9g.large"));
        write_state(
            &c,
            &[
                pending_sized(A, "other", Some("ec2"), "m"),
                ready(HELD, "other", 3600),
            ],
        );
        let folder = c.home.join(".agentspace").join("leases").join(A);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("id_ed25519"), b"k").unwrap();
        let aliases = c.ssh_dir.join("agentspace.d");
        fs::create_dir_all(&aliases).unwrap();
        let alias = aliases.join("agentspace-other-3f2a9c1e");
        fs::write(&alias, format!("# agentspace lease {A}; x\nHost y\n")).unwrap();
        let done = place_pending(&c, "other").unwrap();
        assert_eq!(kinds(&done), [("refused", A)]);
        assert!(!folder.exists());
        assert!(!alias.exists());
    }

    #[test]
    fn a_refusal_whose_leftovers_cannot_go_leaves_the_lease_pending() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        other_ready(&c, Some("m9g.large"));
        let path = write_state(
            &c,
            &[
                pending_sized(A, "other", Some("ec2"), "m"),
                ready(HELD, "other", 3600),
            ],
        );
        let before = fs::read(&path).unwrap();
        let aliases = c.ssh_dir.join("agentspace.d");
        fs::create_dir_all(&aliases).unwrap();
        let alias = aliases.join("agentspace-other-3f2a9c1e");
        fs::write(&alias, b"# a person's file\n").unwrap();
        let done = place_pending(&c, "other").unwrap();
        assert_eq!(kinds(&done), [("failed", A)]);
        assert_eq!(done.unplaced[0].message(), Some(crate::text::FOREIGN_ALIAS));
        assert!(alias.exists());
        assert_eq!(fs::read(&path).unwrap(), before);
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
        // With no registry, the box's record is read with the `m9g.xlarge` a missing file implies,
        // which meets `m`: both are tried, and fail.
        assert_eq!(kinds(&done), [("failed", A), ("failed", B)]);
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
        assert_eq!(done.unplaced.len(), 1);
        assert_eq!(done.unplaced[0].lease_id(), A);
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
        assert_eq!(done.unplaced.len(), 1);
        assert_eq!(done.unplaced[0].lease_id(), A);
        assert_eq!(done.unplaced[0].message(), Some(crate::text::FOREIGN_ALIAS));
        assert!(alias.exists());
    }
}
