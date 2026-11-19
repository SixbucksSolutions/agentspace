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

use std::fs::File;
use std::path::Path;

/// Why `create_private_file` failed. The second variant is the Windows build's; it is kept so that
/// the portable code compiles against the same type.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateError {
    /// No file of this call's making exists at the path.
    NotCreated,
    /// The file was made, and could not be removed again.
    CreatedButFailed,
}

/// Not the contract's: nothing is owner-only on this platform.
pub fn owner_only(_path: &Path) -> bool {
    false
}

/// Not the contract's: no private key's rules are verified on this platform.
pub fn verify_private_key(_path: &Path) -> bool {
    false
}

/// Not the contract's: no private file is made on this platform.
pub fn create_private_file(_path: &Path) -> Result<File, CreateError> {
    Err(CreateError::NotCreated)
}
