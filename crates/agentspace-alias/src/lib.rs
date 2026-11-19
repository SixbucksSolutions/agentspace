//! agentspace-alias (docs/decisions.md, D38): the alias file, the `Include` line and the
//! ProxyCommand line, as text and bytes. No file is read or written here, and no environment
//! variable is read.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The rules are docs/state.md's, "The alias
//! file" and "The `Include` line".

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

/// The values of one alias file. `key_path` and `cert_path` are as `lease_file_text` gives them,
/// already in the file's form. `proxy_command` is the raw command, which `alias_file_bytes`
/// escapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasFile<'a> {
    pub id: &'a str,
    pub comment: &'a str,
    pub alias: &'a str,
    pub user: &'a str,
    pub key_path: &'a str,
    pub cert_path: &'a str,
    pub proxy_command: &'a str,
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
    {
        return Err(err("a value in the alias file holds a line break"));
    }
    if !is_valid_alias(f.alias) {
        return Err(err("the host alias is not a valid name"));
    }
    let proxy = f.proxy_command.replace('%', "%%");
    let text = format!(
        "# agentspace lease {id}; {comment}\n\
         Host {alias}\n\
         \x20 User {user}\n\
         \x20 IdentityFile \"{key}\"\n\
         \x20 CertificateFile \"{cert}\"\n\
         \x20 IdentitiesOnly yes\n\
         \x20 HostKeyAlias agentspace\n\
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
    const PREFIX: &[u8] = b"# agentspace lease ";
    let line = bytes.split(|&b| b == b'\n').next()?;
    if line.len() < PREFIX.len() || !line[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
        return None;
    }
    let rest = &line[PREFIX.len()..];
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
