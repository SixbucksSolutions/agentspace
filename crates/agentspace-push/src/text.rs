//! Bytes to text, refusing rather than replacing a byte that is not valid (operator.md, The
//! receipt: "Bytes that are not valid in the encoding are refused").

/// Which UTF-16 byte order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Order {
    Little,
    Big,
}

/// Decodes UTF-16 in the given order. An odd byte count or an unpaired surrogate is refused.
pub(crate) fn utf16(bytes: &[u8], order: Order) -> Option<String> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let (pairs, _) = bytes.as_chunks::<2>();
    let units = pairs.iter().map(|&pair| match order {
        Order::Little => u16::from_le_bytes(pair),
        Order::Big => u16::from_be_bytes(pair),
    });
    let mut out = String::with_capacity(bytes.len() / 2);
    for c in char::decode_utf16(units) {
        out.push(c.ok()?);
    }
    Some(out)
}

/// The receipt's rule: a byte order mark picks the encoding and is not part of the text
/// (`EF BB BF` UTF-8, `FF FE` UTF-16LE, `FE FF` UTF-16BE); no mark means UTF-8. The stamp file is
/// not read this way: it is UTF-8 alone (`crate::stamp::parse_stamp_file`).
pub(crate) fn decode_marked(bytes: &[u8]) -> Option<String> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        std::str::from_utf8(rest).ok().map(str::to_owned)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        utf16(rest, Order::Little)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        utf16(rest, Order::Big)
    } else {
        std::str::from_utf8(bytes).ok().map(str::to_owned)
    }
}

/// The credential's rule (localswim's `docs/windows-operations.md`, "The credential files"):
/// UTF-8, after an optional UTF-8 byte order mark (`EF BB BF`), which is not part of the text. No
/// other mark is stripped, so a UTF-16 file is refused, as invalid UTF-8 or as text that is not
/// JSON.
pub(crate) fn decode_utf8(bytes: &[u8]) -> Option<&str> {
    let rest = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    std::str::from_utf8(rest).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_pick_the_encoding_and_are_not_part_of_the_text() {
        assert_eq!(decode_marked(b"\xEF\xBB\xBF{}").as_deref(), Some("{}"));
        assert_eq!(decode_marked(b"\xFF\xFE{\0}\0").as_deref(), Some("{}"));
        assert_eq!(decode_marked(b"\xFE\xFF\0{\0}").as_deref(), Some("{}"));
        assert_eq!(decode_marked(b"{}").as_deref(), Some("{}"));
    }

    #[test]
    fn invalid_bytes_are_refused_not_replaced() {
        assert_eq!(decode_marked(b"{\xFF}"), None);
        assert_eq!(
            decode_marked(b"\xFF\xFE{\0}"),
            None,
            "Odd UTF-16 byte count"
        );
        assert_eq!(
            decode_marked(b"\xFF\xFE\x00\xD8"),
            None,
            "Unpaired surrogate"
        );
    }

    #[test]
    fn utf8_with_or_without_its_mark() {
        assert_eq!(decode_utf8(b"\xEF\xBB\xBF{}"), Some("{}"));
        assert_eq!(decode_utf8(b"{}"), Some("{}"));
        assert_eq!(decode_utf8(b"{\xFF}"), None);
        assert_eq!(decode_utf8(b"\xFF\xFE{\0}\0"), None, "UTF-16LE");
        assert_eq!(
            decode_utf8(b"\xEF\xBB\xBF\xEF\xBB\xBF{}"),
            Some("\u{FEFF}{}"),
            "One mark is stripped, not two"
        );
    }
}
