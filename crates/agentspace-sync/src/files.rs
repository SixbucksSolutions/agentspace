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

use agentspace_operator_api::{CallerFailure, Context, Failure, LocalswimTarget};

use crate::embedded::TEMPLATE;

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
/// one per command): `<folder>\<file name>`, the platform's, `localswim-linux-arm64`,
/// `localswim-linux-amd64` or `localswim-windows-amd64.exe` (the seam's sync row; D94).
pub(crate) fn localswim_build(folder: &Path, target: &LocalswimTarget) -> PathBuf {
    folder.join(target.file_name)
}

/// Writes the carried template to `<folder>\agentspace.yaml`, the command's own folder
/// ([`crate::folder::CommandFolder`]), replacing whatever was there, and returns its path.
pub(crate) fn write_template(_places: &Places, folder: &Path) -> Result<PathBuf, Failure> {
    let path = folder.join("agentspace.yaml");
    std::fs::write(&path, TEMPLATE).map_err(|_| cannot_write(&path))?;
    Ok(path)
}

/// Writes a platform's carried host files, the seam's sync row's `files` (`box\system` on Linux,
/// `box\windows` on Windows), to `<folder>\<name>\`, in the command's own folder, after removing
/// whatever that held, so it holds exactly the carried files. Returns that folder's path.
pub(crate) fn write_system(
    folder: &Path,
    name: &str,
    files: &[(&str, &[u8])],
) -> Result<PathBuf, Failure> {
    let folder = folder.join(name);
    remove_any(&folder).map_err(|_| cannot_write(&folder))?;
    std::fs::create_dir(&folder).map_err(|_| cannot_write(&folder))?;
    for (relative, bytes) in files {
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
    use agentspace_operator_api::HostPlatform;

    /// `box\system`'s files, as the seam's `linux/arm64` sync row carries them (E0.12-T03-S03
    /// part 2 deleted this crate's own copy; pin P1).
    fn box_system() -> &'static [(&'static str, &'static [u8])] {
        agentspace_platforms::LINUX_ARM64.sync_plan().unwrap().files
    }

    #[test]
    fn the_system_folder_holds_exactly_the_carried_files() {
        let dir = Scratch::new("files-system");
        let ctx = ctx_in(dir.path());
        let _places = Places::new(&ctx);
        let command = dir.path().join("command");
        let stale = command.join("system").join("stale.txt");
        std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
        std::fs::write(&stale, "old").unwrap();
        let folder = write_system(&command, "system", box_system()).unwrap();
        assert!(!stale.exists());
        let mut names: Vec<String> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        let want: Vec<&str> = box_system().iter().map(|(n, _)| *n).collect();
        assert_eq!(names, want);
        for (name, bytes) in box_system() {
            assert!(std::fs::read(folder.join(name)).unwrap() == *bytes);
        }
    }

    #[test]
    fn a_system_folder_that_is_a_file_is_replaced() {
        let dir = Scratch::new("files-system-file");
        let command = dir.path().join("command");
        std::fs::create_dir_all(&command).unwrap();
        std::fs::write(command.join("system"), "x").unwrap();
        assert!(
            write_system(&command, "system", box_system())
                .unwrap()
                .is_dir()
        );
    }

    #[test]
    fn a_system_folder_that_cannot_be_made_fails_with_fa16() {
        let dir = Scratch::new("files-fa16");
        // A file where the command's folder belongs.
        let command = dir.path().join("command");
        std::fs::write(&command, "x").unwrap();
        let want = format!(
            "agentspace: Cannot write {}.",
            command.join("system").display()
        );
        assert_eq!(
            write_system(&command, "system", box_system())
                .unwrap_err()
                .message,
            want
        );
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
