//! The operator's folder, one per command (docs/operator.md, The operator's folder, one per command;
//! P13, architect, 2026-11-20, for v0.07-T01).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **A command writes what it sends into `%USERPROFILE%\.agentspace\operator\<hex>\`, which it
//!   makes new, and removes with everything in it as it ends**, whether it succeeded or failed. A
//!   command that sends no file makes none: the folder is made on the first call of
//!   [`CommandFolder::path`], just before its first file is written.
//! - **`<hex>` is 32 lowercase hexadecimal digits, drawn at random**, and a name that exists is
//!   drawn again, so a folder is never reused.
//! - **The sweep:** the command that makes its folder first removes every folder of
//!   `%USERPROFILE%\.agentspace\operator\` whose name is 32 lowercase hexadecimal digits and whose
//!   creation time is more than 24 hours before now. A folder it cannot remove is left and changes
//!   nothing else. Anything of another name is left.
//! - **A folder that cannot be made fails the command with `agentspace: Cannot write <path>.`**,
//!   `<path>` the first folder whose creation failed, so an `operator` folder that cannot be made is
//!   named itself and never the command's folder below it.

use std::cell::RefCell;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use agentspace_operator_api::Failure;

use crate::files::{Places, cannot_write};

/// How old a command's folder is before the sweep removes it.
const SWEEP_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// The command's folder, made on first use and removed by [`CommandFolder::remove`].
#[derive(Default)]
pub struct CommandFolder {
    made: RefCell<Option<PathBuf>>,
}

impl CommandFolder {
    pub fn new() -> CommandFolder {
        CommandFolder::default()
    }

    /// The command's folder, made (and the sweep run) on the first call, and the same path on every
    /// later one.
    pub(crate) fn path(&self, places: &Places) -> Result<PathBuf, Failure> {
        if let Some(path) = self.made.borrow().as_ref() {
            return Ok(path.clone());
        }
        std::fs::create_dir_all(&places.operator).map_err(|_| cannot_write(&places.operator))?;
        sweep(&places.operator, SystemTime::now());
        let path = make_new(&places.operator)?;
        *self.made.borrow_mut() = Some(path.clone());
        Ok(path)
    }

    /// Removes the folder with all in it, as the command ends. A folder that cannot be removed is
    /// left, and changes neither the command's messages nor its exit code.
    pub fn remove(&self) {
        if let Some(path) = self.made.borrow_mut().take() {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

/// Whether `name` is 32 lowercase hexadecimal digits.
pub(crate) fn is_command_folder_name(name: &str) -> bool {
    name.len() == 32
        && name
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// 32 random lowercase hexadecimal digits, from the standard library's randomly keyed hasher with
/// the time and the process mixed in.
fn random_name() -> String {
    let state = RandomState::new();
    let mut out = String::with_capacity(32);
    for round in 0..2u64 {
        let mut hasher = state.build_hasher();
        hasher.write_u64(round);
        hasher.write_u32(std::process::id());
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        hasher.write_u128(nanos);
        out.push_str(&format!("{:016x}", hasher.finish()));
    }
    out
}

/// Creates a new folder under `operator`, drawing another name when the one drawn exists.
fn make_new(operator: &Path) -> Result<PathBuf, Failure> {
    let mut last = operator.to_path_buf();
    for _ in 0..16 {
        let path = operator.join(random_name());
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => last = path,
            Err(_) => return Err(cannot_write(&path)),
        }
    }
    Err(cannot_write(&last))
}

/// Removes every command folder under `operator` created more than 24 hours before `now`.
pub(crate) fn sweep(operator: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(operator) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if !is_command_folder_name(&name) {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !meta.is_dir() {
            continue;
        }
        let Ok(created) = meta.created() else {
            continue;
        };
        if now.duration_since(created).is_ok_and(|age| age > SWEEP_AGE) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{Scratch, ctx_in};

    #[test]
    fn a_folder_is_new_each_time_and_removed_with_its_contents() {
        let dir = Scratch::new("folder-new");
        let places = Places::new(&ctx_in(dir.path()));
        let first = CommandFolder::new();
        let path = first.path(&places).unwrap();
        assert_eq!(first.path(&places).unwrap(), path);
        assert!(is_command_folder_name(
            path.file_name().unwrap().to_str().unwrap()
        ));
        std::fs::write(path.join("x"), "x").unwrap();
        let second = CommandFolder::new();
        assert_ne!(second.path(&places).unwrap(), path);
        first.remove();
        assert!(!path.exists());
        second.remove();
    }

    #[test]
    fn no_folder_is_made_until_asked_for() {
        let dir = Scratch::new("folder-lazy");
        let places = Places::new(&ctx_in(dir.path()));
        let folder = CommandFolder::new();
        folder.remove();
        assert!(!places.operator.exists());
    }

    #[test]
    fn the_sweep_removes_old_command_folders_and_nothing_else() {
        let dir = Scratch::new("folder-sweep");
        let places = Places::new(&ctx_in(dir.path()));
        let old = places.operator.join("0123456789abcdef0123456789abcdef");
        let other = places.operator.join("system");
        // Thirty-two characters, but not all hexadecimal digits.
        let upper = places.operator.join("0123456789abcdef0123456789abcdeg");
        for path in [&old, &other, &upper] {
            std::fs::create_dir_all(path).unwrap();
        }
        // A younger folder stays.
        sweep(&places.operator, SystemTime::now());
        assert!(old.exists());
        // Twenty-five hours later it is old.
        sweep(
            &places.operator,
            SystemTime::now() + Duration::from_secs(25 * 60 * 60),
        );
        assert!(!old.exists());
        assert!(other.exists() && upper.exists());
    }

    #[test]
    fn an_operator_folder_that_cannot_be_made_is_named_itself() {
        let dir = Scratch::new("folder-fa16");
        let places = Places::new(&ctx_in(dir.path()));
        std::fs::create_dir_all(&places.agentspace).unwrap();
        std::fs::write(&places.operator, "x").unwrap();
        let want = format!("agentspace: Cannot write {}.", places.operator.display());
        assert_eq!(
            CommandFolder::new().path(&places).unwrap_err().message,
            want
        );
    }
}
