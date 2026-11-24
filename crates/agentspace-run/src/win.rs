//! The one module of this crate that calls Windows, and so the only one with `unsafe` code.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! **Every program this crate starts MUST be started while a [`NoErrorBoxes`] lives on the
//! starting thread** (#0191; the architect's ruling for E0.10-T02-S04, option B). `CreateProcessW`,
//! handed a file named `.exe` that holds no PE image, takes it for an MS-DOS or 16-bit program,
//! which 64-bit Windows cannot run. On a thread whose error mode lacks `SEM_FAILCRITICALERRORS`,
//! Windows then puts a modal "Unsupported 16-Bit Application" dialog on the desktop and the start
//! waits for a person to press OK. With the bit set, the start fails at once with error 216
//! (`ERROR_EXE_MACHINE_TYPE_MISMATCH`), which the caller answers as a program that cannot be run.

use windows_sys::Win32::System::Diagnostics::Debug::{
    GetThreadErrorMode, SEM_FAILCRITICALERRORS, SEM_NOOPENFILEERRORBOX, SetThreadErrorMode,
};

/// The bits a start runs under: no critical-error message box, and no message box when a file
/// cannot be found or opened.
pub(crate) const QUIET: u32 = SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX;

/// Windows' error message boxes, off for the calling thread while this lives: its error mode
/// gains [`QUIET`]'s bits, and is put back as it was when this drops, however the start ended. A
/// mode that cannot be set leaves the thread as it was, and the start goes ahead.
pub(crate) struct NoErrorBoxes {
    old: Option<u32>,
}

impl NoErrorBoxes {
    pub(crate) fn new() -> NoErrorBoxes {
        let mut old = 0u32;
        // SAFETY: reads and sets this thread's own error mode; the out-parameter is a local.
        let ok = unsafe { SetThreadErrorMode(GetThreadErrorMode() | QUIET, &mut old) };
        NoErrorBoxes {
            old: (ok != 0).then_some(old),
        }
    }
}

impl Drop for NoErrorBoxes {
    fn drop(&mut self) {
        if let Some(old) = self.old {
            // SAFETY: sets this thread's own error mode back; no old mode is asked for.
            unsafe { SetThreadErrorMode(old, std::ptr::null_mut()) };
        }
    }
}

/// This thread's error mode.
#[cfg(test)]
pub(crate) fn thread_error_mode() -> u32 {
    // SAFETY: reads this thread's own error mode.
    unsafe { GetThreadErrorMode() }
}

/// Sets this thread's error mode to `mode`.
#[cfg(test)]
pub(crate) fn set_thread_error_mode(mode: u32) {
    // SAFETY: sets this thread's own error mode; no old mode is asked for.
    unsafe { SetThreadErrorMode(mode, std::ptr::null_mut()) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quiet_bits_hold_while_it_lives_and_the_old_mode_comes_back_after() {
        // Run on a fresh thread, so no other test's mode is touched.
        std::thread::spawn(|| {
            set_thread_error_mode(0);
            {
                let _quiet = NoErrorBoxes::new();
                assert_eq!(thread_error_mode() & QUIET, QUIET);
            }
            assert_eq!(thread_error_mode(), 0);
            set_thread_error_mode(SEM_FAILCRITICALERRORS);
            {
                let _quiet = NoErrorBoxes::new();
                assert_eq!(thread_error_mode() & QUIET, QUIET);
            }
            assert_eq!(thread_error_mode(), SEM_FAILCRITICALERRORS);
        })
        .join()
        .unwrap();
    }
}
