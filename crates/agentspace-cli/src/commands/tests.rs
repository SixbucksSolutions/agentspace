//! The commands' order of checks, locks, driver calls, writes and undo, against a fake driver in
//! throwaway folders under the temp folder.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use agentspace_driver_api::{
    Context, Driver, Failure, LeaseRef, PlaceRequest, Placed, Reason, Renewal,
};
use agentspace_state::{Record, Ssh, StateFile, format_time, serialize};
use serde_json::Value;

use crate::{Outcome, World, execute_in};

const NOW: i64 = 1_800_000_000;
const ID_A: &str = "3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f";
const ID_B: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

type Log = Rc<RefCell<Vec<String>>>;

/// A driver that records every call and answers as it is told.
struct Fake {
    name: &'static str,
    log: Log,
    placed: Placed,
    /// How many of the next `release` calls fail.
    release_failures: Cell<u32>,
    leftovers: Vec<String>,
    /// Runs inside `renew`, after the renewal is made.
    on_renew: Box<dyn Fn()>,
    rollback_fails: bool,
    check_fails: bool,
    check_panics: bool,
}

impl Fake {
    fn new(name: &'static str, log: &Log) -> Fake {
        Fake {
            name,
            log: log.clone(),
            // An empty alias is filled from the request in `place`.
            placed: Placed::Ready {
                host_alias: String::new(),
                user: "ubuntu".to_owned(),
                expires_at: NOW + 3600,
            },
            release_failures: Cell::new(0),
            leftovers: Vec::new(),
            on_renew: Box::new(|| {}),
            rollback_fails: false,
            check_fails: false,
            check_panics: false,
        }
    }
}

struct FakeRenewal {
    log: Log,
    expires_at: i64,
    rollback_fails: bool,
}

impl Renewal for FakeRenewal {
    fn expires_at(&self) -> i64 {
        self.expires_at
    }
    fn commit(self: Box<Self>) {
        self.log.borrow_mut().push("commit".to_owned());
    }
    fn rollback(self: Box<Self>) -> Result<(), Failure> {
        self.log.borrow_mut().push("rollback".to_owned());
        if self.rollback_fails {
            Err(Failure {
                message: "The previous certificate could not be put back.".to_owned(),
                leftovers: Vec::new(),
            })
        } else {
            Ok(())
        }
    }
}

impl Drop for FakeRenewal {
    fn drop(&mut self) {
        self.log.borrow_mut().push("renewal dropped".to_owned());
    }
}

impl Driver for Fake {
    fn name(&self) -> &'static str {
        self.name
    }
    fn place(&self, ctx: &Context, req: &PlaceRequest) -> Result<Placed, Failure> {
        self.log.borrow_mut().push(format!(
            "place {} {} {} {} {} {} proxy={:?}",
            req.id,
            req.project,
            req.platform,
            req.profile,
            req.size,
            req.ttl_seconds,
            ctx.proxy_override
        ));
        Ok(match self.placed.clone() {
            Placed::Ready {
                host_alias,
                user,
                expires_at,
            } if host_alias.is_empty() => Placed::Ready {
                host_alias: format!("agentspace-{}-{}", req.project, &req.id[..8]),
                user,
                expires_at,
            },
            other => other,
        })
    }
    fn renew(
        &self,
        _ctx: &Context,
        lease: &LeaseRef,
        ttl_seconds: u32,
    ) -> Result<Box<dyn Renewal>, Failure> {
        self.log
            .borrow_mut()
            .push(format!("renew {} {}", lease.id, ttl_seconds));
        let renewal = FakeRenewal {
            log: self.log.clone(),
            expires_at: NOW + i64::from(ttl_seconds),
            rollback_fails: self.rollback_fails,
        };
        (self.on_renew)();
        Ok(Box::new(renewal))
    }
    fn check_release(&self, _ctx: &Context, lease: &LeaseRef) -> Result<(), Failure> {
        self.log
            .borrow_mut()
            .push(format!("check_release {}", lease.id));
        Ok(())
    }
    fn check_pending(&self, ctx: &Context, lease: &LeaseRef) -> Result<(), Failure> {
        self.log.borrow_mut().push(format!(
            "check_pending {} {} operator={:?} proxy={:?} ssh={:?}",
            lease.id,
            lease.project,
            ctx.operator_program.as_ref().map(|p| p.file_name()),
            ctx.proxy_override,
            ctx.ssh_dir.file_name()
        ));
        if self.check_fails {
            return Err(Failure {
                message: "The lease's host cannot be made.".to_owned(),
                leftovers: Vec::new(),
            });
        }
        if self.check_panics {
            panic!("check_pending panics");
        }
        Ok(())
    }
    fn release(&self, _ctx: &Context, lease: &LeaseRef) -> Result<(), Failure> {
        self.log.borrow_mut().push(format!("release {}", lease.id));
        let left = self.release_failures.get();
        if left > 0 {
            self.release_failures.set(left - 1);
            return Err(Failure {
                message: "The lease's local key, certificate or host alias could not be \
                          deleted: a file is held open. Run release again to finish."
                    .to_owned(),
                leftovers: self.leftovers.clone(),
            });
        }
        Ok(())
    }
}

/// A throwaway home, and the variables that point into it.
struct Env {
    dir: tempfile::TempDir,
    vars: HashMap<String, OsString>,
    /// The world's own executable: `agentspace`, relative, unless a test sets a full one.
    own_exe: PathBuf,
}

impl Env {
    fn new() -> Env {
        let dir = tempfile::TempDir::new().unwrap();
        let mut vars = HashMap::new();
        vars.insert("USERPROFILE".to_owned(), dir.path().as_os_str().to_owned());
        Env {
            dir,
            vars,
            own_exe: PathBuf::from("agentspace"),
        }
    }

    /// No home, and the state folder set inside the throwaway folder.
    fn without_home() -> Env {
        let mut env = Env::new();
        env.vars.remove("USERPROFILE");
        let state = env.dir.path().join("state");
        env.vars
            .insert("AGENTSPACE_STATE_DIR".to_owned(), state.into_os_string());
        env
    }

    fn state_dir(&self) -> PathBuf {
        match self.vars.get("AGENTSPACE_STATE_DIR") {
            Some(v) => PathBuf::from(v),
            None => self.dir.path().join(".agentspace"),
        }
    }

    fn put(&self, records: Vec<Record>) {
        let bytes = serialize(&StateFile { leases: records }).unwrap();
        std::fs::create_dir_all(self.state_dir()).unwrap();
        std::fs::write(self.state_dir().join("leases.json"), bytes).unwrap();
    }

    fn records(&self) -> Vec<Record> {
        agentspace_state::read(&self.state_dir()).unwrap().leases
    }

    fn run(&self, args: &[&str], drivers: &[Box<dyn Driver>], ids: &[&str]) -> Outcome {
        let ids: RefCell<Vec<String>> =
            RefCell::new(ids.iter().rev().map(|s| (*s).to_owned()).collect());
        let vars = self.vars.clone();
        let var = move |name: &str| vars.get(name).cloned();
        let now = || NOW;
        let new_id = || ids.borrow_mut().pop().unwrap_or_default();
        let world = World {
            var: &var,
            cwd: Some(self.dir.path()),
            own_exe: &self.own_exe,
            now: &now,
            new_id: &new_id,
            drivers,
        };
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        execute_in(&args, &world)
    }
}

fn record(id: &str, state: &str, driver: Option<&str>) -> Record {
    let mut r = Record {
        sized_for: None,
        plan: None,
        plan_source: None,
        id: id.to_owned(),
        project: "demo".to_owned(),
        state: state.to_owned(),
        platform: "linux/arm64".to_owned(),
        profile: "agent-linux".to_owned(),
        size: "m".to_owned(),
        ttl_seconds: 3600,
        created_at: format_time(NOW - 600),
        expires_at: None,
        released_at: None,
        ssh: None,
        reasons: None,
        driver: driver.map(str::to_owned),
    };
    match state {
        "ready" => {
            r.expires_at = Some(format_time(NOW + 3000));
            r.ssh = Some(Ssh {
                host_alias: format!("agentspace-demo-{}", &id[..8]),
                user: "ubuntu".to_owned(),
            });
        }
        "released" => {
            r.expires_at = Some(format_time(NOW + 3000));
            r.released_at = Some(format_time(NOW - 60));
        }
        "unsatisfiable" => {
            r.reasons = Some(vec![agentspace_state::Reason {
                code: "unsupported_size".to_owned(),
                message: "Size 'xxl' is not supported; v0 supports sizes s, m, l and xl."
                    .to_owned(),
            }]);
        }
        _ => {}
    }
    r
}

fn expired(id: &str) -> Record {
    let mut r = record(id, "ready", Some("fake"));
    r.expires_at = Some(format_time(NOW - 1));
    r
}

fn err(out: &Outcome) -> Value {
    assert!(out.stdout.is_empty());
    serde_json::from_slice(&out.stderr).unwrap()
}

fn doc(out: &Outcome) -> Value {
    assert_eq!(out.exit_code, 0, "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}

fn log_of(log: &Log) -> Vec<String> {
    log.borrow().clone()
}

// The order examples of lease-api.md, Exit codes and errors.

#[test]
fn renew_of_an_expired_lease_with_a_ttl_out_of_range_is_out_of_range() {
    let env = Env::new();
    env.put(vec![expired(ID_A)]);
    let log = Log::default();
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(Fake::new("fake", &log))];
    let out = env.run(&["lease", "renew", ID_A, "--ttl", "48h"], &drivers, &[]);
    assert_eq!(out.exit_code, 2);
    assert_eq!(err(&out)["code"], "out_of_range");
    assert!(log_of(&log).is_empty());
}

#[test]
fn renew_of_an_unknown_id_with_a_bare_ttl_is_invalid_value_for_ttl() {
    let env = Env::new();
    let out = env.run(&["lease", "renew", ID_A, "--ttl", "8"], &[], &[]);
    let e = err(&out);
    assert_eq!(e["code"], "invalid_value");
    assert_eq!(e["field"], "ttl");
}

#[test]
fn get_with_a_bad_id_and_a_ttl_is_usage() {
    let env = Env::new();
    let out = env.run(&["lease", "get", "nope", "--ttl", "8h"], &[], &[]);
    assert_eq!(err(&out)["code"], "usage");
}

#[test]
fn renew_of_an_expired_lease_is_invalid_state_with_its_advice() {
    let env = Env::new();
    env.put(vec![expired(ID_A)]);
    let out = env.run(&["lease", "renew", ID_A], &[], &[]);
    assert_eq!(out.exit_code, 4);
    let e = err(&out);
    assert_eq!(e["state"], "expired");
    assert_eq!(e["lease_id"], ID_A);
    assert_eq!(
        e["message"],
        format!(
            "Lease '{ID_A}' is expired, and only a ready lease can be renewed. Create a new lease."
        )
    );
}

// create

#[test]
fn create_undoes_a_ready_place_the_records_refuse_and_reports_the_records_failure() {
    let env = Env::new();
    let log = Log::default();
    let mut fake = Fake::new("fake", &log);
    fake.placed = Placed::Ready {
        host_alias: String::new(),
        user: "Ubuntu".to_owned(),
        expires_at: NOW + 3600,
    };
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(fake)];
    let out = env.run(&["lease", "create", "--project", "demo"], &drivers, &[ID_A]);
    assert_eq!(
        err(&out)["message"],
        "agentspace lease create failed: The lease records could not be written: the change is \
         not valid. No lease was recorded."
    );
    assert_eq!(log_of(&log)[1], format!("release {ID_A}"));
    assert!(env.records().is_empty());
}

#[test]
fn create_releases_nothing_for_an_unsatisfiable_result_the_records_refuse() {
    let env = Env::new();
    let log = Log::default();
    let mut fake = Fake::new("fake", &log);
    fake.placed = Placed::Unsatisfiable {
        reasons: Vec::new(),
    };
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(fake)];
    let out = env.run(&["lease", "create", "--project", "demo"], &drivers, &[ID_A]);
    assert_eq!(
        err(&out)["message"],
        "The lease records could not be written: the change is not valid. Nothing was changed."
    );
    assert_eq!(log_of(&log).len(), 1);
}

#[test]
fn create_with_no_driver_fails_and_records_nothing() {
    let env = Env::new();
    let out = env.run(&["lease", "create", "--project", "demo"], &[], &[ID_A]);
    assert_eq!(out.exit_code, 1);
    assert_eq!(
        err(&out)["message"],
        "agentspace lease create failed: No lease driver is available. Nothing was recorded."
    );
    assert!(env.records().is_empty());
}

#[test]
fn create_records_the_driver_name_and_never_prints_it() {
    let mut env = Env::new();
    env.vars
        .insert("AGENTSPACE_PROXY_COMMAND".to_owned(), "proxy %h".into());
    let log = Log::default();
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(Fake::new("secretname", &log))];
    let out = env.run(
        &["lease", "create", "--project", " Demo ", "--ttl", "1h"],
        &drivers,
        &[ID_A],
    );
    let d = doc(&out);
    assert_eq!(d["state"], "ready");
    assert_eq!(d["id"], ID_A);
    assert_eq!(d["created_at"], format_time(NOW));
    assert!(
        !String::from_utf8(out.stdout)
            .unwrap()
            .contains("secretname")
    );
    assert_eq!(
        log_of(&log),
        vec![format!(
            "place {ID_A} demo linux/arm64 agent-linux m 3600 proxy=Some(\"proxy %h\")"
        )]
    );
    let records = env.records();
    assert_eq!(records[0].driver.as_deref(), Some("secretname"));
}

#[test]
fn create_draws_again_while_a_kept_leases_alias_is_the_same() {
    let env = Env::new();
    env.put(vec![record(ID_A, "released", Some("fake"))]);
    let log = Log::default();
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(Fake::new("fake", &log))];
    // Same first 8 hex digits as ID_A, so the same alias, twice; then a free one.
    let twin = "3f2a9c1e-0000-4000-8000-000000000000";
    let out = env.run(
        &["lease", "create", "--project", "demo"],
        &drivers,
        &[ID_A, twin, ID_B],
    );
    assert_eq!(doc(&out)["id"], ID_B);
    // Another project's lease with the same first 8 digits has another alias.
    let out = env.run(
        &["lease", "create", "--project", "other"],
        &drivers,
        &[twin],
    );
    assert_eq!(doc(&out)["id"], twin);
}

#[test]
fn create_fails_after_a_hundred_draws_with_no_free_alias() {
    let env = Env::new();
    env.put(vec![record(ID_A, "unsatisfiable", None)]);
    let log = Log::default();
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(Fake::new("fake", &log))];
    let ids = vec![ID_A; super::create::MAX_DRAWS];
    let out = env.run(&["lease", "create", "--project", "demo"], &drivers, &ids);
    assert_eq!(out.exit_code, 1);
    assert!(log_of(&log).is_empty());
    let mut more = ids.clone();
    more.push(ID_B);
    let out = env.run(&["lease", "create", "--project", "demo"], &drivers, &more);
    assert_eq!(
        out.exit_code,
        1,
        "Only {} draws are made",
        super::create::MAX_DRAWS
    );
}

#[test]
fn create_releases_a_ready_lease_whose_record_cannot_be_written() {
    let env = Env::new();
    let log = Log::default();
    let mut fake = Fake::new("fake", &log);
    // The output allows an uppercase user; the record does not, so the write is refused.
    fake.placed = Placed::Ready {
        host_alias: "agentspace-demo-3f2a9c1e".to_owned(),
        user: "Ubuntu".to_owned(),
        expires_at: NOW + 3600,
    };
    fake.release_failures.set(1);
    fake.leftovers = vec![
        format!("~/.agentspace/leases/{ID_A}/id_ed25519"),
        "the host alias file agentspace-demo-3f2a9c1e".to_owned(),
    ];
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(fake)];
    let out = env.run(&["lease", "create", "--project", "demo"], &drivers, &[ID_A]);
    assert_eq!(out.exit_code, 1);
    assert_eq!(
        err(&out)["message"],
        format!(
            "agentspace lease create failed: The lease records could not be written: the change \
             is not valid. No lease was recorded, and these could not be removed: \
             ~/.agentspace/leases/{ID_A}/id_ed25519, the host alias file agentspace-demo-3f2a9c1e."
        )
    );
    assert_eq!(log_of(&log)[1], format!("release {ID_A}"));
    assert!(env.records().is_empty());
}

#[test]
fn create_records_an_unsatisfiable_lease_with_its_reasons() {
    let env = Env::new();
    let log = Log::default();
    let mut fake = Fake::new("fake", &log);
    fake.placed = Placed::Unsatisfiable {
        reasons: vec![Reason {
            code: "unsupported_size",
            message: "Size 'xxl' is not supported; v0 supports sizes s, m, l and xl.".to_owned(),
        }],
    };
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(fake)];
    let out = env.run(
        &["lease", "create", "--project", "demo", "--size", "XXL"],
        &drivers,
        &[ID_A],
    );
    let d = doc(&out);
    assert_eq!(d["state"], "unsatisfiable");
    assert_eq!(d["size"], "xxl");
    assert_eq!(env.records()[0].driver.as_deref(), Some("fake"));
}

#[test]
fn create_with_a_state_folder_and_no_home_reads_the_records_then_fails_with_no_home() {
    let env = Env::without_home();
    let log = Log::default();
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(Fake::new("fake", &log))];
    let out = env.run(&["lease", "create", "--project", "demo"], &drivers, &[ID_A]);
    assert!(
        err(&out)["message"]
            .as_str()
            .unwrap()
            .contains("The home folder is not set")
    );
    assert!(log_of(&log).is_empty());
}

// get and list

#[test]
fn get_shows_a_ready_lease_past_its_expiry_as_expired_without_ssh_or_driver() {
    let env = Env::new();
    env.put(vec![expired(ID_A)]);
    let d = doc(&env.run(&["lease", "get", &ID_A.to_uppercase()], &[], &[]));
    assert_eq!(d["state"], "expired");
    assert!(d.get("ssh").is_none() && d.get("driver").is_none());
}

#[test]
fn get_and_list_need_no_home_when_the_state_folder_is_set() {
    let env = Env::without_home();
    env.put(vec![record(ID_A, "pending", None)]);
    assert_eq!(
        doc(&env.run(&["lease", "get", ID_A], &[], &[]))["state"],
        "pending"
    );
    let d = doc(&env.run(&["lease", "list", "--state", "pending"], &[], &[]));
    assert_eq!(d["leases"].as_array().unwrap().len(), 1);
}

#[test]
fn get_and_list_do_not_take_the_lock() {
    let env = Env::new();
    env.put(vec![record(ID_A, "pending", None)]);
    let _held = agentspace_state::lock(&env.state_dir()).unwrap();
    let start = std::time::Instant::now();
    assert_eq!(env.run(&["lease", "get", ID_A], &[], &[]).exit_code, 0);
    assert_eq!(env.run(&["lease", "list"], &[], &[]).exit_code, 0);
    assert!(start.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn get_of_an_unknown_id_is_not_found() {
    let env = Env::new();
    let out = env.run(&["lease", "get", ID_A], &[], &[]);
    assert_eq!(out.exit_code, 3);
    assert_eq!(err(&out)["message"], format!("No lease has ID '{ID_A}'."));
}

#[test]
fn list_checks_the_project_before_the_state() {
    let env = Env::new();
    let out = env.run(
        &["lease", "list", "--state", "nope", "--project", "-x"],
        &[],
        &[],
    );
    assert_eq!(err(&out)["field"], "project");
}

#[test]
fn list_decides_every_state_at_one_instant() {
    let env = Env::new();
    env.put(vec![expired(ID_A), record(ID_B, "ready", Some("fake"))]);
    let calls = Cell::new(0);
    let vars = env.vars.clone();
    let var = move |name: &str| vars.get(name).cloned();
    let now = || {
        calls.set(calls.get() + 1);
        NOW
    };
    let new_id = String::new;
    let world = World {
        var: &var,
        cwd: Some(env.dir.path()),
        own_exe: Path::new("agentspace"),
        now: &now,
        new_id: &new_id,
        drivers: &[],
    };
    let out = execute_in(
        &[
            "lease".into(),
            "list".into(),
            "--state".into(),
            "ready".into(),
        ],
        &world,
    );
    let d = doc(&out);
    assert_eq!(d["leases"].as_array().unwrap().len(), 1);
    assert_eq!(calls.get(), 1);
}

// renew

#[test]
fn renew_writes_the_record_then_commits() {
    let env = Env::new();
    env.put(vec![record(ID_A, "ready", Some("fake"))]);
    let log = Log::default();
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(Fake::new("fake", &log))];
    let d = doc(&env.run(&["lease", "renew", ID_A, "--ttl", "2h"], &drivers, &[]));
    assert_eq!(d["expires_at"], format_time(NOW + 7200));
    assert_eq!(d["ttl_seconds"], 7200);
    assert_eq!(
        log_of(&log),
        vec![
            format!("renew {ID_A} 7200"),
            "commit".to_owned(),
            "renewal dropped".to_owned()
        ]
    );
    assert_eq!(env.records()[0].ttl_seconds, 7200);
}

#[test]
fn renew_without_a_ttl_uses_the_leases_own() {
    let env = Env::new();
    env.put(vec![record(ID_A, "ready", Some("fake"))]);
    let log = Log::default();
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(Fake::new("fake", &log))];
    doc(&env.run(&["lease", "renew", ID_A], &drivers, &[]));
    assert_eq!(log_of(&log)[0], format!("renew {ID_A} 3600"));
}

fn block_the_state_file(state_dir: &Path) {
    let file = state_dir.join("leases.json");
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();
}

#[test]
fn renew_rolls_back_when_the_record_cannot_be_written() {
    let env = Env::new();
    env.put(vec![record(ID_A, "ready", Some("fake"))]);
    let log = Log::default();
    let mut fake = Fake::new("fake", &log);
    let dir = env.state_dir();
    fake.on_renew = Box::new(move || block_the_state_file(&dir));
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(fake)];
    let out = env.run(&["lease", "renew", ID_A], &drivers, &[]);
    assert_eq!(out.exit_code, 1);
    assert_eq!(
        err(&out)["message"],
        "The lease records could not be written: a folder is where the state file belongs. \
         Nothing was changed."
    );
    assert_eq!(
        log_of(&log)[1..],
        ["rollback".to_owned(), "renewal dropped".to_owned()]
    );
}

#[test]
fn renew_says_so_when_the_rollback_fails_too() {
    let env = Env::new();
    env.put(vec![record(ID_A, "ready", Some("fake"))]);
    let log = Log::default();
    let mut fake = Fake::new("fake", &log);
    fake.rollback_fails = true;
    let dir = env.state_dir();
    fake.on_renew = Box::new(move || block_the_state_file(&dir));
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(fake)];
    let out = env.run(&["lease", "renew", ID_A], &drivers, &[]);
    assert_eq!(
        err(&out)["message"],
        "agentspace lease renew failed: The lease records could not be written: a folder is \
         where the state file belongs. Its previous certificate could not be put back; renew the \
         lease again."
    );
}

#[test]
fn a_ready_record_without_a_driver_is_the_default_drivers() {
    let env = Env::new();
    env.put(vec![record(ID_A, "ready", None)]);
    let log = Log::default();
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(Fake::new("ec2", &log))];
    doc(&env.run(&["lease", "renew", ID_A], &drivers, &[]));
    doc(&env.run(&["lease", "release", ID_A], &drivers, &[]));
    // Neither renew nor release adds the field.
    assert_eq!(env.records()[0].driver, None);
    assert_eq!(log_of(&log).len(), 5);
}

#[test]
fn a_driver_this_build_does_not_have_fails_without_its_name() {
    let env = Env::new();
    env.put(vec![record(ID_A, "ready", Some("secretname"))]);
    let before = std::fs::read(env.state_dir().join("leases.json")).unwrap();
    for sub in ["renew", "release"] {
        let out = env.run(&["lease", sub, ID_A], &[], &[]);
        assert_eq!(out.exit_code, 1);
        assert_eq!(
            err(&out)["message"],
            format!(
                "agentspace lease {sub} failed: The lease was placed by a driver this agentspace \
                 does not have. Nothing was changed."
            )
        );
    }
    assert_eq!(
        std::fs::read(env.state_dir().join("leases.json")).unwrap(),
        before
    );
}

#[test]
fn renew_with_a_state_folder_and_no_home_finds_its_errors_first() {
    let env = Env::without_home();
    env.put(vec![record(ID_B, "pending", None)]);
    assert_eq!(env.run(&["lease", "renew", ID_A], &[], &[]).exit_code, 3);
    assert_eq!(env.run(&["lease", "renew", ID_B], &[], &[]).exit_code, 4);
}

// release

#[test]
fn release_records_released_before_it_deletes_and_a_retry_finishes() {
    let env = Env::new();
    env.put(vec![record(ID_A, "ready", Some("fake"))]);
    let log = Log::default();
    let fake = Fake::new("fake", &log);
    fake.release_failures.set(1);
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(fake)];
    let out = env.run(&["lease", "release", ID_A], &drivers, &[]);
    assert_eq!(out.exit_code, 1);
    assert!(
        err(&out)["message"]
            .as_str()
            .unwrap()
            .starts_with("agentspace lease release failed: The lease's local key")
    );
    assert_eq!(env.records()[0].state, "released");
    assert_eq!(
        log_of(&log),
        vec![format!("check_release {ID_A}"), format!("release {ID_A}")]
    );
    let first = env.records()[0].released_at.clone();

    let d = doc(&env.run(&["lease", "release", ID_A], &drivers, &[]));
    assert_eq!(d["state"], "released");
    assert_eq!(d["released_at"].as_str(), first.as_deref());
    assert!(d.get("ssh").is_none() && d.get("expires_at").is_some());
    assert_eq!(log_of(&log).len(), 4);
}

#[test]
fn release_of_a_pending_lease_asks_no_driver_and_needs_no_home() {
    let env = Env::without_home();
    env.put(vec![record(ID_A, "pending", None)]);
    let d = doc(&env.run(&["lease", "release", ID_A], &[], &[]));
    assert_eq!(d["state"], "released");
    assert_eq!(d["released_at"], format_time(NOW));
    assert!(d.get("expires_at").is_none());
}

#[test]
fn release_of_an_unsatisfiable_lease_changes_nothing() {
    let env = Env::without_home();
    env.put(vec![record(ID_A, "unsatisfiable", Some("gone"))]);
    let before = std::fs::read(env.state_dir().join("leases.json")).unwrap();
    let d = doc(&env.run(&["lease", "release", ID_A], &[], &[]));
    assert_eq!(d["state"], "unsatisfiable");
    assert_eq!(
        std::fs::read(env.state_dir().join("leases.json")).unwrap(),
        before
    );
}

#[test]
fn release_of_a_ready_lease_with_no_home_fails_with_no_home() {
    let env = Env::without_home();
    env.put(vec![record(ID_A, "ready", Some("fake"))]);
    let log = Log::default();
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(Fake::new("fake", &log))];
    let out = env.run(&["lease", "release", ID_A], &drivers, &[]);
    assert!(
        err(&out)["message"]
            .as_str()
            .unwrap()
            .starts_with("agentspace lease release failed: The home folder is not set")
    );
    assert_eq!(env.records()[0].state, "ready");
}

/// The build holds the EC2 driver, and only it (E0.05-T04-S01): without the registration, no
/// command can place a lease.
#[test]
fn the_build_holds_the_ec2_driver() {
    let names: Vec<&str> = super::drivers().iter().map(|d| d.name()).collect();
    assert_eq!(names, vec!["ec2"]);
}

// check_pending, release of a pending lease, and the operator program

/// The log's `check_pending` lines.
fn checks(log: &Log) -> Vec<String> {
    log_of(log)
        .into_iter()
        .filter(|l| l.starts_with("check_pending"))
        .collect()
}

#[test]
fn get_of_a_pending_lease_checks_it_and_never_writes_the_state_file() {
    let env = Env::new();
    env.put(vec![record(ID_A, "pending", Some("fake"))]);
    let before = std::fs::read(env.state_dir().join("leases.json")).unwrap();
    let log = Log::default();
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(Fake::new("fake", &log))];
    let out = env.run(&["lease", "get", ID_A], &drivers, &[]);
    assert_eq!(doc(&out)["state"], "pending");
    assert_eq!(
        checks(&log),
        vec![format!(
            "check_pending {ID_A} demo operator=Some(Some(\"agentspace-operator.exe\")) \
             proxy=None ssh=Some(\".ssh\")"
        )]
    );
    assert_eq!(
        std::fs::read(env.state_dir().join("leases.json")).unwrap(),
        before
    );
}

#[test]
fn get_of_a_pending_lease_passes_the_transport_override_and_ssh_folder() {
    let mut env = Env::new();
    env.vars
        .insert("AGENTSPACE_PROXY_COMMAND".to_owned(), "stand-in ssh".into());
    env.vars.insert(
        "AGENTSPACE_SSH_DIR".to_owned(),
        env.dir.path().join("sshdir").into_os_string(),
    );
    env.put(vec![record(ID_A, "pending", Some("fake"))]);
    let log = Log::default();
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(Fake::new("fake", &log))];
    env.run(&["lease", "get", ID_A], &drivers, &[]);
    let line = checks(&log).remove(0);
    assert!(line.contains("proxy=Some(\"stand-in ssh\")"), "{line}");
    assert!(line.contains("ssh=Some(\"sshdir\")"), "{line}");
}

#[test]
fn get_drops_a_check_that_fails_or_panics() {
    for (fails, panics) in [(true, false), (false, true)] {
        let env = Env::new();
        env.put(vec![record(ID_A, "pending", Some("fake"))]);
        let log = Log::default();
        let mut fake = Fake::new("fake", &log);
        fake.check_fails = fails;
        fake.check_panics = panics;
        let drivers: Vec<Box<dyn Driver>> = vec![Box::new(fake)];
        let out = env.run(&["lease", "get", ID_A], &drivers, &[]);
        assert_eq!(out.exit_code, 0);
        assert!(out.stderr.is_empty());
        assert_eq!(doc(&out)["state"], "pending");
        assert_eq!(checks(&log).len(), 1);
    }
}

#[test]
fn get_checks_only_a_pending_lease_with_a_driver_this_build_has() {
    let env = Env::new();
    env.put(vec![
        record(ID_A, "ready", Some("fake")),
        record(ID_B, "pending", None),
    ]);
    let log = Log::default();
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(Fake::new("fake", &log))];
    env.run(&["lease", "get", ID_A], &drivers, &[]);
    env.run(&["lease", "get", ID_B], &drivers, &[]);
    env.put(vec![record(ID_A, "pending", Some("gone"))]);
    let out = env.run(&["lease", "get", ID_A], &drivers, &[]);
    assert_eq!(doc(&out)["state"], "pending");
    assert!(checks(&log).is_empty());
}

#[test]
fn get_prints_a_pending_lease_when_its_check_has_no_context() {
    // No home, so no context: the check is dropped and the lease is printed.
    let env = Env::without_home();
    env.put(vec![record(ID_A, "pending", Some("fake"))]);
    let log = Log::default();
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(Fake::new("fake", &log))];
    let out = env.run(&["lease", "get", ID_A], &drivers, &[]);
    assert_eq!(out.exit_code, 0);
    assert!(checks(&log).is_empty());
}

#[test]
fn list_never_checks_a_pending_lease() {
    let env = Env::new();
    env.put(vec![record(ID_A, "pending", Some("fake"))]);
    let log = Log::default();
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(Fake::new("fake", &log))];
    env.run(&["lease", "list"], &drivers, &[]);
    assert!(log_of(&log).is_empty());
}

#[test]
fn release_of_a_pending_lease_with_a_driver_hands_it_to_the_driver() {
    let env = Env::new();
    env.put(vec![record(ID_A, "pending", Some("fake"))]);
    let log = Log::default();
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(Fake::new("fake", &log))];
    let d = doc(&env.run(&["lease", "release", ID_A], &drivers, &[]));
    assert_eq!(d["state"], "released");
    assert_eq!(
        log_of(&log),
        vec![format!("check_release {ID_A}"), format!("release {ID_A}")]
    );
    assert_eq!(env.records()[0].state, "released");
}

#[test]
fn release_of_a_pending_lease_whose_delete_fails_stays_released_and_a_retry_finishes() {
    let env = Env::new();
    env.put(vec![record(ID_A, "pending", Some("fake"))]);
    let log = Log::default();
    let fake = Fake::new("fake", &log);
    fake.release_failures.set(1);
    let drivers: Vec<Box<dyn Driver>> = vec![Box::new(fake)];
    let out = env.run(&["lease", "release", ID_A], &drivers, &[]);
    assert_eq!(out.exit_code, 1);
    assert_eq!(env.records()[0].state, "released");
    let d = doc(&env.run(&["lease", "release", ID_A], &drivers, &[]));
    assert_eq!(d["state"], "released");
}

/// The Context's operator program in a world with the given variables, own executable and
/// working folder.
fn operator_program_of(
    vars: &[(&str, &str)],
    own_exe: &Path,
    cwd: Option<&Path>,
) -> Option<PathBuf> {
    let map: HashMap<String, OsString> = vars
        .iter()
        .map(|(k, v)| ((*k).to_owned(), OsString::from(*v)))
        .collect();
    let var = move |name: &str| map.get(name).cloned();
    let now = || NOW;
    let new_id = String::new;
    let world = World {
        var: &var,
        cwd,
        own_exe,
        now: &now,
        new_id: &new_id,
        drivers: &[],
    };
    super::support::context_operator_program(&world)
}

#[test]
fn the_operator_program_is_beside_the_cli_by_default() {
    let dir = tempfile::TempDir::new().unwrap();
    let exe = dir.path().join("bin").join("agentspace.exe");
    let beside = dir.path().join("bin").join("agentspace-operator.exe");
    assert_eq!(
        operator_program_of(&[], &exe, Some(dir.path())),
        Some(beside.clone())
    );
    // Empty is unset, and AGENTSPACE_OPERATOR_CMD is not read.
    assert_eq!(
        operator_program_of(
            &[
                ("AGENTSPACE_PROVISION_CMD", ""),
                ("AGENTSPACE_OPERATOR_CMD", "other.exe")
            ],
            &exe,
            Some(dir.path())
        ),
        Some(beside)
    );
}

#[test]
fn the_operator_program_is_agentspace_provision_cmd_made_full() {
    let dir = tempfile::TempDir::new().unwrap();
    let exe = dir.path().join("bin").join("agentspace.exe");
    let full = dir.path().join("stand-in.cmd");
    assert_eq!(
        operator_program_of(
            &[("AGENTSPACE_PROVISION_CMD", full.to_str().unwrap())],
            &exe,
            None
        ),
        Some(full)
    );
    assert_eq!(
        operator_program_of(
            &[("AGENTSPACE_PROVISION_CMD", "rel.cmd")],
            &exe,
            Some(dir.path())
        ),
        Some(dir.path().join("rel.cmd"))
    );
    assert_eq!(
        operator_program_of(&[("AGENTSPACE_PROVISION_CMD", "rel.cmd")], &exe, None),
        None
    );
}

/// The lease CLI's `--plan` (E0.10-T03-S01, #0219).
mod plan;
