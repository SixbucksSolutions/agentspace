//! agentspace-alias (docs/decisions.md, D38): the alias file, the `Include` line and the
//! ProxyCommand line, as text and bytes, and the files they live in. The text half (this file) reads
//! and writes no file and reads no environment variable. The disk half ([`ensure_include`],
//! [`write_alias`], [`check_alias_owner`], [`remove_alias`] and [`find_proxy_program`]) is on
//! Windows the contract's; on another system it builds with std equivalents that are NOT the
//! contract's.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The rules are docs/state.md's, "The alias
//! file" and "The `Include` line".

#![deny(unsafe_code)]

mod disk;
// The only module that calls Windows, and so the only one that may hold `unsafe`.
mod windows;

pub use disk::{
    AliasFailure, AliasFailureKind, check_alias_owner, check_host_alias_owner, ensure_include,
    find_proxy_program, remove_alias, remove_host_alias, write_alias, write_host_alias,
};

use std::fmt;

/// Why an alias file, a ProxyCommand or an `Include` line could not be made: the `<why>` of
/// lease-api.md's message, in agentspace's own words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasError(pub String);

impl fmt::Display for AliasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AliasError {}

fn err(why: &str) -> AliasError {
    AliasError(why.to_string())
}

/// The lease's alias: `agentspace-<project>-<first 8 hex digits of the id>`. The caller passes the
/// lowercased project and the normalized ID; both are used as given.
pub fn alias_name(project: &str, id: &str) -> String {
    let short: String = id.chars().take(8).collect();
    format!("agentspace-{project}-{short}")
}

/// Whether `alias` matches `^[A-Za-z0-9._-]+\z`, case-sensitively, and does not start with `-`.
pub fn is_valid_alias(alias: &str) -> bool {
    !alias.is_empty()
        && !alias.starts_with('-')
        && alias
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Writes `s` the way the alias file holds a path: every `\` as `/`, every `%` as `%%`.
fn path_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' => out.push('/'),
            '%' => out.push_str("%%"),
            _ => out.push(c),
        }
    }
    out
}

/// The path of `<home>\.agentspace\leases\<id>\<file>` in the alias file's form (state.md, the
/// `<key path>` row, gap FA-05). `home_full` is the home folder already made full.
///
/// The home folder's trailing `\` and `/` are dropped, and the rest is appended after one `\`.
/// Every `\` is then written `/` and every `%` as `%%`.
pub fn lease_file_text(home_full: &str, id: &str, file: &str) -> String {
    // Trailing separators are dropped and one `\` is added, so a home that already ends in a
    // separator gets no further one: "C:\" gives "C:/.agentspace/..." (state.md, the
    // `<key path>` row; architect, 2026-11-19).
    let trimmed = home_full.trim_end_matches(['\\', '/']);
    let joined = format!("{trimmed}\\.agentspace\\leases\\{id}\\{file}");
    path_text(&joined)
}

/// The path of `<home>\.agentspace\hosts\<project>\<file>` in the alias file's form, written as
/// [`lease_file_text`] writes a lease's: the home's trailing `\` and `/` dropped, the rest joined as
/// text, every `\` as `/` and every `%` as `%%`. The result is
/// `<home>/.agentspace/hosts/<project>/<file>`.
///
/// Added at review (architect, 2026-11-20, for v0.07-T02).
pub fn host_file_text(home_full: &str, project: &str, file: &str) -> String {
    let trimmed = home_full.trim_end_matches(['\\', '/']);
    let joined = format!("{trimmed}\\.agentspace\\hosts\\{project}\\{file}");
    path_text(&joined)
}

/// The path of `<home>\.agentspace\known_hosts` in the alias file's form, written as
/// [`lease_file_text`] writes a lease's. The result is `<home>/.agentspace/known_hosts`.
///
/// Added at review (architect, 2026-11-20, for v0.07-T02).
pub fn known_hosts_text(home_full: &str) -> String {
    let trimmed = home_full.trim_end_matches(['\\', '/']);
    path_text(&format!("{trimmed}\\.agentspace\\known_hosts"))
}

/// The values of one alias file. `key_path`, `cert_path` and `known_hosts` are as
/// `lease_file_text` gives a path, already in the file's form. `proxy_command` is the raw command,
/// which `alias_file_bytes` escapes.
///
/// A lease on the box passes `host_key_alias` as `agentspace` and `known_hosts` as `None`, and its
/// bytes are v0.06's. A lease on another host passes `agentspace-host.<project>` and the path of
/// agentspace's `known_hosts`, as [`known_hosts_text`] gives it, which adds the
/// `UserKnownHostsFile` line (docs/state.md, "A lease alias for a host that is not the box").
/// `host_key_alias` MUST be a valid alias, as [`is_valid_alias`] says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasFile<'a> {
    pub id: &'a str,
    pub comment: &'a str,
    pub alias: &'a str,
    pub user: &'a str,
    pub key_path: &'a str,
    pub cert_path: &'a str,
    pub host_key_alias: &'a str,
    pub known_hosts: Option<&'a str>,
    pub proxy_command: &'a str,
}

/// Whether `project` is a project as the host registry names it,
/// `^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$`, with no trailing newline accepted. It is also one plain
/// path component on Windows (no `.`, so no trailing dot).
pub(crate) fn is_valid_project(project: &str) -> bool {
    let bytes = project.as_bytes();
    let plain = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    !bytes.is_empty()
        && bytes.len() <= 32
        && bytes.iter().all(|b| plain(b) || *b == b'-')
        && bytes.first().is_some_and(plain)
        && bytes.last().is_some_and(plain)
}

fn has_line_break(s: &str) -> bool {
    s.contains(['\r', '\n'])
}

/// The alias file's bytes, exactly as docs/state.md's block: UTF-8 with no byte order mark, each
/// line ending in one LF. A CR or LF in any value, or an alias that does not meet the alias rule,
/// is refused and nothing is made.
pub fn alias_file_bytes(f: &AliasFile) -> Result<Vec<u8>, AliasError> {
    if has_line_break(f.key_path) || has_line_break(f.cert_path) {
        return Err(err("a path in the alias file holds a line break"));
    }
    if has_line_break(f.proxy_command) {
        return Err(err("the ProxyCommand holds a line break"));
    }
    // Every value on one line refuses a CR or LF, not only the paths and the ProxyCommand
    // (state.md, The alias file; architect, 2026-11-19).
    if has_line_break(f.id)
        || has_line_break(f.comment)
        || has_line_break(f.alias)
        || has_line_break(f.user)
        || has_line_break(f.host_key_alias)
    {
        return Err(err("a value in the alias file holds a line break"));
    }
    if f.known_hosts.is_some_and(has_line_break) {
        return Err(err("a path in the alias file holds a line break"));
    }
    if !is_valid_alias(f.alias) {
        return Err(err("the host alias is not a valid name"));
    }
    // `HostKeyAlias` is written bare, so it MUST be a valid alias too.
    if !is_valid_alias(f.host_key_alias) {
        return Err(err("the host key alias is not a valid name"));
    }
    let proxy = f.proxy_command.replace('%', "%%");
    // The `UserKnownHostsFile` line follows `HostKeyAlias` and is there only for another host's
    // lease (state.md, "A lease alias for a host that is not the box").
    let known_hosts = f
        .known_hosts
        .map(|path| format!("\x20 UserKnownHostsFile \"{path}\"\n"))
        .unwrap_or_default();
    let text = format!(
        "# agentspace lease {id}; {comment}\n\
         Host {alias}\n\
         \x20 User {user}\n\
         \x20 IdentityFile \"{key}\"\n\
         \x20 CertificateFile \"{cert}\"\n\
         \x20 IdentitiesOnly yes\n\
         \x20 HostKeyAlias {host_key_alias}\n\
         {known_hosts}\
         \x20 StrictHostKeyChecking yes\n\
         \x20 ProxyCommand {proxy}\n\
         \x20 ServerAliveInterval 30\n\
         \x20 ServerAliveCountMax 4\n\
         \x20 RemoteForward 127.0.0.1:8811 127.0.0.1:8811\n\
         \x20 RemoteForward 127.0.0.1:53147 127.0.0.1:53147\n",
        id = f.id,
        comment = f.comment,
        alias = f.alias,
        user = f.user,
        key = f.key_path,
        cert = f.cert_path,
        host_key_alias = f.host_key_alias,
    );
    Ok(text.into_bytes())
}

/// The name of a host's operator alias, `agentspace-host.<project>`, which is also the `Host` name,
/// the `HostKeyAlias` and the file's name in `agentspace.d` (docs/state.md, "A host's operator
/// alias"). The caller passes the project as the host registry gives it.
pub fn host_alias_name(project: &str) -> String {
    format!("agentspace-host.{project}")
}

/// The values of one host's operator alias. `key_path` and `cert_path` are in the file's form, as
/// [`host_file_text`] gives them, and `known_hosts` as [`known_hosts_text`] gives it. `proxy_command` is the raw command, which
/// `host_alias_file_bytes` escapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostAliasFile<'a> {
    pub project: &'a str,
    pub comment: &'a str,
    pub key_path: &'a str,
    pub cert_path: &'a str,
    pub known_hosts: &'a str,
    pub proxy_command: &'a str,
}

/// A host's operator alias's bytes, exactly as docs/state.md, "A host's operator alias": UTF-8 with
/// no byte order mark, each line ending in one LF. A CR or LF in any value, or a project that is not
/// as the host registry names it (`^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$`), is refused and nothing
/// is made.
pub fn host_alias_file_bytes(f: &HostAliasFile) -> Result<Vec<u8>, AliasError> {
    if has_line_break(f.key_path) || has_line_break(f.cert_path) || has_line_break(f.known_hosts) {
        return Err(err("a path in the alias file holds a line break"));
    }
    if has_line_break(f.proxy_command) {
        return Err(err("the ProxyCommand holds a line break"));
    }
    if has_line_break(f.project) || has_line_break(f.comment) {
        return Err(err("a value in the alias file holds a line break"));
    }
    let host = host_alias_name(f.project);
    if !is_valid_project(f.project) || !is_valid_alias(&host) {
        return Err(err("the host alias is not a valid name"));
    }
    let proxy = f.proxy_command.replace('%', "%%");
    let text = format!(
        "# agentspace host {project}; {comment}\n\
         Host {host}\n\
         \x20 User ubuntu\n\
         \x20 IdentityFile \"{key}\"\n\
         \x20 CertificateFile \"{cert}\"\n\
         \x20 IdentitiesOnly yes\n\
         \x20 HostKeyAlias {host}\n\
         \x20 UserKnownHostsFile \"{known_hosts}\"\n\
         \x20 StrictHostKeyChecking accept-new\n\
         \x20 ProxyCommand {proxy}\n\
         \x20 ServerAliveInterval 30\n\
         \x20 ServerAliveCountMax 4\n",
        project = f.project,
        comment = f.comment,
        key = f.key_path,
        cert = f.cert_path,
        known_hosts = f.known_hosts,
    );
    Ok(text.into_bytes())
}

/// The default ProxyCommand line, not yet escaped for the alias file (`alias_file_bytes` doubles
/// each `%`). The program is in double quotes; when its extension is `.cmd` or `.bat`, in any ASCII
/// case, `<SystemRoot>\System32\cmd.exe /d /c` comes first. A `\` or `/` at the end of
/// `system_root` is dropped first. An unset or empty `system_root` is refused when that form is
/// needed, and no folder is guessed (state.md, the `<system folder>` row; gaps FA-06 and FB-20).
pub fn default_proxy_command(
    program_full_path: &str,
    alias: &str,
    system_root: Option<&str>,
) -> Result<String, AliasError> {
    if !is_valid_alias(alias) {
        return Err(err("the host alias is not a valid name"));
    }
    let name = program_full_path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(program_full_path);
    let needs_interpreter = name.rfind('.').is_some_and(|i| {
        let ext = &name[i + 1..];
        ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat")
    });
    if !needs_interpreter {
        return Ok(format!("\"{program_full_path}\" {alias}"));
    }
    // A root that is nothing but separators leaves no folder, so it is refused like an empty one.
    match system_root.map(|r| r.trim_end_matches(['\\', '/'])) {
        Some(root) if !root.is_empty() => Ok(format!(
            "{root}\\System32\\cmd.exe /d /c \"{program_full_path}\" {alias}"
        )),
        _ => Err(err(
            "the lease's way to reach its host cannot be found because Windows' system folder is not known",
        )),
    }
}

/// The normalized (lowercase) lease ID of an alias file's owner mark: its first line, up to the
/// first LF, `# agentspace lease <id>;`. The prefix is read without regard to ASCII case, and the
/// ID is the non-white-space run up to the last `;` in it, returned lowercased so it matches its
/// lease (state.md, The alias file, the owner mark).
pub fn owner_mark(bytes: &[u8]) -> Option<String> {
    mark_after(bytes, b"# agentspace lease ")
}

/// The normalized (lowercase) project of a host's operator alias's owner mark: its first line, up
/// to the first LF, `# agentspace host <project>;`, read as [`owner_mark`] reads a lease's, except
/// for the prefix. The two never match each other's files: `# agentspace host ` is not
/// `# agentspace lease ` (state.md, "A host's operator alias").
pub fn host_owner_mark(bytes: &[u8]) -> Option<String> {
    mark_after(bytes, b"# agentspace host ")
}

/// The word after `prefix` on a file's first line, up to the last `;` in it, lowercased.
fn mark_after(bytes: &[u8], prefix: &[u8]) -> Option<String> {
    let line = bytes.split(|&b| b == b'\n').next()?;
    if line.len() < prefix.len() || !line[..prefix.len()].eq_ignore_ascii_case(prefix) {
        return None;
    }
    let rest = &line[prefix.len()..];
    let run_len = rest
        .iter()
        .position(|b| b.is_ascii_whitespace())
        .unwrap_or(rest.len());
    let run = &rest[..run_len];
    let semi = run.iter().rposition(|&b| b == b';')?;
    if semi == 0 {
        return None;
    }
    let id = std::str::from_utf8(&run[..semi]).ok()?;
    Some(id.to_lowercase())
}

/// What `create` has to do about the `Include` line in a config's bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncludePlan {
    /// A line equal to `Include agentspace.d/*` is above the first `Host` or `Match` line.
    Found,
    /// The line is there only below a `Host` or `Match` line: a person has to move it.
    MustMove,
    /// The new config: the original's bytes with the line inserted as the first line.
    Insert(Vec<u8>),
}

const INCLUDE_LINE: &[u8] = b"Include agentspace.d/*";

fn trim_line(line: &[u8]) -> &[u8] {
    let is_pad = |b: &u8| matches!(b, b' ' | b'\t' | b'\r');
    let start = line.iter().position(|b| !is_pad(b)).unwrap_or(line.len());
    let end = line
        .iter()
        .rposition(|b| !is_pad(b))
        .map_or(start, |i| i + 1);
    &line[start..end]
}

/// Whether a trimmed line is equal to the `Include` line: any run of spaces or tabs between the
/// two words, ASCII case ignored, nothing else on the line.
fn is_include_line(line: &[u8]) -> bool {
    const WORD: &[u8] = b"Include";
    const PATTERN: &[u8] = b"agentspace.d/*";
    if line.len() < WORD.len() || !line[..WORD.len()].eq_ignore_ascii_case(WORD) {
        return false;
    }
    let rest = &line[WORD.len()..];
    let gap = rest
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t'))
        .unwrap_or(rest.len());
    gap > 0 && rest[gap..].eq_ignore_ascii_case(PATTERN)
}

/// Whether a trimmed line is a `Host` or `Match` line: its first word, in any ASCII case,
/// followed by a space, a tab, `=` or the end of the line.
fn is_block_line(line: &[u8]) -> bool {
    [&b"Host"[..], &b"Match"[..]].iter().any(|word| {
        line.len() >= word.len()
            && line[..word.len()].eq_ignore_ascii_case(word)
            && line
                .get(word.len())
                .is_none_or(|b| matches!(b, b' ' | b'\t' | b'='))
    })
}

/// The plan for the `Include` line on a config's bytes (state.md, "The `Include` line").
pub fn include_plan(config: &[u8]) -> IncludePlan {
    const BOM: &[u8] = b"\xEF\xBB\xBF";
    let has_bom = config.starts_with(BOM);
    let body = if has_bom {
        &config[BOM.len()..]
    } else {
        config
    };

    let mut in_block = false;
    let mut found_below = false;
    for raw in body.split(|&b| b == b'\n') {
        let line = trim_line(raw);
        if is_include_line(line) {
            if in_block {
                found_below = true;
            } else {
                return IncludePlan::Found;
            }
        } else if is_block_line(line) {
            in_block = true;
        }
    }
    if found_below {
        return IncludePlan::MustMove;
    }

    let has_crlf = body.windows(2).any(|w| w == b"\r\n");
    let mut out = Vec::with_capacity(config.len() + INCLUDE_LINE.len() + 2);
    if has_bom {
        out.extend_from_slice(BOM);
    }
    out.extend_from_slice(INCLUDE_LINE);
    out.extend_from_slice(if has_crlf { b"\r\n" } else { b"\n" });
    out.extend_from_slice(body);
    IncludePlan::Insert(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease<'a>(host_key_alias: &'a str, known_hosts: Option<&'a str>) -> AliasFile<'a> {
        AliasFile {
            id: "3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90",
            comment: "test",
            alias: "agentspace-p-3f2a9c1e",
            user: "ubuntu",
            key_path: "C:/h/id",
            cert_path: "C:/h/id-cert.pub",
            host_key_alias,
            known_hosts,
            proxy_command: "\"C:\\a\\p.exe\" agentspace-p-3f2a9c1e",
        }
    }

    fn host() -> HostAliasFile<'static> {
        HostAliasFile {
            project: "proj",
            comment: "test",
            key_path: "C:/h/id",
            cert_path: "C:/h/id-cert.pub",
            known_hosts: "C:/h/known_hosts",
            proxy_command: "\"C:\\a\\p 100%.exe\" agentspace-host.proj",
        }
    }

    #[test]
    fn the_boxs_lease_alias_is_v006s_bytes_and_another_hosts_adds_one_line() {
        let box_bytes = alias_file_bytes(&lease("agentspace", None)).unwrap();
        let text = String::from_utf8(box_bytes).unwrap();
        assert!(text.contains("\n  HostKeyAlias agentspace\n  StrictHostKeyChecking yes\n"));
        assert!(!text.contains("UserKnownHostsFile"));
        assert_eq!(text.lines().count(), 13);
        let other =
            alias_file_bytes(&lease("agentspace-host.proj", Some("C:/h/known_hosts"))).unwrap();
        let other = String::from_utf8(other).unwrap();
        assert!(other.contains(
            "\n  HostKeyAlias agentspace-host.proj\n  UserKnownHostsFile \"C:/h/known_hosts\"\n  StrictHostKeyChecking yes\n"
        ));
        assert_eq!(other.lines().count(), 14);
    }

    #[test]
    fn a_line_break_in_the_new_lease_values_is_refused() {
        assert!(alias_file_bytes(&lease("a\nb", None)).is_err());
        assert!(alias_file_bytes(&lease("a", Some("C:/h\r/k"))).is_err());
    }

    #[test]
    fn a_host_alias_is_the_bytes_state_md_gives() {
        let want = "# agentspace host proj; test\n\
            Host agentspace-host.proj\n\
            \x20 User ubuntu\n\
            \x20 IdentityFile \"C:/h/id\"\n\
            \x20 CertificateFile \"C:/h/id-cert.pub\"\n\
            \x20 IdentitiesOnly yes\n\
            \x20 HostKeyAlias agentspace-host.proj\n\
            \x20 UserKnownHostsFile \"C:/h/known_hosts\"\n\
            \x20 StrictHostKeyChecking accept-new\n\
            \x20 ProxyCommand \"C:\\a\\p 100%%.exe\" agentspace-host.proj\n\
            \x20 ServerAliveInterval 30\n\
            \x20 ServerAliveCountMax 4\n";
        assert_eq!(host_alias_file_bytes(&host()).unwrap(), want.as_bytes());
    }

    #[test]
    fn a_host_alias_refuses_a_line_break_in_any_value_and_a_bad_project() {
        for i in 0..6 {
            let mut h = host();
            let bad = "x\ny";
            match i {
                0 => h.project = bad,
                1 => h.comment = bad,
                2 => h.key_path = bad,
                3 => h.cert_path = bad,
                4 => h.known_hosts = "x\ry",
                _ => h.proxy_command = bad,
            }
            assert!(host_alias_file_bytes(&h).is_err(), "{i}");
        }
        for project in ["", "a b", "a/b"] {
            let mut h = host();
            h.project = project;
            assert!(host_alias_file_bytes(&h).is_err(), "{project:?}");
        }
    }

    #[test]
    fn a_host_alias_takes_only_a_project_the_host_registry_names() {
        let long_ok = "a".repeat(32);
        for project in ["a", "proj", "a-b", "a1", long_ok.as_str()] {
            let mut h = host();
            h.project = project;
            assert!(host_alias_file_bytes(&h).is_ok(), "{project:?}");
        }
        // Each of these makes a valid alias name, and none is a registry project. `x.` is the
        // collision: Windows drops a trailing dot, so it would name the same file as `x`.
        let long_bad = "a".repeat(33);
        for project in [
            "A",
            "a_b",
            "a.b",
            "..",
            "x.",
            "-a",
            "a-",
            "a\n",
            "",
            long_bad.as_str(),
        ] {
            let mut h = host();
            h.project = project;
            assert!(host_alias_file_bytes(&h).is_err(), "{project:?}");
        }
    }

    #[test]
    fn a_lease_alias_refuses_a_host_key_alias_that_is_not_a_valid_alias() {
        for bad in ["", "a b", "-x", "a/b", "a\"b", "a%b", "a\\b"] {
            assert!(alias_file_bytes(&lease(bad, None)).is_err(), "{bad:?}");
        }
        assert!(alias_file_bytes(&lease("agentspace-host.p-1", None)).is_ok());
    }

    #[test]
    fn host_file_text_and_known_hosts_text_are_written_as_a_lease_files_path_is() {
        let home = "C:\\Users\\a%b\\";
        assert_eq!(
            host_file_text(home, "proj", "id_ed25519-cert.pub"),
            "C:/Users/a%%b/.agentspace/hosts/proj/id_ed25519-cert.pub"
        );
        assert_eq!(
            known_hosts_text(home),
            "C:/Users/a%%b/.agentspace/known_hosts"
        );
        // The same form as a lease's: only the folder after `.agentspace` differs.
        let lease_path = lease_file_text(home, "id", "f");
        assert_eq!(lease_path, "C:/Users/a%%b/.agentspace/leases/id/f");
        assert!(host_file_text(home, "proj", "f").starts_with("C:/Users/a%%b/.agentspace/"));
        // A home that ends in a separator gets no further one, as for a lease.
        assert_eq!(known_hosts_text("C:\\"), "C:/.agentspace/known_hosts");
        assert_eq!(host_file_text("C:/", "p", "f"), "C:/.agentspace/hosts/p/f");
    }

    #[test]
    fn the_two_owner_marks_never_match_each_other() {
        let lease_bytes = alias_file_bytes(&lease("agentspace", None)).unwrap();
        let host_bytes = host_alias_file_bytes(&host()).unwrap();
        assert_eq!(
            owner_mark(&lease_bytes).as_deref(),
            Some("3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90")
        );
        assert_eq!(owner_mark(&host_bytes), None);
        assert_eq!(host_owner_mark(&host_bytes).as_deref(), Some("proj"));
        assert_eq!(host_owner_mark(&lease_bytes), None);
        assert_eq!(
            host_owner_mark(b"# AGENTSPACE HOST Proj; x\nHost y\n").as_deref(),
            Some("proj")
        );
        assert_eq!(host_owner_mark(b"# agentspace host ;x\n"), None);
        assert_eq!(host_owner_mark(b"# agentspace host proj x\n"), None);
    }
}
