//! A process's creation time as the holder record writes it (keeper.md, Trusting the record,
//! item 3; state.md, The holder record).

#![cfg_attr(not(windows), allow(dead_code))]

/// Milliseconds from 1601-01-01, where a Windows `FILETIME` counts from, to 1970-01-01.
const UNIX_EPOCH_FROM_1601_MS: i64 = 11_644_473_600_000;

const MS_PER_DAY: i64 = 86_400_000;

/// A `FILETIME`, 100-nanosecond ticks since 1601-01-01 UTC, as `YYYY-MM-DDTHH:MM:SS.fffZ` in UTC,
/// truncated (never rounded) to the millisecond.
pub(crate) fn filetime_text(ticks: u64) -> String {
    // ticks / 10_000 is at most about 1.8e15, which fits an i64.
    let ms = i64::try_from(ticks / 10_000).unwrap_or(i64::MAX) - UNIX_EPOCH_FROM_1601_MS;
    let days = ms.div_euclid(MS_PER_DAY);
    let of_day = ms.rem_euclid(MS_PER_DAY);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        of_day / 3_600_000,
        of_day / 60_000 % 60,
        of_day / 1000 % 60,
        of_day % 1000
    )
}

/// The proleptic Gregorian date of a count of days since 1970-01-01 (Howard Hinnant's
/// `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ticks for a Unix time in milliseconds, plus extra ticks below the millisecond.
    fn ticks(unix_ms: i64, extra: u64) -> u64 {
        u64::try_from(unix_ms + UNIX_EPOCH_FROM_1601_MS).unwrap() * 10_000 + extra
    }

    #[test]
    fn the_unix_epoch() {
        assert_eq!(filetime_text(ticks(0, 0)), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn a_known_time_to_the_millisecond() {
        // 2026-11-19T16:33:05.123Z is 1795105985123 ms after the epoch.
        assert_eq!(
            filetime_text(ticks(1_795_105_985_123, 0)),
            "2026-11-19T16:33:05.123Z"
        );
    }

    #[test]
    fn below_the_millisecond_is_truncated_never_rounded() {
        assert_eq!(
            filetime_text(ticks(1_795_105_985_123, 9_999)),
            "2026-11-19T16:33:05.123Z"
        );
    }

    #[test]
    fn leap_days_and_year_ends() {
        // 2024-02-29T23:59:59.999Z and the millisecond after it.
        assert_eq!(
            filetime_text(ticks(1_709_251_199_999, 0)),
            "2024-02-29T23:59:59.999Z"
        );
        assert_eq!(
            filetime_text(ticks(1_709_251_200_000, 0)),
            "2024-03-01T00:00:00.000Z"
        );
        // 2000-12-31T23:59:59.000Z, a leap year divisible by 400.
        assert_eq!(
            filetime_text(ticks(978_307_199_000, 0)),
            "2000-12-31T23:59:59.000Z"
        );
    }

    #[test]
    fn before_1970_and_the_filetime_epoch() {
        assert_eq!(filetime_text(0), "1601-01-01T00:00:00.000Z");
        assert_eq!(filetime_text(ticks(-1, 0)), "1969-12-31T23:59:59.999Z");
    }

    #[test]
    fn the_largest_filetime_does_not_panic() {
        assert!(filetime_text(u64::MAX).ends_with('Z'));
    }
}
