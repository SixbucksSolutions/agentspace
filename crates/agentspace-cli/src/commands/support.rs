//! What the five commands share: values from the command line, the folders, the records, the
//! driver and its context, and the messages of lease-api.md (Exit codes and errors, Messages).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use agentspace_driver_api::{Context, Driver, Failure};
use agentspace_state::{OutputKind, Record, StoreError, validate_output};
use agentspace_values::{ValueCode, ValueError};
use serde_json::Value;

use crate::osx::{Piece, pieces};
use crate::quote::escape_controls;
use crate::{ErrorCode, LeaseError, ParsedCommand, Sub, World};

/// The name a stored `ready` or `released` record without `driver` is read as, in contract v0
/// (docs/state.md, The lease state file; driver.md, The driver's name).
pub const DEFAULT_DRIVER: &str = "ec2";

/// The first value of a flag, by its lowercase name.
pub fn flag<'c>(cmd: &'c ParsedCommand, name: &str) -> Option<&'c OsStr> {
    cmd.flags
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_os_str())
}

/// Every value of a flag, in line order.
pub fn flags<'c>(cmd: &'c ParsedCommand, name: &str) -> Vec<&'c OsStr> {
    cmd.flags
        .iter()
        .filter(|(n, _)| n == name)
        .map(|(_, v)| v.as_os_str())
        .collect()
}

/// A refused value as its error document.
pub fn value_error(e: ValueError) -> LeaseError {
    LeaseError {
        code: match e.code {
            ValueCode::InvalidValue => ErrorCode::InvalidValue,
            ValueCode::OutOfRange => ErrorCode::OutOfRange,
        },
        message: e.message,
        field: Some(e.field.to_owned()),
        lease_id: None,
        state: None,
    }
}

/// The first of the blocks of private-use characters (planes 15 and 16) that no character of the
/// values uses, to stand in for their unpaired surrogates while the values crate checks them.
fn free_block(values: &[Vec<Piece>]) -> Option<u32> {
    (0xF_0000..=0x10_F800).step_by(0x800).find(|&base| {
        !values.iter().flatten().any(|p| match p {
            Piece::Char(c) => (base..base + 0x800).contains(&u32::from(*c)),
            Piece::Bad(_) => false,
        })
    })
}

/// Checks values from the command line with a check of the values crate, which reads text.
///
/// A value that is not valid Unicode is read as written, never with a replacement character, and
/// refused with `invalid_value` for its field; where the message quotes it, each unpaired surrogate
/// is written as `\u` and four lowercase hex digits (lease-api.md, Input tolerance, gap FA-09). So
/// the check, its order and its message template stay the values crate's: each surrogate is lent
/// a private-use character that holds no meaning in any value (none is a letter, a digit, a
/// separator or whitespace in its rules), and the message is given the escape back.
pub fn check_values<T>(
    values: &[&OsStr],
    field: &'static str,
    check: impl Fn(&[&str]) -> Result<T, ValueError>,
) -> Result<T, LeaseError> {
    if let Some(texts) = values
        .iter()
        .map(|v| v.to_str())
        .collect::<Option<Vec<_>>>()
    {
        return check(&texts).map_err(value_error);
    }
    let all: Vec<Vec<Piece>> = values.iter().map(|v| pieces(v)).collect();
    // With no block free, nothing can stand in for a surrogate without a replacement character,
    // which a value is never converted with (lease-api.md, Input tolerance), so the value is
    // refused with the fallback message.
    let Some(base) = free_block(&all) else {
        return Err(not_text(values, field));
    };
    let lent: Option<Vec<String>> = all
        .iter()
        .map(|ps| {
            ps.iter()
                .map(|p| match *p {
                    Piece::Char(c) => Some(c),
                    Piece::Bad(code) => char::from_u32(base + u32::from(code) - 0xD800),
                })
                .collect()
        })
        .collect();
    let Some(lent) = lent else {
        return Err(not_text(values, field));
    };
    let texts: Vec<&str> = lent.iter().map(String::as_str).collect();
    match check(&texts) {
        Err(mut e) => {
            e.message = give_back(&e.message, base);
            Err(value_error(e))
        }
        // No value with a character outside ASCII and the whitespace list is accepted, so this is
        // never reached; it is refused all the same.
        Ok(_) => Err(not_text(values, field)),
    }
}

/// The fallback refusal of values that are not valid Unicode, quoting the first with
/// `quote_os`'s escapes.
fn not_text(values: &[&OsStr], field: &'static str) -> LeaseError {
    LeaseError {
        code: ErrorCode::InvalidValue,
        message: format!(
            "The value {} for {field} is not valid text.",
            crate::quote::quote_os(values.first().copied().unwrap_or_default())
        ),
        field: Some(field.to_owned()),
        lease_id: None,
        state: None,
    }
}

/// Writes each lent private-use character of a message back as the escape of its surrogate.
fn give_back(message: &str, base: u32) -> String {
    let mut out = String::with_capacity(message.len());
    for c in message.chars() {
        let code = u32::from(c);
        if (base..base + 0x800).contains(&code) {
            out.push_str(&format!("\\u{:04x}", code - base + 0xD800));
        } else {
            out.push(c);
        }
    }
    out
}

/// One value from the command line, checked.
pub fn check_value<T>(
    value: &OsStr,
    field: &'static str,
    check: impl Fn(&str) -> Result<T, ValueError>,
) -> Result<T, LeaseError> {
    check_values(&[value], field, |texts| {
        check(texts.first().copied().unwrap_or(""))
    })
}

/// The lease ID of `get`, `renew` or `release`, normalized. The parser has already refused a line
/// without one.
pub fn lease_id(cmd: &ParsedCommand) -> Result<String, LeaseError> {
    let empty = OsString::new();
    let id = cmd.id.as_ref().unwrap_or(&empty);
    check_value(id, "id", agentspace_values::lease_id)
}

/// `agentspace lease <sub> failed: <what failed>` (lease-api.md, Messages, "Anything else").
pub fn failed(sub: Sub, what: &str) -> LeaseError {
    LeaseError::internal(format!("agentspace lease {} failed: {what}", sub.name()))
}

/// The no-home failure (lease-api.md, Messages, "No home folder").
pub fn no_home(sub: Sub) -> LeaseError {
    failed(
        sub,
        "The home folder is not set (USERPROFILE is unset or empty), so agentspace cannot find \
         its files. Nothing was changed.",
    )
}

/// A failure of the lease records (lease-api.md, Messages: "Records busy", "Records cannot be
/// locked", "Records unreadable" and "Records unwritable").
pub fn store_error(e: StoreError) -> LeaseError {
    LeaseError::internal(match e {
        StoreError::Busy => "The lease records are busy: another agentspace lease command held \
                             them for 30 seconds. Try again."
            .to_owned(),
        StoreError::CannotLock(why) => {
            format!("The lease records could not be locked: {why}. Nothing was changed.")
        }
        StoreError::Unreadable(why) | StoreError::NotValid(why) => {
            format!("The lease records could not be read: {why}. Nothing was changed.")
        }
        StoreError::Unwritable(why) => {
            format!("The lease records could not be written: {why}. Nothing was changed.")
        }
    })
}

/// The `<why>` of a record write that failed, for a message the command composes itself.
pub fn unwritable_why(e: &StoreError) -> &str {
    match e {
        StoreError::Unwritable(why) | StoreError::CannotLock(why) => why,
        StoreError::Unreadable(why) | StoreError::NotValid(why) => why,
        StoreError::Busy => "the lease records are busy",
    }
}

/// A driver's failure: `agentspace lease <sub> failed: ` and the driver's message as it is, with
/// its control characters escaped and nothing more (driver.md, Failure). The message already names
/// what the driver could not remove, so nothing is appended (architect, 2026-11-19).
pub fn driver_failure(sub: Sub, f: &Failure) -> LeaseError {
    if f.message.is_empty() {
        return LeaseError::internal(format!("agentspace lease {} failed.", sub.name()));
    }
    failed(sub, &escape_controls(&f.message))
}

/// The record's lease ID was not found (lease-api.md, Messages, `not_found`).
pub fn not_found(id: &str) -> LeaseError {
    LeaseError {
        code: ErrorCode::NotFound,
        message: format!("No lease has ID '{id}'."),
        field: None,
        lease_id: Some(id.to_owned()),
        state: None,
    }
}

/// The folders of a run. Each is full, unless the working folder cannot be read and the folder's
/// variable is relative, which fails only the command that needs that folder, at the first point
/// it needs it (lease-api.md, Messages; Exit codes and errors, gap FB-22).
pub struct Paths {
    resolved: agentspace_state::Paths,
    /// Whether the world's working folder could be read.
    cwd_known: bool,
}

impl Paths {
    /// A folder as a command uses it: `None` when its variable (or the home under it) is unset,
    /// and the working-folder failure when it is relative and there is no working folder to make
    /// it full against.
    fn usable<'p>(
        &self,
        folder: &'p Option<PathBuf>,
        sub: Sub,
    ) -> Result<Option<&'p Path>, LeaseError> {
        match folder.as_deref() {
            Some(p) if !self.cwd_known && !p.is_absolute() => Err(no_cwd(sub)),
            other => Ok(other),
        }
    }
}

/// The working-folder failure (lease-api.md, Messages, architect, 2026-11-19).
pub fn no_cwd(sub: Sub) -> LeaseError {
    failed(
        sub,
        "The working folder cannot be read, so a relative folder in the environment cannot be \
         made a full path. Nothing was changed.",
    )
}

/// The folders, from `USERPROFILE`, `AGENTSPACE_STATE_DIR` and `AGENTSPACE_SSH_DIR` through the
/// world's `var`, made full against its working folder. With no working folder, a relative one is
/// left relative here and refused where it is used.
pub fn paths(world: &World<'_>) -> Result<Paths, LeaseError> {
    let resolved = agentspace_state::resolve_paths(world.var, world.cwd.unwrap_or(Path::new("")))
        .map_err(store_error)?;
    Ok(Paths {
        resolved,
        cwd_known: world.cwd.is_some(),
    })
}

/// The state folder, or the no-home failure when it is the default one under a home that is not
/// set (lease-api.md, Exit codes and errors, step 3, gap FB-22), or the working-folder failure.
pub fn state_dir(paths: &Paths, sub: Sub) -> Result<&Path, LeaseError> {
    paths
        .usable(&paths.resolved.state_dir, sub)?
        .ok_or_else(|| no_home(sub))
}

/// A variable as text: unset or empty is `None`.
fn text_var(world: &World<'_>, name: &str) -> Option<Result<String, OsString>> {
    (world.var)(name)
        .filter(|v| !v.is_empty())
        .map(OsString::into_string)
}

/// What the context carries besides the folders, for each command (lease-api.md, Environment).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// `create`: the resolved ssh folder and `AGENTSPACE_PROXY_COMMAND`.
    Place,
    /// `renew`: it reads neither `AGENTSPACE_SSH_DIR` nor `AGENTSPACE_PROXY_COMMAND`
    /// (lease-api.md, Environment; driver.md, Renew a lease, "The CLI passes neither"), so the
    /// ssh folder is the home's own and no override is passed.
    Renew,
    /// `release`: the resolved ssh folder, and no override.
    Release,
}

/// The driver's context. It needs the home folder, so this is the first point a command with
/// `AGENTSPACE_STATE_DIR` set needs one, and fails with no home there ("A command fails with no
/// home only when it needs a path under the home folder, and at the first point it needs one",
/// lease-api.md, Exit codes and errors, gap FB-22).
pub fn context(
    world: &World<'_>,
    paths: &Paths,
    sub: Sub,
    purpose: Purpose,
) -> Result<Context, LeaseError> {
    let home: PathBuf = paths
        .usable(&paths.resolved.home, sub)?
        .ok_or_else(|| no_home(sub))?
        .to_path_buf();
    let ssh_dir = match purpose {
        Purpose::Renew => home.join(".ssh"),
        Purpose::Place | Purpose::Release => paths
            .usable(&paths.resolved.ssh_dir, sub)?
            .map_or_else(|| home.join(".ssh"), Path::to_path_buf),
    };
    let proxy_override = match purpose {
        Purpose::Place => match text_var(world, "AGENTSPACE_PROXY_COMMAND") {
            None => None,
            Some(Ok(text)) => Some(text),
            // lease-api.md, Messages, "AGENTSPACE_PROXY_COMMAND not valid text" (architect,
            // 2026-11-19), before any driver is asked.
            Some(Err(_)) => {
                return Err(failed(
                    sub,
                    "AGENTSPACE_PROXY_COMMAND is not valid text. Nothing was recorded.",
                ));
            }
        },
        Purpose::Renew | Purpose::Release => None,
    };
    // A `SystemRoot` that is not valid text is read as unset (lease-api.md, Messages, architect,
    // 2026-11-19).
    let system_root = text_var(world, "SystemRoot").and_then(Result::ok);
    // Every command that asks for a context has already resolved the state folder to read the
    // lease state file, so this adds no failure (architect, 2026-11-20, the pin before v0.07-T03).
    let state_dir = state_dir(paths, sub)?.to_path_buf();
    Ok(Context {
        home,
        ssh_dir,
        proxy_override,
        system_root,
        own_exe: world.own_exe.to_path_buf(),
        state_dir,
        // E0.07-T03-S01 resolves it from AGENTSPACE_PROVISION_CMD or the CLI's own folder.
        operator_program: None,
    })
}

/// The driver a stored record names, found among the world's drivers by `Driver::name()`. A
/// stored `ready` or `released` record with no `driver` is the default driver's. A name the world
/// does not have fails without naming it (lease-api.md, Messages, "Unknown stored driver").
pub fn stored_driver<'w>(
    world: &'w World<'_>,
    record: &Record,
    sub: Sub,
) -> Result<&'w dyn Driver, LeaseError> {
    let name = record.driver.as_deref().unwrap_or(DEFAULT_DRIVER);
    world
        .drivers
        .iter()
        .find(|d| d.name() == name)
        .map(|d| d.as_ref())
        .ok_or_else(|| {
            failed(
                sub,
                "The lease was placed by a driver this agentspace does not have. Nothing was \
                 changed.",
            )
        })
}

/// A document checked against its output schema before it is printed (lease-api.md, Output). A
/// document that fails is `internal`.
pub fn checked(doc: Value, kind: OutputKind, sub: Sub) -> Result<Value, LeaseError> {
    if validate_output(kind, &doc) {
        Ok(doc)
    } else {
        Err(not_valid_document(sub))
    }
}

/// The failure of a document that does not validate, before any change.
pub fn not_valid_document(sub: Sub) -> LeaseError {
    failed(
        sub,
        "The lease's document is not valid. Nothing was changed.",
    )
}

/// Whether a document validates against the lease output schema.
pub fn lease_document_is_valid(doc: &Value) -> bool {
    validate_output(OutputKind::Lease, doc)
}

/// A lease's host alias: `agentspace-<project>-<first 8 hex digits of id>` (lease-api.md, Fields).
pub fn alias_of(project: &str, id: &str) -> String {
    format!("agentspace-{project}-{}", id.get(..8).unwrap_or(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn with_surrogate(before: &str, after: &str) -> OsString {
        use std::os::windows::ffi::OsStringExt;
        let mut wide: Vec<u16> = before.encode_utf16().collect();
        wide.push(0xD800);
        wide.extend(after.encode_utf16());
        OsString::from_wide(&wide)
    }

    #[cfg(unix)]
    fn with_surrogate(before: &str, after: &str) -> OsString {
        use std::os::unix::ffi::OsStringExt;
        let mut bytes = before.as_bytes().to_vec();
        bytes.push(0xff);
        bytes.extend(after.as_bytes());
        OsString::from_vec(bytes)
    }

    #[cfg(windows)]
    const ESCAPED: &str = "\\ud800";
    #[cfg(unix)]
    const ESCAPED: &str = "\\udcff";

    #[test]
    fn a_value_that_is_not_unicode_is_invalid_and_quoted_with_its_escape() {
        let v = with_surrogate("ab", "\n");
        let e = check_value(&v, "project", agentspace_values::project).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidValue);
        assert_eq!(e.field.as_deref(), Some("project"));
        assert!(e.message.starts_with(&format!(
            "Project 'ab{ESCAPED}\\u000a' is not a project name."
        )));
    }

    #[test]
    fn a_ttl_that_is_not_unicode_keeps_the_values_crates_template() {
        let v = with_surrogate("8M", "");
        let e = check_value(&v, "ttl", agentspace_values::ttl).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidValue);
        assert_eq!(
            e.message,
            format!(
                "TTL '8M{ESCAPED}' uses 'M', which could mean months or minutes. Write minutes as \
                 'm' or 'min'."
            )
        );
        let v = with_surrogate("8", "h");
        let e = check_value(&v, "ttl", agentspace_values::ttl).unwrap_err();
        assert!(e.message.contains("is not a duration"));
    }

    #[test]
    fn a_state_that_is_not_unicode_is_the_name_refused() {
        let v = with_surrogate(" x", " ");
        let ready = OsString::from("ready");
        let e =
            check_values(&[ready.as_os_str(), &v], "state", agentspace_values::states).unwrap_err();
        assert_eq!(
            e.message,
            format!(
                "State 'x{ESCAPED}' is not a state. Use pending, ready, expired, released or \
                 unsatisfiable."
            )
        );
    }

    #[test]
    fn a_lent_character_is_never_one_the_value_already_holds() {
        let held = format!("{}", char::from_u32(0xF_0001).unwrap());
        let v = with_surrogate(&held, "");
        let e = check_value(&v, "size", agentspace_values::size).unwrap_err();
        assert_eq!(
            e.message,
            format!(
                "Size '{held}{ESCAPED}' is not a size. Use 1 to 8 letters and digits, as in 'm'."
            )
        );
    }

    #[test]
    fn with_no_private_use_block_free_the_value_is_refused_without_a_replacement_character() {
        // One character from each of the 64 blocks, so none is free to lend a surrogate.
        let every: String = (0xF_0000..=0x10_F800_u32)
            .step_by(0x800)
            .map(|b| char::from_u32(b + 1).unwrap())
            .collect();
        let v = with_surrogate(&every, "");
        let e = check_value(&v, "size", agentspace_values::size).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidValue);
        assert_eq!(e.field.as_deref(), Some("size"));
        assert!(!e.message.contains('\u{FFFD}'), "{}", e.message);
        assert_eq!(
            e.message,
            format!(
                "The value {} for size is not valid text.",
                crate::quote::quote_os(&v)
            )
        );
        assert!(e.message.contains(ESCAPED), "{}", e.message);
    }

    #[test]
    fn the_alias_is_built_from_the_project_and_the_id() {
        assert_eq!(
            alias_of("swimcoach", "3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f"),
            "agentspace-swimcoach-3f2a9c1e"
        );
    }

    #[test]
    fn a_driver_failure_is_prefixed_and_escaped_and_nothing_is_appended() {
        let f = Failure {
            message: "The credential could not be issued.\nTry again.".to_owned(),
            leftovers: vec!["~/.agentspace/leases/x/id_ed25519".to_owned()],
        };
        assert_eq!(
            driver_failure(Sub::Create, &f).message,
            "agentspace lease create failed: The credential could not be issued.\\u000aTry again."
        );
    }
}
