//! A fake driver for the lease commands' tests (E0.05-T03-S02).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. The fake implements the
//! pinned `agentspace_driver_api::Driver`. It answers as it is scripted, touches no file, and logs
//! every call in order, so a test can assert what the CLI asked of a driver and in what order. A
//! `Renewal` it hands out records whether it was committed, rolled back or dropped with neither.
//!
//! A test file includes this module with `#[path = "common/fake_driver.rs"] mod fake_driver;`, so
//! the tests of the front (E0.05-T01-S01), which share `common/mod.rs`, need none of its
//! dependencies.

#![allow(dead_code)]

use agentspace_driver_api::{
    Context, Driver, Failure, LeaseRef, PlaceRequest, Placed, Reason, Renewal,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

/// The clock the fake's default answers use, in whole seconds since the Unix epoch.
pub const ISSUE_TIME: i64 = 1_800_000_000;

/// One call the CLI made to a driver, with the values it passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Place {
        id: String,
        project: String,
        platform: String,
        profile: String,
        size: String,
        ttl_seconds: u32,
    },
    Renew {
        id: String,
        project: String,
        ttl_seconds: u32,
    },
    /// `Renewal::commit` was called.
    Commit,
    /// `Renewal::rollback` was called.
    Rollback,
    /// A `Renewal` was dropped with neither `commit` nor `rollback`, which puts the previous
    /// certificate back and so undoes the renew (card #0106's ruling from T02).
    RenewalDropped,
    CheckRelease {
        id: String,
        project: String,
    },
    Release {
        id: String,
        project: String,
    },
    /// `Driver::check_pending` was called (E0.07-T03-S01).
    CheckPending {
        id: String,
        project: String,
    },
}

/// Runs inside every call and returns a snapshot the test reads afterwards, for instance whether
/// the lease state lock is held, or what the state file holds, at that moment.
pub type Probe = Arc<dyn Fn(&Call) -> String + Send + Sync>;

#[derive(Default)]
struct Shared {
    calls: Vec<Call>,
    probes: Vec<String>,
    contexts: Vec<Context>,
    place: VecDeque<Result<Placed, Failure>>,
    renew: VecDeque<Result<i64, Failure>>,
    rollback: VecDeque<Result<(), Failure>>,
    check_release: VecDeque<Result<(), Failure>>,
    release: VecDeque<Result<(), Failure>>,
    check_pending: VecDeque<Result<(), Failure>>,
    probe: Option<Probe>,
    issue_time: i64,
}

fn lock(shared: &Arc<Mutex<Shared>>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

/// Logs a call, then runs the probe outside the lock.
fn log(shared: &Arc<Mutex<Shared>>, call: Call) {
    let probe = {
        let mut s = lock(shared);
        s.calls.push(call.clone());
        s.probe.clone()
    };
    let snapshot = probe.map(|p| p(&call)).unwrap_or_default();
    lock(shared).probes.push(snapshot);
}

/// The alias a lease has: `agentspace-<project>-<first 8 hex digits of the id>`.
pub fn alias_of(project: &str, id: &str) -> String {
    format!("agentspace-{project}-{}", &id[..8])
}

/// The test's handle on a fake: scripts its answers, and reads what it was asked.
#[derive(Clone)]
pub struct Handle(Arc<Mutex<Shared>>);

impl Handle {
    /// Every call so far, in order.
    pub fn calls(&self) -> Vec<Call> {
        lock(&self.0).calls.clone()
    }

    /// What the probe returned for each call, in the same order as `calls`.
    pub fn probes(&self) -> Vec<String> {
        lock(&self.0).probes.clone()
    }

    /// The `Context` each operation (`place`, `renew`, `check_release`, `release`, `check_pending`) received.
    pub fn contexts(&self) -> Vec<Context> {
        lock(&self.0).contexts.clone()
    }

    /// Forgets the calls, probes and contexts, not the scripted answers.
    pub fn clear(&self) {
        let mut s = lock(&self.0);
        s.calls.clear();
        s.probes.clear();
        s.contexts.clear();
    }

    pub fn set_probe(&self, probe: Probe) {
        lock(&self.0).probe = Some(probe);
    }

    /// The clock the default answers use: a place's expiry is this plus the TTL.
    pub fn set_issue_time(&self, secs: i64) {
        lock(&self.0).issue_time = secs;
    }

    /// The next `place` answers this; with none scripted, it answers a `ready` lease.
    pub fn script_place(&self, answer: Result<Placed, Failure>) {
        lock(&self.0).place.push_back(answer);
    }

    /// The next `renew` answers a renewal with this expiry, or this failure.
    pub fn script_renew(&self, answer: Result<i64, Failure>) {
        lock(&self.0).renew.push_back(answer);
    }

    /// The next `Renewal::rollback` answers this; with none scripted, `Ok`.
    pub fn script_rollback(&self, answer: Result<(), Failure>) {
        lock(&self.0).rollback.push_back(answer);
    }

    pub fn script_check_release(&self, answer: Result<(), Failure>) {
        lock(&self.0).check_release.push_back(answer);
    }

    pub fn script_release(&self, answer: Result<(), Failure>) {
        lock(&self.0).release.push_back(answer);
    }

    /// The next `check_pending` answers this; with none scripted, `Ok`.
    pub fn script_check_pending(&self, answer: Result<(), Failure>) {
        lock(&self.0).check_pending.push_back(answer);
    }
}

/// The fake driver. Make one with [`FakeDriver::new`], keep the [`Handle`], and box the driver
/// into the world's `drivers`.
pub struct FakeDriver {
    name: &'static str,
    shared: Arc<Mutex<Shared>>,
}

impl FakeDriver {
    /// A fake that answers `name` from `Driver::name`.
    pub fn new(name: &str) -> (FakeDriver, Handle) {
        let shared = Arc::new(Mutex::new(Shared {
            issue_time: ISSUE_TIME,
            ..Shared::default()
        }));
        let name: &'static str = Box::leak(name.to_string().into_boxed_str());
        (
            FakeDriver {
                name,
                shared: shared.clone(),
            },
            Handle(shared),
        )
    }

    fn see(&self, ctx: &Context) {
        lock(&self.shared).contexts.push(ctx.clone());
    }
}

struct FakeRenewal {
    shared: Arc<Mutex<Shared>>,
    expires_at: i64,
    done: bool,
}

impl Renewal for FakeRenewal {
    fn expires_at(&self) -> i64 {
        self.expires_at
    }

    fn commit(mut self: Box<Self>) {
        self.done = true;
        log(&self.shared, Call::Commit);
    }

    fn rollback(mut self: Box<Self>) -> Result<(), Failure> {
        self.done = true;
        log(&self.shared, Call::Rollback);
        lock(&self.shared).rollback.pop_front().unwrap_or(Ok(()))
    }
}

impl Drop for FakeRenewal {
    fn drop(&mut self) {
        if !self.done {
            log(&self.shared, Call::RenewalDropped);
        }
    }
}

impl Driver for FakeDriver {
    fn name(&self) -> &'static str {
        self.name
    }

    fn place(&self, ctx: &Context, req: &PlaceRequest) -> Result<Placed, Failure> {
        self.see(ctx);
        log(
            &self.shared,
            Call::Place {
                id: req.id.to_string(),
                project: req.project.to_string(),
                platform: req.platform.to_string(),
                profile: req.profile.to_string(),
                size: req.size.to_string(),
                ttl_seconds: req.ttl_seconds,
            },
        );
        let (scripted, issue) = {
            let mut s = lock(&self.shared);
            (s.place.pop_front(), s.issue_time)
        };
        scripted.unwrap_or_else(|| {
            Ok(Placed::Ready {
                host_alias: alias_of(req.project, req.id),
                user: "ubuntu".to_string(),
                expires_at: issue + i64::from(req.ttl_seconds),
            })
        })
    }

    fn renew(
        &self,
        ctx: &Context,
        lease: &LeaseRef,
        ttl_seconds: u32,
    ) -> Result<Box<dyn Renewal>, Failure> {
        self.see(ctx);
        log(
            &self.shared,
            Call::Renew {
                id: lease.id.to_string(),
                project: lease.project.to_string(),
                ttl_seconds,
            },
        );
        let (scripted, issue) = {
            let mut s = lock(&self.shared);
            (s.renew.pop_front(), s.issue_time)
        };
        let expires_at = scripted.unwrap_or(Ok(issue + i64::from(ttl_seconds)))?;
        Ok(Box::new(FakeRenewal {
            shared: self.shared.clone(),
            expires_at,
            done: false,
        }))
    }

    fn check_release(&self, ctx: &Context, lease: &LeaseRef) -> Result<(), Failure> {
        self.see(ctx);
        log(
            &self.shared,
            Call::CheckRelease {
                id: lease.id.to_string(),
                project: lease.project.to_string(),
            },
        );
        lock(&self.shared)
            .check_release
            .pop_front()
            .unwrap_or(Ok(()))
    }

    fn release(&self, ctx: &Context, lease: &LeaseRef) -> Result<(), Failure> {
        self.see(ctx);
        log(
            &self.shared,
            Call::Release {
                id: lease.id.to_string(),
                project: lease.project.to_string(),
            },
        );
        lock(&self.shared).release.pop_front().unwrap_or(Ok(()))
    }

    fn check_pending(&self, ctx: &Context, lease: &LeaseRef) -> Result<(), Failure> {
        self.see(ctx);
        log(
            &self.shared,
            Call::CheckPending {
                id: lease.id.to_string(),
                project: lease.project.to_string(),
            },
        );
        lock(&self.shared)
            .check_pending
            .pop_front()
            .unwrap_or(Ok(()))
    }
}

/// A failure with no leftovers.
pub fn fail(message: &str) -> Failure {
    Failure {
        message: message.to_string(),
        leftovers: Vec::new(),
    }
}

/// A failure that carries leftovers.
pub fn fail_with(message: &str, leftovers: &[&str]) -> Failure {
    Failure {
        message: message.to_string(),
        leftovers: leftovers.iter().map(|s| s.to_string()).collect(),
    }
}

/// A reason, as lease-api.md words the three that v0 reports.
pub fn reason(code: &'static str, message: &str) -> Reason {
    Reason {
        code,
        message: message.to_string(),
    }
}
