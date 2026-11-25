//! The launcher creates the holder (docs/keeper.md, Starting a holder, items 1 to 5) with
//! `CreateProcessW`:
//! - **Stdin is `NUL`; stdout and stderr are one handle that appends to the request's log**,
//!   opened for append only, sharing read, write and delete (docs/state.md, The keeper's logs).
//! - **It inherits those two handles and no other**: `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` names
//!   them, so no other inheritable handle of the launcher reaches it.
//! - **No window** (`CREATE_NO_WINDOW`, and `SW_HIDE` for a program that makes one), **a new
//!   process group** (`CREATE_NEW_PROCESS_GROUP`), and **the request's working folder**.
//! - **It leaves the launcher's job where Windows allows**: when the launcher is in a job, the
//!   create asks for `CREATE_BREAKAWAY_FROM_JOB`, and only where Windows refuses that with access
//!   denied is the create made again without it, in the launcher's job (item 5). A refused create
//!   makes no process, so at most one holder is made (item 6).
//! - **Its environment is the launcher's**, which WMI gave the user's logon environment.
//!
//! The holder is created suspended, so its creation time and command line are read back before it
//! runs. One whose creation time cannot be read is ended before it ever ran, and the start fails
//! with `The holder's start time could not be read.`, so no holder runs that its pass is not told
//! of.
//!
//! **A holder is never left suspended.** It is let run before `spawn` returns, so before the
//! launcher writes its result: a launcher that ends between the two leaves a holder that runs
//! (keeper.md, The handshake, step 4: "the holder runs on"), never one that a pass would find
//! running and trust while it holds no forward. Every other way out of `spawn` ends it.
//!
//! **The create never waits on a person (#0191).** For a program file Windows takes for a 16-bit
//! one (a `.exe` that is not a Windows program), `CreateProcessW` raises a modal "Unsupported
//! 16-Bit Application" message box on the desktop and does not return until someone closes it,
//! unless the calling thread has asked for no critical-error message boxes. The create calls run
//! with `SEM_FAILCRITICALERRORS` set for this thread alone (`NoErrorBoxes`), so the create fails
//! without waiting, with Windows' own text, which the launcher answers (keeper.md, The handshake,
//! step 4). The thread's mode is put back afterwards, and the process's mode, which the holder
//! inherits, is never changed.

use super::process::{command_line, creation_time};
use super::{Owned, error_text, wide};
use crate::launcher::Made;
use agentspace_keeper_files::LaunchRequest;
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, GENERIC_READ, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_ALWAYS, OPEN_EXISTING, SYNCHRONIZE,
};
use windows_sys::Win32::System::Diagnostics::Debug::{
    GetThreadErrorMode, SEM_FAILCRITICALERRORS, SetThreadErrorMode,
};
use windows_sys::Win32::System::JobObjects::IsProcessInJob;
use windows_sys::Win32::System::Threading::{
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CREATE_SUSPENDED,
    CreateProcessW, DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess,
    InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION, ResumeThread, STARTF_USESHOWWINDOW,
    STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
};

pub(crate) const NUL_ERROR: &str = "NUL could not be opened.";
pub(crate) const LOG_ERROR: &str = "The holder's log could not be opened.";
pub(crate) const TIME_ERROR: &str = "The holder's start time could not be read.";

/// `SW_HIDE`.
const SW_HIDE: u16 = 0;

/// Opens a file with an inheritable handle, or `None`.
fn open_inheritable(path: &str, access: u32, share: u32, disposition: u32) -> Option<Owned> {
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };
    let name = wide(path);
    // SAFETY: the name ends in a NUL and the attributes are ours; the handle is checked.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            access,
            share,
            &attributes,
            disposition,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    (handle != INVALID_HANDLE_VALUE && !handle.is_null()).then_some(Owned(handle))
}

/// A process attribute list naming the only handles the holder inherits.
struct HandleList {
    buffer: Vec<u64>,
    handles: Box<[HANDLE; 2]>,
}

impl HandleList {
    fn new(handles: [HANDLE; 2]) -> Result<HandleList, String> {
        let mut size: usize = 0;
        // SAFETY: a call with no list only gives the size one needs.
        unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size) };
        let mut buffer = vec![0u64; size.div_ceil(8) + 1];
        // SAFETY: the buffer is ours and at least `size` bytes.
        let ok = unsafe {
            InitializeProcThreadAttributeList(buffer.as_mut_ptr().cast(), 1, 0, &mut size)
        };
        if ok == 0 {
            // SAFETY: reads this thread's last error.
            return Err(error_text(unsafe { GetLastError() }));
        }
        // Only an initialized list becomes a `HandleList`, whose drop deletes it.
        let mut list = HandleList {
            buffer,
            handles: Box::new(handles),
        };
        // SAFETY: the list was made above; the handle array lives as long as the list, in a box
        // that never moves.
        let ok = unsafe {
            UpdateProcThreadAttribute(
                list.raw(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                list.handles.as_ptr().cast(),
                std::mem::size_of::<[HANDLE; 2]>(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if ok == 0 {
            // SAFETY: reads this thread's last error.
            return Err(error_text(unsafe { GetLastError() }));
        }
        Ok(list)
    }

    fn raw(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.buffer.as_mut_ptr().cast()
    }
}

impl Drop for HandleList {
    fn drop(&mut self) {
        // SAFETY: the list was initialized in `new` before any value of this type existed.
        unsafe { DeleteProcThreadAttributeList(self.raw()) };
    }
}

/// Windows' critical-error message boxes, off for the calling thread while this lives: its error
/// mode gains `SEM_FAILCRITICALERRORS`, and is put back as it was when this drops. A mode that
/// cannot be set leaves the thread as it was, and the create goes ahead.
struct NoErrorBoxes {
    old: Option<u32>,
}

impl NoErrorBoxes {
    fn new() -> NoErrorBoxes {
        let mut old = 0u32;
        // SAFETY: reads and sets this thread's own error mode; the out-parameter is ours.
        let ok =
            unsafe { SetThreadErrorMode(GetThreadErrorMode() | SEM_FAILCRITICALERRORS, &mut old) };
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

#[cfg(test)]
thread_local! {
    /// This thread's error mode at its last create call, for this module's tests.
    static MODE_AT_CREATE: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

/// Whether this process is in a job.
fn in_a_job() -> bool {
    let mut answer = 0;
    // SAFETY: a pseudo handle to this process, no job handle, and an out-parameter of ours.
    let ok = unsafe { IsProcessInJob(GetCurrentProcess(), std::ptr::null_mut(), &mut answer) };
    ok != 0 && answer != 0
}

/// Creates the holder the request names, suspended; reads back its creation time and command
/// line; then lets it run.
pub(crate) fn spawn(request: &LaunchRequest) -> Result<Made, String> {
    let nul = open_inheritable(
        "NUL",
        GENERIC_READ,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        OPEN_EXISTING,
    )
    .ok_or_else(|| NUL_ERROR.to_string())?;
    let log = open_inheritable(
        &request.log,
        FILE_APPEND_DATA | SYNCHRONIZE,
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        OPEN_ALWAYS,
    )
    .ok_or_else(|| LOG_ERROR.to_string())?;
    let mut list = HandleList::new([nul.0, log.0])?;

    // SAFETY: an all-zero STARTUPINFOEXW is its documented empty value.
    let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES | STARTF_USESHOWWINDOW;
    startup.StartupInfo.wShowWindow = SW_HIDE;
    startup.StartupInfo.hStdInput = nul.0;
    startup.StartupInfo.hStdOutput = log.0;
    startup.StartupInfo.hStdError = log.0;
    startup.lpAttributeList = list.raw();

    let flags = EXTENDED_STARTUPINFO_PRESENT
        | CREATE_NO_WINDOW
        | CREATE_NEW_PROCESS_GROUP
        | CREATE_SUSPENDED;
    let application = wide(&request.application);
    let directory = wide(&request.directory);
    let create = |flags: u32| -> Result<PROCESS_INFORMATION, u32> {
        // CreateProcessW may write to the command line's buffer, so each try gets its own.
        let mut line = wide(&request.command_line);
        // SAFETY: an all-zero PROCESS_INFORMATION is an empty out-parameter.
        let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        #[cfg(test)]
        // SAFETY: reads this thread's own error mode.
        MODE_AT_CREATE.with(|m| m.set(Some(unsafe { GetThreadErrorMode() })));
        // SAFETY: every string ends in a NUL and outlives the call; the startup information and
        // its attribute list are ours and initialized; inheritance is limited to the list.
        let ok = unsafe {
            CreateProcessW(
                application.as_ptr(),
                line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                flags,
                std::ptr::null(),
                directory.as_ptr(),
                &startup.StartupInfo,
                &mut info,
            )
        };
        if ok == 0 {
            // SAFETY: reads this thread's last error.
            Err(unsafe { GetLastError() })
        } else {
            Ok(info)
        }
    };
    let no_boxes = NoErrorBoxes::new();
    let made = if in_a_job() {
        match create(flags | CREATE_BREAKAWAY_FROM_JOB) {
            Err(ERROR_ACCESS_DENIED) => create(flags),
            other => other,
        }
    } else {
        create(flags)
    };
    drop(no_boxes);
    let info = made.map_err(error_text)?;
    let holder = Suspended {
        process: Owned(info.hProcess),
        thread: Owned(info.hThread),
        runs: false,
    };
    drop(list);

    // A holder created suspended whose start time cannot be read is terminated before it runs,
    // and the launcher answers `The holder's start time could not be read.` (keeper.md, The
    // handshake, step 7, the rest of the rulings; architect, 2026-11-19, for E0.06-T02-S04).
    let Some(process_started_at) = creation_time(holder.process.0) else {
        return Err(TIME_ERROR.to_string());
    };
    let command_line = command_line(holder.process.0);
    holder.resume()?;
    Ok(Made {
        pid: info.dwProcessId,
        process_started_at,
        command_line,
    })
}

/// A holder created suspended. It is let run by `resume`, and on every other way out of `spawn`
/// (an error, or a panic that unwinds) it is ended before it ever ran: a holder is never left
/// suspended, where a pass would find it running, trust it, and wait on forwards it never opens.
struct Suspended {
    process: Owned,
    thread: Owned,
    runs: bool,
}

impl Suspended {
    /// Lets the holder run, or ends it and gives Windows' text for why it could not be let run.
    fn resume(mut self) -> Result<(), String> {
        // SAFETY: the thread handle is the new process's first thread, created suspended.
        if unsafe { ResumeThread(self.thread.0) } == u32::MAX {
            // SAFETY: reads this thread's last error.
            return Err(error_text(unsafe { GetLastError() }));
        }
        self.runs = true;
        Ok(())
    }
}

impl Drop for Suspended {
    fn drop(&mut self) {
        if !self.runs {
            // SAFETY: the process is ours and has never run.
            unsafe { TerminateProcess(self.process.0, 1) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Scratch;

    /// A request whose program is not there. Its create fails at once with `ERROR_FILE_NOT_FOUND`
    /// whatever the error mode, so this module's tests never raise a message box, with the guard
    /// or without it.
    fn missing_program(dir: &std::path::Path) -> LaunchRequest {
        let application = dir.join("nowhere").join("ssh.exe");
        LaunchRequest {
            application: application.to_string_lossy().into_owned(),
            command_line: format!("\"{}\" -N", application.display()),
            log: dir
                .join("board-holder-ssh.log")
                .to_string_lossy()
                .into_owned(),
            directory: dir.to_string_lossy().into_owned(),
            not_after: "2099-01-01T00:00:00.0000000Z".to_string(),
            result: dir.join("result.json").to_string_lossy().into_owned(),
        }
    }

    /// Runs `spawn` on a thread of its own whose error mode is `mode` before the call, and gives
    /// the thread's mode at the create call, its mode after `spawn` returned, and the outcome.
    fn spawn_on_a_thread_in(
        mode: u32,
        request: LaunchRequest,
    ) -> (Option<u32>, u32, Result<Made, String>) {
        std::thread::spawn(move || {
            // SAFETY: sets this thread's own error mode; no old mode is asked for.
            let set = unsafe { SetThreadErrorMode(mode, std::ptr::null_mut()) };
            assert_ne!(set, 0, "The test thread's error mode can be set.");
            let out = spawn(&request);
            let at_create = MODE_AT_CREATE.with(|m| m.get());
            // SAFETY: reads this thread's own error mode.
            let after = unsafe { GetThreadErrorMode() };
            (at_create, after, out)
        })
        .join()
        .expect("The spawn thread did not panic.")
    }

    #[test]
    fn the_holder_is_created_with_windows_message_boxes_off_and_the_thread_mode_is_put_back() {
        let dir = Scratch::new("error-boxes");
        for mode in [0, SEM_FAILCRITICALERRORS] {
            let (at_create, after, out) = spawn_on_a_thread_in(mode, missing_program(dir.path()));
            let at_create = at_create.expect("The create was called.");
            assert_ne!(
                at_create & SEM_FAILCRITICALERRORS,
                0,
                "The create runs with critical-error message boxes off (mode {mode:#x} before), \
                 so it cannot wait on a person (#0191)."
            );
            assert_eq!(
                after, mode,
                "The thread's own mode is put back after the create."
            );
            let error = out.expect_err("A program that is not there is not started.");
            assert!(!error.is_empty(), "Windows' own text says why.");
        }
    }

    #[test]
    fn no_error_boxes_sets_the_flag_and_puts_back_the_mode_it_found() {
        std::thread::spawn(|| {
            // SAFETY: sets this thread's own error mode; no old mode is asked for.
            unsafe { SetThreadErrorMode(0, std::ptr::null_mut()) };
            {
                let _guard = NoErrorBoxes::new();
                // SAFETY: reads this thread's own error mode.
                let during = unsafe { GetThreadErrorMode() };
                assert_eq!(during & SEM_FAILCRITICALERRORS, SEM_FAILCRITICALERRORS);
            }
            // SAFETY: reads this thread's own error mode.
            assert_eq!(unsafe { GetThreadErrorMode() }, 0);
        })
        .join()
        .expect("The guard's thread did not panic.");
    }
}
