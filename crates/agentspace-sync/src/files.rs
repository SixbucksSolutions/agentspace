//! The laptop files `sync` and `deploy` read and write (docs/operator.md, Laptop files, What the
//! prototype took from its checkout; docs/state.md, Files in the checkout).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **Every path is built from [`Context::home`]**, made full against the working folder, and
//!   never from an environment variable.
//! - **A file the operator writes under `%USERPROFILE%\.agentspace\operator` that cannot be
//!   written MUST fail the command before anything reads it**, with exactly
//!   `agentspace: Cannot write <path>.`, `<path>` the file's or folder's full path (FA-16): the
//!   first one whose creation or write failed, so an `operator` folder that cannot be made is
//!   named itself, never a file or folder below it (operator.md, What the prototype took from
//!   its checkout).

use std::io;
use std::path::{Path, PathBuf};

use agentspace_box::{CacheRead, InstanceIdCache};
use agentspace_operator_api::{CallerFailure, Context, Failure};

use crate::embedded::{BOX_SYSTEM, TEMPLATE};

/// The places under the home folder.
pub(crate) struct Places {
    /// `%USERPROFILE%\.agentspace`.
    pub(crate) agentspace: PathBuf,
    /// `%USERPROFILE%\.agentspace\operator`.
    pub(crate) operator: PathBuf,
}

impl Places {
    pub(crate) fn new(ctx: &Context) -> Places {
        let home = std::path::absolute(&ctx.home).unwrap_or_else(|_| ctx.home.clone());
        let agentspace = home.join(".agentspace");
        Places {
            operator: agentspace.join("operator"),
            agentspace,
        }
    }

    /// `%USERPROFILE%\.agentspace\ca\agentspace_ca.pub`.
    pub(crate) fn ca_public_key(&self) -> PathBuf {
        self.agentspace.join("ca").join("agentspace_ca.pub")
    }

    /// `%USERPROFILE%\.agentspace\stack-parameters.json`.
    pub(crate) fn stack_parameters(&self) -> PathBuf {
        self.agentspace.join("stack-parameters.json")
    }

    /// `%USERPROFILE%\.agentspace\instance-id.json`.
    pub(crate) fn instance_id_cache(&self) -> PathBuf {
        self.agentspace.join("instance-id.json")
    }

    /// `%USERPROFILE%\.agentspace\operator\localswim-linux-arm64`.
    pub(crate) fn localswim_build(&self) -> PathBuf {
        self.operator.join("localswim-linux-arm64")
    }

    /// `%USERPROFILE%\.claude\CLAUDE.md`.
    pub(crate) fn claude_md(&self) -> PathBuf {
        self.home().join(".claude").join("CLAUDE.md")
    }

    /// `%USERPROFILE%\.ssh\id_ed25519.pub`.
    pub(crate) fn user_public_key(&self) -> PathBuf {
        self.home().join(".ssh").join("id_ed25519.pub")
    }

    fn home(&self) -> &Path {
        self.agentspace.parent().unwrap_or(&self.agentspace)
    }
}

/// FA-16's failure for a file or folder that cannot be written.
pub(crate) fn cannot_write(path: &Path) -> Failure {
    Failure {
        message: format!("agentspace: Cannot write {}.", path.display()),
        caller: CallerFailure::CouldNotReach,
    }
}

/// Makes sure the operator's folder exists (state.md: created by the first command that writes in
/// it, with inherited access rules).
pub(crate) fn make_operator_folder(places: &Places) -> Result<(), Failure> {
    std::fs::create_dir_all(&places.operator).map_err(|_| cannot_write(&places.operator))
}

/// Writes the carried template to `%USERPROFILE%\.agentspace\operator\agentspace.yaml`, replacing
/// whatever was there, and returns its path.
pub(crate) fn write_template(places: &Places) -> Result<PathBuf, Failure> {
    make_operator_folder(places)?;
    let path = places.operator.join("agentspace.yaml");
    std::fs::write(&path, TEMPLATE).map_err(|_| cannot_write(&path))?;
    Ok(path)
}

/// Writes the carried `box\system` to `%USERPROFILE%\.agentspace\operator\system\`, after removing
/// whatever that folder held, so it holds exactly the carried files. Returns the folder's path.
pub(crate) fn write_system(places: &Places) -> Result<PathBuf, Failure> {
    make_operator_folder(places)?;
    let folder = places.operator.join("system");
    remove_any(&folder).map_err(|_| cannot_write(&folder))?;
    std::fs::create_dir(&folder).map_err(|_| cannot_write(&folder))?;
    for (relative, bytes) in BOX_SYSTEM {
        let mut path = folder.clone();
        path.extend(relative.split('/'));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| cannot_write(parent))?;
        }
        std::fs::write(&path, bytes).map_err(|_| cannot_write(&path))?;
    }
    Ok(folder)
}

/// Removes whatever is at `path`, a folder with all in it or a file; nothing there is not an
/// error. A link is removed, never followed.
fn remove_any(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path).or_else(|_| std::fs::remove_dir(path)),
    }
}

/// The instance-ID cache, `%USERPROFILE%\.agentspace\instance-id.json` (state.md, Files in the
/// checkout), read whole and replaced atomically through `agentspace-state`.
pub(crate) struct FileCache {
    path: PathBuf,
}

impl FileCache {
    pub(crate) fn new(places: &Places) -> FileCache {
        FileCache {
            path: places.instance_id_cache(),
        }
    }

    /// Deletes the cache (operator.md, deploy, step 5). A cache already missing is not an error.
    /// One that cannot be deleted, like one that cannot be written, fails with `agentspace: Cannot
    /// write <path>.`, `<path>` the cache's full path (operator.md, The test seam, "Not covered";
    /// Laptop files, The instance-ID cache).
    pub(crate) fn delete(&self) -> Result<(), Failure> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(cannot_write(&self.path)),
        }
    }
}

impl InstanceIdCache for FileCache {
    fn path(&self) -> &Path {
        &self.path
    }

    fn read(&self) -> CacheRead {
        CacheRead::from(std::fs::read(&self.path))
    }

    fn write(&self, bytes: &[u8]) -> Result<(), Failure> {
        // A replace never creates a folder (state.md, Replacing a file), so the folder comes first.
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| cannot_write(&self.path))?;
        }
        agentspace_state::write_atomically(&self.path, bytes).map_err(|_| cannot_write(&self.path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{Scratch, ctx_in};

    #[test]
    fn the_system_folder_holds_exactly_the_carried_files() {
        let dir = Scratch::new("files-system");
        let ctx = ctx_in(dir.path());
        let places = Places::new(&ctx);
        let stale = places.operator.join("system").join("stale.txt");
        std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
        std::fs::write(&stale, "old").unwrap();
        let folder = write_system(&places).unwrap();
        assert!(!stale.exists());
        let mut names: Vec<String> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        let want: Vec<&str> = BOX_SYSTEM.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, want);
        for (name, bytes) in BOX_SYSTEM {
            assert!(std::fs::read(folder.join(name)).unwrap() == *bytes);
        }
    }

    #[test]
    fn a_system_folder_that_is_a_file_is_replaced() {
        let dir = Scratch::new("files-system-file");
        let places = Places::new(&ctx_in(dir.path()));
        std::fs::create_dir_all(&places.operator).unwrap();
        std::fs::write(places.operator.join("system"), "x").unwrap();
        assert!(write_system(&places).unwrap().is_dir());
    }

    #[test]
    fn an_operator_folder_that_cannot_be_made_fails_with_fa16() {
        let dir = Scratch::new("files-fa16");
        let places = Places::new(&ctx_in(dir.path()));
        std::fs::create_dir_all(&places.agentspace).unwrap();
        // A file where the operator's folder belongs.
        std::fs::write(&places.operator, "x").unwrap();
        let want = format!("agentspace: Cannot write {}.", places.operator.display());
        assert_eq!(write_system(&places).unwrap_err().message, want);
        assert_eq!(write_template(&places).unwrap_err().message, want);
    }

    #[test]
    fn a_template_path_that_is_a_folder_fails_with_fa16() {
        let dir = Scratch::new("files-template");
        let places = Places::new(&ctx_in(dir.path()));
        let path = places.operator.join("agentspace.yaml");
        std::fs::create_dir_all(&path).unwrap();
        assert_eq!(
            write_template(&places).unwrap_err().message,
            format!("agentspace: Cannot write {}.", path.display())
        );
        std::fs::remove_dir(&path).unwrap();
        let written = write_template(&places).unwrap();
        assert!(std::fs::read(written).unwrap() == TEMPLATE);
    }

    #[test]
    fn the_cache_is_written_with_its_folder_and_deleted() {
        let dir = Scratch::new("files-cache");
        let places = Places::new(&ctx_in(dir.path()));
        let cache = FileCache::new(&places);
        assert_eq!(cache.read(), CacheRead::Missing);
        cache.delete().unwrap();
        cache.write(b"{}").unwrap();
        assert_eq!(cache.read(), CacheRead::Bytes(b"{}".to_vec()));
        cache.delete().unwrap();
        assert_eq!(cache.read(), CacheRead::Missing);
        std::fs::create_dir(cache.path()).unwrap();
        assert_eq!(cache.read(), CacheRead::Unopenable);
        let want = format!("agentspace: Cannot write {}.", cache.path().display());
        assert_eq!(cache.delete().unwrap_err().message, want);
        assert_eq!(cache.write(b"{}").unwrap_err().message, want);
    }
}
