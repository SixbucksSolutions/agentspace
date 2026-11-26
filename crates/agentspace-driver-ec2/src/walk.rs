//! The release's walks for the alias's clients, and the line it logs for them (docs/driver.md,
//! "Release a lease", "Ending a lease's connections", "The order of a release" and "Where the
//! count is logged"; docs/state.md, "The release log").
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **A walk MUST be one call of `agentspace_connections::end_alias_clients`**, or of
//!   `end_alias_clients_under` for a scoped walker, and the line's text MUST come from
//!   `agentspace_connections::line::release_text`, never re-derived here. A release of three walks
//!   or more gives it two results that stand for all of them ([`release_line_text`]).
//! - **A walk MUST NOT fail the release**: its result is only counted, by `release_text`.
//! - **After its second walk, a release MUST walk again while the walk before ended a client**
//!   (#0196; [`further_walk_due`]), within [`MOST_WALKS`] walks and with no walk after the second
//!   starting [`FURTHER_WALKS_WITHIN`] or more after the first started.
//! - **A log that cannot be opened or written MUST NOT fail the release**, and nothing is written
//!   to standard output or standard error.
//! - **There is no pause between the walks here.** The test-only one is `ops::release_pausing`,
//!   which exists only under `cfg(test)`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::Arc;
use std::time::Duration;

use agentspace_connections::line::release_text;
use agentspace_connections::{
    EndError, Result as WalkResult, end_alias_clients, end_alias_clients_under,
};
use agentspace_state::{format_time, now};

/// The most walks one release makes, its first two included (driver.md, "Release a lease").
pub(crate) const MOST_WALKS: usize = 6;

/// No walk after a release's second starts this long or longer after its first walk started
/// (driver.md, "Release a lease"). Each walk returns within 10 seconds of its own start, so a
/// release's walks still take at most 20 seconds between them, as its first two always could.
pub(crate) const FURTHER_WALKS_WITHIN: Duration = Duration::from_secs(10);

/// The clients a walk ended, when it read the process list.
fn ended_by(walk: &WalkResult<usize>) -> Option<usize> {
    match walk {
        Ok(ended) | Err(EndError::NotAllEnded { ended, .. }) => Some(*ended),
        Err(EndError::ListUnreadable { .. }) => None,
    }
}

/// Whether a release that has made `walks`, at least its first two, the first of them
/// `since_first` ago, walks again (#0196; driver.md, "Release a lease"). It does while the last walk
/// ended at least one client: a client below a program that has exited, while the client above it
/// held that program, is no node of the upper client's tree and is not counted on its own while the
/// upper client runs, so it is found on its own only by a walk after the one that ended the upper
/// client, and a client that came back while a walk ran is ended by the next. It stops at the first
/// walk that ends none, or that cannot read the process list, at [`MOST_WALKS`] walks, and once
/// [`FURTHER_WALKS_WITHIN`] has passed since the first walk started.
pub(crate) fn further_walk_due(walks: &[WalkResult<usize>], since_first: Duration) -> bool {
    if walks.len() < 2 || walks.len() >= MOST_WALKS || since_first >= FURTHER_WALKS_WITHIN {
        return false;
    }
    walks
        .last()
        .and_then(ended_by)
        .is_some_and(|ended| ended > 0)
}

/// The text of a release's one line for all of its walks, from `release_text` (driver.md, "Where
/// the count is logged"). Two walks are passed to it as they are. Three or more are passed as the
/// two results that give the same counts by driver.md's rules for two: `<n>` the clients every walk
/// that read the process list ended, summed; `<k>` the clients the last walk that read it could not
/// end, each once, so a client an earlier walk could not end and a later one ended or found gone is
/// not in it; and `<e>` the Windows error of the first walk that could not read it.
pub(crate) fn release_line_text(walks: &[WalkResult<usize>]) -> String {
    if let [first, second] = walks {
        return release_text(first, second);
    }
    let mut ended = 0usize;
    let mut last_read: Option<&WalkResult<usize>> = None;
    let mut failed: Option<u32> = None;
    for walk in walks {
        match walk {
            Err(EndError::ListUnreadable { windows_error }) => {
                failed.get_or_insert(*windows_error);
            }
            _ => {
                ended = ended.saturating_add(ended_by(walk).unwrap_or(0));
                last_read = Some(walk);
            }
        }
    }
    let Some(last) = last_read else {
        // No walk read the list, or there was none: the first two walks as they are.
        return match walks {
            [first, second, ..] => release_text(first, second),
            [only] => release_text(only, only),
            [] => release_text(&Ok(0), &Ok(0)),
        };
    };
    match failed {
        None => {
            let before_last = ended.saturating_sub(ended_by(last).unwrap_or(0));
            release_text(&Ok(before_last), last)
        }
        Some(windows_error) => {
            let left = match last {
                Err(EndError::NotAllEnded { not_ended, .. }) => not_ended.clone(),
                _ => Vec::new(),
            };
            let read = if left.is_empty() {
                Ok(ended)
            } else {
                Err(EndError::NotAllEnded {
                    ended,
                    not_ended: left,
                })
            };
            release_text(&read, &Err(EndError::ListUnreadable { windows_error }))
        }
    }
}

/// How the release walks for an alias's clients.
///
/// `Walker::default()` is the product's: every client of the alias on the laptop, found by
/// `end_alias_clients`. A walker made with [`Walker::under`] is TEST-ONLY: it ends only a client
/// whose executable is inside a folder (`end_alias_clients_under`), so that a test walks the
/// laptop's real process list without being able to end any process of Terry's.
#[derive(Clone, Default)]
pub(crate) struct Walker {
    under: Option<PathBuf>,
    /// TEST-ONLY, under `cfg(test)`: called at the start of each walk, so that a test sees what
    /// the release had done by then without relying on timing.
    #[cfg(test)]
    observer: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Walker {
    /// A walker that ends only the clients whose executable is inside `dir`.
    pub(crate) fn under(dir: PathBuf) -> Walker {
        Walker {
            under: Some(dir),
            #[cfg(test)]
            observer: None,
        }
    }

    /// TEST-ONLY. The same walker, calling `observer` at the start of each walk.
    #[cfg(test)]
    pub(crate) fn observed_by(mut self, observer: impl Fn() + Send + Sync + 'static) -> Walker {
        self.observer = Some(Arc::new(observer));
        self
    }

    /// One walk: one call of the connections crate.
    pub(crate) fn end(&self, alias: &str) -> WalkResult<usize> {
        #[cfg(test)]
        if let Some(observer) = &self.observer {
            observer();
        }
        match &self.under {
            Some(dir) => end_alias_clients_under(alias, dir),
            None => end_alias_clients(alias),
        }
    }
}

/// Appends the release's one line to `<state folder>\release.log`: a UTC time, `lease=<id>` and
/// the text, ending in one LF, written whole in one write. The file is created when missing and
/// opened for append sharing read, write and delete. Any failure is ignored (driver.md, "Where
/// the count is logged").
pub(crate) fn append_release_line(state_dir: &Path, lease_id: &str, text: &str) {
    let line = format!("{} lease={lease_id} {text}\n", format_time(now()));
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE.
        options.share_mode(0x1 | 0x2 | 0x4);
    }
    if let Ok(mut file) = options.open(state_dir.join("release.log")) {
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentspace_connections::Identity;

    fn id(pid: u32, created: u64) -> Identity {
        Identity { pid, created }
    }

    fn left(ended: usize, not_ended: &[Identity]) -> WalkResult<usize> {
        Err(EndError::NotAllEnded {
            ended,
            not_ended: not_ended.to_vec(),
        })
    }

    fn unread(windows_error: u32) -> WalkResult<usize> {
        Err(EndError::ListUnreadable { windows_error })
    }

    const SOON: Duration = Duration::from_millis(5);

    #[test]
    fn a_further_walk_is_due_while_the_walk_before_ended_a_client() {
        assert!(further_walk_due(&[Ok(0), Ok(1)], SOON));
        assert!(further_walk_due(&[Ok(3), left(1, &[id(7, 70)])], SOON));
        assert!(further_walk_due(&[Ok(1), Ok(1), Ok(1)], SOON));
        assert!(further_walk_due(&[unread(5), Ok(2)], SOON));
    }

    #[test]
    fn no_further_walk_after_one_that_ended_none_or_could_not_look() {
        assert!(!further_walk_due(&[Ok(1), Ok(0)], SOON));
        assert!(!further_walk_due(&[Ok(1), left(0, &[id(7, 70)])], SOON));
        assert!(!further_walk_due(&[Ok(1), unread(5)], SOON));
        assert!(!further_walk_due(&[Ok(2), Ok(1), Ok(0)], SOON));
    }

    #[test]
    fn the_first_two_walks_are_never_further_walks() {
        // The release makes its first two walks whatever they find; this rule is only for after.
        assert!(!further_walk_due(&[], SOON));
        assert!(!further_walk_due(&[Ok(5)], SOON));
    }

    #[test]
    fn no_further_walk_past_the_most_walks() {
        let five = [Ok(1), Ok(1), Ok(1), Ok(1), Ok(1)];
        assert!(further_walk_due(&five, SOON));
        let six = [Ok(1), Ok(1), Ok(1), Ok(1), Ok(1), Ok(1)];
        assert_eq!(six.len(), MOST_WALKS);
        assert!(!further_walk_due(&six, SOON));
    }

    #[test]
    fn no_further_walk_starts_ten_seconds_or_more_after_the_first_started() {
        let just_under = FURTHER_WALKS_WITHIN - Duration::from_millis(1);
        assert!(further_walk_due(&[Ok(1), Ok(1)], just_under));
        assert!(!further_walk_due(&[Ok(1), Ok(1)], FURTHER_WALKS_WITHIN));
        assert!(!further_walk_due(&[Ok(1), Ok(1)], Duration::from_secs(25)));
        // A last walk that returned by its own deadline, 10 seconds from its start, after one that
        // started before this bound: the walks take at most 20 seconds in all.
        assert_eq!(FURTHER_WALKS_WITHIN, Duration::from_secs(10));
    }

    #[test]
    fn two_walks_give_release_text_s_own_line() {
        let a = id(10, 100);
        let pairs = [
            (Ok(2), Ok(1)),
            (left(3, &[a]), left(0, &[a])),
            (Ok(4), unread(5)),
            (unread(5), left(1, &[a])),
            (unread(5), unread(6)),
            (left(1, &[a]), Ok(1)),
        ];
        for (first, second) in pairs {
            assert_eq!(
                release_line_text(&[first.clone(), second.clone()]),
                release_text(&first, &second)
            );
        }
    }

    #[test]
    fn three_walks_or_more_sum_what_each_walk_ended() {
        assert_eq!(
            release_line_text(&[Ok(1), Ok(1), Ok(1), Ok(0)]),
            "Ended 3 of the lease's connections."
        );
        assert_eq!(
            release_line_text(&[Ok(2), Ok(1), Ok(0)]),
            "Ended 3 of the lease's connections."
        );
    }

    #[test]
    fn three_walks_or_more_count_only_what_the_last_walk_that_read_left() {
        let a = id(10, 100);
        let b = id(11, 101);
        // A client the first walk could not end, and a later walk ended: no longer left.
        assert_eq!(
            release_line_text(&[left(1, &[a]), Ok(2), Ok(0)]),
            "Ended 3 of the lease's connections."
        );
        // One still left at the last walk, found by every walk, is counted once.
        assert_eq!(
            release_line_text(&[left(1, &[a]), left(1, &[a]), left(0, &[a])]),
            "Ended 2 of the lease's connections; 1 could not be ended."
        );
        assert_eq!(
            release_line_text(&[left(0, &[a, b]), left(1, &[a]), left(0, &[a])]),
            "Ended 1 of the lease's connections; 1 could not be ended."
        );
    }

    #[test]
    fn three_walks_or_more_keep_a_walk_that_could_not_look() {
        let a = id(10, 100);
        // The third could not read the list: what the first two ended stays, and the second's
        // client left is the last that a walk which read the list could not end.
        assert_eq!(
            release_line_text(&[Ok(1), left(1, &[a]), unread(5)]),
            "Ended 2 of the lease's connections; 1 could not be ended; a look failed: Windows error 5."
        );
        assert_eq!(
            release_line_text(&[Ok(1), Ok(1), unread(5)]),
            "Ended 2 of the lease's connections; a look failed: Windows error 5."
        );
        // The first could not read it, and the later two did: the first failing walk's error.
        assert_eq!(
            release_line_text(&[unread(5), Ok(2), Ok(0)]),
            "Ended 2 of the lease's connections; a look failed: Windows error 5."
        );
        assert_eq!(
            release_line_text(&[unread(5), Ok(1), unread(6)]),
            "Ended 1 of the lease's connections; a look failed: Windows error 5."
        );
    }

    #[test]
    fn any_number_of_walks_gives_a_line_without_a_panic() {
        assert_eq!(
            release_line_text(&[]),
            "Ended 0 of the lease's connections."
        );
        assert_eq!(
            release_line_text(&[Ok(3)]),
            "Ended 3 of the lease's connections."
        );
        assert_eq!(
            release_line_text(&[unread(5)]),
            "The lease's connections could not be looked for: Windows error 5."
        );
        assert_eq!(
            release_line_text(&[unread(5), unread(6), unread(7)]),
            "The lease's connections could not be looked for: Windows error 5."
        );
    }

    /// The release over real stand-in processes (Windows only).
    ///
    /// **Process safety, and the rules every test here keeps** (driver.md, "How it is checked"):
    /// - **Every release walks with a `Walker` over a throwaway folder**, so only a process whose
    ///   executable is in that folder can be ended, and the stand-ins are the only such processes.
    /// - **Every lease ID is a UUIDv4 drawn at random for the test.**
    /// - **The stand-ins are copies of this test program**, run as the one test that acts,
    ///   `chain_stand_in`, which does nothing unless its environment says so. Each stand-in ends
    ///   itself when the test's `stop` file appears, or after 2 minutes, and the test's guard
    ///   makes that file and ends the one stand-in it started itself, whatever the assertions do.
    ///   No test opens, ends or reads any process by its ID.
    /// - **No test prints a command line or an environment.**
    #[cfg(windows)]
    mod chain {
        use super::super::*;
        use crate::ops::release;
        use agentspace_driver_api::{Context, LeaseRef};
        use std::collections::hash_map::RandomState;
        use std::fs;
        use std::hash::{BuildHasher, Hasher};
        use std::os::windows::fs::OpenOptionsExt;
        use std::process::{Child, Command, Stdio};
        use std::sync::Mutex;
        use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

        /// The folder a stand-in keeps its files in; set only in a stand-in's environment.
        const STAND: &str = "AGENTSPACE_EC2_WALK_TEST_STAND";
        /// `ssh` or `mid`.
        const ROLE: &str = "AGENTSPACE_EC2_WALK_TEST_ROLE";
        /// The roles still to start below this stand-in, comma-separated, the next first.
        const CHAIN: &str = "AGENTSPACE_EC2_WALK_TEST_CHAIN";
        /// How deep this stand-in is: 0 for the one the test starts.
        const LEVEL: &str = "AGENTSPACE_EC2_WALK_TEST_LEVEL";
        const SSH: &str = "AGENTSPACE_EC2_WALK_TEST_SSH";
        const MID: &str = "AGENTSPACE_EC2_WALK_TEST_MID";
        const ALIAS: &str = "AGENTSPACE_EC2_WALK_TEST_ALIAS";
        /// The libtest filter that selects `chain_stand_in` and nothing else.
        const FILTER: &str = "walk::tests::chain::chain_stand_in";
        /// Windows' ERROR_SHARING_VIOLATION.
        const SHARING_VIOLATION: i32 = 32;

        /// Not a test of its own: the stand-in. In an ordinary test run it returns at once. In a
        /// stand-in, an `ssh` holds `alive-<level>` open, shared with nobody, for as long as it
        /// runs, starts the next role of its chain, keeps that child's handle open, writes
        /// `exited-<level + 1>` once that child has exited, and waits for `stop`. A `mid` starts
        /// the next role of its chain and exits at once, so that it has exited while the `ssh`
        /// above it still holds its handle, as a desktop app's proxy program that has gone can be
        /// held by the `ssh.exe` that started it.
        #[test]
        fn chain_stand_in() {
            let Some(dir) = std::env::var_os(STAND).map(PathBuf::from) else {
                return;
            };
            let role = std::env::var(ROLE).unwrap_or_default();
            let level: u32 = std::env::var(LEVEL)
                .ok()
                .and_then(|l| l.parse().ok())
                .unwrap_or(0);
            let _alive = (role == "ssh").then(|| {
                fs::OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .share_mode(0)
                    .open(dir.join(format!("alive-{level}")))
                    .expect("A stand-in holds its file")
            });
            let chain = std::env::var(CHAIN).unwrap_or_default();
            let mut roles = chain.split(',').filter(|r| !r.is_empty());
            let mut below: Option<Child> = roles.next().map(|next| {
                let rest: Vec<&str> = roles.collect();
                let program = std::env::var_os(if next == "ssh" { SSH } else { MID })
                    .expect("A stand-in knows its programs");
                let mut command = Command::new(program);
                if next == "ssh" {
                    command.arg(std::env::var(ALIAS).expect("A stand-in knows the alias"));
                }
                command
                    .arg(FILTER)
                    .arg("--test-threads=1")
                    .env(ROLE, next)
                    .env(LEVEL, (level + 1).to_string())
                    .env(CHAIN, rest.join(","))
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .expect("A stand-in starts its child")
            });
            if role != "ssh" {
                return;
            }
            let mut reported = false;
            let until = Instant::now() + Duration::from_secs(120);
            while Instant::now() < until && !dir.join("stop").exists() {
                if !reported
                    && let Some(child) = below.as_mut()
                    && matches!(child.try_wait(), Ok(Some(_)))
                {
                    // `try_wait` keeps the handle open: the child stays held after it has exited.
                    let _ = fs::write(dir.join(format!("exited-{}", level + 1)), b"");
                    reported = true;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }

        /// A UUIDv4 drawn at random for this call, in its normalized form.
        fn random_uuid() -> String {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let mut bytes = [0u8; 16];
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

        /// A throwaway world: a home with a `ready` lease's key and alias file, a state folder,
        /// the folder the walks are scoped to, and the stand-ins' folder.
        struct World {
            root: tempfile::TempDir,
            ctx: Context,
            scope: PathBuf,
            stand: PathBuf,
            id: String,
            alias: String,
            top: Option<Child>,
        }

        impl World {
            fn new() -> World {
                let root = tempfile::tempdir().unwrap();
                let home = root.path().join("home");
                let state = root.path().join("state");
                let scope = root.path().join("scope");
                let stand = root.path().join("stand");
                for dir in [&home, &state, &scope, &stand, &root.path().join("mid")] {
                    fs::create_dir_all(dir).unwrap();
                }
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
                let alias = agentspace_alias::alias_name("swim", &id);
                let folder = home.join(".agentspace").join("leases").join(&id);
                fs::create_dir_all(&folder).unwrap();
                fs::write(folder.join("id_ed25519"), b"key").unwrap();
                let aliases = ctx.ssh_dir.join("agentspace.d");
                fs::create_dir_all(&aliases).unwrap();
                fs::write(
                    aliases.join(&alias),
                    format!("# agentspace lease {id}; test\nHost x\n"),
                )
                .unwrap();
                World {
                    root,
                    ctx,
                    scope,
                    stand,
                    id,
                    alias,
                    top: None,
                }
            }

            fn lease(&self) -> LeaseRef<'_> {
                LeaseRef {
                    id: &self.id,
                    project: "swim",
                }
            }

            /// Copies this test program to `path`, once.
            fn copy_to(path: &Path) {
                if !path.exists() {
                    fs::copy(std::env::current_exe().unwrap(), path).unwrap();
                }
            }

            /// Starts `ssh.exe <alias>` in the scope folder, which starts `chain` below it, each
            /// role below the one before: `ssh` a stand-in `ssh.exe <alias>` in the scope folder,
            /// which is a client, and `mid` a stand-in `claude-ssh-proxy.exe` outside it, which
            /// is none.
            fn start_chain(&mut self, chain: &[&str]) {
                let ssh = self.scope.join("ssh.exe");
                let mid = self.root.path().join("mid").join("claude-ssh-proxy.exe");
                World::copy_to(&ssh);
                World::copy_to(&mid);
                let home = self.root.path().join("stand-home");
                fs::create_dir_all(&home).unwrap();
                let mut command = Command::new(&ssh);
                command
                    .arg(&self.alias)
                    .arg(FILTER)
                    .arg("--test-threads=1")
                    .env(STAND, &self.stand)
                    .env(ROLE, "ssh")
                    .env(LEVEL, "0")
                    .env(CHAIN, chain.join(","))
                    .env(SSH, &ssh)
                    .env(MID, &mid)
                    .env(ALIAS, &self.alias);
                for name in ["USERPROFILE", "HOME", "LOCALAPPDATA", "APPDATA"] {
                    command.env(name, &home);
                }
                self.top = Some(
                    command
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                        .unwrap(),
                );
            }

            /// Whether the `ssh` stand-in at `level` runs: its file is held, shared with nobody.
            fn running(&self, level: u32) -> bool {
                match fs::OpenOptions::new()
                    .read(true)
                    .share_mode(0)
                    .open(self.stand.join(format!("alive-{level}")))
                {
                    Ok(_) => false,
                    Err(e) => e.raw_os_error() == Some(SHARING_VIOLATION),
                }
            }

            /// Waits, for at most 30 seconds, until `ready` holds.
            fn wait(&self, what: &str, ready: impl Fn() -> bool) {
                let until = Instant::now() + Duration::from_secs(30);
                while !ready() {
                    assert!(Instant::now() < until, "{what} within 30 seconds");
                    std::thread::sleep(Duration::from_millis(25));
                }
            }

            /// The release's own `release.log` lines, each without its time and lease ID.
            fn texts(&self) -> Vec<String> {
                let prefix = format!(" lease={} ", self.id);
                fs::read_to_string(self.ctx.state_dir.join("release.log"))
                    .unwrap_or_default()
                    .lines()
                    .filter_map(|line| {
                        line.find(&prefix)
                            .map(|at| line[at + prefix.len()..].to_string())
                    })
                    .collect()
            }
        }

        impl Drop for World {
            /// Ends every stand-in: the `stop` file for those below the first, and the first by
            /// its handle. Then waits, for at most 10 seconds, until none holds its file.
            fn drop(&mut self) {
                let _ = fs::write(self.stand.join("stop"), b"");
                if let Some(top) = self.top.as_mut() {
                    let _ = top.kill();
                    let _ = top.wait();
                }
                let until = Instant::now() + Duration::from_secs(10);
                while (0..8).any(|level| self.running(level)) && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        }

        /// #0196 and #0228: a client below a program that has exited, while the client above it
        /// still holds that program's handle, is no node of the upper client's tree. From v0.12
        /// the walk that ends the upper client takes it as a top of its own and ends it too
        /// (D99): with two such programs in a chain, the release's first walk ends all three
        /// clients, its second ends none, and its line counts each client once.
        #[test]
        fn a_client_below_a_program_that_has_gone_is_ended_by_the_first_walk() {
            let mut w = World::new();
            w.start_chain(&["mid", "ssh", "mid", "ssh"]);
            w.wait("Every stand-in ssh.exe runs", || {
                [0, 2, 4].iter().all(|l| w.running(*l))
            });
            w.wait("Both stand-in proxy programs have exited", || {
                w.stand.join("exited-1").exists() && w.stand.join("exited-3").exists()
            });
            let walks = std::sync::Arc::new(Mutex::new(0usize));
            let walker = Walker::under(w.scope.clone()).observed_by({
                let walks = std::sync::Arc::clone(&walks);
                move || *walks.lock().unwrap() += 1
            });
            release(&w.ctx, &w.lease(), &walker).unwrap();
            for level in [0, 2, 4] {
                assert!(
                    !w.running(level),
                    "The client {level} deep is still running when the release returned"
                );
            }
            // The first walk ends all three clients, and the second ends none.
            assert_eq!(*walks.lock().unwrap(), 2);
            assert_eq!(w.texts(), vec!["Ended 3 of the lease's connections."]);
        }
    }
}
