//! The registration time in the one form the installer uses (keeper.md, The task's fields and
//! The installer, `-DryRun`): UTC with a `Z`, to the whole second, for the task's start boundary
//! and for the dry run's `Trigger 2:` line alike (architect, 2026-11-19, for E0.06-T02-S06: the
//! dry run prints the boundary exactly as the definition holds it).
//!
//! The formatting is plain code. Where the time comes from is [`crate::sys`].

/// A date and a time of day, to the whole second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parts {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// The registration time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Now {
    /// `YYYY-MM-DDTHH:MM:SSZ`.
    pub utc: String,
}

/// `YYYY-MM-DDTHH:MM:SSZ`.
pub fn format_utc(utc: &Parts) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        utc.year, utc.month, utc.day, utc.hour, utc.minute, utc.second
    )
}

/// The registration time, from its UTC parts.
pub fn now_from_parts(utc: &Parts) -> Now {
    Now {
        utc: format_utc(utc),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> Parts {
        Parts {
            year: y,
            month: mo,
            day: d,
            hour: h,
            minute: mi,
            second: s,
        }
    }

    #[test]
    fn utc_has_a_z_and_zero_padding() {
        assert_eq!(
            format_utc(&parts(2026, 1, 2, 3, 4, 5)),
            "2026-01-02T03:04:05Z"
        );
    }

    #[test]
    fn the_registration_time_is_the_utc_form() {
        let now = now_from_parts(&parts(2026, 11, 19, 10, 20, 30));
        assert_eq!(now.utc, "2026-11-19T10:20:30Z");
    }
}
