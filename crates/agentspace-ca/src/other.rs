//! The build for a platform that is not Windows. NOT THE CONTRACT'S (docs/state.md, Access rules:
//! a caller that is not Windows is later, and not blocking, D8).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **This build exists so that the crate compiles and its portable half is tested on Linux**
//!   (D38). It MUST NOT be taken for an implementation of the access rules.
//! - **Every function here MUST fail closed:** no user is known, no rule is read or set. So `ca`
//!   and `check_for_sync` fail on such a platform, which is the safe answer until a later release
//!   builds the Unix rules.

use std::path::Path;

use crate::Host;
use crate::access::Rules;

const NOT_BUILT: &str = "Access rules are not built for this platform.";

/// Not the contract's: a host that knows no user and reads and sets no rule.
pub(crate) struct Unsupported;

impl Host for Unsupported {
    fn user_sid(&self) -> Result<String, String> {
        Err(NOT_BUILT.to_string())
    }

    fn account_name(&self, _sid: &str) -> Option<String> {
        None
    }

    fn user_name(&self) -> Option<String> {
        None
    }

    /// No elevation is known here; the lock that follows fails instead.
    fn elevated(&self) -> Result<bool, String> {
        Ok(false)
    }

    fn rules(&self, _path: &Path) -> Result<Rules, String> {
        Err(NOT_BUILT.to_string())
    }

    fn lock_folder(&self, _path: &Path, _user_sid: &str) -> Result<(), String> {
        Err(NOT_BUILT.to_string())
    }

    fn lock_key(&self, _path: &Path, _user_sid: &str) -> Result<(), String> {
        Err(NOT_BUILT.to_string())
    }

    fn local_date(&self) -> String {
        crate::utc_date(std::time::SystemTime::now())
    }
}
