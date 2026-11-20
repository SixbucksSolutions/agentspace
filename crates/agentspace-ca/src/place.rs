//! Step 2 of `ca`: the CA's folder MUST NOT be inside a OneDrive folder or a git working tree
//! (docs/operator.md, ca, and What the prototype took from its checkout; D37).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **Paths MUST be compared without regard to case**, as whole folder names: `C:\One` holds
//!   `C:\One\x` and `C:\One` itself, never `C:\OneMore`.
//! - **A git working tree is a folder that holds an entry named `.git`**, a folder or the file a git
//!   worktree has, at the CA's folder or any folder above it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Where the CA's folder is that it MUST NOT be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Inside {
    /// `<folder>` of the message: the OneDrive variable's value as it was set, or the working
    /// tree's folder.
    pub(crate) folder: PathBuf,
}

/// A path as text for comparison: full, with `\` separators, no trailing separator, lower case.
fn comparable(path: &Path) -> String {
    let full = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let text = full.to_string_lossy().replace('/', "\\").to_lowercase();
    text.trim_end_matches('\\').to_string()
}

/// Whether `inner` is `outer` or inside it, compared without regard to case.
pub(crate) fn is_within(inner: &Path, outer: &Path) -> bool {
    let inner = comparable(inner);
    let outer = comparable(outer);
    if outer.is_empty() {
        return false;
    }
    inner == outer || inner.starts_with(&format!("{outer}\\"))
}

/// The first OneDrive folder, in the order given, that holds `ca_folder`.
///
/// `one_drive` holds the values of `OneDrive`, `OneDriveConsumer` and `OneDriveCommercial`, in that
/// order, each one that is set and not empty.
pub(crate) fn inside_one_drive(ca_folder: &Path, one_drive: &[OsString]) -> Option<Inside> {
    one_drive
        .iter()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .find(|folder| is_within(ca_folder, folder))
        .map(|folder| Inside { folder })
}

/// The nearest git working tree that holds `ca_folder`: the CA's folder itself, or the nearest
/// folder above it, that holds an entry named `.git`.
pub(crate) fn inside_git(ca_folder: &Path) -> Option<Inside> {
    let full = std::path::absolute(ca_folder).unwrap_or_else(|_| ca_folder.to_path_buf());
    full.ancestors()
        .filter(|folder| !folder.as_os_str().is_empty())
        .find(|folder| std::fs::symlink_metadata(folder.join(".git")).is_ok())
        .map(|folder| Inside {
            folder: folder.to_path_buf(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_holds_itself_and_what_is_below_it_and_nothing_else() {
        let base = std::env::temp_dir().join("agentspace-ca-place");
        let one = base.join("One");
        assert!(is_within(&one, &one));
        assert!(is_within(&one.join("x").join("ca"), &one));
        assert!(!is_within(&base.join("OneMore").join("ca"), &one));
        assert!(!is_within(&base, &one));
        assert!(!is_within(&one, Path::new("")));
    }

    #[cfg(windows)]
    #[test]
    fn folders_compare_without_regard_to_case_and_as_whole_names() {
        assert!(is_within(Path::new(r"C:\One\x\ca"), Path::new(r"c:\ONE")));
        assert!(is_within(Path::new(r"C:\One"), Path::new(r"C:\One\")));
        assert!(is_within(Path::new(r"C:\One\ca"), Path::new("C:/one")));
        assert!(!is_within(
            Path::new(r"C:\OneMore\ca"),
            Path::new(r"C:\One")
        ));
        assert!(!is_within(Path::new(r"C:\ca"), Path::new(r"C:\One")));
        assert!(is_within(Path::new(r"C:\ca"), Path::new(r"C:\")));
    }

    #[cfg(windows)]
    #[test]
    fn the_first_one_drive_folder_that_holds_it_is_named_as_set() {
        let set = [
            OsString::from(r"C:\Elsewhere"),
            OsString::from(r"c:\users\SOMEONE\"),
            OsString::from(r"C:\Users"),
        ];
        let found = inside_one_drive(Path::new(r"C:\Users\someone\.agentspace\ca"), &set);
        assert_eq!(found.unwrap().folder, PathBuf::from(r"c:\users\SOMEONE\"));
        assert!(inside_one_drive(Path::new(r"D:\ca"), &set).is_none());
        assert!(inside_one_drive(Path::new(r"D:\ca"), &[OsString::new()]).is_none());
    }
}
