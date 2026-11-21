//! Folders that are made when first needed, and the file where a folder belongs (docs/state.md, The
//! home folder: every folder is created by whichever writer needs it, a missing folder is never an
//! error for a reader, and a value that names a file makes every command that needs the folder
//! fail, writing nothing in it).

use crate::error::Failure;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

/// Whether a file stands where `dir`, or a folder above it, belongs: walks up from `dir`
/// (itself included) to its first existing ancestor and says whether that is something other than
/// a folder. A link is followed, as a folder MAY be a link to a folder. A path that is missing, or
/// under a file, moves the walk up one level; any other failure to look ends it with `false`, as
/// nothing is known.
pub fn is_blocked(dir: &Path) -> bool {
    for ancestor in dir.ancestors() {
        match fs::metadata(ancestor) {
            Ok(m) => return !m.is_dir(),
            Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {}
            Err(_) => return false,
        }
    }
    false
}

/// Makes `dir` and every missing folder above it. A file where a folder belongs, or a folder that
/// cannot be made, is an error and makes nothing.
pub fn ensure(dir: &Path) -> Result<(), Failure> {
    if is_blocked(dir) {
        return Err(Failure::FolderIsAFile);
    }
    fs::create_dir_all(dir).map_err(|_| Failure::CannotMakeFolder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_at_a_folder_or_above_it_blocks_and_makes_nothing() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("a-file");
        fs::write(&f, b"x").unwrap();
        assert!(is_blocked(&f));
        assert!(is_blocked(&f.join("below").join("further")));
        assert_eq!(ensure(&f.join("below")), Err(Failure::FolderIsAFile));
        assert!(!f.join("below").exists());
        assert!(!is_blocked(&t.path().join("missing").join("x")));
        ensure(&t.path().join("missing").join("x")).unwrap();
        assert!(t.path().join("missing").join("x").is_dir());
    }
}
