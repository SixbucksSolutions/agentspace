//! The holder record, `<state folder>\board-holder.json` (state.md, The holder record).

use crate::NotValid;
use crate::json::{Schema, file_bytes, is_time, is_valid, object, read_value, why_not_valid};
use agentspace_keeper_api::HolderRecord;
use serde_json::Value;

/// The highest PID a record names (state.md, The holder record).
const MAX_PID: u64 = 2_147_483_647;

/// The record's bytes, as a writer MUST write them: strict to the schema (`lease_id` a normalized
/// version 4 UUID, `alias` `agentspace-<project>-<8 hex digits>`), compact, in the order `pid`,
/// `process_started_at`, `lease_id`, `alias`, then one LF. A record that would not validate is
/// refused, and nothing is produced.
pub fn serialize_holder_record(record: &HolderRecord) -> Result<Vec<u8>, NotValid> {
    let doc = object([
        ("pid", Value::from(record.pid)),
        (
            "process_started_at",
            Value::from(record.process_started_at.as_str()),
        ),
        ("lease_id", Value::from(record.lease_id.as_str())),
        ("alias", Value::from(record.alias.as_str())),
    ]);
    if !is_valid(Schema::Holder, &doc) {
        return Err(why_not_valid(Schema::Holder, &doc, "The holder record"));
    }
    file_bytes(&doc)
}

/// Whether `s` is an 8-4-4-4-12 hex UUID in any case: the form a reader accepts for `lease_id`
/// (state.md, The holder record; gap FB-23).
fn is_uuid(s: &str) -> bool {
    let groups: Vec<&str> = s.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8usize, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Whether `s` matches `^[A-Za-z0-9][A-Za-z0-9._-]{0,127}\z`: the form a reader accepts for
/// `alias` (state.md, The holder record; architect, 2026-11-19, gap FB-23). Hand-written, so that
/// no `$` can let a trailing LF through.
pub(crate) fn is_alias(s: &str) -> bool {
    let b = s.as_bytes();
    (1..=128).contains(&b.len())
        && b[0].is_ascii_alphanumeric()
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}

/// Reads a holder record's bytes. **A record that cannot be read or parsed, or whose members do
/// not have these forms, is no record**, and so is `None` (state.md, The holder record):
/// - the text is not UTF-8 JSON of one object, or an object names a member twice;
/// - a member is missing, or is not of its type: `pid` an integer from 1 to 2147483647,
///   `process_started_at` a string `YYYY-MM-DDTHH:MM:SS.fffZ`, `lease_id` and `alias` strings;
/// - `lease_id` is not an 8-4-4-4-12 hex UUID (any case, any version, as the prototype accepts);
/// - `alias` does not match `^[A-Za-z0-9][A-Za-z0-9._-]{0,127}\z` (gap FB-23): an alias with a
///   trailing LF, a space, a slash or a leading dot is no record.
///
/// A reader is more tolerant than the writer, so a record the strict schema refuses may be read.
/// It does not require the final LF and accepts whitespace JSON allows; it tolerates a leading
/// UTF-8 byte order mark and a member the schema does not name, which it ignores; and it keeps
/// `lease_id` as stored, for the keeper to compare without regard to ASCII case (state.md, The
/// holder record; architect, 2026-11-19, for E0.06-T01-S07).
pub fn parse_holder_record(bytes: &[u8]) -> Option<HolderRecord> {
    let doc = read_value(bytes)?;
    let o = doc.as_object()?;
    let pid = o
        .get("pid")?
        .as_u64()
        .filter(|p| (1..=MAX_PID).contains(p))?;
    let started = o
        .get("process_started_at")?
        .as_str()
        .filter(|s| is_time(s, 3))?;
    let lease_id = o.get("lease_id")?.as_str().filter(|s| is_uuid(s))?;
    let alias = o.get("alias")?.as_str().filter(|s| is_alias(s))?;
    Some(HolderRecord {
        pid: u32::try_from(pid).ok()?,
        process_started_at: started.to_string(),
        lease_id: lease_id.to_string(),
        alias: alias.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> HolderRecord {
        HolderRecord {
            pid: 4242,
            process_started_at: "2026-11-19T04:05:06.123Z".to_string(),
            lease_id: "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d".to_string(),
            alias: "agentspace-demo-0a1b2c3d".to_string(),
        }
    }

    #[test]
    fn the_bytes_are_compact_in_member_order_and_end_in_one_lf() {
        let bytes = serialize_holder_record(&record()).unwrap();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "{\"pid\":4242,\"process_started_at\":\"2026-11-19T04:05:06.123Z\",\
             \"lease_id\":\"0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d\",\
             \"alias\":\"agentspace-demo-0a1b2c3d\"}\n"
        );
    }

    #[test]
    fn a_record_round_trips() {
        let bytes = serialize_holder_record(&record()).unwrap();
        assert_eq!(parse_holder_record(&bytes), Some(record()));
    }

    #[test]
    fn a_writer_refuses_what_the_strict_schema_refuses() {
        let mut r = record();
        r.pid = 0;
        assert!(serialize_holder_record(&r).is_err());
        let mut r = record();
        r.pid = 2_147_483_648;
        assert!(serialize_holder_record(&r).is_err());
        let mut r = record();
        r.process_started_at = "2026-11-19T04:05:06Z".to_string();
        assert!(serialize_holder_record(&r).is_err());
        let mut r = record();
        r.lease_id = "0A1B2C3D-4E5F-4A6B-8C7D-9E0F1A2B3C4D".to_string();
        assert!(serialize_holder_record(&r).is_err());
        let mut r = record();
        r.alias = "plain-alias".to_string();
        assert!(serialize_holder_record(&r).is_err());
        let mut r = record();
        r.alias = "agentspace-demo-0a1b2c3d\n".to_string();
        assert!(serialize_holder_record(&r).is_err());
        let mut r = record();
        r.pid = 2_147_483_647;
        assert!(serialize_holder_record(&r).is_ok());
    }

    fn doc(pid: &str, started: &str, lease: &str, alias: &str) -> String {
        format!(
            "{{\"pid\":{pid},\"process_started_at\":{started},\"lease_id\":{lease},\"alias\":{alias}}}\n"
        )
    }

    fn good() -> [&'static str; 4] {
        [
            "7",
            "\"2026-11-19T04:05:06.123Z\"",
            "\"0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d\"",
            "\"agentspace-demo-0a1b2c3d\"",
        ]
    }

    fn read_with(i: usize, v: &str) -> Option<HolderRecord> {
        let mut m = good();
        m[i] = v;
        parse_holder_record(doc(m[0], m[1], m[2], m[3]).as_bytes())
    }

    #[test]
    fn a_good_document_is_read() {
        assert!(read_with(0, "7").is_some());
    }

    #[test]
    fn a_record_whose_members_are_not_of_their_forms_is_no_record() {
        for pid in [
            "0",
            "-1",
            "2147483648",
            "1.5",
            "\"7\"",
            "null",
            "true",
            "7.0",
        ] {
            assert!(read_with(0, pid).is_none(), "pid {pid}");
        }
        for t in [
            "\"2026-11-19T04:05:06Z\"",
            "\"2026-11-19T04:05:06.123456Z\"",
            "\"2026-11-19 04:05:06.123Z\"",
            "\"2026-11-19T04:05:06.123+00:00\"",
            "\"2026-13-19T04:05:06.123Z\"",
            "7",
            "null",
        ] {
            assert!(read_with(1, t).is_none(), "time {t}");
        }
        for l in [
            "\"0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4\"",
            "\"0a1b2c3d4e5f4a6b8c7d9e0f1a2b3c4d\"",
            "\"0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4g\"",
            "\"0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d\\n\"",
            "7",
            "null",
        ] {
            assert!(read_with(2, l).is_none(), "lease {l}");
        }
    }

    #[test]
    fn a_reader_accepts_any_uuid_in_any_case() {
        let r = read_with(2, "\"0A1B2C3D-4E5F-1A6B-0C7D-9E0F1A2B3C4D\"").unwrap();
        assert_eq!(r.lease_id, "0A1B2C3D-4E5F-1A6B-0C7D-9E0F1A2B3C4D");
    }

    #[test]
    fn a_reader_accepts_the_alias_form_of_gap_fb_23() {
        for a in ["a", "A9", "plain.alias_x-y", "agentspace-Demo-0A1B2C3D"] {
            let r = read_with(3, &format!("\"{a}\"")).unwrap();
            assert_eq!(r.alias, a);
        }
        let longest = format!("a{}", "b".repeat(127));
        assert!(read_with(3, &format!("\"{longest}\"")).is_some());
    }

    #[test]
    fn an_alias_that_does_not_match_the_reader_rule_is_no_record() {
        let too_long = format!("a{}", "b".repeat(128));
        let cases = [
            "\"\"".to_string(),
            "\".hidden\"".to_string(),
            "\"-dash\"".to_string(),
            "\"_under\"".to_string(),
            "\"has space\"".to_string(),
            "\"a/b\"".to_string(),
            "\"a\\\\b\"".to_string(),
            "\"ünï\"".to_string(),
            "\"alias\\n\"".to_string(),
            "\"alias\\r\\n\"".to_string(),
            format!("\"{too_long}\""),
            "7".to_string(),
            "null".to_string(),
        ];
        for a in cases {
            assert!(read_with(3, &a).is_none(), "alias {a}");
        }
    }

    #[test]
    fn a_record_that_cannot_be_read_or_parsed_is_no_record() {
        assert!(parse_holder_record(b"").is_none());
        assert!(parse_holder_record(b"not json").is_none());
        assert!(parse_holder_record(b"[]").is_none());
        assert!(parse_holder_record(b"null").is_none());
        assert!(parse_holder_record(b"\xFF\xFE{}").is_none());
        let full = doc(good()[0], good()[1], good()[2], good()[3]);
        assert!(parse_holder_record(&full.as_bytes()[..full.len() - 4]).is_none());
        // A member missing.
        for drop in ["pid", "process_started_at", "lease_id", "alias"] {
            let mut v: Value = serde_json::from_str(&full).unwrap();
            v.as_object_mut().unwrap().remove(drop);
            assert!(
                parse_holder_record(v.to_string().as_bytes()).is_none(),
                "{drop}"
            );
        }
        // A member named twice.
        let twice = full.replace("\"pid\":7", "\"pid\":7,\"pid\":8");
        assert!(parse_holder_record(twice.as_bytes()).is_none());
    }
}
