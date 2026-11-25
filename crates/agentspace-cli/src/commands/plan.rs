//! The plan of `create --plan <file>` (lease-api.md, Plan and `agentspace lease create`; D74, D75,
//! D81): the file read as a regular file of at most 262144 bytes, its bytes given to
//! agentspace-plan's parse, and every refusal framed as `invalid_value` for `plan`.
//!
//! RFC 2119 keywords; the capitals are load-bearing.
//!
//! - **The argument is used as given:** never trimmed or normalized, made full against the world's
//!   working folder when it is relative, and quoted as given in a refusal (pin P14).
//! - **No `<why>` carries an operating system's error text, or a path other than the argument**
//!   (D81, as changed for v0.10-T01): a file that cannot be read for any reason is `the file could
//!   not be read`.
//! - **No more than 262145 bytes of the file are ever read** (pin P1), and a device or a pipe is
//!   never read at all, so `create` never waits on one (pin P7).

use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use agentspace_plan::{MAX_FILE_BYTES, Plan, WHY_TOO_LARGE, WHY_UNREADABLE};

use crate::quote::quote_os;
use crate::{ErrorCode, LeaseError, World};

/// The refusal of a plan: `Plan '<value>' is not a valid plan: <why>.`, `<value>` the argument as
/// given, quoted by lease-api.md's Messages rules (D81, as changed for v0.10-T01; pins P7, P12).
fn refused(value: &OsStr, why: &str) -> LeaseError {
    LeaseError {
        code: ErrorCode::InvalidValue,
        message: format!("Plan {} is not a valid plan: {why}.", quote_os(value)),
        field: Some("plan".to_owned()),
        lease_id: None,
        state: None,
    }
}

/// Reads and parses the plan that `--plan` names.
pub fn read_plan(value: &OsStr, world: &World<'_>) -> Result<Plan, LeaseError> {
    let path = plan_path(value, world).ok_or_else(|| refused(value, WHY_UNREADABLE))?;
    let bytes = read_bounded(&path).map_err(|why| refused(value, why))?;
    agentspace_plan::parse(&bytes).map_err(|e| refused(value, &e.message))
}

/// The file the argument names, or `None` when no file is to be opened for it: an argument that
/// is not valid Unicode or holds U+FEFF (lease-api.md, Plan), or a relative one when the working
/// folder cannot be read.
fn plan_path(value: &OsStr, world: &World<'_>) -> Option<PathBuf> {
    let text = value.to_str()?;
    if text.contains('\u{feff}') {
        return None;
    }
    let given = Path::new(text);
    if given.is_absolute() {
        return Some(given.to_path_buf());
    }
    // `join` keeps Windows' own reading of a path with a root and no drive, or a drive and no
    // root, as the system would read it from that working folder.
    world.cwd.map(|cwd| cwd.join(given))
}

/// The file's bytes, a byte order mark included, or the `<why>` of check 1 or check 2.
fn read_bounded(path: &Path) -> Result<Vec<u8>, &'static str> {
    let limit = MAX_FILE_BYTES as u64;
    // Check 1, by the size found before the file is opened, then check 2: only a regular file is
    // opened, so a folder, a device or a pipe is never read or waited on.
    let found = std::fs::metadata(path).map_err(|_| WHY_UNREADABLE)?;
    if !found.is_file() {
        return Err(WHY_UNREADABLE);
    }
    if found.len() > limit {
        return Err(WHY_TOO_LARGE);
    }
    let file = File::open(path).map_err(|_| WHY_UNREADABLE)?;
    // What was opened is checked again, since the path can name something else by now.
    let opened = file.metadata().map_err(|_| WHY_UNREADABLE)?;
    if !opened.is_file() || !crate::disk::is_disk_file(&file) {
        return Err(WHY_UNREADABLE);
    }
    if opened.len() > limit {
        return Err(WHY_TOO_LARGE);
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| WHY_UNREADABLE)?;
    // A file that grew past the bound as it was read.
    if bytes.len() > MAX_FILE_BYTES {
        return Err(WHY_TOO_LARGE);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_at<R>(cwd: Option<&Path>, f: impl FnOnce(&World<'_>) -> R) -> R {
        let var = |_: &str| None;
        let now = || 0;
        let new_id = String::new;
        let world = World {
            var: &var,
            cwd,
            own_exe: Path::new(""),
            now: &now,
            new_id: &new_id,
            drivers: &[],
        };
        f(&world)
    }

    const PLAN: &str = r#"{"name":"v0.09","turns":[{"name":"T03","stories":[{"name":"S01","subagents":[{"name":"a","model":"Opus","effort":"high"},{"name":"b","model":"Opus","effort":"high","after":["a"]}]}]}]}"#;

    fn message(dir: &Path, value: &str) -> String {
        world_at(Some(dir), |w| read_plan(OsStr::new(value), w))
            .unwrap_err()
            .message
    }

    #[test]
    fn a_relative_path_is_read_from_the_worlds_working_folder() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("plan.json"), PLAN).unwrap();
        let plan = world_at(Some(dir.path()), |w| read_plan(OsStr::new("plan.json"), w)).unwrap();
        assert_eq!(plan.busiest(), ("T03", 2));
        // With no working folder, a relative path names no file.
        let e = world_at(None, |w| read_plan(OsStr::new("plan.json"), w)).unwrap_err();
        assert_eq!(
            e.message,
            "Plan 'plan.json' is not a valid plan: the file could not be read."
        );
        assert_eq!(e.field.as_deref(), Some("plan"));
        assert_eq!(e.code, ErrorCode::InvalidValue);
    }

    #[test]
    fn a_leading_byte_order_mark_is_the_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut bytes = b"\xef\xbb\xbf".to_vec();
        bytes.extend_from_slice(PLAN.as_bytes());
        std::fs::write(dir.path().join("bom.json"), bytes).unwrap();
        let plan = world_at(Some(dir.path()), |w| read_plan(OsStr::new("bom.json"), w)).unwrap();
        assert_eq!(plan.name(), Some("v0.09"));
    }

    #[test]
    fn a_missing_file_a_folder_and_a_device_could_not_be_read() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("folder")).unwrap();
        for value in ["missing.json", "folder", " ", "-"] {
            assert_eq!(
                message(dir.path(), value),
                format!("Plan '{value}' is not a valid plan: the file could not be read."),
                "{value}"
            );
        }
        #[cfg(windows)]
        for device in ["NUL", "CON", "\\\\.\\NUL"] {
            assert_eq!(
                message(dir.path(), device),
                format!("Plan '{device}' is not a valid plan: the file could not be read."),
                "{device}"
            );
        }
    }

    #[test]
    fn an_argument_with_a_byte_order_mark_opens_no_file() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("\u{feff}plan.json"), PLAN).unwrap();
        assert_eq!(
            message(dir.path(), "\u{feff}plan.json"),
            "Plan '\u{feff}plan.json' is not a valid plan: the file could not be read."
        );
    }

    #[test]
    fn the_size_bound_counts_a_byte_order_mark_and_reads_no_more_than_one_byte_past_it() {
        let dir = tempfile::TempDir::new().unwrap();
        // Exactly the bound, padded with JSON whitespace after the document, is read.
        let mut fits = b"\xef\xbb\xbf".to_vec();
        fits.extend_from_slice(PLAN.as_bytes());
        fits.resize(MAX_FILE_BYTES, b' ');
        std::fs::write(dir.path().join("fits.json"), &fits).unwrap();
        assert!(world_at(Some(dir.path()), |w| read_plan(OsStr::new("fits.json"), w)).is_ok());
        let mut over = fits.clone();
        over.push(b' ');
        std::fs::write(dir.path().join("over.json"), &over).unwrap();
        assert_eq!(
            message(dir.path(), "over.json"),
            "Plan 'over.json' is not a valid plan: the file is larger than 262144 bytes."
        );
        assert!(
            read_bounded(&dir.path().join("fits.json")) == Ok(fits),
            "A file of exactly the bound is read whole"
        );
    }

    #[test]
    fn a_plan_parse_refuses_is_framed_with_the_argument_as_given() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("bad.json"), "{").unwrap();
        let full = dir.path().join("bad.json");
        let full = full.to_str().unwrap();
        assert_eq!(
            message(dir.path(), "bad.json"),
            "Plan 'bad.json' is not a valid plan: the file is not valid JSON."
        );
        // An absolute argument is quoted as given, and nothing else of the path is added. The
        // comparison prints neither side, since the path is the temp folder's.
        let got = message(Path::new("/"), full);
        assert!(
            got == format!("Plan '{full}' is not a valid plan: the file is not valid JSON."),
            "The absolute argument's message is not the template's"
        );
    }

    #[test]
    fn a_control_character_in_the_argument_is_escaped_in_the_message() {
        let dir = tempfile::TempDir::new().unwrap();
        assert_eq!(
            message(dir.path(), "a\nb"),
            "Plan 'a\\u000ab' is not a valid plan: the file could not be read."
        );
    }
}
