//! The Bot credential, `bot.xml` (operator.md, The Bot credential): CLIXML down to the `SS`
//! element's hex, then the decryption behind [`Unprotect`], then the token.

use crate::text::{Encoding, Order};
use crate::{Token, Unprotect};
use roxmltree::{Document, Node};
use zeroize::Zeroizing;

/// The namespace every CLIXML element is in.
const NS: &str = "http://schemas.microsoft.com/powershell/2004/04";
const PS_CREDENTIAL: &str = "System.Management.Automation.PSCredential";

fn is(node: &Node, name: &str) -> bool {
    node.is_element() && node.tag_name().namespace() == Some(NS) && node.tag_name().name() == name
}

/// The value of an attribute in no namespace. roxmltree's own `attribute("N")` matches the local
/// name alone, so it would also find a prefixed `x:N`, which is not the `N` attribute.
fn plain_attribute<'a>(node: &Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|a| a.namespace().is_none() && a.name() == name)
        .map(|a| a.value())
}

/// An element's text: every text node under it, CDATA included, joined in order.
fn text_of(node: &Node) -> String {
    node.descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect()
}

/// XML's whitespace.
fn is_xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// `Objs`'s `Version`: two to four decimal numbers joined by `.`, the first 1, whitespace around
/// it ignored.
///
/// Each number is ASCII digits that fit a 32-bit signed integer, leading zeros allowed, and the
/// first MUST be 1, so `01.1` is accepted (operator.md, What the laptop checks; architect,
/// 2026-11-19, for E0.06-T01-S04).
fn version_ok(value: &str) -> bool {
    let parts: Vec<&str> = value.trim_matches(is_xml_space).split('.').collect();
    if !(2..=4).contains(&parts.len()) {
        return false;
    }
    let mut numbers = Vec::with_capacity(parts.len());
    for p in &parts {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        match p.parse::<i32>() {
            Ok(n) => numbers.push(n),
            Err(_) => return false,
        }
    }
    numbers.first() == Some(&1)
}

/// The XML declaration's `encoding`, when the text starts with a declaration that names one.
fn declared_encoding(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("<?xml")?;
    if !rest.starts_with(is_xml_space) {
        return None;
    }
    let decl = &rest[..rest.find("?>")?];
    let after = &decl[decl.find("encoding")? + "encoding".len()..];
    let after = after.trim_start_matches(is_xml_space).strip_prefix('=')?;
    let after = after.trim_start_matches(is_xml_space);
    let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let value = &after[1..];
    Some(&value[..value.find(quote)?])
}

/// Whether the XML declaration, when it names an encoding, names the bytes' own: UTF-8 for UTF-8
/// bytes, or UTF-16 (or UTF-16LE or UTF-16BE, matching the order) for UTF-16 bytes. A declaration
/// that names any other encoding is refused (operator.md, What the laptop checks; architect,
/// 2026-11-19, for E0.06-T01-S04).
fn declaration_agrees(text: &str, encoding: Encoding) -> bool {
    let Some(name) = declared_encoding(text) else {
        return true;
    };
    match encoding {
        Encoding::Utf8 => name.eq_ignore_ascii_case("utf-8"),
        Encoding::Utf16(order) => {
            name.eq_ignore_ascii_case("utf-16")
                || (order == Order::Little && name.eq_ignore_ascii_case("utf-16le"))
                || (order == Order::Big && name.eq_ignore_ascii_case("utf-16be"))
        }
    }
}

/// The `SS` element's text as bytes: an even number of hex digits, at least two, and nothing
/// else.
fn hex_bytes(text: &str) -> Option<Vec<u8>> {
    let digits = text.as_bytes();
    if digits.is_empty() || !digits.len().is_multiple_of(2) {
        return None;
    }
    let nibble = |b: u8| -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    };
    let (pairs, _) = digits.as_chunks::<2>();
    pairs
        .iter()
        .map(|&[high, low]| Some((nibble(high)? << 4) | nibble(low)?))
        .collect()
}

/// Reads `bot.xml`'s bytes down to the DPAPI blob in its `SS` element. `None` is a refusal.
/// The blob is the token encrypted, not the token; the token exists only after [`Unprotect`].
pub fn parse_credential_blob(bytes: &[u8]) -> Option<Vec<u8>> {
    let (text, encoding) = crate::text::decode_xml(bytes)?;
    if !declaration_agrees(&text, encoding) {
        return None;
    }
    // roxmltree refuses any document type declaration by default (`allow_dtd: false`), so no
    // entity is ever expanded.
    let doc = Document::parse(&text).ok()?;
    let objs = doc.root_element();

    // 1. The root element is `Objs`, and its `Version`, when present, is 1.x.
    if !is(&objs, "Objs") {
        return None;
    }
    if let Some(v) = plain_attribute(&objs, "Version")
        && !version_ok(v)
    {
        return None;
    }

    // 2. Exactly one child element `Obj`.
    let mut objs_children = objs.children().filter(|n| is(n, "Obj"));
    let obj = objs_children.next()?;
    if objs_children.next().is_some() {
        return None;
    }

    // 3. A `TN` with a `T` child that names PSCredential.
    let named = obj
        .children()
        .filter(|n| is(n, "TN"))
        .flat_map(|tn| tn.children().filter(|n| is(n, "T")))
        .any(|t| text_of(&t) == PS_CREDENTIAL);
    if !named {
        return None;
    }

    // 4. In `Props`, exactly one `S` named UserName and exactly one `SS` named Password, the
    //    names in any ASCII case.
    //    The members of every `Props` element are counted together: a second UserName or
    //    Password anywhere among them is refused (operator.md, What the laptop checks; architect,
    //    2026-11-19, for E0.06-T01-S04).
    let mut user_names = 0usize;
    let mut passwords = Vec::new();
    for props in obj.children().filter(|n| is(n, "Props")) {
        for member in props.children().filter(|n| n.is_element()) {
            let Some(name) = plain_attribute(&member, "N") else {
                continue;
            };
            if name.eq_ignore_ascii_case("UserName") {
                // The `UserName` element MUST be present; nothing reads its text, so an empty one
                // (`<S N="UserName"></S>` or `<S N="UserName"/>`) is accepted (architect,
                // 2026-11-19, for E0.06-T01-S04).
                if !is(&member, "S") {
                    return None;
                }
                user_names += 1;
            } else if name.eq_ignore_ascii_case("Password") {
                // A password in `S`, as plain text, is refused, since it is not protected.
                if !is(&member, "SS") {
                    return None;
                }
                passwords.push(member);
            }
        }
    }
    if user_names != 1 || passwords.len() != 1 {
        return None;
    }
    let ss = passwords.first()?;
    if ss.children().any(|n| n.is_element()) {
        return None;
    }
    hex_bytes(&text_of(ss))
}

/// The token from the plaintext [`Unprotect`] gave: UTF-16LE, an even byte count, valid UTF-16.
pub(crate) fn token_from_plaintext(plain: &[u8]) -> Option<Token> {
    if !plain.len().is_multiple_of(2) {
        return None;
    }
    // Room for the longest UTF-8 the plaintext can give (3 bytes for each 2), so no reallocation
    // leaves a copy of the token behind.
    let mut text = Zeroizing::new(String::with_capacity(plain.len() / 2 * 3));
    let (pairs, _) = plain.as_chunks::<2>();
    let units = pairs.iter().map(|&pair| u16::from_le_bytes(pair));
    for c in char::decode_utf16(units) {
        text.push(c.ok()?);
    }
    Some(Token(text))
}

/// Reads `bot.xml`'s bytes and decrypts its password to the token. `None` is a refusal, a failed
/// decryption included.
pub fn read_token(bytes: &[u8], unprotect: &dyn Unprotect) -> Option<Token> {
    let blob = parse_credential_blob(bytes)?;
    let plain = unprotect.unprotect(&blob)?;
    token_from_plaintext(&plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(props: &str) -> String {
        format!(
            r#"<Objs Version="1.1.0.1" xmlns="{NS}"><Obj RefId="0"><TN RefId="0"><T>System.Management.Automation.PSCredential</T><T>System.Object</T></TN><ToString>System.Management.Automation.PSCredential</ToString><Props>{props}</Props></Obj></Objs>"#
        )
    }

    const GOOD: &str = r#"<S N="UserName">bot</S><SS N="Password">0a0B</SS>"#;

    #[test]
    fn a_prefixed_n_or_version_is_not_the_attribute() {
        // The `N` and `Version` attributes are in no namespace (operator.md, The Bot credential).
        let prefixed_n =
            format!(r#"<S xmlns:x="{NS}" x:N="UserName">bot</S><SS N="Password">0a0B</SS>"#);
        assert_eq!(parse_credential_blob(doc(&prefixed_n).as_bytes()), None);
        let prefixed_version = doc(GOOD).replacen(
            r#"<Objs Version="1.1.0.1""#,
            r#"<Objs xmlns:x="urn:x" x:Version="9""#,
            1,
        );
        assert_eq!(
            parse_credential_blob(prefixed_version.as_bytes()),
            Some(vec![0x0a, 0x0b]),
            "An Objs with no Version is accepted"
        );
    }

    #[test]
    fn reads_the_hex_from_the_written_structure() {
        assert_eq!(
            parse_credential_blob(doc(GOOD).as_bytes()),
            Some(vec![0x0a, 0x0b])
        );
    }

    #[test]
    fn utf16le_with_its_mark_and_utf8_with_a_mark_are_read() {
        let text = format!(
            "<?xml version=\"1.0\" encoding=\"utf-16\"?>\r\n{}",
            doc(GOOD)
        );
        let mut le = vec![0xFF, 0xFE];
        le.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(parse_credential_blob(&le), Some(vec![0x0a, 0x0b]));
        let mut u8m = vec![0xEF, 0xBB, 0xBF];
        u8m.extend(doc(GOOD).as_bytes());
        assert_eq!(parse_credential_blob(&u8m), Some(vec![0x0a, 0x0b]));
    }

    #[test]
    fn a_prefixed_namespace_is_the_same() {
        let text = format!(
            r#"<p:Objs xmlns:p="{NS}"><p:Obj><p:TN><p:T>System.Management.Automation.PSCredential</p:T></p:TN><p:Props><p:S N="username">bot</p:S><p:SS N="PASSWORD">ff</p:SS></p:Props></p:Obj></p:Objs>"#
        );
        assert_eq!(parse_credential_blob(text.as_bytes()), Some(vec![0xff]));
    }

    #[test]
    fn an_empty_user_name_is_accepted() {
        // The `UserName` element MUST be present; its text is not read (architect, 2026-11-19,
        // for E0.06-T01-S04).
        for user_name in [r#"<S N="UserName"></S>"#, r#"<S N="UserName"/>"#] {
            let props = format!(r#"{user_name}<SS N="Password">0a</SS>"#);
            assert_eq!(
                parse_credential_blob(doc(&props).as_bytes()),
                Some(vec![0x0a]),
                "{user_name}"
            );
        }
    }

    #[test]
    fn refusals() {
        let cases = [
            doc(r#"<S N="UserName">bot</S>"#),
            doc(r#"<SS N="Password">0a</SS>"#),
            doc(r#"<S N="UserName">bot</S><S N="Password">plain</S>"#),
            doc(r#"<S N="UserName">bot</S><SS N="Password">0a</SS><SS N="password">0a</SS>"#),
            doc(r#"<S N="UserName">bot</S><S N="USERNAME">b</S><SS N="Password">0a</SS>"#),
            doc(r#"<S N="UserName">bot</S><SS N="Password"></SS>"#),
            doc(r#"<S N="UserName">bot</S><SS N="Password">0a0</SS>"#),
            doc(r#"<S N="UserName">bot</S><SS N="Password"> 0a</SS>"#),
            doc(r#"<S N="UserName">bot</S><SS N="Password">0g</SS>"#),
            format!("<!DOCTYPE Objs>{}", doc(GOOD)),
            doc(GOOD).replace("1.1.0.1", "2.1"),
            doc(GOOD).replace("1.1.0.1", ""),
            doc(GOOD).replace("1.1.0.1", "1"),
            doc(GOOD).replace("1.1.0.1", "1.1.1.1.1"),
            doc(GOOD).replace("PSCredential</T>", "PSCredentialX</T>"),
            doc(GOOD).replace("</Obj></Objs>", "</Obj><Obj/></Objs>"),
            doc(GOOD).replace(NS, "urn:other"),
            format!(
                "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>{}",
                doc(GOOD)
            ),
            String::new(),
            "not xml".to_owned(),
        ];
        for c in cases {
            assert_eq!(parse_credential_blob(c.as_bytes()), None, "{c}");
        }
    }

    /// The architect's 2026-11-19 rulings for the credential's reader (operator.md, What the
    /// laptop checks).
    #[test]
    fn the_rulings_for_the_reader() {
        // `Version`: numbers that each fit a 32-bit signed integer, leading zeros allowed, the
        // first MUST be 1.
        for ok in [
            "01.1",
            "1.01",
            "001.0.0.0",
            "1.2147483647",
            "1.0000000000001",
        ] {
            assert!(version_ok(ok), "{ok}");
        }
        for bad in [
            "1.2147483648",
            "1.99999999999",
            "10.1",
            "0.1",
            "00.1",
            "1.+1",
        ] {
            assert!(!version_ok(bad), "{bad}");
        }

        // An XML declaration that names an encoding other than the bytes' own is refused.
        let utf8_named_utf16 = format!("<?xml version=\"1.0\" encoding=\"utf-16\"?>{}", doc(GOOD));
        assert_eq!(parse_credential_blob(utf8_named_utf16.as_bytes()), None);
        let named_utf8 = format!("<?xml version=\"1.0\" encoding='UTF-8'?>{}", doc(GOOD));
        assert!(parse_credential_blob(named_utf8.as_bytes()).is_some());
        let mut utf16_named_utf8 = vec![0xFF, 0xFE];
        utf16_named_utf8.extend(named_utf8.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(parse_credential_blob(&utf16_named_utf8), None);
        let named_be = format!("<?xml version=\"1.0\" encoding=\"utf-16be\"?>{}", doc(GOOD));
        let mut le_named_be = vec![0xFF, 0xFE];
        le_named_be.extend(named_be.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(parse_credential_blob(&le_named_be), None);

        // The members of every `Props` are counted together.
        let split = doc(GOOD).replace(
            r#"</S><SS N="Password">"#,
            r#"</S></Props><Props><SS N="Password">"#,
        );
        assert_eq!(
            parse_credential_blob(split.as_bytes()),
            Some(vec![0x0a, 0x0b])
        );
        for second in [
            r#"<SS N="password">0c</SS>"#,
            r#"<S N="USERNAME">other</S>"#,
        ] {
            let twice = doc(GOOD).replace("</Props>", &format!("</Props><Props>{second}</Props>"));
            assert_eq!(parse_credential_blob(twice.as_bytes()), None, "{second}");
        }
    }

    #[test]
    fn version_absent_or_spaced_is_accepted() {
        let no_version = doc(GOOD).replace(r#" Version="1.1.0.1""#, "");
        assert!(parse_credential_blob(no_version.as_bytes()).is_some());
        let spaced = doc(GOOD).replace("1.1.0.1", " 1.1 ");
        assert!(parse_credential_blob(spaced.as_bytes()).is_some());
    }

    #[test]
    fn comments_and_cdata_mean_what_xml_says() {
        let props = r#"<S N="UserName">bot</S><!-- c --><SS N="Password">0a<!-- c --><![CDATA[0b]]>&#x30;c</SS>"#;
        assert_eq!(
            parse_credential_blob(doc(props).as_bytes()),
            Some(vec![0x0a, 0x0b, 0x0c])
        );
    }

    #[test]
    fn plaintext_rules() {
        assert!(
            token_from_plaintext(&[0x61, 0x00, 0x62]).is_none(),
            "Odd byte count"
        );
        assert!(
            token_from_plaintext(&[0x00, 0xD8]).is_none(),
            "Unpaired surrogate"
        );
        let t = token_from_plaintext(&[0x61, 0x00, 0x62, 0x00]).expect("Token");
        assert_eq!(t.expose(), "ab");
    }
}
