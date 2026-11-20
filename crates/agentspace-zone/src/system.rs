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
