//! `LaunchTime`, an ISO 8601 date and time with its offset, compared in UTC (operator.md, Calls by
//! command, What is read from each answer).

use std::time::{SystemTime, UNIX_EPOCH};

const NANOS_PER_SECOND: i128 = 1_000_000_000;

/// The instant `text` names, as nanoseconds since the Unix epoch, or `None` when it is not of the
/// form `YYYY-MM-DDTHH:MM:SS[.fraction](Z|+HH:MM|-HH:MM)`.
///
/// The AWS CLI prints it as `2026-11-19T08:12:34+00:00` or `2026-11-19T08:12:34.000Z`. The
/// separator MAY also be `t` or a space, the `Z` MAY be `z`, and the offset MAY be written without
/// its colon.
///
/// A time with no UTC offset is refused: it is a member not of its form (the architect's ruling
/// of 2026-11-19 for E0.06-T01-S03, in Calls by command), so the step fails with the "Unexpected
/// answer" message. The AWS CLI always prints one.
pub(crate) fn parse(text: &str) -> Option<i128> {
    let b = text.as_bytes();
    if b.len() < 20 {
        return None;
    }
    let year = digits(b, 0, 4)?;
    expect(b, 4, b'-')?;
    let month = digits(b, 5, 2)?;
    expect(b, 7, b'-')?;
    let day = digits(b, 8, 2)?;
    if !matches!(b[10], b'T' | b't' | b' ') {
        return None;
    }
    let hour = digits(b, 11, 2)?;
    expect(b, 13, b':')?;
    let minute = digits(b, 14, 2)?;
    expect(b, 16, b':')?;
    let second = digits(b, 17, 2)?;
    if !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }

    let mut i = 19;
    let mut nanos: i128 = 0;
    if b[i] == b'.' {
        i += 1;
        let start = i;
        let mut scale = NANOS_PER_SECOND / 10;
        while i < b.len() && b[i].is_ascii_digit() {
            nanos += i128::from(b[i] - b'0') * scale;
            scale /= 10;
            i += 1;
        }
        if i == start {
            return None;
        }
    }

    let offset_seconds: i128 = match b.get(i)? {
        b'Z' | b'z' if i + 1 == b.len() => 0,
        sign @ (b'+' | b'-') => {
            let rest = &b[i + 1..];
            let (hours, minutes) = match rest.len() {
                5 if rest[2] == b':' => (digits(rest, 0, 2)?, digits(rest, 3, 2)?),
                4 => (digits(rest, 0, 2)?, digits(rest, 2, 2)?),
                _ => return None,
            };
            if hours > 23 || minutes > 59 {
                return None;
            }
            let magnitude = i128::from(hours * 3600 + minutes * 60);
            if *sign == b'+' { magnitude } else { -magnitude }
        }
        _ => return None,
    };

    let days = days_from_civil(i128::from(year), i128::from(month), i128::from(day));
    let seconds =
        days * 86_400 + i128::from(hour) * 3600 + i128::from(minute) * 60 + i128::from(second)
            - offset_seconds;
    Some(seconds * NANOS_PER_SECOND + nanos)
}

/// `time` as nanoseconds since the Unix epoch, negative before it.
pub(crate) fn nanos_since_epoch(time: SystemTime) -> i128 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_nanos() as i128,
        Err(before) => -(before.duration().as_nanos() as i128),
    }
}

fn digits(b: &[u8], at: usize, count: usize) -> Option<u32> {
    let slice = b.get(at..at + count)?;
    let mut value = 0u32;
    for &c in slice {
        if !c.is_ascii_digit() {
            return None;
        }
        value = value * 10 + u32::from(c - b'0');
    }
    Some(value)
}

fn expect(b: &[u8], at: usize, c: u8) -> Option<()> {
    (b.get(at) == Some(&c)).then_some(())
}

fn is_leap(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i128, month: i128, day: i128) -> i128 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(seconds: u64) -> i128 {
        nanos_since_epoch(UNIX_EPOCH + Duration::from_secs(seconds))
    }

    #[test]
    fn the_cli_forms_parse() {
        // 2026-11-19T08:12:34Z is 1_795_075_954 seconds after the epoch.
        let expected = at(1_795_075_954);
        assert_eq!(parse("2026-11-19T08:12:34+00:00"), Some(expected));
        assert_eq!(parse("2026-11-19T08:12:34Z"), Some(expected));
        assert_eq!(parse("2026-11-19T08:12:34.000Z"), Some(expected));
        assert_eq!(parse("2026-11-19t08:12:34z"), Some(expected));
        assert_eq!(parse("2026-11-19 08:12:34+0000"), Some(expected));
        assert_eq!(parse("2026-11-19T00:12:34-08:00"), Some(expected));
        assert_eq!(parse("2026-11-19T13:42:34+05:30"), Some(expected));
        assert_eq!(
            parse("2026-11-19T08:12:34.5Z"),
            Some(expected + NANOS_PER_SECOND / 2)
        );
    }

    #[test]
    fn the_epoch_and_a_leap_day_parse() {
        assert_eq!(parse("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse("2024-02-29T00:00:00Z"), Some(at(1_709_164_800)));
    }

    #[test]
    fn other_forms_are_refused() {
        for text in [
            "",
            "2026-11-19",
            "2026-11-19T08:12:34",
            "2026-11-19T08:12:34+00",
            "2026-11-19T08:12:34Zjunk",
            "2026-11-19T08:12:34.Z",
            "2026-13-19T08:12:34Z",
            "2026-02-30T08:12:34Z",
            "2025-02-29T08:12:34Z",
            "2026-11-19T24:00:00Z",
            "2026-11-19T08:60:00Z",
            "2026-11-19X08:12:34Z",
            "2026/11/19T08:12:34Z",
            "２０２６-11-19T08:12:34Z",
            "2026-11-19T08:12:34+24:00",
        ] {
            assert_eq!(parse(text), None, "{text}");
        }
    }

    #[test]
    fn a_time_before_the_epoch_is_negative() {
        assert_eq!(
            nanos_since_epoch(UNIX_EPOCH - Duration::from_secs(1)),
            -NANOS_PER_SECOND
        );
    }
}
