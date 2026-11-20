//! The instance-ID cache as a file (docs/operator.md, Laptop files; docs/state.md, Files in the
//! checkout).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The cache is `%USERPROFILE%\.agentspace\instance-id.json`. It is moved by `USERPROFILE` and by
//! no other variable. The ProxyCommand reads it, and writes it only when `up`'s first step finds
//! it missing.

use std::path::{Path, PathBuf};

use agentspace_box::{CacheRead, InstanceIdCache};
use agentspace_operator_api::{CallerFailure, Failure};

/// The cache's file name.
pub const CACHE_FILE: &str = "instance-id.json";

/// The file at `<home>\.agentspace\instance-id.json`.
#[derive(Debug, Clone)]
pub struct FileCache {
    path: PathBuf,
}

impl FileCache {
    /// The cache under `home`, which is `USERPROFILE`.
    pub fn new(home: &Path) -> FileCache {
        FileCache {
            path: home.join(".agentspace").join(CACHE_FILE),
        }
    }
}

impl InstanceIdCache for FileCache {
    fn path(&self) -> &Path {
        &self.path
    }

    fn read(&self) -> CacheRead {
        std::fs::read(&self.path).into()
    }

    /// Makes the folder if it is missing, then replaces the file atomically, through
    /// `instance-id.json.<hex>.tmp` (state.md, Replacing a file).
    fn write(&self, bytes: &[u8]) -> Result<(), Failure> {
        let written = match self.path.parent() {
            Some(dir) => std::fs::create_dir_all(dir)
                .and_then(|()| agentspace_state::write_atomically(&self.path, bytes)),
            None => Err(std::io::ErrorKind::InvalidInput.into()),
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

    fn throwaway(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "agentspace-ssh-proxy-cache-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_cache_lives_in_the_dot_agentspace_folder_as_json() {
        let home = throwaway("path");
        let cache = FileCache::new(&home);
        assert_eq!(
            cache.path(),
            home.join(".agentspace").join("instance-id.json")
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_missing_cache_is_missing_and_a_write_makes_the_folder() {
        let home = throwaway("write");
        let cache = FileCache::new(&home);
        assert_eq!(cache.read(), CacheRead::Missing);
        cache
            .write(&agentspace_box::cache_bytes("i-1234567890abcdef0"))
            .unwrap();
        assert_eq!(
            agentspace_box::read_cache(cache.path(), cache.read()),
            Ok(Some("i-1234567890abcdef0".to_string()))
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_folder_at_the_cache_path_cannot_be_opened() {
        let home = throwaway("folder");
        let cache = FileCache::new(&home);
        std::fs::create_dir_all(cache.path()).unwrap();
        assert_eq!(cache.read(), CacheRead::Unopenable);
        let failure = cache.write(b"{}").unwrap_err();
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        assert!(failure.message.starts_with("agentspace: Cannot write "));
        let _ = std::fs::remove_dir_all(&home);
    }
}
