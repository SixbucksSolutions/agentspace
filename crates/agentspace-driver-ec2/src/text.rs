//! The driver's words, and how it names a file in them (docs/lease-api.md, "Messages" and
//! "Concurrency and atomicity"; docs/driver.md, "Failure").
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **A message MUST be in agentspace's own words:** no absolute path, no user name, no driver or
//!   backend name, and no other program's or library's error text.
//! - **A file or folder is named only home-relative** (`~/.agentspace/leases/<id>/id_ed25519`), or,
//!   for an alias file outside the home folder, as `the host alias file <alias>` (gap FA-04).
//! - **A message MUST NOT be empty or start with a space.**

use std::path::{Component, Path};

use agentspace_driver_api::Failure;

/// A value quoted in a message: each control character (U+0000 to U+001F, U+007F) written as `\u`
/// and four lowercase hex digits (lease-api.md, "Exit codes and errors").
pub fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if (c as u32) < 0x20 || c as u32 == 0x7f {
            out.push_str(&format!("\\u{:04x}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

fn key(part: &str) -> String {
    if cfg!(windows) {
        part.to_lowercase()
    } else {
        part.to_string()
    }
}

/// The components of a path, as (as written, as compared). A `.` is dropped.
fn parts(path: &Path) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            // A path with `..` in it cannot be said to be under the home folder by looking at it.
            Component::ParentDir => return None,
            other => {
                let s = other.as_os_str().to_string_lossy().into_owned();
                let k = key(&s);
                out.push((s, k));
            }
        }
    }
    Some(out)
}

/// `~/<rest>` when `path` is under `home`, compared by components (without regard to case on
/// Windows), and `None` when it is not, or is the home folder itself.
pub fn home_relative(home: &Path, path: &Path) -> Option<String> {
    let h = parts(home)?;
    let p = parts(path)?;
    if p.len() <= h.len() || h.iter().zip(&p).any(|(a, b)| a.1 != b.1) {
        return None;
    }
    let rest: Vec<&str> = p[h.len()..].iter().map(|(s, _)| s.as_str()).collect();
    Some(format!("~/{}", rest.join("/")))
}

/// How a message names a lease's alias file: home-relative when the OpenSSH client folder is under
/// the home folder, and otherwise by what it is and its own name (gap FA-04).
pub fn alias_leftover(home: &Path, ssh_dir: &Path, alias: &str) -> String {
    let path = ssh_dir.join("agentspace.d").join(alias);
    home_relative(home, &path).unwrap_or_else(|| format!("the host alias file {alias}"))
}

/// A lease's folder, home-relative.
pub fn lease_folder_name(id: &str) -> String {
    format!("~/.agentspace/leases/{id}")
}

/// A file in a lease's folder, home-relative.
pub fn lease_file_name(id: &str, file: &str) -> String {
    format!("~/.agentspace/leases/{id}/{file}")
}

/// `a`, `a and b`, `a, b and c`.
pub fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// The `<why>` that names what is left: `a is still there`, `a and b are still there`.
pub fn still_there(leftovers: &[String]) -> String {
    let verb = if leftovers.len() == 1 { "is" } else { "are" };
    format!("{} {verb} still there", list(leftovers))
}

/// Puts a capital on the first letter of a phrase that has none.
pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The message with each leftover named in it, as a last sentence.
pub fn with_leftovers(message: String, leftovers: &[String]) -> String {
    if leftovers.is_empty() {
        message
    } else {
        format!("{message} Still there: {}.", leftovers.join(", "))
    }
}

/// A failure whose message names its leftovers, in the message and in `leftovers` both.
pub fn failure(message: String, leftovers: Vec<String>) -> Failure {
    Failure {
        message: with_leftovers(message, &leftovers),
        leftovers,
    }
}

/// lease-api.md's "Alias cannot be set up (`create`)" message, `<why>` filled in.
pub fn alias_setup_failed(why: &str) -> String {
    format!(
        "The host alias could not be added to the OpenSSH client configuration: {why}. No lease was recorded."
    )
}

/// lease-api.md's "Way to reach the host not found (`create`)" message.
pub const TRANSPORT_MISSING: &str = "The lease's way to reach its host cannot be found: agentspace's ProxyCommand program is not installed beside this program. No lease was recorded.";

/// The message for a transport that cannot be made for a reason other than a missing program,
/// from the alias crate's phrase, which has no capital and no full stop.
pub fn transport_failed(why: &str) -> String {
    format!("{}. No lease was recorded.", capitalize(why))
}

/// lease-api.md's "Alias file is not this lease's (`release`)" message.
pub const FOREIGN_ALIAS: &str = "The lease's host alias file belongs to another lease or to no lease, so nothing was deleted. A person has to move that file.";

/// A release check that could not be made, `<why>` filled in.
pub fn alias_check_failed(why: &str) -> String {
    format!("The lease's host alias file could not be checked: {why}. Nothing was deleted.")
}

/// lease-api.md's "Credential or alias not deleted (`release`)" message, `<why>` filled in.
pub fn delete_failed(why: &str) -> String {
    format!(
        "The lease's local key, certificate or host alias could not be deleted: {why}. Run release again to finish."
    )
}

/// driver.md's "The lease's host could not be looked up" message (Where it places a lease),
/// `<why>` filled in: the host registry, its lock or the project list could not be used.
pub fn host_lookup_failed(why: &str) -> String {
    format!("The lease's host could not be looked up: {why}. Nothing was recorded.")
}

/// driver.md's `no_capacity` message (Where it places a lease), for a project with no host on the
/// lease's platform that no entry of the project list names, on any platform: v0.07's message,
/// byte for byte, whatever platform was asked for (pin P3). `project` is the normalized project,
/// quoted as every reason quotes.
pub fn no_capacity(project: &str) -> String {
    format!(
        "No capacity can be found for project '{}': it has no host and is not on agentspace's project list.",
        quote(project)
    )
}

/// driver.md's `no_capacity` message from v0.12 (Where it places a lease; D91; pin P3), for a
/// project with no host on the lease's platform whose entry on the project list does not approve
/// that platform, and for no other lease. **It MUST be emitted byte for byte.** `project` and
/// `platform` are the normalized values, quoted as every reason quotes.
pub fn no_capacity_on_platform(project: &str, platform: &str) -> String {
    format!(
        "No capacity can be found for project '{}' on platform '{}': it has no host there and is not approved for that platform on agentspace's project list.",
        quote(project),
        quote(platform)
    )
}

/// D66's `no_capacity` message (driver.md, "On a `ready` host"; lease-api.md, Fields), for a lease
/// whose project's host is smaller than the size asked for while another live `ready` lease of the
/// project holds it. **It MUST be emitted byte for byte** (D71; pin P8). `project` and `size` are
/// the normalized values, quoted as every reason quotes.
pub fn no_capacity_at_size(project: &str, size: &str) -> String {
    format!(
        "No capacity can be found for project '{}' at size '{}': its host is serving other leases at a smaller size. Release them, or ask for a size they fit.",
        quote(project),
        quote(size)
    )
}

/// driver.md's message for an operator program that is not beside the CLI, with
/// `AGENTSPACE_PROVISION_CMD` unset or empty (Starting a provisioning run, step 3).
pub const OPERATOR_NOT_BESIDE: &str = "The lease's host cannot be made: agentspace's operator program is not installed beside this program. Nothing was recorded.";

/// driver.md's message for an `AGENTSPACE_PROVISION_CMD` that names no existing file.
pub const OPERATOR_NAMED_MISSING: &str = "The lease's host cannot be made: the program that AGENTSPACE_PROVISION_CMD names does not exist. Nothing was recorded.";

/// driver.md's message for a `.cmd` or `.bat` program with `SystemRoot` unset or empty.
pub const OPERATOR_NO_INTERPRETER: &str = "The lease's host cannot be made: Windows' command interpreter cannot be found, because SystemRoot is not set. Nothing was recorded.";

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn control_characters_are_escaped_and_others_kept() {
        assert_eq!(quote("a\nb\u{7f}c\u{1}"), "a\\u000ab\\u007fc\\u0001");
        assert_eq!(quote("linux/arm64 é"), "linux/arm64 é");
    }

    #[test]
    fn a_path_under_the_home_folder_is_named_home_relative() {
        let home = PathBuf::from(r"C:\Users\someone");
        let path = home.join(".ssh").join("agentspace.d").join("a");
        assert_eq!(
            home_relative(&home, &path).as_deref(),
            Some("~/.ssh/agentspace.d/a")
        );
    }

    #[cfg(windows)]
    #[test]
    fn the_comparison_ignores_case_on_windows() {
        let home = PathBuf::from(r"C:\Users\Someone");
        let path = PathBuf::from(r"c:\users\someone\.ssh");
        assert_eq!(home_relative(&home, &path).as_deref(), Some("~/.ssh"));
    }

    #[test]
    fn a_path_elsewhere_the_home_itself_and_a_dotdot_path_are_not_home_relative() {
        let home = PathBuf::from("/a/home");
        assert_eq!(home_relative(&home, Path::new("/a/other/x")), None);
        assert_eq!(home_relative(&home, Path::new("/a/homey/x")), None);
        assert_eq!(home_relative(&home, &home), None);
        assert_eq!(home_relative(&home, Path::new("/a/home/../x")), None);
    }

    #[test]
    fn an_alias_outside_the_home_is_named_by_what_it_is() {
        let home = PathBuf::from("/a/home");
        let ssh = PathBuf::from("/b/ssh");
        assert_eq!(
            alias_leftover(&home, &ssh, "agentspace-p-12345678"),
            "the host alias file agentspace-p-12345678"
        );
        assert_eq!(
            alias_leftover(&home, &home.join(".ssh"), "agentspace-p-12345678"),
            "~/.ssh/agentspace.d/agentspace-p-12345678"
        );
    }

    #[test]
    fn lists_and_still_there_read_as_lease_api_does() {
        let one = vec!["a".to_string()];
        let two = vec!["a".to_string(), "b".to_string()];
        let three = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(still_there(&one), "a is still there");
        assert_eq!(still_there(&two), "a and b are still there");
        assert_eq!(list(&three), "a, b and c");
    }

    #[test]
    fn leftovers_are_named_in_the_message_and_in_the_list() {
        let f = failure("It failed.".to_string(), vec!["~/x".to_string()]);
        assert_eq!(f.message, "It failed. Still there: ~/x.");
        assert_eq!(f.leftovers, ["~/x"]);
        let f = failure("It failed.".to_string(), Vec::new());
        assert_eq!(f.message, "It failed.");
    }

    #[test]
    fn the_no_capacity_message_for_a_smaller_host_is_d66s_byte_for_byte() {
        assert_eq!(
            no_capacity_at_size("agentspace", "xl"),
            "No capacity can be found for project 'agentspace' at size 'xl': its host is serving other leases at a smaller size. Release them, or ask for a size they fit."
        );
        // v0.07's message for a project with no host is a different one, and stays as it was.
        assert_ne!(no_capacity_at_size("other", "m"), no_capacity("other"));
    }

    #[test]
    fn the_two_no_capacity_messages_for_a_project_with_no_host_are_driver_mds_byte_for_byte() {
        assert_eq!(
            no_capacity("other"),
            "No capacity can be found for project 'other': it has no host and is not on agentspace's project list."
        );
        assert_eq!(
            no_capacity_on_platform("other", "linux/amd64"),
            "No capacity can be found for project 'other' on platform 'linux/amd64': it has no host there and is not approved for that platform on agentspace's project list."
        );
    }

    #[test]
    fn templates_follow_lease_api_wording() {
        assert_eq!(
            alias_setup_failed("x is wrong"),
            "The host alias could not be added to the OpenSSH client configuration: x is wrong. No lease was recorded."
        );
        assert_eq!(
            delete_failed("a is still there"),
            "The lease's local key, certificate or host alias could not be deleted: a is still there. Run release again to finish."
        );
        assert_eq!(
            transport_failed("the way is not known"),
            "The way is not known. No lease was recorded."
        );
    }
}
