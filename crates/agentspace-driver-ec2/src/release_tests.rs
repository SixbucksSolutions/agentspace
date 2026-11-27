//! The EC2 driver's release, in order and with its two walks (docs/driver.md, "The order of a
//! release", "Where the count is logged" and "How it is checked"; E0.08-T03-S04).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! **Process safety, and the rules every test here keeps** (driver.md, "How it is checked"):
//! - **No test makes a walk over the laptop's whole process list.** Every release here uses a
//!   `Walker` over a throwaway folder (`end_alias_clients_under`), so only a process whose
//!   executable is in that folder can be ended, and the stand-ins are the only such processes.
//! - **Every lease ID is a UUIDv4 drawn at random for the test**, and so is the second lease's,
//!   whose stand-in MUST be left running.
//! - **The stand-ins are copies of this test program** under the name `ssh.exe`, started with the
//!   alias as the destination and a filter that selects `stand_in_client` below, which does nothing
//!   unless its environment variable is set. Every one is held by a guard that ends it when the
//!   test ends, whatever the assertions do, and it ends itself after 2 minutes.
//! - **No test prints a command line or an environment.**

use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use agentspace_alias::alias_name;
#[cfg(windows)]
use agentspace_connections::line::release_text;
use agentspace_driver_api::{Context, LeaseRef};

use crate::ops::{release, release_pausing, remove_files};
use crate::walk::Walker;

/// Set only in a stand-in's environment, to make `stand_in_client` wait.
const STAND_IN: &str = "AGENTSPACE_EC2_RELEASE_TEST_STAND_IN";

/// The test a stand-in runs. In an ordinary test run it returns at once; in a stand-in, which has
/// `STAND_IN` set, it waits until it is ended, or for 2 minutes.
#[test]
fn stand_in_client() {
    if std::env::var_os(STAND_IN).is_none() {
        return;
    }
    let until = Instant::now() + Duration::from_secs(120);
    while Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A UUIDv4 drawn at random for this call, in its normalized form.
fn random_uuid() -> String {
    let mut bytes = [0u8; 16];
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for (i, chunk) in bytes.chunks_mut(8).enumerate() {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u128(nanos);
        hasher.write_u32(std::process::id());
        hasher.write_usize(i);
        chunk.copy_from_slice(&hasher.finish().to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// A stand-in process that its guard ends when it is dropped.
struct Stand(Child);

impl Drop for Stand {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Stand {
    /// Whether the process has ended, within `wait`.
    fn ended_within(&mut self, wait: Duration) -> bool {
        let until = Instant::now() + wait;
        loop {
            if matches!(self.0.try_wait(), Ok(Some(_))) {
                return true;
            }
            if Instant::now() >= until {
                return false;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

/// A throwaway world: a home, an `.ssh` folder, a state folder and the folder the walks are
/// scoped to, which holds a copy of this test program named `ssh.exe`.
struct World {
    _root: tempfile::TempDir,
    ctx: Context,
    scope: PathBuf,
    id: String,
    alias: String,
}

impl World {
    fn new() -> World {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let state = root.path().join("state");
        let scope = root.path().join("scope");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&state).unwrap();
        fs::create_dir_all(&scope).unwrap();
        let ctx = Context {
            home: home.clone(),
            ssh_dir: home.join(".ssh"),
            proxy_override: None,
            system_root: None,
            own_exe: root.path().join("bin").join("agentspace.exe"),
            state_dir: state,
            operator_program: None,
            state_dir_env: None,
            ssh_dir_env: None,
        };
        let id = random_uuid();
        let alias = alias_name("swim", &id);
        World {
            _root: root,
            ctx,
            scope,
            id,
            alias,
        }
    }

    fn lease(&self) -> LeaseRef<'_> {
        LeaseRef {
            id: &self.id,
            project: "swim",
            platform: "linux/arm64",
        }
    }

    fn walker(&self) -> Walker {
        Walker::under(self.scope.clone())
    }

    fn folder(&self) -> PathBuf {
        self.ctx
            .home
            .join(".agentspace")
            .join("leases")
            .join(&self.id)
    }

    fn alias_path(&self) -> PathBuf {
        self.ctx.ssh_dir.join("agentspace.d").join(&self.alias)
    }

    fn log_path(&self) -> PathBuf {
        self.ctx.state_dir.join("release.log")
    }

    /// The lease's key and its alias file, as a `ready` lease has them.
    fn place_files(&self) {
        fs::create_dir_all(self.folder()).unwrap();
        fs::write(self.folder().join("id_ed25519"), b"key").unwrap();
        fs::create_dir_all(self.alias_path().parent().unwrap()).unwrap();
        fs::write(
            self.alias_path(),
            format!("# agentspace lease {}; test\nHost x\n", self.id),
        )
        .unwrap();
    }

    fn log(&self) -> String {
        fs::read_to_string(self.log_path()).unwrap_or_default()
    }

    /// The release's own `release.log` lines, each without its time and lease ID.
    fn texts(&self) -> Vec<String> {
        let prefix = format!(" lease={} ", self.id);
        self.log()
            .lines()
            .map(|line| {
                let at = line.find(&prefix).expect("A line names the lease");
                line[at + prefix.len()..].to_string()
            })
            .collect()
    }

    /// Starts a stand-in `ssh.exe <alias>` inside the scope folder.
    fn start_client(&self, alias: &str) -> Stand {
        start_client(&self.scope, alias)
    }
}

/// Starts a stand-in `ssh.exe <alias>`: a copy of this test program, in `scope`, run as the one
/// test that waits.
fn start_client(scope: &Path, alias: &str) -> Stand {
    let exe = scope.join("ssh.exe");
    if !exe.exists() {
        fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
    }
    let child = Command::new(&exe)
        .arg(alias)
        .arg("release_tests::stand_in_client")
        .arg("--test-threads=1")
        .env(STAND_IN, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    Stand(child)
}

fn lines_have_the_form(world: &World) {
    let log = world.log();
    assert!(log.ends_with('\n'));
    assert!(!log.starts_with('\u{feff}'));
    assert!(!log.contains('\r'));
    for line in log.lines() {
        let b = line.as_bytes();
        // `YYYY-MM-DDTHH:MM:SSZ lease=<id> <text>`.
        assert!(
            b.len() > 21 + 6 + 36 + 2,
            "A line has a time, a lease and a text"
        );
        assert!(b[4] == b'-' && b[7] == b'-' && b[10] == b'T' && b[13] == b':' && b[16] == b':');
        assert_eq!(b[19], b'Z');
        assert_eq!(b[20], b' ');
        assert!(line[21..].starts_with(&format!("lease={} ", world.id)));
        assert!(!line.contains(&world.alias));
    }
}

#[test]
fn a_release_with_no_client_logs_one_line_for_both_walks() {
    let w = World::new();
    w.place_files();
    release(&w.ctx, &w.lease(), &w.walker()).unwrap();
    assert!(!w.alias_path().exists());
    assert!(!w.folder().exists());
    assert_eq!(w.texts(), vec!["Ended 0 of the lease's connections."]);
    lines_have_the_form(&w);
}

#[test]
fn a_second_release_logs_a_line_of_its_own_and_appends_it() {
    let w = World::new();
    w.place_files();
    release(&w.ctx, &w.lease(), &w.walker()).unwrap();
    release(&w.ctx, &w.lease(), &w.walker()).unwrap();
    assert_eq!(w.texts().len(), 2);
    lines_have_the_form(&w);
}

#[test]
fn a_release_of_a_lease_with_no_files_still_walks_and_logs() {
    // A pending lease, or one that a failed create just placed: no alias, no folder.
    let w = World::new();
    release(&w.ctx, &w.lease(), &w.walker()).unwrap();
    assert_eq!(w.texts(), vec!["Ended 0 of the lease's connections."]);
}

#[test]
fn a_refused_alias_ends_nothing_logs_nothing_and_deletes_nothing() {
    let w = World::new();
    w.place_files();
    fs::write(
        w.alias_path(),
        format!("# agentspace lease {}; other\nHost x\n", random_uuid()),
    )
    .unwrap();
    let mut paused = 0;
    let mut client = w.start_client(&w.alias);
    assert!(release_pausing(&w.ctx, &w.lease(), &w.walker(), &mut || paused += 1).is_err());
    assert!(
        !client.ended_within(Duration::from_millis(500)),
        "No walk was made"
    );
    assert_eq!(paused, 0, "No walk was made, so there was no pause");
    assert!(!w.log_path().exists());
    assert!(w.alias_path().exists());
    assert!(w.folder().join("id_ed25519").exists());
}

#[test]
fn the_pause_is_between_the_first_walk_and_the_alias_file_s_deletion() {
    let w = World::new();
    w.place_files();
    let mut seen = Vec::new();
    let (alias, folder, log) = (w.alias_path(), w.folder(), w.log_path());
    release_pausing(&w.ctx, &w.lease(), &w.walker(), &mut || {
        seen.push((alias.exists(), folder.exists(), log.exists()));
    })
    .unwrap();
    // The alias, the folder and no log yet, at the pause, and once only.
    assert_eq!(seen, vec![(true, true, false)]);
    assert!(!w.alias_path().exists());
    assert!(!w.folder().exists());
}

#[cfg(windows)]
#[test]
fn the_first_walk_ends_a_client_and_the_line_counts_it() {
    let w = World::new();
    w.place_files();
    let mut client = w.start_client(&w.alias);
    let mut other = w.start_client(&alias_name("swim", &random_uuid()));
    assert!(!client.ended_within(Duration::from_millis(300)));
    release(&w.ctx, &w.lease(), &w.walker()).unwrap();
    assert!(client.ended_within(Duration::from_secs(5)));
    // Another lease's alias is left running.
    assert!(!other.ended_within(Duration::from_millis(500)));
    assert_eq!(w.texts(), vec!["Ended 1 of the lease's connections."]);
}

#[cfg(windows)]
#[test]
fn the_second_walk_ends_a_client_that_came_back_while_the_first_ran() {
    let w = World::new();
    w.place_files();
    let mut first = w.start_client(&w.alias);
    let mut other = w.start_client(&alias_name("swim", &random_uuid()));
    // In the pause, between the first walk and the alias file's deletion, a client starts, as the
    // desktop app's would by itself. The first walk is over, so only the second can end it.
    let mut at_pause = Vec::new();
    let mut started: Vec<Stand> = Vec::new();
    release_pausing(&w.ctx, &w.lease(), &w.walker(), &mut || {
        at_pause.push((
            first.ended_within(Duration::from_secs(5)),
            w.alias_path().exists(),
        ));
        started.push(w.start_client(&w.alias));
    })
    .unwrap();
    assert_eq!(
        at_pause,
        vec![(true, true)],
        "The first walk ended its client before the pause, which was called once"
    );
    assert_eq!(started.len(), 1);
    assert!(
        started[0].ended_within(Duration::from_secs(5)),
        "The second walk ended it"
    );
    // Another lease's alias is left running.
    assert!(!other.ended_within(Duration::from_millis(500)));
    // One line counts both walks.
    assert_eq!(w.texts(), vec![release_text(&Ok(1), &Ok(1))]);
    assert_eq!(w.texts(), vec!["Ended 2 of the lease's connections."]);
}

#[test]
fn the_alias_file_is_deleted_after_the_first_walk_and_before_the_second() {
    // Seen from the walks themselves, with no timing assumed: what exists when each walk starts.
    let w = World::new();
    w.place_files();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (alias, folder, log) = (w.alias_path(), w.folder(), w.log_path());
    let walker = w.walker().observed_by({
        let seen = Arc::clone(&seen);
        move || {
            seen.lock()
                .unwrap()
                .push((alias.exists(), folder.exists(), log.exists()));
        }
    });
    release(&w.ctx, &w.lease(), &walker).unwrap();
    assert_eq!(
        *seen.lock().unwrap(),
        // The first walk finds the alias file; the second runs after its deletion; the folder
        // stays until both walks and the log's line are done.
        vec![(true, true, false), (false, true, false)]
    );
    assert!(!w.folder().exists());
    assert_eq!(w.texts().len(), 1);
}

#[test]
fn a_log_that_cannot_be_written_does_not_fail_the_release() {
    let w = World::new();
    w.place_files();
    fs::create_dir(w.log_path()).unwrap();
    release(&w.ctx, &w.lease(), &w.walker()).unwrap();
    assert!(!w.alias_path().exists());
    assert!(!w.folder().exists());
    assert!(w.log_path().is_dir());
}

#[test]
fn a_missing_state_folder_does_not_fail_the_release() {
    let mut w = World::new();
    w.place_files();
    w.ctx.state_dir = w.ctx.state_dir.join("never-made");
    release(&w.ctx, &w.lease(), &w.walker()).unwrap();
    assert!(!w.folder().exists());
    assert!(!w.ctx.state_dir.exists(), "The log makes no folder");
}

/// Holds a file open for read, sharing read only, so that it cannot be deleted while held.
#[cfg(windows)]
fn hold_open(path: &Path) -> fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1)
        .open(path)
        .unwrap()
}

#[cfg(windows)]
#[test]
fn an_alias_that_cannot_be_deleted_still_has_its_second_walk_and_its_line_and_keeps_the_folder() {
    let w = World::new();
    w.place_files();
    let holder = hold_open(&w.alias_path());
    let f = release(&w.ctx, &w.lease(), &w.walker()).unwrap_err();
    assert!(f.message.contains("could not be deleted"));
    assert!(w.alias_path().exists());
    assert!(w.folder().join("id_ed25519").exists());
    assert_eq!(w.texts(), vec!["Ended 0 of the lease's connections."]);
    // A retried release runs it again and finishes.
    drop(holder);
    release(&w.ctx, &w.lease(), &w.walker()).unwrap();
    assert!(!w.alias_path().exists());
    assert!(!w.folder().exists());
    assert_eq!(w.texts().len(), 2);
}

#[cfg(windows)]
#[test]
fn a_failed_alias_deletion_still_has_its_second_walk_that_ends_a_client_from_the_pause() {
    let w = World::new();
    w.place_files();
    let mut holder = None;
    let mut started: Vec<Stand> = Vec::new();
    // In the pause a client starts and the alias file is held open, so that its deletion fails.
    let f = release_pausing(&w.ctx, &w.lease(), &w.walker(), &mut || {
        started.push(w.start_client(&w.alias));
        holder = Some(hold_open(&w.alias_path()));
    })
    .unwrap_err();
    assert!(f.message.contains("could not be deleted"));
    assert_eq!(started.len(), 1);
    assert!(
        started[0].ended_within(Duration::from_secs(5)),
        "The second walk runs when the deletion failed"
    );
    assert_eq!(w.texts(), vec!["Ended 1 of the lease's connections."]);
    assert!(w.alias_path().exists());
    assert!(w.folder().join("id_ed25519").exists());
    // A retried release finishes, and logs a line of its own.
    drop(holder);
    release(&w.ctx, &w.lease(), &w.walker()).unwrap();
    assert!(!w.alias_path().exists());
    assert!(!w.folder().exists());
    assert_eq!(
        w.texts(),
        vec![
            "Ended 1 of the lease's connections.".to_string(),
            "Ended 0 of the lease's connections.".to_string()
        ]
    );
}

#[cfg(windows)]
#[test]
fn the_line_is_written_before_the_folder_is_deleted() {
    let w = World::new();
    w.place_files();
    // The folder cannot be deleted while one of its files is held.
    let holder = hold_open(&w.folder().join("id_ed25519"));
    let f = release(&w.ctx, &w.lease(), &w.walker()).unwrap_err();
    assert!(f.message.contains("could not be deleted"));
    assert!(!w.alias_path().exists(), "The alias went first");
    assert_eq!(w.texts(), vec!["Ended 0 of the lease's connections."]);
    drop(holder);
    release(&w.ctx, &w.lease(), &w.walker()).unwrap();
    assert!(!w.folder().exists());
}

#[test]
fn removing_the_files_walks_for_no_client_and_logs_nothing() {
    // place_pending's steps 4 and 7 are not a release.
    let w = World::new();
    w.place_files();
    let mut client = w.start_client(&w.alias);
    remove_files(&w.ctx, &w.lease()).unwrap();
    assert!(!w.alias_path().exists());
    assert!(!w.folder().exists());
    assert!(
        !client.ended_within(Duration::from_millis(500)),
        "No walk was made"
    );
    assert!(!w.log_path().exists());
}
