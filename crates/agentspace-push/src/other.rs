//! The push's calls on a system that is not Windows. These are NOT THE CONTRACT'S: they exist so
//! that the crate builds and its portable logic is tested on Linux (operator.md, The token push is
//! the laptop's, and the laptop is Windows).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

use std::process::Child;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

/// There is no DPAPI here: every blob is refused.
pub fn unprotect(_blob: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    None
}

/// Waits until the process has exited or `deadline` passes, by polling. `true` when it has exited.
pub fn wait_for_exit(child: &mut Child, deadline: Instant) -> bool {
    loop {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Stops the ssh alone. The Windows half stops its whole tree; this does not. It waits a moment for
/// the killed process, so that it does not linger as a zombie.
pub fn terminate_tree(child: &mut Child) {
    let _ = child.kill();
    let _ = wait_for_exit(child, Instant::now() + Duration::from_secs(1));
}
