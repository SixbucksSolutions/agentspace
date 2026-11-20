//! Bytes to text, refusing rather than replacing a byte that is not valid (operator.md, The
//! receipt: "Bytes that are not valid in the encoding are refused").

/// Which UTF-16 byte order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Order {
    Little,
    Big,
}

/// The encoding a file's bytes were read in, for the XML declaration's check.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Encoding {
    Utf8,
    Utf16(Order),
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

/// XML 1.0's detection (its Appendix F), for the encodings agentspace reads: a byte order mark,
/// or, with none, the first characters `<?` in UTF-16 of either order; anything else is UTF-8.
pub(crate) fn decode_xml(bytes: &[u8]) -> Option<(String, Encoding)> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        let text = std::str::from_utf8(rest).ok()?;
        Some((text.to_owned(), Encoding::Utf8))
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        Some((utf16(rest, Order::Little)?, Encoding::Utf16(Order::Little)))
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        Some((utf16(rest, Order::Big)?, Encoding::Utf16(Order::Big)))
    } else if bytes.starts_with(&[0x3C, 0x00, 0x3F, 0x00]) {
        Some((utf16(bytes, Order::Little)?, Encoding::Utf16(Order::Little)))
    } else if bytes.starts_with(&[0x00, 0x3C, 0x00, 0x3F]) {
        Some((utf16(bytes, Order::Big)?, Encoding::Utf16(Order::Big)))
    } else {
        let text = std::str::from_utf8(bytes).ok()?;
        Some((text.to_owned(), Encoding::Utf8))
    }
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
    fn xml_without_a_mark_in_utf16_is_detected() {
        let le: Vec<u8> = "<?xml?>"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let be: Vec<u8> = "<?xml?>"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect();
        assert_eq!(
            decode_xml(&le),
            Some(("<?xml?>".to_owned(), Encoding::Utf16(Order::Little)))
        );
        assert_eq!(
            decode_xml(&be),
            Some(("<?xml?>".to_owned(), Encoding::Utf16(Order::Big)))
        );
    }
}
