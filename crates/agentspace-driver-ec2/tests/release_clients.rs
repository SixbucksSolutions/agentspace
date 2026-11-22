//! The EC2 driver's release ends the lease's connections (docs/driver.md, "Release a lease",
//! "Ending a lease's connections", "The order of a release", "Which processes are the alias's
//! clients", "The count", "Where the count is logged"; E0.08-T03-S04, #0180).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! **Process safety.** Every client here is a stand-in: a renamed copy of `tests/support/stub.rs`'s
//! program in a folder of its own, started with the command line a case stands for and ended by
//! the test's `Lab` when the test ends, whatever the assertions do. Each test builds its lease from
//! a UUIDv4 drawn at random for that test, so that no process of Terry's, and no other test's
//! stand-in, can match its alias. No test lists, reads or prints any other process or a command
//! line, and the throwaway home holds every file the release touches.
//!
//! **The walk is always scoped.** Every release here is made by `Ec2Driver::with_walk_under`, over
//! the folder of this test's stand-ins, so that only a process whose executable is in that folder
//! can be ended.
//!
//! **What these tests assume of the crate, beyond its public API:** a `[[bin]]` named
//! `agentspace-driver-ec2-stub` in `Cargo.toml`, with `test = false` and its source at
//! `tests/support/stub.rs`, and `Ec2Driver::release_pausing(ctx, lease, &mut dyn FnMut())`, the
//! release with a test-only pause between the first walk and the alias file's deletion, which is
//! there when the crate's `test-pause` feature is on (the integration tests turn it on).
#![cfg(windows)]

#[path = "common/clients.rs"]
mod clients;
mod common;

use agentspace_connections::line::release_text;
use agentspace_connections::{EndError, Identity};
use agentspace_driver_api::{Context, Driver, Failure, LeaseRef, PlaceRequest, Placed};
use agentspace_driver_ec2::Ec2Driver;
use clients::*;
use common::*;
use std::fs;
use std::path::{Path, PathBuf};

/// A lease for these tests: a throwaway home, a lease ID drawn at random, and the aliases.
struct Rel {
    fx: Fx,
    id: String,
    alias: String,
    other: String,
}

fn alias_of(id: &str) -> String {
    format!("agentspace-{PROJECT}-{}", &id[..8])
}

impl Rel {
    fn with(fx: Fx) -> Rel {
        let id = random_uuid_v4();
        let alias = alias_of(&id);
        let mut other = alias_of(&random_uuid_v4());
        while other == alias {
            other = alias_of(&random_uuid_v4());
        }
        fs::create_dir_all(&fx.ctx.state_dir).unwrap();
        Rel {
            fx,
            id,
            alias,
            other,
        }
    }

    /// A lease that `place` made `ready`.
    fn ready_in(fx: Fx, ttl: u32) -> Rel {
        let rel = Rel::with(fx);
        let req = PlaceRequest {
            id: &rel.id,
            project: PROJECT,
            platform: "linux/arm64",
            profile: "agent-linux",
            size: "m",
            ttl_seconds: ttl,
        };
        match rel.fx.driver().place(&rel.fx.ctx, &req) {
            Ok(Placed::Ready { host_alias, .. }) => assert_eq!(host_alias, rel.alias),
            other => panic!("Expected a ready place, got {other:?}"),
        }
        rel
    }

    fn ready(ttl: u32) -> Rel {
        Rel::ready_in(Fx::with_ca(), ttl)
    }

    /// A lease that was never placed, as a `pending` one is: it has no files.
    fn pending() -> Rel {
        Rel::with(Fx::without_ca())
    }

    fn lease(&self) -> LeaseRef<'_> {
        LeaseRef {
            id: &self.id,
            project: PROJECT,
        }
    }
    fn alias_file(&self) -> PathBuf {
        self.fx.alias_dir().join(&self.alias)
    }
    fn lease_dir(&self) -> PathBuf {
        self.fx.lease_dir(&self.id)
    }
    fn log_path(&self) -> PathBuf {
        self.fx.ctx.state_dir.join("release.log")
    }
    /// The release, walking only for processes whose executable is in the lab's folder.
    fn release(&self, lab: &Lab) -> Result<(), Failure> {
        Ec2Driver::new()
            .with_walk_under(&lab.dir)
            .release(&self.fx.ctx, &self.lease())
    }
    /// The same, with `pause` called once between the first walk and the alias file's deletion.
    /// `dir` is the lab's folder, which the closure needs the lab itself for.
    fn release_paused(&self, dir: &Path, pause: &mut dyn FnMut()) -> Result<(), Failure> {
        Ec2Driver::new()
            .with_walk_under(dir)
            .release_pausing(&self.fx.ctx, &self.lease(), pause)
    }

    /// The log's lines, each of which MUST be one whole line of the form
    /// `<UTC time> lease=<id> <text>` with a time in `[since, until]` (to the second).
    /// Returns each line's text.
    fn log_texts(&self, since: i64, until: i64) -> Vec<String> {
        let bytes = fs::read(self.log_path()).expect("release.log exists");
        assert!(
            !bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
            "The log has a byte order mark"
        );
        assert!(
            bytes.is_empty() || bytes.ends_with(b"\n"),
            "A line ends in LF"
        );
        assert!(!bytes.contains(&b'\r'), "A line ends in LF alone");
        let text = String::from_utf8(bytes).expect("The log is UTF-8");
        let prefix = format!(" lease={} ", self.id);
        text.split_terminator('\n')
            .map(|line| {
                assert!(line.len() > 20, "The line is too short");
                let time = parse_utc(&line[..20])
                    .unwrap_or_else(|| panic!("A line's time is not YYYY-MM-DDTHH:MM:SSZ"));
                assert!(
                    time >= since - 1 && time <= until + 1,
                    "The line's time is not the release's"
                );
                let rest = &line[20..];
                let Some(text) = rest.strip_prefix(&prefix) else {
                    panic!("The line does not name the lease after the time");
                };
                assert!(!line.contains(&self.alias), "The line holds the alias");
                text.to_string()
            })
            .collect()
    }

    /// The one line of the log, as text.
    fn one_text(&self, since: i64, until: i64) -> String {
        let mut texts = self.log_texts(since, until);
        assert_eq!(
            texts.len(),
            1,
            "The log holds {} lines, not one",
            texts.len()
        );
        texts.remove(0)
    }
}

fn ok(n: usize) -> Result<usize, EndError> {
    Ok(n)
}

// ---- A ready lease, an expired one and a pending one ----

#[test]
fn a_ready_leases_release_ends_the_aliass_clients_and_only_them() {
    let rel = Rel::ready(600);
    let mut lab = Lab::new();
    let (a, b) = (rel.alias.clone(), rel.other.clone());

    // The clients of the alias, in the forms driver.md's table of examples gives.
    let ssh = lab.start("ssh.exe", &[&a]);
    let ssh_options = lab.start(
        "ssh.exe",
        &[
            "-T",
            "-o",
            "ProxyCommand=x y",
            "-o",
            "ClearAllForwardings=yes",
            &a,
        ],
    );
    let ssh_user = lab.start("ssh.exe", &[&format!("ubuntu@{a}")]);
    let proxy = lab.start("agentspace-ssh-proxy.exe", &[&a]);
    let clients = [ssh, ssh_options, ssh_user, proxy];

    // Not clients: another alias's, another image's, the alias as an option's value or as a
    // remote command, a proxy with more than the alias, and a longer alias.
    let ssh_other = lab.start("ssh.exe", &[&b]);
    let proxy_other = lab.start("agentspace-ssh-proxy.exe", &[&b]);
    let other_image = lab.start("claude-ssh-proxy.exe", &[&a]);
    let option_value = lab.start("ssh.exe", &["-J", &a, "elsewhere"]);
    let remote_command = lab.start("ssh.exe", &["elsewhere", &a]);
    let proxy_extra = lab.start("agentspace-ssh-proxy.exe", &[&a, "extra"]);
    let longer = lab.start("ssh.exe", &[&format!("{a}0")]);
    let spared = [
        ssh_other,
        proxy_other,
        other_image,
        option_value,
        remote_command,
        proxy_extra,
        longer,
    ];

    let (since, until) = {
        let since = unix_now();
        rel.release(&lab).expect("The release succeeds");
        (since, unix_now())
    };

    for (n, i) in clients.iter().enumerate() {
        assert!(!lab.alive(*i), "Client {n} is still running");
        assert_eq!(
            lab.exit_code(*i),
            Some(1),
            "Client {n} was not given exit code 1"
        );
    }
    for (n, i) in spared.iter().enumerate() {
        assert!(lab.alive(*i), "Process {n}, no client, was ended");
    }
    assert!(!rel.alias_file().exists());
    assert!(!rel.lease_dir().exists());
    assert_eq!(
        rel.one_text(since, until),
        release_text(&ok(4), &ok(0)),
        "The line counts four clients ended by the first walk and none by the second"
    );
}

#[test]
fn a_client_is_ended_with_its_tree_and_counted_once() {
    let rel = Rel::ready(600);
    let mut lab = Lab::new();
    let proxy = lab.copy_of("agentspace-ssh-proxy.exe");
    let other = lab.copy_of("node.exe");
    let client = lab.start_with(
        "ssh.exe",
        &[&rel.alias],
        Opts {
            // A client inside the client's tree, and a child of any name.
            children: vec![(proxy, vec![rel.alias.clone()]), (other, vec![])],
            ..Opts::default()
        },
    );
    let (since, until) = (unix_now(), {
        rel.release(&lab).unwrap();
        unix_now()
    });
    assert!(!lab.alive(client));
    for (n, h) in lab.children(client).iter().enumerate() {
        assert!(!h.alive(), "Child {n} of the client is still running");
    }
    assert_eq!(rel.one_text(since, until), release_text(&ok(1), &ok(0)));
}

#[test]
fn an_expired_leases_release_does_the_same() {
    // The driver holds no clock and reads no certificate, so a lease that has expired is one
    // whose certificate is of no use: here, one that is no certificate at all.
    let rel = Rel::ready(60);
    fs::write(rel.lease_dir().join("id_ed25519-cert.pub"), b"expired\n").unwrap();
    let mut lab = Lab::new();
    let client = lab.start("ssh.exe", &[&rel.alias]);
    let other = lab.start("ssh.exe", &[&rel.other]);
    let since = unix_now();
    rel.release(&lab).expect("The release succeeds");
    let until = unix_now();
    assert!(!lab.alive(client));
    assert!(lab.alive(other));
    assert!(!rel.alias_file().exists());
    assert!(!rel.lease_dir().exists());
    assert_eq!(rel.one_text(since, until), release_text(&ok(1), &ok(0)));
}

#[test]
fn a_pending_leases_release_ends_nothing_and_logs_that() {
    // A pending lease has no files, and its alias was never handed to anyone. The driver cannot
    // tell it from a ready one, so a client of another alias is what shows nothing was ended.
    let rel = Rel::pending();
    let mut lab = Lab::new();
    let other = lab.start("ssh.exe", &[&rel.other]);
    let proxy_other = lab.start("agentspace-ssh-proxy.exe", &[&rel.other]);
    let since = unix_now();
    rel.release(&lab).expect("The release succeeds");
    let until = unix_now();
    assert!(lab.alive(other));
    assert!(lab.alive(proxy_other));
    assert!(!rel.lease_dir().exists());
    assert!(!rel.alias_file().exists());
    assert_eq!(rel.one_text(since, until), release_text(&ok(0), &ok(0)));
    assert_eq!(
        release_text(&ok(0), &ok(0)),
        "Ended 0 of the lease's connections.",
        "The text of a release that found no client"
    );
}

// ---- The line ----

#[test]
fn each_release_appends_one_line_and_a_repeat_ends_a_later_client() {
    let rel = Rel::ready(600);
    let mut lab = Lab::new();
    let first = lab.start("ssh.exe", &[&rel.alias]);
    let t0 = unix_now();
    rel.release(&lab).unwrap();
    assert!(!lab.alive(first));
    assert_eq!(rel.one_text(t0, unix_now()), release_text(&ok(1), &ok(0)));

    // A client that appears after the release, and the release repeated (the lease is `released`).
    let later = lab.start("ssh.exe", &[&rel.alias]);
    rel.release(&lab).expect("A repeated release succeeds");
    assert!(!lab.alive(later));
    let texts = rel.log_texts(t0, unix_now());
    assert_eq!(
        texts,
        vec![release_text(&ok(1), &ok(0)), release_text(&ok(1), &ok(0))]
    );

    // And one more, with nothing to end.
    rel.release(&lab).unwrap();
    let texts = rel.log_texts(t0, unix_now());
    assert_eq!(texts.len(), 3);
    assert_eq!(texts[2], release_text(&ok(0), &ok(0)));
}

#[test]
fn a_release_log_that_cannot_be_written_does_not_fail_the_release() {
    let rel = Rel::ready(600);
    fs::create_dir_all(rel.log_path()).unwrap();
    let mut lab = Lab::new();
    let client = lab.start("ssh.exe", &[&rel.alias]);
    rel.release(&lab)
        .expect("The release succeeds without its log");
    assert!(!lab.alive(client));
    assert!(
        rel.log_path().is_dir(),
        "The folder at the log's place was replaced"
    );
    assert!(!rel.alias_file().exists());
    assert!(!rel.lease_dir().exists());
}

// ---- A refused release, and the check step ----

#[test]
fn a_foreign_alias_file_ends_nothing_logs_nothing_and_deletes_nothing() {
    let rel = Rel::ready(600);
    let foreign = format!("# agentspace lease {OTHER_ID}; written by agentspace\nHost x\n");
    fs::write(rel.alias_file(), foreign.as_bytes()).unwrap();
    let mut lab = Lab::new();
    let client = lab.start("ssh.exe", &[&rel.alias]);
    let before = rel.fx.snapshot();

    let f = rel
        .release(&lab)
        .expect_err("A foreign alias file MUST be refused");
    rel.fx.assert_failure_plain(&f);
    assert!(lab.alive(client), "A client was ended by a refused release");
    assert!(!rel.log_path().exists(), "A refused release logged");
    assert_eq!(rel.fx.snapshot(), before);

    let f = rel
        .fx
        .driver()
        .check_release(&rel.fx.ctx, &rel.lease())
        .expect_err("The check step refuses it too");
    rel.fx.assert_failure_plain(&f);
    assert!(lab.alive(client));
    assert!(!rel.log_path().exists());
}

#[test]
fn the_check_step_ends_nothing_and_logs_nothing() {
    let rel = Rel::ready(600);
    let mut lab = Lab::new();
    let client = lab.start("ssh.exe", &[&rel.alias]);
    let before = rel.fx.snapshot();
    rel.fx
        .driver()
        .check_release(&rel.fx.ctx, &rel.lease())
        .unwrap();
    assert!(lab.alive(client));
    assert!(!rel.log_path().exists());
    assert_eq!(rel.fx.snapshot(), before);
}

// ---- The order: the first walk, the alias's deletion, the second walk ----

#[test]
fn the_clients_are_ended_before_the_alias_file_is_deleted() {
    let rel = Rel::ready(600);
    let mut lab = Lab::new();
    let log = lab.dir.join("client.log");
    let client = lab.start_with(
        "ssh.exe",
        &[&rel.alias],
        Opts {
            watch: Some((rel.alias_file(), log.clone())),
            ..Opts::default()
        },
    );
    rel.release(&lab).unwrap();
    assert!(!lab.alive(client));
    assert_eq!(
        lines_of(&log),
        vec!["exists".to_string()],
        "The stand-in saw the alias file gone before it was ended"
    );
    assert!(!rel.alias_file().exists());
    assert!(!rel.lease_dir().exists());
}

#[test]
fn the_second_walk_ends_a_client_started_after_the_first_and_both_are_counted() {
    let rel = Rel::ready(600);
    let mut lab = Lab::new();
    let dir = lab.dir.clone();
    let log = lab.dir.join("later.log");
    let first = lab.start("ssh.exe", &[&rel.alias]);
    let other = lab.start("ssh.exe", &[&rel.other]);

    let mut at_pause = Vec::new();
    let mut later = None;
    let since = unix_now();
    let result = {
        let mut pause = || {
            // The first walk has ended its client, and the alias file is still there.
            at_pause.push((lab.alive(first), rel.alias_file().is_file()));
            // A client that comes back through the alias in the gap, and watches the alias file.
            later = Some(lab.start_with(
                "ssh.exe",
                &[&rel.alias],
                Opts {
                    watch: Some((rel.alias_file(), log.clone())),
                    ..Opts::default()
                },
            ));
        };
        rel.release_paused(&dir, &mut pause)
    };
    let until = unix_now();
    result.expect("The release succeeds");

    assert_eq!(
        at_pause,
        vec![(false, true)],
        "The pause is called once, after the first walk ended its client and with the alias file there"
    );
    let later = later.expect("The pause was called");
    assert!(!lab.alive(later), "The second walk did not end the client");
    assert_eq!(lab.exit_code(later), Some(1));
    assert!(lab.alive(other), "A client of another alias was ended");
    assert_eq!(
        lines_of(&log),
        vec!["exists".to_string(), "missing".to_string()],
        "The alias file MUST be deleted after the first walk and before the second"
    );
    assert!(!rel.alias_file().exists());
    assert!(!rel.lease_dir().exists());
    assert_eq!(
        rel.one_text(since, until),
        release_text(&ok(1), &ok(1)),
        "One line counts both walks"
    );
}

#[test]
fn a_refused_release_never_calls_the_pause_and_ends_nothing() {
    let rel = Rel::ready(600);
    fs::write(
        rel.alias_file(),
        format!("# agentspace lease {OTHER_ID}; written by agentspace\nHost x\n"),
    )
    .unwrap();
    let mut lab = Lab::new();
    let dir = lab.dir.clone();
    let client = lab.start("ssh.exe", &[&rel.alias]);
    let mut calls = 0;
    let result = rel.release_paused(&dir, &mut || calls += 1);
    assert!(result.is_err());
    assert_eq!(calls, 0, "No walk was made, so there was no gap");
    assert!(lab.alive(client));
    assert!(!rel.log_path().exists());
}

#[test]
fn a_failed_alias_deletion_still_has_its_second_walk_and_its_line() {
    // The alias file's folder is outside the home, and a deny rule put on it in the pause makes the
    // deletion fail. A client that starts in the pause is ended by the second walk all the same.
    let rel = Rel::ready_in(Fx::with_ca_ssh_elsewhere(), 600);
    let mut lab = Lab::new();
    let dir = lab.dir.clone();
    let first = lab.start("ssh.exe", &[&rel.alias]);

    let mut deny = None;
    let mut later = None;
    let since = unix_now();
    let result = {
        let mut pause = || {
            later = Some(lab.start("ssh.exe", &[&rel.alias]));
            deny = Some(Deny::new(&rel.fx.alias_dir(), "(OI)(CI)(DE,DC)"));
        };
        rel.release_paused(&dir, &mut pause)
    };
    let until = unix_now();
    let f = result.expect_err("A deletion that fails fails the release");
    rel.fx.assert_failure_plain(&f);
    assert!(!f.leftovers.is_empty());

    let later = later.expect("The pause was called");
    assert!(!lab.alive(first));
    assert!(
        !lab.alive(later),
        "The second walk runs when the deletion failed"
    );
    assert_eq!(rel.one_text(since, until), release_text(&ok(1), &ok(1)));
    assert!(rel.alias_file().is_file());
    assert!(rel.lease_dir().join("id_ed25519").is_file());

    // A retried release ends any client that appeared since, and finishes.
    deny.take().expect("The deny was made").lift();
    let again = lab.start("ssh.exe", &[&rel.alias]);
    rel.release(&lab).expect("The retried release succeeds");
    assert!(!lab.alive(again));
    assert!(!rel.alias_file().exists());
    assert!(!rel.lease_dir().exists());
    assert_eq!(
        rel.log_texts(since, unix_now()),
        vec![release_text(&ok(1), &ok(1)), release_text(&ok(1), &ok(0))]
    );
}

#[test]
fn a_failed_alias_deletion_ends_the_clients_logs_its_line_and_keeps_the_key() {
    // The alias file's folder is outside the home, and a deny rule on it makes the deletion fail.
    let rel = Rel::ready_in(Fx::with_ca_ssh_elsewhere(), 600);
    let mut lab = Lab::new();
    let client = lab.start("ssh.exe", &[&rel.alias]);
    let deny = Deny::new(&rel.fx.alias_dir(), "(OI)(CI)(DE,DC)");

    let since = unix_now();
    let f = rel
        .release(&lab)
        .expect_err("A deletion that fails fails the release");
    let until = unix_now();
    rel.fx.assert_failure_plain(&f);
    assert!(!f.leftovers.is_empty());
    assert!(!lab.alive(client), "The clients the walks ended stay ended");
    assert_eq!(
        rel.one_text(since, until),
        release_text(&ok(1), &ok(0)),
        "The line is written though the alias file could not be deleted"
    );
    // The alias file stays, so the key it points at stays.
    assert!(rel.alias_file().is_file());
    assert!(rel.lease_dir().join("id_ed25519").is_file());

    // A retried release ends any client that appeared since, and finishes.
    deny.lift();
    let again = lab.start("ssh.exe", &[&rel.alias]);
    rel.release(&lab).expect("The retried release succeeds");
    assert!(!lab.alive(again));
    assert!(!rel.alias_file().exists());
    assert!(!rel.lease_dir().exists());
    assert_eq!(
        rel.log_texts(since, unix_now()),
        vec![release_text(&ok(1), &ok(0)), release_text(&ok(1), &ok(0))]
    );
}

// ---- A client that cannot be ended ----

/// What the child process of the next test needs: it is told by these variables, which are the
/// throwaway home's paths and the lease, and none is the real profile's.
const CHILD_RESULT: &str = "ASDRV_RELEASE_CHILD_RESULT";
const CHILD_HOME: &str = "ASDRV_RELEASE_CHILD_HOME";
const CHILD_SSH: &str = "ASDRV_RELEASE_CHILD_SSH";
const CHILD_STATE: &str = "ASDRV_RELEASE_CHILD_STATE";
const CHILD_EXE: &str = "ASDRV_RELEASE_CHILD_EXE";
const CHILD_ID: &str = "ASDRV_RELEASE_CHILD_ID";
const CHILD_UNDER: &str = "ASDRV_RELEASE_CHILD_UNDER";

/// Not a test of its own: the release the next test runs as a descendant of a stand-in client. It
/// passes at once when it is not told to release, as in every ordinary run.
#[test]
fn spared_child_runs_the_release() {
    let Some(result) = std::env::var_os(CHILD_RESULT).map(PathBuf::from) else {
        return;
    };
    let var = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .expect("A variable")
    };
    let home = var(CHILD_HOME);
    assert!(
        home.starts_with(std::env::temp_dir()),
        "The home is not under the temp folder"
    );
    let ctx = Context {
        home,
        ssh_dir: var(CHILD_SSH),
        proxy_override: None,
        own_exe: var(CHILD_EXE),
        system_root: Some(system_root()),
        state_dir: var(CHILD_STATE),
        operator_program: None,
        state_dir_env: None,
        ssh_dir_env: None,
    };
    let id = var(CHILD_ID).to_string_lossy().into_owned();
    let lease = LeaseRef {
        id: &id,
        project: PROJECT,
    };
    let driver = Ec2Driver::new().with_walk_under(var(CHILD_UNDER));
    let outcome = match driver.release(&ctx, &lease) {
        Ok(()) => "ok",
        Err(_) => "err",
    };
    fs::write(result, outcome).unwrap();
}

#[test]
fn a_release_run_below_a_client_spares_it_and_its_tree_and_counts_it_once() {
    let rel = Rel::ready(600);
    let mut lab = Lab::new();
    let result = lab.dir.join("child-result.txt");
    let own_test_exe = std::env::current_exe().expect("The test program's own path");
    let sibling = lab.copy_of("sibling.exe");
    let env = [
        (CHILD_RESULT, result.clone()),
        (CHILD_HOME, rel.fx.home.clone()),
        (CHILD_SSH, rel.fx.ssh_dir.clone()),
        (CHILD_STATE, rel.fx.ctx.state_dir.clone()),
        (CHILD_EXE, rel.fx.ctx.own_exe.clone()),
        (CHILD_ID, PathBuf::from(&rel.id)),
        (CHILD_UNDER, lab.dir.clone()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string_lossy().into_owned()))
    .collect();

    let since = unix_now();
    // The client has two children: the release, and a process of any name beside it.
    let client = lab.start_with(
        "ssh.exe",
        &[&rel.alias],
        Opts {
            children: vec![
                (
                    own_test_exe,
                    vec![
                        "--exact".to_string(),
                        "spared_child_runs_the_release".to_string(),
                        "--test-threads=1".to_string(),
                    ],
                ),
                (sibling, vec![]),
            ],
            env,
            home: Some(rel.fx.home.clone()),
            ..Opts::default()
        },
    );
    wait_for(
        || fs::read_to_string(&result).is_ok_and(|t| !t.is_empty()),
        "The release below the client",
    );
    let until = unix_now();
    assert_eq!(
        fs::read_to_string(&result).unwrap(),
        "ok",
        "The release succeeds though a client could not be ended"
    );
    wait_for(
        || !lab.children(client)[0].alive(),
        "The release's own process's exit",
    );

    assert!(
        lab.alive(client),
        "The release ended an ancestor of its own process"
    );
    assert!(
        lab.children(client)[1].alive(),
        "The release ended a process beside it under a spared client"
    );
    assert_eq!(
        lab.children(client)[0].exit_code(),
        Some(0),
        "The release's own process was ended"
    );
    assert!(
        !rel.alias_file().exists(),
        "The alias file is deleted all the same"
    );
    assert!(!rel.lease_dir().exists());

    // Both walks find the client, and the release counts it once.
    let left = || EndError::NotAllEnded {
        ended: 0,
        not_ended: vec![Identity { pid: 1, created: 1 }],
    };
    let text = rel.one_text(since, until);
    assert_eq!(text, release_text(&Err(left()), &Err(left())));
    assert_eq!(
        text, "Ended 0 of the lease's connections; 1 could not be ended.",
        "The text of one client that could not be ended"
    );
}
