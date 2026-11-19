//! The TTL: its short form and its ISO 8601 form (lease-api.md, Values, TTL). RFC 2119 keywords,
//! and the capitals are load-bearing.

use crate::{ValueCode, ValueError, is_trim_space, quote, trim};

const MIN_SECONDS: u128 = 60;
const MAX_SECONDS: u128 = 86_400;

/// A unit, ranked from the largest (0) to the smallest (3).
#[derive(Clone, Copy)]
enum Unit {
    Day,
    Hour,
    Minute,
    Second,
}

impl Unit {
    fn rank(self) -> u8 {
        self as u8
    }

    fn seconds(self) -> u128 {
        match self {
            Unit::Day => 86_400,
            Unit::Hour => 3_600,
            Unit::Minute => 60,
            Unit::Second => 1,
        }
    }
}

/// A unit's spelling in the short form. `m` is lowercase only, and the rest are in any case.
fn short_unit(word: &str) -> Option<Unit> {
    if word == "m" {
        return Some(Unit::Minute);
    }
    match word.to_ascii_lowercase().as_str() {
        "d" | "day" | "days" => Some(Unit::Day),
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(Unit::Hour),
        "min" | "mins" | "minute" | "minutes" => Some(Unit::Minute),
        "s" | "sec" | "secs" | "second" | "seconds" => Some(Unit::Second),
        _ => None,
    }
}

/// A run of ASCII digits, as a number that saturates rather than overflows.
fn number(digits: &str) -> u128 {
    digits.bytes().fold(0u128, |n, b| {
        n.saturating_mul(10).saturating_add(u128::from(b - b'0'))
    })
}

/// Adds parts to a total, refusing units that repeat or come out of order. Overflow saturates, so
/// a number too large to compute ends up above the range and never panics.
struct Total {
    seconds: u128,
    last: Option<u8>,
}

impl Total {
    fn new() -> Self {
        Total {
            seconds: 0,
            last: None,
        }
    }

    /// Returns false when the unit is not strictly smaller than the one before it.
    fn add(&mut self, digits: &str, unit: Unit) -> bool {
        if self.last.is_some_and(|l| unit.rank() <= l) {
            return false;
        }
        self.last = Some(unit.rank());
        self.seconds = self
            .seconds
            .saturating_add(number(digits).saturating_mul(unit.seconds()));
        true
    }
}

/// The short form: parts of a number and a unit, largest first, each unit once, with optional
/// whitespace between a number and its unit, between parts, and around the whole.
fn parse_short(t: &str) -> Option<u128> {
    let mut total = Total::new();
    let mut rest = t;
    loop {
        rest = rest.trim_start_matches(is_trim_space);
        if rest.is_empty() {
            break;
        }
        let digits_len = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits_len == 0 {
            return None;
        }
        let (digits, after) = rest.split_at(digits_len);
        let after = after.trim_start_matches(is_trim_space);
        let word_len = after.bytes().take_while(u8::is_ascii_alphabetic).count();
        if word_len == 0 {
            return None;
        }
        let (word, after) = after.split_at(word_len);
        if !total.add(digits, short_unit(word)?) {
            return None;
        }
        rest = after;
    }
    total.last.map(|_| total.seconds)
}

/// The ISO 8601 form, `P[nD][T[nH][nM][nS]]` in any case, with no whitespace inside.
fn parse_iso(t: &str) -> Option<u128> {
    let b = t.as_bytes();
    if !b.first().is_some_and(|c| c.eq_ignore_ascii_case(&b'p')) {
        return None;
    }
    let mut total = Total::new();
    let mut i = 1;
    let mut in_time = false;
    let mut parts_after_t = 0;
    while i < b.len() {
        if b[i].eq_ignore_ascii_case(&b't') {
            if in_time {
                return None;
            }
            in_time = true;
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if start == i || i >= b.len() {
            return None;
        }
        let unit = match (in_time, b[i].to_ascii_lowercase()) {
            (false, b'd') => Unit::Day,
            (true, b'h') => Unit::Hour,
            (true, b'm') => Unit::Minute,
            (true, b's') => Unit::Second,
            _ => return None,
        };
        if !total.add(&t[start..i], unit) {
            return None;
        }
        if in_time {
            parts_after_t += 1;
        }
        i += 1;
    }
    if total.last.is_none() || (in_time && parts_after_t == 0) {
        return None;
    }
    Some(total.seconds)
}

/// Whether the trimmed value holds a digit, optional whitespace, then an uppercase `M` that ends
/// the value or is followed by a character that is not a letter.
fn uses_ambiguous_m(t: &str) -> bool {
    let chars: Vec<char> = t.chars().collect();
    chars.iter().enumerate().any(|(i, &c)| {
        if c != 'M' {
            return false;
        }
        let followed_ok = chars.get(i + 1).is_none_or(|n| !n.is_ascii_alphabetic());
        let mut j = i;
        while j > 0 && is_trim_space(chars[j - 1]) {
            j -= 1;
        }
        followed_ok && j > 0 && chars[j - 1].is_ascii_digit()
    })
}

/// A TTL in seconds, from 60 to 86400 both inclusive. Never clamped: a well-formed value outside
/// the range is `out_of_range`, including one too large to compute. The field is `ttl`.
pub fn ttl(v: &str) -> Result<u32, ValueError> {
    let t = trim(v);
    let parsed = if t.starts_with(['P', 'p']) {
        parse_iso(t)
    } else {
        parse_short(t)
    };
    match parsed {
        // The range check keeps the number within u32.
        Some(s) if (MIN_SECONDS..=MAX_SECONDS).contains(&s) => {
            Ok(u32::try_from(s).unwrap_or(u32::MAX))
        }
        Some(_) => Err(ValueError {
            code: ValueCode::OutOfRange,
            field: "ttl",
            message: format!(
                "TTL {} is outside the allowed range. Give from 1m to 24h; it is never clamped.",
                quote(v)
            ),
        }),
        None => {
            let message = if !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()) {
                format!("TTL {} has no unit. Give one, as in '8h'.", quote(v))
            } else if uses_ambiguous_m(t) {
                format!(
                    "TTL {} uses 'M', which could mean months or minutes. Write minutes as 'm' \
                     or 'min'.",
                    quote(v)
                )
            } else {
                format!(
                    "TTL {} is not a duration. Give one as in '8h', '90m', '1h30m', '2 hours' \
                     or 'PT8H'.",
                    quote(v)
                )
            };
            Err(ValueError {
                code: ValueCode::InvalidValue,
                field: "ttl",
                message,
            })
        }
    }
}
