//! The instance-ID cache as a file: `%USERPROFILE%\.agentspace\instance-id.json` (operator.md,
//! Laptop files; docs/state.md, Files in the checkout; D21, D40).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The decisions about the cache's bytes are `agentspace-box`'s. This module reads the file, and
//! replaces it atomically with `agentspace-state`'s `write_atomically`, making its folder first,
//! since a replace never creates one.

use std::path::{Path, PathBuf};

use agentspace_box::{CacheRead, InstanceIdCache};
use agentspace_operator_api::{CallerFailure, Context, Failure};

/// The cache's path under `home`: `<home>\.agentspace\instance-id.json`. Only `USERPROFILE` moves
/// it (state.md, What moves which files).
pub fn cache_path(home: &Path) -> PathBuf {
    home.join(".agentspace").join("instance-id.json")
}

/// The instance-ID cache, read and written as a file.
#[derive(Debug, Clone)]
pub struct FileInstanceIdCache {
    path: PathBuf,
}

impl FileInstanceIdCache {
    /// The cache under the home folder of `ctx`.
    pub fn new(ctx: &Context) -> Self {
        Self {
            path: cache_path(&ctx.home),
        }
    }
}

impl InstanceIdCache for FileInstanceIdCache {
    fn path(&self) -> &Path {
        &self.path
    }

    fn read(&self) -> CacheRead {
        CacheRead::from(std::fs::read(&self.path))
    }

    /// A cache that cannot be written fails with `agentspace: Cannot write <path>.`, and caller
    /// mode's "Could not reach the host" (operator.md, Laptop files, the instance-ID cache;
    /// architect, 2026-11-19, for E0.06-T03-S01).
    fn write(&self, bytes: &[u8]) -> Result<(), Failure> {
        let written = match self.path.parent() {
            Some(dir) => std::fs::create_dir_all(dir)
                .and_then(|()| agentspace_state::write_atomically(&self.path, bytes)),
            None => Err(std::io::Error::other("the path has no folder")),
        };
        written.map_err(|_| Failure {
            message: format!("agentspace: Cannot write {}.", self.path.display()),
            caller: CallerFailure::CouldNotReach,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempHome;
    use agentspace_box::{cache_bytes, parse_cache};

    #[test]
    fn the_cache_is_under_the_agentspace_folder_of_the_home() {
        let home = TempHome::new("cache-path");
        let cache = FileInstanceIdCache::new(&home.ctx());
        assert_eq!(
            cache.path(),
            home.path()
                .join("home")
                .join(".agentspace")
                .join("instance-id.json")
        );
    }

    #[test]
    fn a_missing_cache_reads_missing_and_a_write_makes_the_folder() {
        let home = TempHome::new("cache-write");
        let cache = FileInstanceIdCache::new(&home.ctx());
        assert_eq!(cache.read(), CacheRead::Missing);
        cache.write(&cache_bytes("i-1234567890abcdef0")).unwrap();
        let CacheRead::Bytes(bytes) = cache.read() else {
            panic!("the cache was not written");
        };
        assert_eq!(parse_cache(&bytes).as_deref(), Some("i-1234567890abcdef0"));
    }

    #[test]
    fn a_folder_at_the_path_cannot_be_opened() {
        let home = TempHome::new("cache-folder");
        let cache = FileInstanceIdCache::new(&home.ctx());
        std::fs::create_dir_all(cache.path()).unwrap();
        assert_eq!(cache.read(), CacheRead::Unopenable);
    }

    #[test]
    fn a_cache_that_cannot_be_written_names_its_path() {
        let home = TempHome::new("cache-no-write");
        let cache = FileInstanceIdCache::new(&home.ctx());
        // A file where the cache's folder belongs.
        std::fs::create_dir_all(home.path().join("home")).unwrap();
        std::fs::write(home.path().join("home").join(".agentspace"), b"x").unwrap();
        let failure = cache.write(b"{}").unwrap_err();
        assert_eq!(
            failure.message,
            format!("agentspace: Cannot write {}.", cache.path().display())
        );
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
    }
}
