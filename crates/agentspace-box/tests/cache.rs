//! The instance-ID cache (docs/operator.md, Laptop files; docs/state.md, Files in the checkout;
//! schemas/state/instance-id.state.schema.json): its bytes, every case that counts as missing,
//! FA-16's message, the rebuild from `cloudformation describe-stacks`, and the "no InstanceId
//! output" failure that writes no cache.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Nothing here touches a file: the crate
//! gives bytes and decisions, and writing is T03's.

mod common;
use common::*;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;

const STACKS: &str = "cloudformation describe-stacks";

/// A path under the temp folder, only ever named in a message and never opened.
fn cache_path() -> PathBuf {
    std::env::temp_dir()
        .join("agentspace-box-test-home")
        .join(".agentspace")
        .join("instance-id.json")
}

fn cache_json(id: &str) -> String {
    format!(r#"{{"version":0,"instance_id":"{id}"}}"#)
}

fn read(bytes: &[u8]) -> Result<Option<String>, Failure> {
    api::read_cache(&cache_path(), Ok(bytes.to_vec()))
}

// -- The bytes -------------------------------------------------------------------------------

#[test]
fn the_cache_is_compact_json_with_no_bom_and_no_line_ending() {
    let bytes = api::cache_bytes(ID);
    assert_eq!(bytes, cache_json(ID).into_bytes());
    assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]), "A byte order mark");
    assert!(
        !bytes.ends_with(b"\n") && !bytes.contains(&b'\r'),
        "A line ending"
    );
    assert!(
        !bytes.contains(&b' ') && !bytes.contains(&b'\n'),
        "Not compact"
    );
}

#[test]
fn the_cache_holds_the_id_exactly_as_given_and_reads_back() {
    for id in ["i-0123456789abcdef0", "i-0123abcd", "i-fedcba9876543210f"] {
        let bytes = api::cache_bytes(id);
        assert_eq!(bytes, cache_json(id).into_bytes());
        assert_eq!(read(&bytes), Ok(Some(id.to_string())));
    }
}

// -- What reads as an ID -----------------------------------------------------------------------

#[test]
fn a_valid_cache_reads_whatever_json_allows_around_it() {
    let id = ID;
    let valid: Vec<(&str, Vec<u8>)> = vec![
        ("Compact", cache_json(id).into_bytes()),
        (
            "Newline at the end",
            format!("{}\n", cache_json(id)).into_bytes(),
        ),
        (
            "CRLF at the end",
            format!("{}\r\n", cache_json(id)).into_bytes(),
        ),
        (
            "Pretty printed",
            format!("{{\n  \"version\": 0,\n  \"instance_id\": \"{id}\"\n}}\n").into_bytes(),
        ),
        (
            "Tabs and spaces",
            format!("\t {{ \"version\" :\t0 , \"instance_id\" : \"{id}\" }} \t\r\n").into_bytes(),
        ),
        (
            "Members the other way round",
            format!(r#"{{"instance_id":"{id}","version":0}}"#).into_bytes(),
        ),
        ("Byte order mark", {
            let mut b = vec![0xEF, 0xBB, 0xBF];
            b.extend(cache_json(id).into_bytes());
            b
        }),
        ("Byte order mark and whitespace", {
            let mut b = vec![0xEF, 0xBB, 0xBF];
            b.extend(format!("\r\n {}\r\n", cache_json(id)).into_bytes());
            b
        }),
    ];
    for (name, bytes) in valid {
        assert_eq!(read(&bytes), Ok(Some(id.to_string())), "{name}");
    }
}

#[test]
fn an_eight_digit_id_reads_too() {
    assert_eq!(
        read(cache_json("i-0123abcd").as_bytes()),
        Ok(Some("i-0123abcd".to_string()))
    );
}

// -- What counts as missing ----------------------------------------------------------------------

#[test]
fn an_empty_cache_is_missing() {
    assert_eq!(read(b""), Ok(None));
}

#[test]
fn a_cache_of_only_whitespace_is_missing() {
    for bytes in [
        &b" "[..],
        b"\n",
        b"\r\n",
        b" \t\r\n \n",
        b"                ",
    ] {
        assert_eq!(read(bytes), Ok(None), "{bytes:?}");
    }
}

#[test]
fn a_cache_of_only_a_byte_order_mark_is_missing() {
    assert_eq!(read(&[0xEF, 0xBB, 0xBF]), Ok(None));
    assert_eq!(read(&[0xEF, 0xBB, 0xBF, b'\n']), Ok(None));
}

#[test]
fn a_cache_that_cannot_be_parsed_is_missing() {
    let unparsable: Vec<(&str, Vec<u8>)> = vec![
        ("An open brace", b"{".to_vec()),
        ("Truncated", cache_json(ID).as_bytes()[..20].to_vec()),
        ("Not JSON", b"not json".to_vec()),
        (
            "Single quotes",
            format!("{{'version':0,'instance_id':'{ID}'}}").into_bytes(),
        ),
        (
            "Unquoted names",
            format!("{{version:0,instance_id:\"{ID}\"}}").into_bytes(),
        ),
        (
            "A trailing comma",
            format!(r#"{{"version":0,"instance_id":"{ID}",}}"#).into_bytes(),
        ),
        (
            "Two documents",
            format!("{}{}", cache_json(ID), cache_json(ID)).into_bytes(),
        ),
        (
            "Trailing text",
            format!("{} x", cache_json(ID)).into_bytes(),
        ),
        ("Not UTF-8", vec![0xFF, 0xFE, 0x00, 0x7B]),
        ("UTF-16", {
            let mut b = vec![0xFF, 0xFE];
            for u in cache_json(ID).encode_utf16() {
                b.extend(u.to_le_bytes());
            }
            b
        }),
        // The prototype's caches, which the rewrite MUST NOT read (docs/state.md).
        ("The prototype's plain ID", ID.as_bytes().to_vec()),
        (
            "The prototype's plain ID and a line ending",
            format!("{ID}\n").into_bytes(),
        ),
    ];
    for (name, bytes) in unparsable {
        assert_eq!(read(&bytes), Ok(None), "{name}");
    }
}

#[test]
fn a_cache_that_does_not_match_its_schema_is_missing() {
    let mismatched: Vec<(&str, String)> = vec![
        ("Null", "null".to_string()),
        ("An array", "[]".to_string()),
        ("An array of the object", format!("[{}]", cache_json(ID))),
        ("A string", format!("\"{ID}\"")),
        ("A number", "0".to_string()),
        ("An empty object", "{}".to_string()),
        ("No version", format!(r#"{{"instance_id":"{ID}"}}"#)),
        ("No instance_id", r#"{"version":0}"#.to_string()),
        (
            "Version 1",
            format!(r#"{{"version":1,"instance_id":"{ID}"}}"#),
        ),
        (
            "Version a string",
            format!(r#"{{"version":"0","instance_id":"{ID}"}}"#),
        ),
        (
            "Version null",
            format!(r#"{{"version":null,"instance_id":"{ID}"}}"#),
        ),
        (
            "Version negative",
            format!(r#"{{"version":-1,"instance_id":"{ID}"}}"#),
        ),
        (
            "An extra member",
            format!(r#"{{"version":0,"instance_id":"{ID}","x":1}}"#),
        ),
        (
            "An ID that is null",
            r#"{"version":0,"instance_id":null}"#.to_string(),
        ),
        (
            "An ID that is a number",
            r#"{"version":0,"instance_id":5}"#.to_string(),
        ),
        (
            "An ID that is an array",
            format!(r#"{{"version":0,"instance_id":["{ID}"]}}"#),
        ),
        ("An empty ID", cache_json("")),
        ("An ID without its prefix", cache_json("0123456789abcdef0")),
        (
            "An ID with another prefix",
            cache_json("vol-0123456789abcdef0"),
        ),
        ("An ID in capitals", cache_json("i-0123456789ABCDEF0")),
        (
            "An ID with a capital prefix",
            cache_json("I-0123456789abcdef0"),
        ),
        ("An ID too short", cache_json("i-0123abc")),
        ("An ID of ten digits", cache_json("i-0123456789")),
        ("An ID of sixteen digits", cache_json("i-0123456789abcdef")),
        ("An ID too long", cache_json("i-0123456789abcdef01")),
        (
            "An ID that is not hexadecimal",
            cache_json("i-0123456789abcdeg0"),
        ),
        (
            "An ID with a space before",
            cache_json(" i-0123456789abcdef0"),
        ),
        (
            "An ID with a space after",
            cache_json("i-0123456789abcdef0 "),
        ),
        // The schema's anchored pattern ends `$(?!\n)`: a line feed inside the string fails it.
        (
            "An ID with a line feed inside the string",
            cache_json("i-0123456789abcdef0\\n"),
        ),
        (
            "An ID with a second line",
            cache_json("i-0123456789abcdef0\\ni-0123456789abcdef0"),
        ),
    ];
    for (name, text) in mismatched {
        assert_eq!(read(text.as_bytes()), Ok(None), "{name}: {text}");
    }
}

#[test]
fn a_cache_that_does_not_exist_is_missing() {
    assert_eq!(
        api::read_cache(&cache_path(), Err(Error::from(ErrorKind::NotFound))),
        Ok(None)
    );
}

// -- A cache that cannot be opened (FA-16) ---------------------------------------------------------

fn fa16_message(path: &std::path::Path) -> String {
    format!(
        "agentspace: Cannot read the instance-ID cache {}; remove it and run the command again.",
        path.display()
    )
}

#[test]
fn a_cache_that_cannot_be_opened_fails_with_fa16s_message_and_its_full_path() {
    for kind in [
        ErrorKind::PermissionDenied,
        ErrorKind::IsADirectory,
        ErrorKind::Other,
        ErrorKind::InvalidInput,
        ErrorKind::TimedOut,
    ] {
        let path = cache_path();
        let failure = api::read_cache(&path, Err(Error::from(kind))).unwrap_err();
        assert_eq!(failure.message, fa16_message(&path), "{kind:?}");
        // A cache that cannot be read is "any other failure" to a caller.
        assert_eq!(failure.caller, CallerFailure::CouldNotReach, "{kind:?}");
    }
}

#[test]
fn fa16s_message_names_the_path_it_was_given() {
    let other = std::env::temp_dir()
        .join("another home")
        .join(".agentspace")
        .join("instance-id.json");
    let failure =
        api::read_cache(&other, Err(Error::from(ErrorKind::PermissionDenied))).unwrap_err();
    assert_eq!(failure.message, fa16_message(&other));
}

#[test]
fn the_io_errors_own_text_never_reaches_the_message() {
    let failure = api::read_cache(
        &cache_path(),
        Err(Error::new(
            ErrorKind::PermissionDenied,
            "Secret detail of the system",
        )),
    )
    .unwrap_err();
    assert!(
        !failure.message.contains("Secret detail"),
        "{}",
        failure.message
    );
    assert_eq!(failure.message, fa16_message(&cache_path()));
}

// -- The rebuild ------------------------------------------------------------------------------------

fn stack(outputs: Value) -> Result<Value, AwsError> {
    Ok(json!({"Stacks":[{"StackName":"agentbox","Outputs":outputs}]}))
}

#[test]
fn the_rebuild_asks_for_the_stack_and_reads_the_answer_as_json() {
    let sim = Sim::new(Mode::Person);
    sim.script(
        STACKS,
        vec![stack(json!([{"OutputKey":"InstanceId","OutputValue":ID}]))],
    );
    assert_eq!(
        api::rebuild_cache(&sim.aws),
        Ok((ID.to_string(), cache_json(ID).into_bytes()))
    );
    let calls = sim.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].text(),
        "cloudformation describe-stacks --stack-name agentbox"
    );
    assert_eq!(calls[0].output, Output::Json);
}

#[test]
fn the_rebuild_takes_the_element_whose_key_is_instance_id_wherever_it_is() {
    let outputs = json!([
        {"OutputKey":"VolumeId","OutputValue":"vol-0123456789abcdef0"},
        {"OutputKey":"InstanceIdLookalike","OutputValue":"i-0000000000000000a"},
        {"OutputKey":"InstanceId","OutputValue":ID},
        {"OutputKey":"InstanceId","OutputValue":"i-0000000000000000b"},
    ]);
    let sim = Sim::new(Mode::Person);
    sim.script(STACKS, vec![stack(outputs)]);
    let (id, bytes) = api::rebuild_cache(&sim.aws).unwrap();
    assert_eq!(id, ID);
    assert_eq!(bytes, api::cache_bytes(ID));
}

#[test]
fn the_rebuild_reads_only_the_first_stack() {
    let sim = Sim::new(Mode::Person);
    sim.script(
        STACKS,
        vec![Ok(json!({"Stacks":[
            {"Outputs":[{"OutputKey":"InstanceId","OutputValue":ID}]},
            {"Outputs":[{"OutputKey":"InstanceId","OutputValue":"i-0000000000000000a"}]},
        ]}))],
    );
    assert_eq!(api::rebuild_cache(&sim.aws).unwrap().0, ID);

    let sim = Sim::new(Mode::Person);
    sim.script(
        STACKS,
        vec![Ok(json!({"Stacks":[
            {"Outputs":[]},
            {"Outputs":[{"OutputKey":"InstanceId","OutputValue":ID}]},
        ]}))],
    );
    let failure = api::rebuild_cache(&sim.aws).unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: Stack 'agentbox' has no InstanceId output."
    );
}

#[test]
fn a_stack_with_no_instance_id_output_fails_and_gives_no_cache() {
    let no_output: Vec<(&str, Value)> = vec![
        ("No outputs", json!([])),
        (
            "Another output",
            json!([{"OutputKey":"VolumeId","OutputValue":"vol-0123456789abcdef0"}]),
        ),
        (
            "A key in another case",
            json!([{"OutputKey":"instanceid","OutputValue":ID}]),
        ),
        (
            "A key in another case, again",
            json!([{"OutputKey":"InstanceID","OutputValue":ID}]),
        ),
        (
            "A key with a space",
            json!([{"OutputKey":"InstanceId ","OutputValue":ID}]),
        ),
        ("Outputs that are null", Value::Null),
    ];
    for (name, outputs) in no_output {
        both_modes(|mode| {
            let sim = Sim::new(mode);
            sim.script(STACKS, vec![stack(outputs.clone())]);
            // An `Err` carries no bytes: nothing is written.
            let failure = api::rebuild_cache(&sim.aws).unwrap_err();
            assert_eq!(
                failure.message, "agentspace: Stack 'agentbox' has no InstanceId output.",
                "{name}"
            );
            assert_eq!(failure.caller, CallerFailure::CouldNotReach, "{name}");
            assert_eq!(sim.count(STACKS), 1, "{name}");
        });
    }
}

#[test]
fn a_stack_with_no_outputs_member_at_all_has_no_instance_id_output() {
    let sim = Sim::new(Mode::Person);
    sim.script(
        STACKS,
        vec![Ok(json!({"Stacks":[{"StackName":"agentbox"}]}))],
    );
    let failure = api::rebuild_cache(&sim.aws).unwrap_err();
    assert_eq!(
        failure.message,
        "agentspace: Stack 'agentbox' has no InstanceId output."
    );
}

#[test]
fn an_answer_with_no_stack_fails_without_a_cache_and_without_backend_words_for_a_caller() {
    // A missing `Stacks[0]` or `Outputs` is the "no InstanceId output" failure, and no cache is
    // written (operator.md, Calls by command, the architect's ruling of 2026-11-19 for
    // E0.06-T01-S03). A caller sees "Could not reach the host".
    for answer in [
        Ok(json!({"Stacks":[]})),
        Ok(json!({})),
        Ok(Value::Null),
        Ok(json!({"Stacks":{}})),
        Ok(json!({"Stacks":[null]})),
        Ok(json!({"Stacks":[{"StackName":"agentbox","Outputs":{}}]})),
        Ok(json!({"Stacks":[{"StackName":"agentbox","Outputs":"InstanceId"}]})),
    ] {
        let sim = Sim::new(Mode::Person);
        sim.script(STACKS, vec![answer.clone()]);
        let failure = api::rebuild_cache(&sim.aws).unwrap_err();
        assert_eq!(
            failure.message, "agentspace: Stack 'agentbox' has no InstanceId output.",
            "{answer:?}"
        );
        assert_eq!(failure.caller, CallerFailure::CouldNotReach, "{answer:?}");
    }
}

#[test]
fn an_output_element_with_no_value_gives_no_cache() {
    // An `InstanceId` output whose value is not a string matching the instance-ID cache's schema
    // is the "no InstanceId output" failure, and no cache is written (operator.md, Calls by
    // command, the architect's ruling of 2026-11-19 for E0.06-T01-S03): a cache holding it would
    // count as missing at every later read.
    for element in [
        json!({"OutputKey":"InstanceId"}),
        json!({"OutputKey":"InstanceId","OutputValue":null}),
        json!({"OutputKey":"InstanceId","OutputValue":7}),
        json!({"OutputKey":"InstanceId","OutputValue":[ID]}),
        json!({"OutputKey":"InstanceId","OutputValue":""}),
        json!({"OutputKey":"InstanceId","OutputValue":"None"}),
        json!({"OutputKey":"InstanceId","OutputValue":"i-0123456789ABCDEF0"}),
        json!({"OutputKey":"InstanceId","OutputValue":"i-0123456789abcdef"}),
        json!({"OutputKey":"InstanceId","OutputValue":"i-0123456789abcdef0\n"}),
        json!({"OutputKey":"InstanceId","OutputValue":" i-0123456789abcdef0"}),
        json!({"OutputKey":"InstanceId","OutputValue":"vol-0123456789abcdef0"}),
    ] {
        let sim = Sim::new(Mode::Person);
        sim.script(STACKS, vec![stack(json!([element.clone()]))]);
        let failure = api::rebuild_cache(&sim.aws).unwrap_err();
        assert_eq!(
            failure.message, "agentspace: Stack 'agentbox' has no InstanceId output.",
            "{element}"
        );
        assert_eq!(failure.caller, CallerFailure::CouldNotReach, "{element}");
    }
}

#[test]
fn every_id_the_rebuild_gives_reads_back_from_its_cache() {
    // Both lengths the schema allows: what the rebuild writes is never a cache that counts as
    // missing.
    for id in ["i-0123abcd", ID] {
        let sim = Sim::new(Mode::Person);
        sim.script(
            STACKS,
            vec![stack(json!([{"OutputKey":"InstanceId","OutputValue":id}]))],
        );
        let (given, bytes) = api::rebuild_cache(&sim.aws).unwrap();
        assert_eq!(given, id);
        assert_eq!(read(&bytes), Ok(Some(id.to_string())));
    }
}

#[test]
fn a_failed_describe_stacks_fails_with_the_aws_failure_and_gives_no_cache() {
    for error in [
        AwsError::SignIn,
        AwsError::Failed("Stack with id agentbox does not exist".to_string()),
        AwsError::CannotRun(
            "agentspace: AGENTSPACE_AWS_CMD names x, which cannot be run.".to_string(),
        ),
    ] {
        let sim = Sim::new(Mode::Person);
        sim.script(STACKS, vec![Err(error.clone())]);
        assert_eq!(api::rebuild_cache(&sim.aws), Err(Failure::from(error)));
        assert_eq!(sim.count(STACKS), 1);
    }
}
