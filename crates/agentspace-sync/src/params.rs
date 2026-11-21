//! The stack parameters file, `%USERPROFILE%\.agentspace\stack-parameters.json` (docs/operator.md,
//! deploy, step 2; docs/state.md, Files in the checkout; D40).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! A person writes it, so a UTF-8 byte order mark and the whitespace JSON allows MUST be accepted.
//! A file that cannot be opened, cannot be parsed, or does not match its schema MUST fail `deploy`
//! before it calls AWS, with one reason that names the first problem found and holds no line
//! break. Nothing is skipped silently.
//!
//! **A file that names `SshPublicKey`, or names any member twice, MUST NOT be taken as valid**
//! (operator.md, deploy, step 2; architect, 2026-11-19, for E0.06-T03-S02): `deploy` sets
//! `SshPublicKey` itself from the user's public key, and a file that could override it would put
//! another key on the box with no word said. The reason names the member. A JSON reader keeps the
//! last of two members of one name, so the repeat is looked for in the text itself.

use std::fmt;
use std::path::Path;
use std::sync::OnceLock;

use agentspace_operator_api::{CallerFailure, Failure};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};

use crate::embedded::STACK_PARAMETERS_SCHEMA;

/// The longest value the schema allows, in characters.
const MAX_VALUE: usize = 4096;
/// The longest name the schema allows, in characters.
const MAX_NAME: usize = 255;
/// The stack parameter `deploy` sets itself, from `%USERPROFILE%\.ssh\id_ed25519.pub`, which the
/// file MUST NOT name (operator.md, deploy, step 2).
const DEPLOYS_OWN_PARAMETER: &str = "SshPublicKey";
/// The stack parameters `deploy` sets itself for a host that is not the box, which the file MUST
/// NOT name either: on the box either would change the `agentbox` stack's names or its first
/// boot, whose change set MUST stay empty (operator.md, deploy, step 2; D44).
const DEPLOYS_OWN_FOR_A_HOST: [&str; 2] = ["HostName", "CaPublicKey"];

/// Reads and checks the file at `path`, returning its parameters as `(name, value)` in the file's
/// order.
pub(crate) fn read(path: &Path) -> Result<Vec<(String, String)>, Failure> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(fail(format!(
                "agentspace: Missing {}; copy infra\\stack-parameters.example.json and fill it in.",
                path.display()
            )));
        }
        Err(_) => return Err(not_valid(path, "it cannot be opened")),
    };
    parse(&bytes).map_err(|reason| not_valid(path, &reason))
}

fn fail(message: String) -> Failure {
    Failure {
        message,
        caller: CallerFailure::CouldNotReach,
    }
}

fn not_valid(path: &Path, reason: &str) -> Failure {
    fail(format!(
        "agentspace: {} is not a valid stack parameters file: {reason}.",
        path.display()
    ))
}

/// The parameters in a file's bytes, or the reason the file is not valid: a sentence fragment
/// with no line break and no final period.
pub(crate) fn parse(bytes: &[u8]) -> Result<Vec<(String, String)>, String> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| "it is not UTF-8 text".to_string())?;
    let value: Value = serde_json::from_str(text).map_err(|e| {
        format!(
            "it is not valid JSON (line {}, column {})",
            e.line(),
            e.column()
        )
    })?;
    if let Some(name) = repeated_member(text) {
        return Err(format!("it names the member '{}' twice", printable(&name)));
    }
    let object = value
        .as_object()
        .ok_or_else(|| "it is not a JSON object".to_string())?;
    let parameters = check_top(object)?;
    let mut out = Vec::with_capacity(parameters.len());
    for (name, value) in parameters {
        if name == DEPLOYS_OWN_PARAMETER {
            return Err(format!(
                "it names the parameter {DEPLOYS_OWN_PARAMETER}, which deploy sets itself from id_ed25519.pub"
            ));
        }
        out.push((name.clone(), check_parameter(name, value)?));
    }
    // The schema itself, carried inside the operator, is the last word: every check above is one
    // of its rules, so this refuses only what they missed.
    if !schema_accepts(&value) {
        return Err("it does not match its schema".to_string());
    }
    // operator.md's own check, made after the schema, which does not forbid them (architect,
    // 2026-11-20, for v0.07-T01).
    if let Some((name, _)) = out
        .iter()
        .find(|(name, _)| DEPLOYS_OWN_FOR_A_HOST.contains(&name.as_str()))
    {
        // Architect's to confirm: the reason's wording.
        return Err(format!(
            "it names the parameter {name}, which deploy sets itself for a host that is not the box"
        ));
    }
    Ok(out)
}

fn check_top(object: &Map<String, Value>) -> Result<&Map<String, Value>, String> {
    if let Some(other) = object
        .keys()
        .find(|k| k.as_str() != "version" && k.as_str() != "parameters")
    {
        return Err(format!(
            "it has a member '{}', which the format does not have",
            printable(other)
        ));
    }
    let version = object
        .get("version")
        .ok_or_else(|| "it has no version".to_string())?;
    // The schema's `"const": 0`, which JSON Schema compares as a number.
    if version.as_f64() != Some(0.0) {
        return Err("its version is not 0".to_string());
    }
    object
        .get("parameters")
        .ok_or_else(|| "it has no parameters".to_string())?
        .as_object()
        .ok_or_else(|| "its parameters member is not an object".to_string())
}

fn check_parameter(name: &str, value: &Value) -> Result<String, String> {
    let shown = printable(name);
    let mut chars = name.chars();
    let name_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric())
        && name.len() <= MAX_NAME;
    if !name_ok {
        return Err(format!(
            "the parameter name '{shown}' is not a letter followed by up to 254 letters and digits"
        ));
    }
    let value = value
        .as_str()
        .ok_or_else(|| format!("the parameter {shown} is not a string"))?;
    if value.is_empty() {
        return Err(format!("the parameter {shown} is empty"));
    }
    if value.chars().count() > MAX_VALUE {
        return Err(format!(
            "the parameter {shown} is longer than {MAX_VALUE} characters"
        ));
    }
    let first = value.chars().next().is_some_and(char::is_whitespace);
    let last = value.chars().next_back().is_some_and(char::is_whitespace);
    if first || last {
        return Err(format!(
            "the parameter {shown} starts or ends with whitespace"
        ));
    }
    if value.contains(['\n', '\r', '\u{2028}', '\u{2029}']) {
        return Err(format!("the parameter {shown} holds a line break"));
    }
    Ok(value.to_string())
}

fn schema_accepts(value: &Value) -> bool {
    static VALIDATOR: OnceLock<Option<jsonschema::Validator>> = OnceLock::new();
    VALIDATOR
        .get_or_init(|| {
            let schema: Value = serde_json::from_str(STACK_PARAMETERS_SCHEMA).ok()?;
            jsonschema::draft202012::new(&schema).ok()
        })
        .as_ref()
        .is_some_and(|v| v.is_valid(value))
}

/// A name as it is shown in a reason: each control character written `?`, and at most 64
/// characters, so the message stays one short line.
fn printable(text: &str) -> String {
    let mut shown: String = text
        .chars()
        .take(64)
        .map(|c| if c.is_control() { '?' } else { c })
        .collect();
    if text.chars().count() > 64 {
        shown.push_str("...");
    }
    shown
}

/// The first member name an object in `text` gives twice, at any depth. `text` already parsed.
fn repeated_member(text: &str) -> Option<String> {
    let mut de = serde_json::Deserializer::from_str(text);
    match FindRepeat.deserialize(&mut de) {
        Ok(()) => None,
        Err(e) => {
            // serde_json adds its own ` at line N column M` after the end mark; the name is
            // everything up to the last mark, whatever the name itself holds.
            let message = e.to_string();
            let rest = message.strip_prefix(REPEAT_PREFIX)?;
            let end = rest.rfind(REPEAT_END)?;
            Some(rest[..end].to_string())
        }
    }
}

const REPEAT_PREFIX: &str = "repeated member \u{1}";
const REPEAT_END: char = '\u{1}';

/// Walks a JSON document and fails at the first object that names a member twice.
struct FindRepeat;

impl<'de> DeserializeSeed<'de> for FindRepeat {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for FindRepeat {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("JSON")
    }
    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while seq.next_element_seed(FindRepeat)?.is_some() {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut seen = std::collections::HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(de::Error::custom(format!(
                    "{REPEAT_PREFIX}{key}{REPEAT_END}"
                )));
            }
            map.next_value_seed(FindRepeat)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(text: &str) -> String {
        parse(text.as_bytes()).unwrap_err()
    }

    #[test]
    fn a_good_file_gives_its_parameters_in_order() {
        let text = "\u{feff}{ \"version\": 0,\r\n \"parameters\": {\"SubnetId\": \"SUBNET_X\", \"VpcId\": \"VPC_X\", \"RootVolumeGiB\": \"12\"} }\r\n";
        assert_eq!(
            parse(text.as_bytes()).unwrap(),
            vec![
                ("SubnetId".to_string(), "SUBNET_X".to_string()),
                ("VpcId".to_string(), "VPC_X".to_string()),
                ("RootVolumeGiB".to_string(), "12".to_string()),
            ]
        );
        assert_eq!(
            parse(br#"{"version":0,"parameters":{}}"#).unwrap(),
            Vec::<(String, String)>::new()
        );
    }

    #[test]
    fn each_problem_has_its_own_reason_on_one_line() {
        let cases = [
            ("", "it is not valid JSON (line 1, column 0)"),
            (
                "{\"version\":0,",
                "it is not valid JSON (line 1, column 13)",
            ),
            ("[]", "it is not a JSON object"),
            (r#"{"parameters":{}}"#, "it has no version"),
            (r#"{"version":1,"parameters":{}}"#, "its version is not 0"),
            (r#"{"version":"0","parameters":{}}"#, "its version is not 0"),
            (r#"{"version":0}"#, "it has no parameters"),
            (
                r#"{"version":0,"parameters":[]}"#,
                "its parameters member is not an object",
            ),
            (
                r#"{"version":0,"parameters":{},"extra":1}"#,
                "it has a member 'extra', which the format does not have",
            ),
            (
                r#"{"version":0,"parameters":{"1x":"a"}}"#,
                "the parameter name '1x' is not a letter followed by up to 254 letters and digits",
            ),
            (
                r#"{"version":0,"parameters":{"Vpc_Id":"a"}}"#,
                "the parameter name 'Vpc_Id' is not a letter followed by up to 254 letters and digits",
            ),
            (
                r#"{"version":0,"parameters":{"A":8}}"#,
                "the parameter A is not a string",
            ),
            (
                r#"{"version":0,"parameters":{"A":""}}"#,
                "the parameter A is empty",
            ),
            (
                r#"{"version":0,"parameters":{"A":" a"}}"#,
                "the parameter A starts or ends with whitespace",
            ),
            (
                r#"{"version":0,"parameters":{"A":"a\tb\nc"}}"#,
                "the parameter A holds a line break",
            ),
            (
                r#"{"version":0,"parameters":{"A":"a","A":"b"}}"#,
                "it names the member 'A' twice",
            ),
            (
                r#"{"version":0,"version":0,"parameters":{}}"#,
                "it names the member 'version' twice",
            ),
            (
                r#"{"version":0,"parameters":{"VpcId":"a","SshPublicKey":"ssh-ed25519 AAAA other"}}"#,
                "it names the parameter SshPublicKey, which deploy sets itself from id_ed25519.pub",
            ),
        ];
        for (text, want) in cases {
            let got = reason(text);
            assert_eq!(got, want, "{text}");
            assert!(!got.contains('\n') && !got.contains('\r'));
        }
        assert_eq!(parse(b"\xff\xfe{}").unwrap_err(), "it is not UTF-8 text");
        let long = format!(
            r#"{{"version":0,"parameters":{{"A":"{}"}}}}"#,
            "x".repeat(4097)
        );
        assert_eq!(
            reason(&long),
            "the parameter A is longer than 4096 characters"
        );
        let ok = format!(
            r#"{{"version":0,"parameters":{{"A":"{}"}}}}"#,
            "x".repeat(4096)
        );
        assert!(parse(ok.as_bytes()).is_ok());
    }

    /// serde_json keeps the last of two members of one name, so a reader that trusted its value
    /// would pass `b` here with no word said. Each repeat is refused, at any depth, whatever the
    /// name holds and however the two spellings differ in the text.
    #[test]
    fn a_member_named_twice_is_refused_wherever_it_is() {
        let cases = [
            // The last one would win: two values for one stack parameter.
            (
                r#"{"version":0,"parameters":{"VpcId":"a","SubnetId":"s","VpcId":"b"}}"#,
                "VpcId",
            ),
            // Equal values are a repeat all the same.
            (
                r#"{"version":0,"parameters":{"VpcId":"a","VpcId":"a"}}"#,
                "VpcId",
            ),
            // The same name spelled with an escape is the same member.
            (
                r#"{"version":0,"parameters":{"VpcId":"a","\u0056pcId":"b"}}"#,
                "VpcId",
            ),
            (
                r#"{"version":0,"parameters":{},"parameters":{"VpcId":"a"}}"#,
                "parameters",
            ),
            (r#"{"version":1,"version":0,"parameters":{}}"#, "version"),
            // A repeat is found before the other checks, so it is the first problem named.
            (r#"[{"a":1,"a":2}]"#, "a"),
            // A name that holds the mark this module's own error text uses.
            (
                r#"{"version":0,"parameters":{"a\u0001b":"x","a\u0001b":"y"}}"#,
                "a?b",
            ),
        ];
        for (text, name) in cases {
            assert_eq!(
                reason(text),
                format!("it names the member '{name}' twice"),
                "{text}"
            );
        }
        // Names that differ, in case or in one object and another, are not a repeat.
        assert!(parse(br#"{"version":0,"parameters":{"VpcId":"a","vpcId":"b"}}"#).is_ok());
        assert_eq!(repeated_member(r#"[{"a":1},{"a":2},{"b":{"a":3}}]"#), None);
    }

    /// `deploy` sets `SshPublicKey` itself, so a file that names it is refused, alone or among
    /// others, whatever its value; a name that only resembles it is another parameter.
    #[test]
    fn a_file_that_names_ssh_public_key_is_refused() {
        let want =
            "it names the parameter SshPublicKey, which deploy sets itself from id_ed25519.pub";
        for text in [
            r#"{"version":0,"parameters":{"SshPublicKey":"ssh-ed25519 AAAA other"}}"#,
            r#"{"version":0,"parameters":{"SubnetId":"s","SshPublicKey":"x","VpcId":"v"}}"#,
            // Its own name is the problem, before anything about its value.
            r#"{"version":0,"parameters":{"SshPublicKey":""}}"#,
            r#"{"version":0,"parameters":{"SshPublicKey":7}}"#,
        ] {
            assert_eq!(reason(text), want, "{text}");
        }
        assert_eq!(
            parse(br#"{"version":0,"parameters":{"SshPublicKey2":"x","sshPublicKey":"y"}}"#)
                .unwrap()
                .len(),
            2
        );
    }

    /// `deploy` sets `HostName` and `CaPublicKey` itself for a host, and on the box either would
    /// change its change set, so a file that names either is refused.
    #[test]
    fn a_file_that_names_host_name_or_ca_public_key_is_refused() {
        for name in ["HostName", "CaPublicKey"] {
            let text = format!(r#"{{"version":0,"parameters":{{"VpcId":"v","{name}":"x"}}}}"#);
            assert_eq!(
                reason(&text),
                format!(
                    "it names the parameter {name}, which deploy sets itself for a host that is not the box"
                )
            );
        }
        assert!(
            parse(br#"{"version":0,"parameters":{"Hostname":"x","CaPublicKeys":"y"}}"#).is_ok()
        );
    }

    #[test]
    fn a_control_character_in_a_name_is_shown_as_a_question_mark() {
        assert_eq!(
            reason(r#"{"version":0,"parameters":{},"a\nb":1}"#),
            "it has a member 'a?b', which the format does not have"
        );
    }

    #[test]
    fn the_carried_schema_builds_and_agrees() {
        assert!(schema_accepts(
            &serde_json::json!({"version":0,"parameters":{"VpcId":"VPC_X"}})
        ));
        assert!(!schema_accepts(
            &serde_json::json!({"version":0,"parameters":{"VpcId":""}})
        ));
    }

    #[test]
    fn missing_and_unopenable_files() {
        let dir = crate::tests::Scratch::new("params");
        let path = dir.path().join("stack-parameters.json");
        let f = read(&path).unwrap_err();
        assert_eq!(
            f.message,
            format!(
                "agentspace: Missing {}; copy infra\\stack-parameters.example.json and fill it in.",
                path.display()
            )
        );
        std::fs::create_dir(&path).unwrap();
        let f = read(&path).unwrap_err();
        assert_eq!(
            f.message,
            format!(
                "agentspace: {} is not a valid stack parameters file: it cannot be opened.",
                path.display()
            )
        );
    }
}
