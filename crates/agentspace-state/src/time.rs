//! UTC times in the contract's form, `YYYY-MM-DDTHH:MM:SSZ` (lease-api.md, Times).
//!
//! Days are counted in the proleptic Gregorian calendar, and a leap second is not an instant.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds from 0000-01-01T00:00:00Z, the form's first value, to the Unix epoch.
const MIN_UNIX: i64 = -62_167_219_200;
/// The form's last value, 9999-12-31T23:59:59Z.
const MAX_UNIX: i64 = 253_402_300_799;

/// Days from 1970-01-01 to the given civil date, which MUST be a real one.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date of the given day count from 1970-01-01.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap(y) => 29,
        _ => 28,
    }
}

/// Writes a Unix time, in whole seconds, as `YYYY-MM-DDTHH:MM:SSZ`. A time outside the form's
/// years 0000 to 9999 is held at the form's nearest end, so the result always has the form.
pub fn format_time(unix: i64) -> String {
    let unix = unix.clamp(MIN_UNIX, MAX_UNIX);
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3_600,
        secs % 3_600 / 60,
        secs % 60
    )
}

/// Reads a time in exactly the contract's form, `YYYY-MM-DDTHH:MM:SSZ`, to a Unix time. `None`
/// for any other form, and for a time that names no real instant: a day the month lacks, an hour
/// of 24, or a leap second.
pub fn parse_time(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 20 {
        return None;
    }
    let separators = [
        (4, b'-'),
        (7, b'-'),
        (10, b'T'),
        (13, b':'),
        (16, b':'),
        (19, b'Z'),
    ];
    if separators.iter().any(|&(i, c)| b[i] != c) {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> {
        let digits = &b[from..to];
        if digits.iter().all(u8::is_ascii_digit) {
            Some(digits.iter().fold(0, |n, d| n * 10 + i64::from(d - b'0')))
        } else {
            None
        }
    };
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hh, mm, ss) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) || hh > 23 || mm > 59 || ss > 59 {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400 + hh * 3_600 + mm * 60 + ss)
}

/// The current time, UTC, in whole seconds since the Unix epoch, truncated (never rounded).
pub fn now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        // Before the epoch, truncating toward the past means rounding the count up.
        Err(e) => {
            let d = e.duration();
            -i64::try_from(d.as_secs()).unwrap_or(i64::MAX) - i64::from(d.subsec_nanos() > 0)
        }
    }
}
