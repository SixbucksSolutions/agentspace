//! The two UTC forms of a start (docs/state.md, A holder's launch files): the launcher's deadline,
//! `YYYY-MM-DDTHH:MM:SS.fffffffZ` with seven fraction digits, and a process's creation time,
//! `YYYY-MM-DDTHH:MM:SS.fffZ`, truncated to the millisecond. Times are counted in 100-nanosecond
//! ticks or in milliseconds since the Unix epoch, UTC; local time never enters.

use std::time::{SystemTime, UNIX_EPOCH};

/// 100-nanosecond ticks in one millisecond.
const TICKS_PER_MS: i64 = 10_000;
const TICKS_PER_SECOND: i64 = 10_000_000;
const SECONDS_PER_DAY: i64 = 86_400;

/// The UTC clock now, in 100-nanosecond ticks since the Unix epoch.
pub(crate) fn utc_now_ticks() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_nanos() / 100).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_nanos() / 100).unwrap_or(i64::MAX),
    }
}

/// The civil date of a day counted from 1970-01-01 (proleptic Gregorian).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The day counted from 1970-01-01 of a civil date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `YYYY-MM-DDTHH:MM:SS`, then the fraction as given, then `Z`.
fn format(seconds: i64, fraction: &str) -> String {
    let days = seconds.div_euclid(SECONDS_PER_DAY);
    let rest = seconds.rem_euclid(SECONDS_PER_DAY);
    let (y, mo, d) = civil_from_days(days);
    format!(
        "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}.{fraction}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// The launcher's deadline form: `YYYY-MM-DDTHH:MM:SS.fffffffZ`.
pub(crate) fn format_ticks(ticks: i64) -> String {
    let fraction = format!("{:07}", ticks.rem_euclid(TICKS_PER_SECOND));
    format(ticks.div_euclid(TICKS_PER_SECOND), &fraction)
}

/// A creation time's form: `YYYY-MM-DDTHH:MM:SS.fffZ`, truncated to the millisecond.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn format_ms(ms: i64) -> String {
    let fraction = format!("{:03}", ms.rem_euclid(1000));
    format(ms.div_euclid(1000), &fraction)
}

/// The UTC clock's milliseconds as ticks.
pub(crate) fn ms_to_ticks(ms: i64) -> i64 {
    ms.saturating_mul(TICKS_PER_MS)
}

fn digits(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Reads the deadline form, `YYYY-MM-DDTHH:MM:SS.fffffffZ` with exactly seven fraction digits, as
/// ticks. Anything else, or a date or time out of range, is `None`.
pub(crate) fn parse_ticks(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 28
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'.'
        || b[27] != b'Z'
    {
        return None;
    }
    let year = digits(s.get(0..4)?)?;
    let month = digits(s.get(5..7)?)?;
    let day = digits(s.get(8..10)?)?;
    let hour = digits(s.get(11..13)?)?;
    let minute = digits(s.get(14..16)?)?;
    let second = digits(s.get(17..19)?)?;
    let fraction = digits(s.get(20..27)?)?;
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => return None,
    };
    if day < 1 || day > days_in_month || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let seconds =
        days_from_civil(year, month, day) * SECONDS_PER_DAY + hour * 3600 + minute * 60 + second;
    Some(seconds * TICKS_PER_SECOND + fraction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_deadline_form_has_seven_fraction_digits_and_reads_back() {
        let t = 17_635_331_361_234_567;
        let s = format_ticks(t);
        assert_eq!(s.len(), 28);
        assert_eq!(parse_ticks(&s), Some(t));
        assert_eq!(format_ticks(0), "1970-01-01T00:00:00.0000000Z");
        assert_eq!(
            parse_ticks("2000-02-29T23:59:59.9999999Z"),
            Some((days_from_civil(2000, 2, 29) * 86_400 + 86_399) * TICKS_PER_SECOND + 9_999_999)
        );
    }

    #[test]
    fn a_known_date_formats_as_the_calendar_says() {
        // 2026-11-19T04:05:36Z is 1_795_061_136 seconds after the epoch.
        assert_eq!(format_ms(1_795_061_136_456), "2026-11-19T04:05:36.456Z");
        assert_eq!(
            format_ticks(17_950_611_361_234_567),
            "2026-11-19T04:05:36.1234567Z"
        );
    }

    #[test]
    fn a_deadline_in_another_form_is_not_read() {
        for bad in [
            "",
            "2026-11-19T04:05:36.123Z",
            "2026-11-19T04:05:36.12345678Z",
            "2026-11-19 04:05:36.1234567Z",
            "2026-13-19T04:05:36.1234567Z",
            "2026-02-29T04:05:36.1234567Z",
            "2026-11-19T24:05:36.1234567Z",
            "2026-11-19T04:05:36.1234567",
            "2026-11-19T04:05:36.12345x7Z",
            "+026-11-19T04:05:36.1234567Z",
        ] {
            assert_eq!(parse_ticks(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_millisecond_form_truncates() {
        assert_eq!(format_ms(999), "1970-01-01T00:00:00.999Z");
        assert_eq!(ms_to_ticks(1), 10_000);
    }
}
