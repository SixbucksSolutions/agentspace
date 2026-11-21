//! The project list (docs/state.md, The project list; D54): the projects that may have a host
//! made. Written by a person, never by agentspace, so this module only reads it.

use crate::error::{Failure, HostsError, Invalid};
use crate::folder;
use crate::json;
use crate::paths::{agentspace_dir, projects_path};
use crate::sys;
use agentspace_state::{parse_time, validate_schema};
use serde_json::Value;
use std::collections::HashSet;
use std::io::{ErrorKind, Read};
use std::path::Path;

/// The schema's name in agentspace-state.
const SCHEMA: &str = "projects.state";

/// One entry of the project list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// The project, in the lease contract's normalized form.
    pub name: String,
    /// The date Terry approved it, `YYYY-MM-DD`. Nothing compares it with the clock.
    pub approved: String,
}

/// The project list. Its `version` is always 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Projects {
    pub projects: Vec<Project>,
}

/// Whether a project is listed: an entry's `name` equals `project`, exactly. A name is never
/// normalized here.
pub fn is_listed(projects: &Projects, project: &str) -> bool {
    projects.projects.iter().any(|p| p.name == project)
}

/// Reads the project list's bytes: a leading byte order mark and any JSON whitespace are accepted.
/// Bytes that are not UTF-8, a document that fails its schema, an object naming a member twice, two
/// entries of one `name`, and a date that names no real day each make it not valid.
pub fn parse(bytes: &[u8]) -> Result<Projects, Invalid> {
    let doc = json::parse(bytes)?;
    if validate_schema(SCHEMA, &doc) != Some(true) {
        return Err(Invalid::Schema);
    }
    let items = doc
        .get("projects")
        .and_then(Value::as_array)
        .ok_or(Invalid::Schema)?;
    let mut projects = Vec::with_capacity(items.len());
    for item in items {
        let field = |key: &str| item.get(key).and_then(Value::as_str).map(str::to_string);
        projects.push(Project {
            name: field("name").ok_or(Invalid::Schema)?,
            approved: field("approved").ok_or(Invalid::Schema)?,
        });
    }
    let mut seen = HashSet::new();
    if !projects.iter().all(|p| seen.insert(p.name.as_str())) {
        return Err(Invalid::RepeatedName);
    }
    // A date is a real day when it is one at midnight of the contract's time form, counted in the
    // proleptic Gregorian calendar.
    if projects
        .iter()
        .any(|p| parse_time(&format!("{}T00:00:00Z", p.approved)).is_none())
    {
        return Err(Invalid::DateNamesNoDay);
    }
    Ok(Projects { projects })
}

/// Reads `<home>\.agentspace\projects.json`, sharing read, write and delete. It takes no lock and
/// writes nothing. A missing file, and a missing `.agentspace`, list nothing. Something at the path
/// that is not a file, a file that cannot be opened or read, and a file that is not valid list
/// nothing either, but are errors (docs/state.md).
pub fn read_projects(home: &Path) -> Result<Projects, HostsError> {
    if folder::is_blocked(&agentspace_dir(home)) {
        return Err(HostsError::Unreadable(Failure::FolderIsAFile));
    }
    let path = projects_path(home);
    let unreadable = |why: Failure| HostsError::Unreadable(why);
    let mut file = match sys::open_read(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Ok(Projects {
                projects: Vec::new(),
            });
        }
        Err(_) if path.is_dir() => return Err(unreadable(Failure::NotAFile)),
        Err(_) => return Err(unreadable(Failure::CannotOpen)),
    };
    if file.metadata().is_ok_and(|m| m.is_dir()) {
        return Err(unreadable(Failure::NotAFile));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| unreadable(Failure::CannotRead))?;
    parse(&bytes).map_err(HostsError::NotValid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn put(home: &Path, bytes: &[u8]) {
        fs::create_dir_all(agentspace_dir(home)).unwrap();
        fs::write(projects_path(home), bytes).unwrap();
    }

    #[test]
    fn a_missing_file_or_folder_lists_nothing() {
        let t = tempfile::tempdir().unwrap();
        let none = read_projects(t.path()).unwrap();
        assert!(none.projects.is_empty());
        assert!(!is_listed(&none, "agentspace"));
        assert!(!agentspace_dir(t.path()).exists());
    }

    #[test]
    fn a_listed_project_is_found_by_its_exact_name_only() {
        let t = tempfile::tempdir().unwrap();
        put(
            t.path(),
            b"\xEF\xBB\xBF {\"version\":0,\r\n \"projects\":[{\"name\":\"agentspace\",\"approved\":\"2026-11-20\"}]}",
        );
        let list = read_projects(t.path()).unwrap();
        assert_eq!(list.projects.len(), 1);
        assert_eq!(list.projects[0].approved, "2026-11-20");
        assert!(is_listed(&list, "agentspace"));
        for other in ["Agentspace", "agentspace ", "agentspac", "swimcoach", ""] {
            assert!(!is_listed(&list, other), "{other:?}");
        }
    }

    #[test]
    fn a_date_in_the_future_is_valid_and_a_leap_day_counts() {
        for date in ["2999-12-31", "2028-02-29", "2000-02-29"] {
            let doc = format!(r#"{{"version":0,"projects":[{{"name":"a","approved":"{date}"}}]}}"#);
            assert!(parse(doc.as_bytes()).is_ok(), "{date}");
        }
    }

    #[test]
    fn each_refusal_is_an_error_with_its_reason() {
        let cases: [(&str, Invalid); 9] = [
            (
                r#"{"version":0,"projects":[{"name":"a","approved":"2026-02-30"}]}"#,
                Invalid::DateNamesNoDay,
            ),
            (
                r#"{"version":0,"projects":[{"name":"a","approved":"2026-04-31"}]}"#,
                Invalid::DateNamesNoDay,
            ),
            (
                r#"{"version":0,"projects":[{"name":"a","approved":"2026-02-29"}]}"#,
                Invalid::DateNamesNoDay,
            ),
            (
                r#"{"version":0,"projects":[{"name":"a","approved":"2026-11-20"},{"name":"a","approved":"2026-11-21"}]}"#,
                Invalid::RepeatedName,
            ),
            (
                r#"{"version":0,"projects":[{"name":"A","approved":"2026-11-20"}]}"#,
                Invalid::Schema,
            ),
            (r#"{"version":1,"projects":[]}"#, Invalid::Schema),
            (r#"{"version":0,"projects":["a"]}"#, Invalid::Schema),
            (
                r#"{"version":0,"projects":[{"name":"a","name":"a","approved":"2026-11-20"}]}"#,
                Invalid::RepeatedMember,
            ),
            (
                r#"{"version":0,"projects":[{"name":"a","approved":"2026-11-20T00:00:00Z"}]}"#,
                Invalid::Schema,
            ),
        ];
        for (text, why) in cases {
            let t = tempfile::tempdir().unwrap();
            put(t.path(), text.as_bytes());
            assert_eq!(
                read_projects(t.path()),
                Err(HostsError::NotValid(why)),
                "{text}"
            );
        }
        let t = tempfile::tempdir().unwrap();
        put(t.path(), b"\xFF\xFE");
        assert_eq!(
            read_projects(t.path()),
            Err(HostsError::NotValid(Invalid::NotUtf8))
        );
    }

    #[test]
    fn a_folder_at_the_path_is_an_error_not_an_empty_list() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(projects_path(t.path())).unwrap();
        assert_eq!(
            read_projects(t.path()),
            Err(HostsError::Unreadable(Failure::NotAFile))
        );
    }
}
