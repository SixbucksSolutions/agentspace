//! The instance-ID cache, `%USERPROFILE%\.agentspace\instance-id.json` (operator.md, Laptop
//! files; state.md, Files in the checkout; D21, D40).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! This module gives the cache's bytes and the decisions about them. Reading the file, and
//! replacing it atomically through `instance-id.json.<hex>.tmp`, is the program's, behind
//! [`InstanceIdCache`].

use std::path::Path;

use agentspace_operator_api::{Aws, Call, CallerFailure, Failure, Output};
use serde_json::{Value, json};

/// What reading the cache found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheRead {
    /// No file at the cache's path.
    Missing,
    /// The file's whole content.
    Bytes(Vec<u8>),
    /// A file that cannot be opened, such as one access is denied to, or a folder at its path.
    Unopenable,
}

impl From<std::io::Result<Vec<u8>>> for CacheRead {
    /// What a read of the whole file, such as `std::fs::read`, found: a file that is not found is
    /// missing, and any other error is a file that cannot be opened.
    fn from(read: std::io::Result<Vec<u8>>) -> Self {
        match read {
            Ok(bytes) => CacheRead::Bytes(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => CacheRead::Missing,
            Err(_) => CacheRead::Unopenable,
        }
    }
}

/// The decision on one read of the cache at `path`: `Ok(Some(id))` for a cache that holds an ID,
/// `Ok(None)` for one that counts as missing (not found, or bytes [`parse_cache`] refuses), and
/// FA-16's failure ([`cache_unreadable`]) for one that cannot be opened.
pub fn read_cache(path: &Path, read: impl Into<CacheRead>) -> Result<Option<String>, Failure> {
    match read.into() {
        CacheRead::Missing => Ok(None),
        CacheRead::Bytes(bytes) => Ok(parse_cache(&bytes)),
        CacheRead::Unopenable => Err(cache_unreadable(path)),
    }
}

/// The cache file, as the program reads and writes it.
pub trait InstanceIdCache {
    /// The cache's full path, which FA-16's message names.
    fn path(&self) -> &Path;
    /// Reads the cache.
    fn read(&self) -> CacheRead;
    /// Replaces the cache with `bytes`, atomically (state.md, Replacing a file). A write that
    /// fails gives the person's whole message, and SHOULD name `CallerFailure::CouldNotReach`
    /// (operator.md, What a caller sees: "a cache that cannot be read or written");
    /// [`instance_id`] sets that `caller` whatever the failure named.
    fn write(&self, bytes: &[u8]) -> Result<(), Failure>;
}

/// The instance ID in a cache's bytes, or `None` for every case that MUST be treated as missing:
/// empty, only whitespace, not UTF-8, not JSON, or not matching
/// `schemas/state/instance-id.state.schema.json` (`{"version":0,"instance_id":"<id>"}` and no
/// other member). A UTF-8 byte order mark and the whitespace JSON allows are accepted.
pub fn parse_cache(bytes: &[u8]) -> Option<String> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let value: Value = serde_json::from_slice(bytes).ok()?;
    let object = value.as_object()?;
    if object.len() != 2 {
        return None;
    }
    // The schema's `"const": 0`, which JSON Schema compares as a number, so `0.0` matches too.
    if object.get("version")?.as_f64()? != 0.0 {
        return None;
    }
    let id = object.get("instance_id")?.as_str()?;
    is_instance_id(id).then(|| id.to_string())
}

/// The bytes the cache holding `instance_id` is written as: compact JSON, UTF-8 with no byte
/// order mark and no line ending (state.md, Files in the checkout).
pub fn cache_bytes(instance_id: &str) -> Vec<u8> {
    json!({"version": 0, "instance_id": instance_id})
        .to_string()
        .into_bytes()
}

/// FA-16's failure for a cache that cannot be opened: `agentspace: Cannot read the instance-ID
/// cache <path>; remove it and run the command again.`, which a caller sees as "Could not reach
/// the host".
pub fn cache_unreadable(path: &Path) -> Failure {
    Failure {
        message: format!(
            "agentspace: Cannot read the instance-ID cache {}; remove it and run the command again.",
            path.display()
        ),
        caller: CallerFailure::CouldNotReach,
    }
}

/// The stack's `InstanceId` output: `cloudformation describe-stacks --stack-name <stack>`
/// (JSON), `stack` being a host's stack name, `agentbox` for the box or
/// `agentspace-host-<project>` for any other (operator.md, The stack; D44, D55), and in `Stacks[0].Outputs` the element whose `OutputKey` is `InstanceId`, its
/// `OutputValue` (operator.md, Calls by command).
///
/// No such element fails with `agentspace: Stack '<stack>' has no InstanceId output.` So does a
/// missing `Stacks[0]` or `Outputs`, and an `InstanceId` output whose value is not a string
/// matching the cache's schema (the architect's ruling of 2026-11-19 for E0.06-T01-S03, in Calls
/// by command): no cache is written that every later read would treat as missing. A failed call
/// fails under AWS failures.
pub fn instance_id_from_stack(aws: &dyn Aws, stack: &str) -> Result<String, Failure> {
    let answer = aws.call(&Call {
        service: "cloudformation",
        operation: "describe-stacks",
        args: &["--stack-name", stack],
        output: Output::Json,
    })?;
    answer
        .pointer("/Stacks/0/Outputs")
        .and_then(Value::as_array)
        .and_then(|outputs| {
            outputs
                .iter()
                .find(|o| o.get("OutputKey").and_then(Value::as_str) == Some("InstanceId"))
        })
        .and_then(|o| o.get("OutputValue"))
        .and_then(Value::as_str)
        .filter(|id| is_instance_id(id))
        .map(str::to_string)
        .ok_or_else(|| Failure {
            message: format!("agentspace: Stack '{stack}' has no InstanceId output."),
            caller: CallerFailure::CouldNotReach,
        })
}

/// The instance ID (operator.md, up, step 1): from the cache, or, when it counts as missing, from
/// the output of `stack`, the host's stack, writing the cache then. A cache that cannot be opened
/// fails at once, with FA-16's message and no AWS call. A stack with no `InstanceId` output writes no cache.
///
/// A cache that cannot be written fails the command with the message [`InstanceIdCache::write`]
/// gave, and a caller always sees "Could not reach the host" for it, whatever `caller` that
/// failure named (operator.md, What a caller sees: "a cache that cannot be read or written").
pub fn instance_id(
    aws: &dyn Aws,
    cache: &dyn InstanceIdCache,
    stack: &str,
) -> Result<String, Failure> {
    if let Some(id) = read_cache(cache.path(), cache.read())? {
        return Ok(id);
    }
    let id = instance_id_from_stack(aws, stack)?;
    cache.write(&cache_bytes(&id)).map_err(|failure| Failure {
        caller: CallerFailure::CouldNotReach,
        ..failure
    })?;
    Ok(id)
}

/// Whether `id` matches the schema's `^i-([0-9a-f]{8}|[0-9a-f]{17})$(?!\n)`.
fn is_instance_id(id: &str) -> bool {
    id.strip_prefix("i-").is_some_and(|hex| {
        (hex.len() == 8 || hex.len() == 17)
            && hex.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentspace_operator_api::AwsError;
    use std::cell::RefCell;

    const ID: &str = "i-0123456789abcdef0";

    #[test]
    fn a_good_cache_parses() {
        assert_eq!(
            parse_cache(br#"{"version":0,"instance_id":"i-0123456789abcdef0"}"#),
            Some(ID.to_string())
        );
        assert_eq!(
            parse_cache(
                b"\xEF\xBB\xBF \r\n{ \"instance_id\" : \"i-01234567\",\n\t\"version\": 0 }\r\n"
            ),
            Some("i-01234567".to_string())
        );
        assert_eq!(
            parse_cache(br#"{"version":0.0,"instance_id":"i-0123456789abcdef0"}"#),
            Some(ID.to_string())
        );
    }

    #[test]
    fn every_missing_case_counts_as_missing() {
        for bytes in [
            &b""[..],
            b"   \r\n\t ",
            b"\xEF\xBB\xBF",
            b"\xEF\xBB\xBF  ",
            b"i-0123456789abcdef0",
            b"{",
            b"\xFF\xFE{}",
            b"null",
            b"[]",
            b"\"i-0123456789abcdef0\"",
            br#"{}"#,
            br#"{"version":0}"#,
            br#"{"instance_id":"i-0123456789abcdef0"}"#,
            br#"{"version":1,"instance_id":"i-0123456789abcdef0"}"#,
            br#"{"version":"0","instance_id":"i-0123456789abcdef0"}"#,
            br#"{"version":0,"instance_id":"i-0123456789ABCDEF0"}"#,
            br#"{"version":0,"instance_id":"i-0123456789abcdef"}"#,
            br#"{"version":0,"instance_id":"i-0123456789abcdef0\n"}"#,
            br#"{"version":0,"instance_id":""}"#,
            br#"{"version":0,"instance_id":7}"#,
            br#"{"version":0,"instance_id":"i-0123456789abcdef0","extra":1}"#,
            b"\xEF\xBB\xBF\xEF\xBB\xBF{\"version\":0,\"instance_id\":\"i-0123456789abcdef0\"}",
        ] {
            assert_eq!(
                parse_cache(bytes),
                None,
                "{:?}",
                String::from_utf8_lossy(bytes)
            );
        }
    }

    #[test]
    fn the_cache_is_written_compact_with_no_line_ending() {
        let bytes = cache_bytes(ID);
        assert_eq!(
            bytes,
            br#"{"version":0,"instance_id":"i-0123456789abcdef0"}"#
        );
        assert_eq!(parse_cache(&bytes), Some(ID.to_string()));
    }

    #[test]
    fn an_unreadable_cache_gives_fa16s_message() {
        let failure = cache_unreadable(Path::new("/tmp/home/.agentspace/instance-id.json"));
        assert_eq!(
            failure.message,
            "agentspace: Cannot read the instance-ID cache /tmp/home/.agentspace/instance-id.json; remove it and run the command again."
        );
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
    }

    #[test]
    fn a_read_is_decided() {
        use std::io::{Error, ErrorKind};
        let path = Path::new("/tmp/home/.agentspace/instance-id.json");
        assert_eq!(
            read_cache(path, Ok(cache_bytes(ID))).unwrap(),
            Some(ID.to_string())
        );
        assert_eq!(read_cache(path, Ok(b" ".to_vec())).unwrap(), None);
        assert_eq!(
            read_cache(path, Err(Error::from(ErrorKind::NotFound))).unwrap(),
            None
        );
        for kind in [
            ErrorKind::PermissionDenied,
            ErrorKind::IsADirectory,
            ErrorKind::Other,
        ] {
            assert_eq!(
                read_cache(path, Err(Error::from(kind))).unwrap_err(),
                cache_unreadable(path)
            );
        }
        assert_eq!(read_cache(path, CacheRead::Missing).unwrap(), None);
    }

    struct Stack(Result<Value, AwsError>, RefCell<Vec<String>>);

    impl Aws for Stack {
        fn call(&self, call: &Call) -> Result<Value, AwsError> {
            let mut line = vec![call.service, call.operation];
            line.extend(call.args);
            self.1.borrow_mut().push(line.join(" "));
            assert_eq!(call.output, Output::Json);
            self.0.clone()
        }
    }

    struct File(CacheRead, RefCell<Vec<Vec<u8>>>);

    impl InstanceIdCache for File {
        fn path(&self) -> &Path {
            Path::new("/tmp/home/.agentspace/instance-id.json")
        }
        fn read(&self) -> CacheRead {
            self.0.clone()
        }
        fn write(&self, bytes: &[u8]) -> Result<(), Failure> {
            self.1.borrow_mut().push(bytes.to_vec());
            Ok(())
        }
    }

    fn stack_with(outputs: Value) -> Stack {
        Stack(
            Ok(json!({"Stacks":[{"StackName":"agentbox","Outputs":outputs}]})),
            RefCell::new(Vec::new()),
        )
    }

    #[test]
    fn a_good_cache_makes_no_call() {
        let aws = stack_with(json!([]));
        let file = File(CacheRead::Bytes(cache_bytes(ID)), RefCell::new(Vec::new()));
        assert_eq!(instance_id(&aws, &file, "agentbox").unwrap(), ID);
        assert!(aws.1.borrow().is_empty());
        assert!(file.1.borrow().is_empty());
    }

    #[test]
    fn a_missing_cache_is_rebuilt_from_the_stack() {
        for read in [
            CacheRead::Missing,
            CacheRead::Bytes(Vec::new()),
            CacheRead::Bytes(b"  \n".to_vec()),
            CacheRead::Bytes(b"{nope".to_vec()),
            CacheRead::Bytes(br#"{"version":2}"#.to_vec()),
        ] {
            let aws = stack_with(json!([
                {"OutputKey":"Other","OutputValue":"i-ffffffff"},
                {"OutputKey":"InstanceId","OutputValue":ID}
            ]));
            let file = File(read, RefCell::new(Vec::new()));
            assert_eq!(instance_id(&aws, &file, "agentbox").unwrap(), ID);
            assert_eq!(
                *aws.1.borrow(),
                vec!["cloudformation describe-stacks --stack-name agentbox"]
            );
            assert_eq!(*file.1.borrow(), vec![cache_bytes(ID)]);
        }
    }

    #[test]
    fn an_unopenable_cache_fails_with_no_call() {
        let aws = stack_with(json!([{"OutputKey":"InstanceId","OutputValue":ID}]));
        let file = File(CacheRead::Unopenable, RefCell::new(Vec::new()));
        let failure = instance_id(&aws, &file, "agentbox").unwrap_err();
        assert_eq!(
            failure,
            cache_unreadable(Path::new("/tmp/home/.agentspace/instance-id.json"))
        );
        assert!(aws.1.borrow().is_empty());
        assert!(file.1.borrow().is_empty());
    }

    #[test]
    fn no_instance_id_output_fails_and_writes_no_cache() {
        for answer in [
            json!({"Stacks":[{"Outputs":[]}]}),
            json!({"Stacks":[{"Outputs":[{"OutputKey":"Other","OutputValue":ID}]}]}),
            json!({"Stacks":[{}]}),
            json!({"Stacks":[]}),
            json!({}),
            json!({"Stacks":[{"Outputs":[{"OutputKey":"InstanceId"}]}]}),
            json!({"Stacks":[{"Outputs":[{"OutputKey":"InstanceId","OutputValue":""}]}]}),
            json!({"Stacks":[{"Outputs":[{"OutputKey":"InstanceId","OutputValue":3}]}]}),
        ] {
            let aws = Stack(Ok(answer), RefCell::new(Vec::new()));
            let file = File(CacheRead::Missing, RefCell::new(Vec::new()));
            let failure = instance_id(&aws, &file, "agentbox").unwrap_err();
            assert_eq!(
                failure.message,
                "agentspace: Stack 'agentbox' has no InstanceId output."
            );
            assert_eq!(failure.caller, CallerFailure::CouldNotReach);
            assert!(file.1.borrow().is_empty());
        }
    }

    #[test]
    fn a_failed_describe_stacks_fails_under_aws_failures() {
        let aws = Stack(
            Err(AwsError::Failed(
                "Stack with id agentbox does not exist".into(),
            )),
            RefCell::new(Vec::new()),
        );
        let file = File(CacheRead::Missing, RefCell::new(Vec::new()));
        let failure = instance_id(&aws, &file, "agentbox").unwrap_err();
        assert_eq!(
            failure.message,
            "agentspace: Stack with id agentbox does not exist"
        );
        assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        assert!(file.1.borrow().is_empty());

        let aws = Stack(Err(AwsError::SignIn), RefCell::new(Vec::new()));
        let failure = instance_id(&aws, &file, "agentbox").unwrap_err();
        assert_eq!(failure.caller, CallerFailure::Same);
    }

    #[test]
    fn a_failed_write_fails() {
        struct Denied;
        impl InstanceIdCache for Denied {
            fn path(&self) -> &Path {
                Path::new("x")
            }
            fn read(&self) -> CacheRead {
                CacheRead::Missing
            }
            fn write(&self, _: &[u8]) -> Result<(), Failure> {
                Err(Failure {
                    message: "agentspace: Cannot write x.".into(),
                    caller: CallerFailure::CouldNotReach,
                })
            }
        }
        let aws = stack_with(json!([{"OutputKey":"InstanceId","OutputValue":ID}]));
        assert_eq!(
            instance_id(&aws, &Denied, "agentbox").unwrap_err().message,
            "agentspace: Cannot write x."
        );
    }

    #[test]
    fn a_failed_write_is_could_not_reach_to_a_caller_whatever_the_cache_said() {
        // What a caller sees: "a cache that cannot be read or written" is "Could not reach the
        // host", so a write's failure never reaches a caller as its own text.
        struct Wrong(CallerFailure);
        impl InstanceIdCache for Wrong {
            fn path(&self) -> &Path {
                Path::new("x")
            }
            fn read(&self) -> CacheRead {
                CacheRead::Missing
            }
            fn write(&self, _: &[u8]) -> Result<(), Failure> {
                Err(Failure {
                    message: "agentspace: Cannot write the instance-ID cache x.".into(),
                    caller: self.0,
                })
            }
        }
        for caller in [
            CallerFailure::Same,
            CallerFailure::NotAvailable,
            CallerFailure::NotReady,
            CallerFailure::CouldNotReach,
        ] {
            let aws = stack_with(json!([{"OutputKey":"InstanceId","OutputValue":ID}]));
            let failure = instance_id(&aws, &Wrong(caller), "agentbox").unwrap_err();
            assert_eq!(
                failure.message,
                "agentspace: Cannot write the instance-ID cache x."
            );
            assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        }
    }
}
