//! The launcher is created through Windows Management Instrumentation's
//! `Win32_Process.Create` (docs/keeper.md, The handshake, step 2), which makes it outside the
//! pass's job and every job above it, with the user's logon environment. The call itself is
//! agentspace-detach's `wmi::create`, which this crate shares with the detached start of a
//! provisioning run (D48, D55). It is given no environment, so `Win32_ProcessStartup` gives the
//! launcher no window (`ShowWindow` 0, `SW_HIDE`) and sets nothing else, and its environment is
//! WMI's: the logon one.
//!
//! One call runs on a thread of its own, with COM initialized there, so the pass can abandon it at
//! its 10-second bound (`handshake::bounded`).
//!
//! **An abandoned call's thread is left to run, and is never joined.** It owns all it uses: its
//! copies of the command line and the folder, its own COM initialization and every interface it
//! holds. When the call returns at last, its answer goes to a channel nobody reads, the interfaces
//! are released, and the thread uninitializes COM itself and ends. A call that never returns keeps
//! its thread until the process ends, which ends the thread with it: one thread for each abandoned
//! try, at most 3 for a start. The launcher such a call makes late is the claim's to stop, never
//! this module's (keeper.md, The handshake, steps 2 and 3).

use crate::handshake::Create;
use std::ffi::OsStr;

/// One create call of the launcher, unbounded: the bound is the caller's.
pub(crate) fn create(command_line: &str, directory: &str) -> Create {
    match agentspace_detach::wmi::create(OsStr::new(command_line), OsStr::new(directory), &[]) {
        Ok(code) => Create::Returned(code),
        Err(error) => Create::Threw(error),
    }
}
