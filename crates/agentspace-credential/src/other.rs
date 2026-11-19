//! The build for a platform that is not Windows. NOT THE CONTRACT'S (docs/state.md, "Access
//! rules": a caller that is not Windows is later, and not blocking, D8).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **This build exists so that the crate compiles and its portable half is tested on Linux**
//!   (D38). It MUST NOT be taken for an implementation of the access rules.
//! - **Every function here MUST fail closed:** nothing is reported owner-only or protected, and no
//!   private file is made. So `check_ca`, `issue` and `renew` refuse on such a platform, which is
//!   the safe answer until a later release builds the Unix rules.

use std::fs::{self, File};
use std::io;
use std::path::Path;

/// Not the contract's: nothing is owner-only on this platform.
pub fn owner_only(_path: &Path) -> bool {
    false
}

/// Not the contract's: no private key's rules are verified on this platform.
pub fn verify_private_key(_path: &Path) -> bool {
    false
}

/// Not the contract's: no private file is made on this platform.
pub fn create_private_file(_path: &Path) -> Option<File> {
    None
}

/// Whether `meta` (from `symlink_metadata`) is a link, which MUST NOT be entered or followed.
pub fn is_link(meta: &fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}

/// Removes a link itself and nothing it points to.
pub fn remove_link(path: &Path) -> io::Result<()> {
    fs::remove_file(path)
}
