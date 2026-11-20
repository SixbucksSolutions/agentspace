//! The Windows calls the installer makes outside Task Scheduler: the current user's account name
//! and the clock, in UTC. Neither is a COM call. This module and [`crate::scheduler`] are the only ones
//! that use `unsafe`.

use crate::clock::{Now, Parts, now_from_parts};
use windows_sys::Win32::Foundation::{FILETIME, GetLastError, SYSTEMTIME};
use windows_sys::Win32::Security::Authentication::Identity::{GetUserNameExW, NameSamCompatible};
use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use windows_sys::Win32::System::Time::FileTimeToSystemTime;

/// `ERROR_MORE_DATA`.
const ERROR_MORE_DATA: u32 = 234;

/// The current user's account as Windows names it, `DOMAIN\user` (for a local account, the
/// computer's name, a backslash and the user name): `GetUserNameExW` with `NameSamCompatible`.
pub fn current_user() -> Result<String, String> {
    let mut buffer: Vec<u16> = vec![0; 256];
    for _ in 0..2 {
        let mut size = buffer.len() as u32;
        // SAFETY: `buffer` is valid for `size` u16 values, and `size` is a valid pointer; the
        // call writes at most `size` values and sets `size` to the count written (without the
        // terminator), or to the count needed when it fails with ERROR_MORE_DATA.
        let ok = unsafe { GetUserNameExW(NameSamCompatible, buffer.as_mut_ptr(), &mut size) };
        if ok {
            buffer.truncate(size as usize);
            return String::from_utf16(&buffer)
                .map_err(|_| "The current user's name is not valid text.".to_string());
        }
        // SAFETY: A plain read of the calling thread's last error.
        let code = unsafe { GetLastError() };
        if code != ERROR_MORE_DATA {
            return Err(format!(
                "The current user's name could not be read (Windows error {code})."
            ));
        }
        buffer = vec![0; (size as usize).max(buffer.len() * 2)];
    }
    Err("The current user's name could not be read.".to_string())
}

fn parts_of(time: &SYSTEMTIME) -> Parts {
    Parts {
        year: i32::from(time.wYear),
        month: u32::from(time.wMonth),
        day: u32::from(time.wDay),
        hour: u32::from(time.wHour),
        minute: u32::from(time.wMinute),
        second: u32::from(time.wSecond),
    }
}

/// The time now, in UTC.
pub fn now() -> Result<Now, String> {
    let mut file_time = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    // SAFETY: `file_time` is a valid FILETIME to write.
    unsafe { GetSystemTimeAsFileTime(&mut file_time) };
    // SAFETY: A plain struct of integers, for which all-zero is a valid value.
    let mut utc: SYSTEMTIME = unsafe { std::mem::zeroed() };
    // SAFETY: Both pointers are valid for the call.
    let converted = unsafe { FileTimeToSystemTime(&file_time, &mut utc) != 0 };
    if !converted {
        return Err("The time could not be read.".to_string());
    }
    Ok(now_from_parts(&parts_of(&utc)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The value of the account name is never shown, only its form.
    #[test]
    fn the_current_user_has_a_domain_and_a_name() {
        let user = current_user().expect("The current user");
        let (domain, name) = user
            .split_once('\\')
            .expect("A backslash in the account name");
        assert!(!domain.is_empty() && !name.is_empty());
    }

    #[test]
    fn the_clock_gives_the_utc_form() {
        let now = now().expect("The clock");
        assert_eq!(now.utc.len(), "2026-11-19T10:20:30Z".len());
        assert!(now.utc.ends_with('Z'));
        assert_eq!(&now.utc[10..11], "T");
    }
}
