//! The launch files on the disk (docs/state.md, A holder's launch files): the claim, made in
//! create-new mode, and a whole read of the request or the result.

use agentspace_keeper_files::{Claim, serialize_claim};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

/// `FILE_SHARE_READ`: the claim is opened for writing, sharing read (state.md).
#[cfg(windows)]
const SHARE_READ: u32 = 0x1;
/// `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`: a reader of a keeper JSON file
/// SHOULD share all three (state.md, The keeper's files).
#[cfg(windows)]
const SHARE_ALL: u32 = 0x7;

/// Claims a request, or withdraws it, by creating its claim file in create-new mode, which exactly
/// one process can do, and writing `claim`'s line. `Err` means the file could not be created: it
/// exists, so another claimed it first, or the folder refused. A claim whose line cannot be
/// written once the file is made is still won, since the file's existence is the claim.
pub(crate) fn create_claim(path: &Path, claim: Claim) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(SHARE_READ);
    }
    let mut file = options.open(path)?;
    let _ = file.write_all(&serialize_claim(claim));
    Ok(())
}

/// Reads a whole file, sharing read, write and delete, so the reader never blocks a writer's
/// rename or the pass's removal.
pub(crate) fn read_whole(path: &Path) -> io::Result<Vec<u8>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(SHARE_ALL);
    }
    let mut file: File = options.open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Scratch;

    #[test]
    fn a_claim_is_made_once_and_holds_its_line() {
        let dir = Scratch::new("claim");
        let path = dir.path().join("board-holder-launch.ab.claim");
        create_claim(&path, Claim::Launcher(42)).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"launcher 42\n");
        let second = create_claim(&path, Claim::Abandoned).unwrap_err();
        assert_eq!(second.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&path).unwrap(), b"launcher 42\n");
    }

    #[test]
    fn a_whole_read_gives_the_bytes_and_a_missing_file_is_not_found() {
        let dir = Scratch::new("read");
        let path = dir.path().join("x.json");
        assert_eq!(
            read_whole(&path).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        std::fs::write(&path, b"{}\n").unwrap();
        assert_eq!(read_whole(&path).unwrap(), b"{}\n");
    }
}
