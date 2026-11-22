//! The release's two walks for the alias's clients, and the line it logs for them
//! (docs/driver.md, "Ending a lease's connections", "The order of a release" and "Where the count
//! is logged"; docs/state.md, "The release log").
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **A walk MUST be one call of `agentspace_connections::end_alias_clients`**, or of
//!   `end_alias_clients_under` for a scoped walker, and the line's text MUST come from
//!   `agentspace_connections::line::release_text`, never re-derived here.
//! - **A walk MUST NOT fail the release**: its result is only counted, by `release_text`.
//! - **A log that cannot be opened or written MUST NOT fail the release**, and nothing is written
//!   to standard output or standard error.
//! - **There is no pause between the walks here.** The test-only one is `ops::release_pausing`,
//!   which exists only under `cfg(test)`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::Arc;

use agentspace_connections::{Result as WalkResult, end_alias_clients, end_alias_clients_under};
use agentspace_state::{format_time, now};

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
