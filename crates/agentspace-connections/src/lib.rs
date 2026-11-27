//! Ends a lease alias's ssh clients on the laptop, each with its process tree (docs/driver.md,
//! Ending a lease's connections; D59, D63).
//!
//! RFC 2119 keywords; the capitals are load-bearing.
//!
//! The walk's logic is written once, in `walk`, over the `walk::ProcessTable` seam: `windows` gives
//! it Windows, and the unit tests give it a stand-in table and clock. Only `windows` MAY use
//! `unsafe`. Nothing here writes to standard output or standard error, and no result, error or
//! `Debug` output holds a command line, an image path or a user.

#![cfg_attr(not(windows), allow(dead_code))]

pub mod argv;
pub mod line;
mod walk;
#[cfg(windows)]
mod windows;

/// A process's identity across two walks: its ID and its creation time (FILETIME, 100 ns units).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Identity {
    pub pid: u32,
    pub created: u64,
}

/// Why a walk did not end every client it found (driver.md, What cannot be ended).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndError {
    /// The process list could not be read at all; nothing was ended.
    ListUnreadable { windows_error: u32 },
    /// At least one client could not be ended; every other client was.
    NotAllEnded {
        ended: usize,
        not_ended: Vec<Identity>,
    },
}

impl std::fmt::Display for EndError {
    // Counts and the Windows error number only, never a process ID, path or command line
    // (driver.md, What cannot be ended).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndError::ListUnreadable { windows_error } => write!(
                f,
                "The process list could not be read: Windows error {windows_error}."
            ),
            EndError::NotAllEnded { ended, not_ended } => write!(
                f,
                "Ended {ended} of the alias's clients; {} could not be ended.",
                not_ended.len()
            ),
        }
    }
}

impl std::error::Error for EndError {}

pub type Result<T> = std::result::Result<T, EndError>;

/// Ends every client of `alias` on the laptop, each with its tree, and returns how many clients
/// it ended (driver.md, Ending a lease's connections).
///
/// `Ok(n)` counts the tops it ended, never descendants: a top is a client that no other client's
/// tree holds, so one call ends a chain of clients below programs that have exited, whatever its
/// depth, and counts each of its clients once (D99; #0228). A client gone already is neither a
/// failure nor counted. The call returns within 10 seconds of its start. On a platform that is not
/// Windows it returns `Ok(0)`.
pub fn end_alias_clients(alias: &str) -> Result<usize> {
    end_matching(alias, &|_| true)
}

/// TEST-ONLY. The same walk as `end_alias_clients`, except that a process is a client only when,
/// besides every rule of driver.md's section, its executable's full path is inside `dir`.
/// Descendants are found by the walk whatever their path.
///
/// A test that walks the laptop's real process list MUST call this, over a throwaway folder and
/// with an alias built from a UUIDv4 drawn at random for the run, and never `end_alias_clients`.
#[doc(hidden)]
pub fn end_alias_clients_under(alias: &str, dir: &std::path::Path) -> Result<usize> {
    let roots = folder_forms(dir);
    if roots.is_empty() {
        return Ok(0);
    }
    end_matching(alias, &|image: &str| {
        let image = image.to_lowercase();
        roots.iter().any(|root| image.starts_with(root.as_str()))
    })
}

/// The forms of `dir` an image path inside it can start with, each lowercase and ending in a
/// separator: the folder as given, made absolute, and as the file system resolves it (long names,
/// links followed), so a folder named through an 8.3 short name still matches.
fn folder_forms(dir: &std::path::Path) -> Vec<String> {
    let mut forms = Vec::new();
    let given = std::path::absolute(dir).ok();
    let resolved = std::fs::canonicalize(dir).ok();
    for path in given.into_iter().chain(resolved) {
        let Some(text) = path.to_str() else { continue };
        let text = strip_verbatim(text).replace('/', "\\");
        let mut text = text.trim_end_matches('\\').to_lowercase();
        if text.is_empty() {
            continue;
        }
        text.push('\\');
        if !forms.contains(&text) {
            forms.push(text);
        }
    }
    forms
}

/// A Win32 path from one that may carry the `\\?\` prefix `canonicalize` gives.
fn strip_verbatim(text: &str) -> String {
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        text.to_string()
    }
}

#[cfg(windows)]
fn end_matching(alias: &str, in_scope: &dyn Fn(&str) -> bool) -> Result<usize> {
    if alias.is_empty() {
        return Ok(0);
    }
    let clock = walk::RealClock::new();
    let table =
        windows::WinTable::new().map_err(|e| EndError::ListUnreadable { windows_error: e })?;
    walk::end_clients(&table, &clock, alias, in_scope)
}

#[cfg(not(windows))]
fn end_matching(_alias: &str, _in_scope: &dyn Fn(&str) -> bool) -> Result<usize> {
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_holds_counts_and_the_error_number_only() {
        let e = EndError::ListUnreadable { windows_error: 5 };
        assert_eq!(
            e.to_string(),
            "The process list could not be read: Windows error 5."
        );
        let e = EndError::NotAllEnded {
            ended: 3,
            not_ended: vec![Identity {
                pid: 4321,
                created: 98765,
            }],
        };
        let text = e.to_string();
        assert_eq!(
            text,
            "Ended 3 of the alias's clients; 1 could not be ended."
        );
        assert!(!text.contains("4321") && !text.contains("98765"));
    }

    #[test]
    fn verbatim_prefixes_are_stripped() {
        assert_eq!(strip_verbatim(r"\\?\C:\x\y"), r"C:\x\y");
        assert_eq!(strip_verbatim(r"\\?\UNC\server\share"), r"\\server\share");
        assert_eq!(strip_verbatim(r"C:\x"), r"C:\x");
    }

    #[test]
    fn an_empty_alias_ends_nothing() {
        assert_eq!(
            end_alias_clients_under("", &std::env::temp_dir().join("agentspace-none")),
            Ok(0)
        );
    }
}
