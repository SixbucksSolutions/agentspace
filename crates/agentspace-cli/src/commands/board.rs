//! The values and the board read of `create --board <id> --epic <ticket>` and `--turn <name>`
//! (lease-api.md, Board, Epic, Turn, The board read, The plan block and Messages; D83 to D86, D89).
//!
//! RFC 2119 keywords; the capitals are load-bearing.
//!
//! - **The board read's [`OpContext`] MUST be built from the command's own environment, through
//!   the world's `var`, `LOCALAPPDATA` included**, never from `std::env` ([`board_context`]).
//! - **Every outcome of the read is framed here, word for word as lease-api.md gives it**: an
//!   `internal` as `agentspace lease create failed: <text>. Nothing was recorded.`, and each
//!   refusal of the board or the epic as `invalid_value` for `board` or `epic`.
//! - **No message carries the board's answer, the description, the token or a path**: only the
//!   `--board` and `--epic` values as given, the normalized board ID, and a `<why>` of
//!   agentspace-board's or agentspace-plan's.

use std::ffi::OsStr;
use std::path::PathBuf;

use agentspace_board::{BoardError, Epic, plan_block, read_epic};
use agentspace_operator_api::Context as OpContext;
use agentspace_plan::Plan;
use agentspace_push::{Files, Unprotect};
use agentspace_state::PlanSource;
use agentspace_values::{ValueCode, ValueError, quote};
use serde_json::Value;

use super::support::failed;
use crate::quote::quote_os;
use crate::{ErrorCode, LeaseError, Sub, World};

/// The largest ticket, localswim's ticket range's top (lease-api.md, Epic).
const MAX_TICKET: u64 = 9_007_199_254_740_991;

/// The board read's `internal` for a receipt that cannot be read (lease-api.md, The board read,
/// "Each outcome"), which a relative `LOCALAPPDATA` with no working folder gives too (pin P1).
const BAD_RECEIPT: &str = "localswim's service receipt could not be read";

/// How `create` reads an epic: the real board read in the program, a stand-in in a unit test.
pub(crate) trait EpicReader {
    fn read(&self, ctx: &OpContext, board: &str, ticket: u64) -> Result<Epic, BoardError>;
}

/// The board read through agentspace-board's [`read_epic`], with the files and decryption given.
pub(crate) struct Localswim<'a> {
    pub files: &'a dyn Files,
    pub unprotect: &'a dyn Unprotect,
}

impl EpicReader for Localswim<'_> {
    fn read(&self, ctx: &OpContext, board: &str, ticket: u64) -> Result<Epic, BoardError> {
        read_epic(ctx, self.files, self.unprotect, board, ticket)
    }
}

/// Whether a character has the Unicode `White_Space` property, as lease-api.md's Input tolerance
/// lists it character by character. U+FEFF is not whitespace.
fn is_trim_space(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'..='\u{000D}'
            | '\u{0020}'
            | '\u{0085}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
    )
}

fn invalid(field: &'static str, message: String) -> ValueError {
    ValueError {
        code: ValueCode::InvalidValue,
        field,
        message,
    }
}

/// A board ID: a UUIDv4 in any case, with surrounding whitespace, as a lease ID is. Normal form:
/// lowercase, trimmed (lease-api.md, Board; pin P18).
pub(super) fn board(v: &str) -> Result<String, ValueError> {
    agentspace_values::lease_id(v).map_err(|_| {
        invalid(
            "board",
            format!(
                "Board {} is not a board ID. Give the board's UUID, as in '0a1b2c3d-...'.",
                quote(v)
            ),
        )
    })
}

/// An epic's ticket: 1 to 16 ASCII decimal digits, leading zeros allowed, with surrounding
/// whitespace, whose value is from 1 to 9007199254740991. Normal form: the value (lease-api.md,
/// Epic; pin P5).
pub(super) fn epic(v: &str) -> Result<u64, ValueError> {
    let t = v.trim_matches(is_trim_space);
    let digits = !t.is_empty() && t.len() <= 16 && t.bytes().all(|b| b.is_ascii_digit());
    digits
        .then(|| t.parse::<u64>().ok())
        .flatten()
        .filter(|n| (1..=MAX_TICKET).contains(n))
        .ok_or_else(|| {
            invalid(
                "epic",
                format!(
                    "Epic {} is not a ticket number. Give the epic's ticket as digits, as in '52'.",
                    quote(v)
                ),
            )
        })
}

/// A turn's name: never trimmed, `^[!-~]([ -~]{0,62}[!-~])?$`, used as given (lease-api.md, Turn;
/// pin P5).
pub(super) fn turn(v: &str) -> Result<String, ValueError> {
    let b = v.as_bytes();
    let ok = (1..=64).contains(&b.len())
        && b.iter().all(|c| (b' '..=b'~').contains(c))
        && b.first() != Some(&b' ')
        && b.last() != Some(&b' ');
    if ok {
        Ok(v.to_owned())
    } else {
        Err(invalid(
            "turn",
            format!(
                "Turn {} is not a turn name. Give the plan's turn name, as in 'T01'.",
                quote(v)
            ),
        ))
    }
}

/// A well-formed `--turn` that names no turn of the plan: its message names every turn, in the
/// plan's order, each quoted raw (lease-api.md, Messages; pin P5).
pub(super) fn not_a_turn(plan: &Plan, value: &str) -> LeaseError {
    let turns: Vec<String> = plan
        .document()
        .get("turns")
        .and_then(Value::as_array)
        .map(|turns| {
            turns
                .iter()
                .filter_map(|t| t.get("name").and_then(Value::as_str))
                .map(|name| format!("'{name}'"))
                .collect()
        })
        .unwrap_or_default();
    LeaseError {
        code: ErrorCode::InvalidValue,
        message: format!(
            "Turn {} is not a turn of the plan; its turns are {}.",
            quote(value),
            turns.join(", ")
        ),
        field: Some("turn".to_owned()),
        lease_id: None,
        state: None,
    }
}

/// The board read's `internal`: `agentspace lease create failed: <text>. Nothing was recorded.`
/// (lease-api.md, The board read, "Each outcome"; pin P3).
fn board_internal(text: &str) -> LeaseError {
    failed(Sub::Create, &format!("{text}. Nothing was recorded."))
}

/// Whether a variable's value is empty or only whitespace, as `LOCALAPPDATA` is read. A value that
/// is not valid Unicode is not only whitespace.
fn is_blank(v: &OsStr) -> bool {
    v.to_string_lossy().chars().all(is_trim_space)
}

/// The board read's context, from the command's own environment (pin P1): `LOCALAPPDATA` is
/// `None` when it is unset, empty or only whitespace, decided before any join; a relative value is
/// made full against the world's working folder, and with none, the read fails as a receipt that
/// cannot be read. `home` is `USERPROFILE` and `system_root` is `SystemRoot`, each empty when
/// unset; `nested` is false.
pub(super) fn board_context(world: &World<'_>) -> Result<OpContext, LeaseError> {
    let local_app_data = match (world.var)("LOCALAPPDATA") {
        None => None,
        Some(v) if is_blank(&v) => None,
        Some(v) => {
            let given = PathBuf::from(v);
            if given.is_absolute() {
                Some(given)
            } else {
                let cwd = world.cwd.ok_or_else(|| board_internal(BAD_RECEIPT))?;
                Some(cwd.join(given))
            }
        }
    };
    let path_of = |name: &str| (world.var)(name).map(PathBuf::from).unwrap_or_default();
    Ok(OpContext {
        home: path_of("USERPROFILE"),
        local_app_data,
        system_root: path_of("SystemRoot"),
        nested: false,
    })
}

/// A refusal of the epic: `invalid_value` for `epic`.
fn epic_refused(message: String) -> LeaseError {
    LeaseError {
        code: ErrorCode::InvalidValue,
        message,
        field: Some("epic".to_owned()),
        lease_id: None,
        state: None,
    }
}

/// `Epic '<value>' carries no usable plan: <why>.` (lease-api.md, The plan block; pin P11).
fn no_usable_plan(epic_value: &OsStr, why: &str) -> LeaseError {
    epic_refused(format!(
        "Epic {} carries no usable plan: {why}.",
        quote_os(epic_value)
    ))
}

/// The values of a board read: each as given, for a message, and normalized, for the read.
pub(super) struct BoardRead<'v> {
    pub board_value: &'v OsStr,
    pub board: String,
    pub epic_value: &'v OsStr,
    pub ticket: u64,
}

/// Reads the epic and takes its plan (lease-api.md, The board read and The plan block): every
/// outcome of lease-api.md's "Each outcome" table, the first that applies, then the block's
/// faults, then the plan's checks, each framed as its row says. The plan's source is the
/// normalized board, the ticket and the card's revision as read (What is recorded; pin P7).
pub(super) fn plan_from_epic(
    reader: &dyn EpicReader,
    world: &World<'_>,
    read: &BoardRead<'_>,
) -> Result<(Plan, PlanSource), LeaseError> {
    let ctx = board_context(world)?;
    let epic = reader
        .read(&ctx, &read.board, read.ticket)
        .map_err(|e| match e {
            BoardError::Internal(text) => board_internal(&text),
            BoardError::UnknownBoard => LeaseError {
                code: ErrorCode::InvalidValue,
                message: format!(
                    "No board of ID {} that agentspace can read.",
                    quote_os(read.board_value)
                ),
                field: Some("board".to_owned()),
                lease_id: None,
                state: None,
            },
            BoardError::UnknownTicket => epic_refused(format!(
                "No card of ticket {} on board '{}'. Give the epic's ticket number, as in '52'.",
                quote_os(read.epic_value),
                read.board
            )),
            BoardError::TooLarge => no_usable_plan(
                read.epic_value,
                "the board's answer for this card is larger than 64 MiB",
            ),
        })?;
    if epic.archived {
        return Err(epic_refused(format!(
            "Epic {} is archived. Name an epic that is being worked.",
            quote_os(read.epic_value)
        )));
    }
    let content =
        plan_block(&epic.description).map_err(|e| no_usable_plan(read.epic_value, &e.message))?;
    let plan = agentspace_plan::parse(content.as_bytes())
        .map_err(|e| no_usable_plan(read.epic_value, &e.message))?;
    Ok((
        plan,
        PlanSource {
            board: read.board.clone(),
            ticket: read.ticket,
            revision: epic.revision,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::Path;

    fn world_with<R>(
        vars: &[(&str, OsString)],
        cwd: Option<&Path>,
        f: impl FnOnce(&World<'_>) -> R,
    ) -> R {
        let vars: Vec<(String, OsString)> = vars
            .iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect();
        let var = move |name: &str| vars.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone());
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

    /// A full path on this system, which names no real folder.
    fn full(name: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!("Z:\\{name}"))
        } else {
            PathBuf::from(format!("/{name}"))
        }
    }

    fn context_for(
        local_app_data: Option<&str>,
        cwd: Option<&Path>,
    ) -> Result<OpContext, LeaseError> {
        let mut vars = vec![
            ("USERPROFILE", full("home").into_os_string()),
            ("SystemRoot", full("windows").into_os_string()),
        ];
        if let Some(v) = local_app_data {
            vars.push(("LOCALAPPDATA", OsString::from(v)));
        }
        world_with(&vars, cwd, board_context)
    }

    #[test]
    fn localappdata_unset_empty_or_only_whitespace_is_none_before_any_join() {
        let cwd = full("work");
        for value in [None, Some(""), Some(" "), Some("\t\u{3000}\n")] {
            let ctx = context_for(value, Some(&cwd)).unwrap();
            assert_eq!(ctx.local_app_data, None, "{value:?}");
        }
        // Whitespace is decided before a join, so with no working folder it is still none, and
        // never the receipt-unreadable failure.
        let ctx = context_for(Some("  "), None).unwrap();
        assert_eq!(ctx.local_app_data, None);
    }

    #[test]
    fn a_relative_localappdata_is_made_full_against_the_worlds_working_folder() {
        let cwd = full("work");
        let ctx = context_for(Some("data"), Some(&cwd)).unwrap();
        assert_eq!(ctx.local_app_data, Some(cwd.join("data")));
        // Surrounding whitespace is not trimmed from a folder that is not only whitespace.
        let ctx = context_for(Some(" data"), Some(&cwd)).unwrap();
        assert_eq!(ctx.local_app_data, Some(cwd.join(" data")));
    }

    #[test]
    fn a_relative_localappdata_with_no_working_folder_is_the_receipt_unreadable_internal() {
        let e = context_for(Some("data"), None).unwrap_err();
        assert_eq!(e.code, ErrorCode::Internal);
        assert_eq!(e.field, None);
        assert_eq!(
            e.message,
            "agentspace lease create failed: localswim's service receipt could not be read. \
             Nothing was recorded."
        );
    }

    #[test]
    fn an_absolute_localappdata_is_used_as_given_with_or_without_a_working_folder() {
        let abs = full("appdata");
        let text = abs.to_str().unwrap();
        for cwd in [None, Some(full("work"))] {
            let ctx = context_for(Some(text), cwd.as_deref()).unwrap();
            assert_eq!(ctx.local_app_data, Some(abs.clone()));
        }
    }

    #[test]
    fn home_and_system_root_come_from_the_world_and_are_empty_when_unset() {
        let ctx = context_for(None, None).unwrap();
        assert_eq!(ctx.home, full("home"));
        assert_eq!(ctx.system_root, full("windows"));
        assert!(!ctx.nested);
        let ctx = world_with(&[], None, board_context).unwrap();
        assert_eq!(ctx.home, PathBuf::new());
        assert_eq!(ctx.system_root, PathBuf::new());
        assert_eq!(ctx.local_app_data, None);
    }

    const BOARD_ID: &str = "0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f";

    #[test]
    fn a_board_is_a_uuidv4_in_any_case_trimmed_and_lowercased() {
        assert_eq!(board(BOARD_ID).unwrap(), BOARD_ID);
        assert_eq!(
            board(&format!(" {}\n", BOARD_ID.to_uppercase())).unwrap(),
            BOARD_ID
        );
        for bad in [
            "",
            " ",
            "0a1b2c3d-0a1b-1c2d-8e3f-0a1b2c3d4e5f",
            "0a1b2c3d-0a1b-4c2d-7e3f-0a1b2c3d4e5f",
            "{0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f}",
            "urn:uuid:0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f",
            "0a1b2c3d0a1b4c2d8e3f0a1b2c3d4e5f",
            "agentspace",
            "\u{feff}0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f",
        ] {
            let e = board(bad).unwrap_err();
            assert_eq!(e.field, "board", "{bad:?}");
            assert_eq!(e.code, ValueCode::InvalidValue, "{bad:?}");
            assert_eq!(
                e.message,
                format!(
                    "Board {} is not a board ID. Give the board's UUID, as in '0a1b2c3d-...'.",
                    quote(bad)
                ),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn an_epic_is_1_to_16_digits_from_1_to_2_to_the_53_less_1_without_leading_zeros() {
        for (good, n) in [
            ("52", 52),
            ("0052", 52),
            (" 7\t", 7),
            ("0000000000000001", 1),
            ("9007199254740991", 9_007_199_254_740_991),
        ] {
            assert_eq!(epic(good).unwrap(), n, "{good:?}");
        }
        for bad in [
            "",
            " ",
            "0",
            "0000",
            "#0052",
            "+52",
            "-52",
            "52.0",
            "5e1",
            "00000000000000052",
            "9007199254740992",
            "99999999999999999999",
            "５２",
            "\u{feff}52",
        ] {
            let e = epic(bad).unwrap_err();
            assert_eq!(e.field, "epic", "{bad:?}");
            assert_eq!(e.code, ValueCode::InvalidValue, "{bad:?}");
            assert_eq!(
                e.message,
                format!(
                    "Epic {} is not a ticket number. Give the epic's ticket as digits, as in '52'.",
                    quote(bad)
                ),
                "{bad:?}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_board_epic_or_turn_that_is_not_unicode_is_invalid_for_its_field() {
        use crate::commands::support::check_value;
        use std::os::windows::ffi::OsStringExt;
        let bad = OsString::from_wide(&[u16::from(b'T'), 0xD800]);
        let e = check_value(&bad, "turn", turn).unwrap_err();
        assert_eq!(
            (e.code, e.field.as_deref(), e.message.as_str()),
            (
                ErrorCode::InvalidValue,
                Some("turn"),
                "Turn 'T\\ud800' is not a turn name. Give the plan's turn name, as in 'T01'."
            )
        );
        let e = check_value(&bad, "epic", epic).unwrap_err();
        assert_eq!(e.field.as_deref(), Some("epic"));
        assert!(
            e.message
                .starts_with("Epic 'T\\ud800' is not a ticket number.")
        );
        let e = check_value(&bad, "board", board).unwrap_err();
        assert_eq!(e.field.as_deref(), Some("board"));
        assert!(e.message.starts_with("Board 'T\\ud800' is not a board ID."));
    }

    #[test]
    fn a_turn_is_a_names_form_never_trimmed() {
        let longest = format!("T{}x", " ".repeat(62));
        for good in ["T01", "t01", "!", "a b", "~", longest.as_str()] {
            assert_eq!(turn(good).unwrap(), good, "{good:?}");
        }
        let too_long = "T".repeat(65);
        for bad in [
            " T01",
            "T01 ",
            " ",
            "T01\n",
            "T\u{7f}",
            "T\u{e9}",
            "\u{feff}T01",
            too_long.as_str(),
        ] {
            let e = turn(bad).unwrap_err();
            assert_eq!(e.field, "turn", "{bad:?}");
            assert_eq!(
                e.message,
                format!(
                    "Turn {} is not a turn name. Give the plan's turn name, as in 'T01'.",
                    quote(bad)
                ),
                "{bad:?}"
            );
        }
    }
}
