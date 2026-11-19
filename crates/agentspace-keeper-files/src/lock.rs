//! The keeper lock's PID line (state.md, The keeper's lock files; keeper.md, `board-holder-keeper.lock`).

/// What `board-holder-keeper.lock` holds once its keeper has the lock: the keeper's PID as decimal
/// ASCII digits with nothing else, no LF included. It is for a second keeper's log line only.
pub fn serialize_keeper_pid(pid: u32) -> Vec<u8> {
    pid.to_string().into_bytes()
}

/// How many bytes of the lock file a second keeper reads (keeper.md, `board-holder-keeper.lock`).
pub const KEEPER_PID_READ_BYTES: usize = 32;

/// The digits a second keeper logs as `another keeper is running pid=<digits>`: from the first 32
/// bytes it could read, the text left after trimming blanks, if it is 1 to 10 ASCII digits. `None`
/// is logged as `pid=-`: anything else, an empty file included. Bytes past the 32nd are never
/// looked at.
///
/// "Blanks" are ASCII whitespace: space, tab, CR, LF and form feed, and nothing else, a vertical
/// tab included (state.md, The holder record; architect, 2026-11-19, for E0.06-T01-S07). They
/// are exactly what `trim_ascii` trims.
pub fn keeper_pid_digits(bytes: &[u8]) -> Option<String> {
    let bytes = &bytes[..bytes.len().min(KEEPER_PID_READ_BYTES)];
    let text = bytes.trim_ascii();
    if (1..=10).contains(&text.len()) && text.iter().all(u8::is_ascii_digit) {
        String::from_utf8(text.to_vec()).ok()
    } else {
        None
    }
}

/// The PID in those digits, for `Taken::Busy`: `None` when there are no digits, or when they name
/// a number that does not fit a PID (10 digits can be larger than `u32::MAX`).
pub fn parse_keeper_pid(bytes: &[u8]) -> Option<u32> {
    keeper_pid_digits(bytes)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pid_line_is_digits_and_nothing_else() {
        assert_eq!(serialize_keeper_pid(4242), b"4242");
        assert_eq!(serialize_keeper_pid(7), b"7");
        assert_eq!(serialize_keeper_pid(u32::MAX), b"4294967295");
    }

    #[test]
    fn a_written_line_is_read_back() {
        for pid in [1, 7, 4242, 2_147_483_647, u32::MAX] {
            assert_eq!(parse_keeper_pid(&serialize_keeper_pid(pid)), Some(pid));
        }
    }

    #[test]
    fn one_to_ten_digits_after_trimming_blanks_are_a_pid() {
        assert_eq!(keeper_pid_digits(b"  123 \t").as_deref(), Some("123"));
        assert_eq!(keeper_pid_digits(b"123\r\n").as_deref(), Some("123"));
        assert_eq!(keeper_pid_digits(b"0").as_deref(), Some("0"));
        assert_eq!(
            keeper_pid_digits(b"0123456789").as_deref(),
            Some("0123456789")
        );
        assert_eq!(keeper_pid_digits(b"01234567890"), None);
        assert_eq!(keeper_pid_digits(b""), None);
        assert_eq!(keeper_pid_digits(b"   "), None);
        assert_eq!(keeper_pid_digits(b"12a"), None);
        assert_eq!(keeper_pid_digits(b"1 2"), None);
        assert_eq!(keeper_pid_digits(b"-5"), None);
        assert_eq!(keeper_pid_digits(b"\xEF\xBB\xBF12"), None);
    }

    #[test]
    fn only_the_first_32_bytes_are_looked_at() {
        let mut long = b"123".to_vec();
        long.resize(32, b' ');
        long.extend_from_slice(b"junk");
        assert_eq!(keeper_pid_digits(&long).as_deref(), Some("123"));
        let digits = vec![b'1'; 33];
        assert_eq!(keeper_pid_digits(&digits), None);
    }

    #[test]
    fn digits_too_large_for_a_pid_are_logged_but_give_no_number() {
        assert_eq!(
            keeper_pid_digits(b"9999999999").as_deref(),
            Some("9999999999")
        );
        assert_eq!(parse_keeper_pid(b"9999999999"), None);
    }
}
