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

    /// `%USERPROFILE%\.claude\CLAUDE.md`.
    pub(crate) fn claude_md(&self) -> PathBuf {
        self.home().join(".claude").join("CLAUDE.md")
    }

    /// `%USERPROFILE%\.ssh\id_ed25519.pub`.
    pub(crate) fn user_public_key(&self) -> PathBuf {
        self.home().join(".ssh").join("id_ed25519.pub")
    }

    /// `%USERPROFILE%`, made full.
    pub(crate) fn home(&self) -> &Path {
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

/// The localswim CLI's build output in the command's folder (operator.md, The operator's folder,
/// one per command): `<folder>\localswim-linux-arm64`.
pub(crate) fn localswim_build(folder: &Path) -> PathBuf {
    folder.join("localswim-linux-arm64")
}

/// Writes the carried template to `<folder>\agentspace.yaml`, the command's own folder
/// ([`crate::folder::CommandFolder`]), replacing whatever was there, and returns its path.
pub(crate) fn write_template(_places: &Places, folder: &Path) -> Result<PathBuf, Failure> {
    let path = folder.join("agentspace.yaml");
    std::fs::write(&path, TEMPLATE).map_err(|_| cannot_write(&path))?;
    Ok(path)
}

/// Writes the carried `box\system` to `<folder>\system\`, in the command's own folder, after
/// removing whatever that held, so it holds exactly the carried files. Returns the system
/// folder's path.
pub(crate) fn write_system(_places: &Places, folder: &Path) -> Result<PathBuf, Failure> {
    let folder = folder.join("system");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{Scratch, ctx_in};

    #[test]
    fn the_system_folder_holds_exactly_the_carried_files() {
        let dir = Scratch::new("files-system");
        let ctx = ctx_in(dir.path());
        let places = Places::new(&ctx);
        let command = dir.path().join("command");
        let stale = command.join("system").join("stale.txt");
        std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
        std::fs::write(&stale, "old").unwrap();
        let folder = write_system(&places, &command).unwrap();
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
        let command = dir.path().join("command");
        std::fs::create_dir_all(&command).unwrap();
        std::fs::write(command.join("system"), "x").unwrap();
        assert!(write_system(&places, &command).unwrap().is_dir());
    }

    #[test]
    fn a_system_folder_that_cannot_be_made_fails_with_fa16() {
        let dir = Scratch::new("files-fa16");
        let places = Places::new(&ctx_in(dir.path()));
        // A file where the command's folder belongs.
        let command = dir.path().join("command");
        std::fs::write(&command, "x").unwrap();
        let want = format!(
            "agentspace: Cannot write {}.",
            command.join("system").display()
        );
        assert_eq!(write_system(&places, &command).unwrap_err().message, want);
    }

    #[test]
    fn a_template_path_that_is_a_folder_fails_with_fa16() {
        let dir = Scratch::new("files-template");
        let places = Places::new(&ctx_in(dir.path()));
        let command = dir.path().join("command");
        let path = command.join("agentspace.yaml");
        std::fs::create_dir_all(&path).unwrap();
        assert_eq!(
            write_template(&places, &command).unwrap_err().message,
            format!("agentspace: Cannot write {}.", path.display())
        );
        std::fs::remove_dir(&path).unwrap();
        let written = write_template(&places, &command).unwrap();
        assert!(std::fs::read(written).unwrap() == TEMPLATE);
    }
}
