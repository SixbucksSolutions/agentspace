//! The laptop's Windows zone ID, read from the system (operator.md, The time zone, step 1).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. This is the only module of the crate that
//! calls Windows, and so the only one with `unsafe`.

#![allow(unsafe_code)]

/// The Windows ID of the zone the laptop is set to at this moment, such as `Pacific Standard
/// Time`, or `None` when the system does not give one (and always on any other system).
///
/// It is the `TimeZoneKeyName` of `GetDynamicTimeZoneInformation`, which is the key name that
/// .NET's `TimeZoneInfo.Local.Id` is. It is read afresh at each call, so a laptop that travels
/// gives its zone of the moment, and it is never remembered.
#[cfg(windows)]
pub fn windows_zone_id() -> Option<String> {
    use windows_sys::Win32::System::Time::{
        DYNAMIC_TIME_ZONE_INFORMATION, GetDynamicTimeZoneInformation, TIME_ZONE_ID_INVALID,
    };
    let mut info = DYNAMIC_TIME_ZONE_INFORMATION::default();
    // SAFETY: `info` is a valid, writable DYNAMIC_TIME_ZONE_INFORMATION for the call to fill.
    let state = unsafe { GetDynamicTimeZoneInformation(&mut info) };
    if state == TIME_ZONE_ID_INVALID {
        return None;
    }
    let name = &info.TimeZoneKeyName;
    let len = name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(name.len());
    let id = String::from_utf16(&name[..len]).ok()?;
    if id.is_empty() { None } else { Some(id) }
}

/// Not Windows: there is no zone ID to read.
#[cfg(not(windows))]
pub fn windows_zone_id() -> Option<String> {
    None
}

/// The laptop's offset from UTC at the instant `unix` (Unix seconds), in seconds east of UTC, in
/// the zone Windows is set to at this moment with the daylight saving rule that applies to that
/// instant, or `None` when the system does not give one (and always on any other system).
///
/// It is read afresh at each call and never remembered, so a laptop that travels gives the offset
/// of the zone it is in. Added for `status -All`'s `Guard` line (operator.md, status; E0.08-T03-S02).
#[cfg(windows)]
pub fn local_utc_offset(unix: i64) -> Option<i32> {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{
        FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime,
    };
    // Seconds from 1601-01-01, the FILETIME epoch, to 1970-01-01.
    const EPOCH_GAP: i64 = 11_644_473_600;
    const TICKS_PER_SECOND: i64 = 10_000_000;
    let ticks = unix.checked_add(EPOCH_GAP)?.checked_mul(TICKS_PER_SECOND)?;
    if ticks < 0 {
        return None;
    }
    let ticks = ticks as u64;
    let universal = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut universal_time = SYSTEMTIME::default();
    let mut local_time = SYSTEMTIME::default();
    let mut local = FILETIME::default();
    // SAFETY: every pointer is to a valid value of the call's type that lives through the call; the
    // null zone means the zone the system is set to.
    let ok = unsafe {
        FileTimeToSystemTime(&universal, &mut universal_time) != 0
            && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &universal_time, &mut local_time)
                != 0
            && SystemTimeToFileTime(&local_time, &mut local) != 0
    };
    if !ok {
        return None;
    }
    let local_ticks = (i64::from(local.dwHighDateTime) << 32) | i64::from(local.dwLowDateTime);
    let seconds = (local_ticks - ticks as i64).div_euclid(TICKS_PER_SECOND);
    i32::try_from(seconds).ok()
}

/// Not Windows: there is no zone to read.
#[cfg(not(windows))]
pub fn local_utc_offset(_unix: i64) -> Option<i32> {
    None
}
