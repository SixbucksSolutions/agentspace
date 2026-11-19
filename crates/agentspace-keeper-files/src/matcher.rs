//! The holder command and its matcher (keeper.md, The holder command).
//!
//! RFC 2119 keywords in this module's comments, and the capitals are load-bearing.
//!
//! Everything here works on text: a command line is a string, and a path is the Windows path text
//! it names, compared as Windows compares it. Nothing asks the operating system, so the matcher
//! builds and is tested the same on Linux and on Windows. The one fact it cannot know is
//! `<System32>`, which the caller gives it.

use agentspace_keeper_api::Matcher;
use std::path::{Path, PathBuf};

const ARGS: &str = "-N -o ExitOnForwardFailure=yes";

fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// One token of a command line, with where its raw text lies in the line (byte offsets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// Splits a command line into tokens as Windows' `CommandLineToArgvW` does (keeper.md, The holder
/// command, rule 1). The first token, the program, runs to the next `"` when the line starts with
/// one (or to the end if there is none), and otherwise to the next space or tab, with no escapes.
/// Every later token follows the argument rules, including the count that reads quotes following
/// one another as one run. An empty line gives no tokens at all (architect, 2026-11-19, for
/// E0.06-T01-S07), where `CommandLineToArgvW` would give the calling program's own path; any other
/// line gives a first token, which is empty when the line starts with a blank.
pub(crate) fn split_tokens(line: &str) -> Vec<Token> {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let n = chars.len();
    let at = |i: usize| chars.get(i).map_or(line.len(), |&(b, _)| b);
    let mut tokens = Vec::new();
    if n == 0 {
        return tokens;
    }
    let mut i = 0;
    let mut text = String::new();
    if n > 0 && chars[0].1 == '"' {
        i = 1;
        while i < n && chars[i].1 != '"' {
            text.push(chars[i].1);
            i += 1;
        }
        if i < n {
            i += 1;
        }
    } else {
        while i < n && !is_blank(chars[i].1) {
            text.push(chars[i].1);
            i += 1;
        }
    }
    tokens.push(Token {
        text,
        start: 0,
        end: at(i),
    });
    loop {
        while i < n && is_blank(chars[i].1) {
            i += 1;
        }
        if i >= n {
            return tokens;
        }
        let start = at(i);
        let mut text = String::new();
        let mut in_quote = false;
        while i < n {
            let c = chars[i].1;
            if c == '\\' {
                let mut slashes = 0;
                while i < n && chars[i].1 == '\\' {
                    slashes += 1;
                    i += 1;
                }
                if i < n && chars[i].1 == '"' {
                    // 2n backslashes give n, and the quote is a quote; 2n+1 give n and a literal.
                    text.extend(std::iter::repeat_n('\\', slashes / 2));
                    if slashes % 2 == 1 {
                        text.push('"');
                        i += 1;
                    }
                } else {
                    text.extend(std::iter::repeat_n('\\', slashes));
                }
            } else if c == '"' {
                // A run of quotes is read by a count: it starts at 1 inside quoting and at 0
                // outside it, each quote adds one, a quote that brings it to 3 is literal and
                // sets it back to 0, and quoting is on afterwards only if it stands at 1.
                let mut count = u8::from(in_quote);
                while i < n && chars[i].1 == '"' {
                    count += 1;
                    i += 1;
                    if count == 3 {
                        text.push('"');
                        count = 0;
                    }
                }
                in_quote = count == 1;
            } else if !in_quote && is_blank(c) {
                break;
            } else {
                text.push(c);
                i += 1;
            }
        }
        tokens.push(Token {
            text,
            start,
            end: at(i),
        });
    }
}

/// The arguments of a command line as Windows' `CommandLineToArgvW` splits them (keeper.md, The
/// holder command, rule 1), the program first. The same splitter the matcher uses.
pub fn split_command_line(line: &str) -> Vec<String> {
    split_tokens(line).into_iter().map(|t| t.text).collect()
}

/// A path as Windows compares it, or `None` when it is not fully qualified (keeper.md, The holder
/// command, rule 2): a drive letter, a colon and a separator, or a UNC path (`\\server\share\...`,
/// also as `\\?\C:\...` or `\\?\UNC\...`). Separators are made `\`, repeated ones and `.` are
/// dropped, and `..` is resolved without going above the root (or the share). A UNC path's server
/// and share are taken as they are, and one that is empty, `.` or `..` (such as the device path
/// `\\.\C:\...`) is not fully qualified here. Case is kept, and `same_file` compares without it.
///
/// Normalization is separators, `.`, `..` and case only: not trailing dots or spaces, links or
/// short (8.3) names; `\\?\C:\` is read as `C:\` and `\\?\UNC\` as `\\` (architect, 2026-11-19,
/// for E0.06-T01-S07).
pub(crate) fn normalize_path(path: &str) -> Option<String> {
    let s = path.replace('/', "\\");
    let s = if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s
    };
    let b = s.as_bytes();
    let (prefix, rest) =
        if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\' {
            (s[..3].to_string(), &s[3..])
        } else {
            let unc = s.strip_prefix(r"\\")?;
            let mut root = unc.splitn(3, '\\');
            let server = root.next().unwrap_or("");
            let share = root.next().unwrap_or("");
            let well_formed = |p: &str| !matches!(p, "" | "." | "..");
            if !well_formed(server) || !well_formed(share) {
                return None;
            }
            (format!(r"\\{server}\{share}\"), root.next().unwrap_or(""))
        };
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('\\') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    let joined = format!("{prefix}{}", parts.join("\\"));
    // A UNC root is written without a trailing separator, as `\\server\share`.
    Some(match joined.strip_suffix('\\') {
        Some(root) if !prefix.ends_with(r":\") => root.to_string(),
        _ => joined,
    })
}

/// A path folded for a comparison without regard to case: each character to its one-character
/// upper case, as Windows' upcase table maps one UTF-16 unit to one, so that `ß` is never `SS`. A
/// character whose upper case is more than one character is kept as it is.
fn fold(s: &str) -> String {
    s.chars()
        .map(|c| {
            let mut up = c.to_uppercase();
            match (up.next(), up.next()) {
                (Some(u), None) => u,
                _ => c,
            }
        })
        .collect()
}

/// Whether both paths are fully qualified and name the same file as Windows compares them.
fn same_file(a: &str, b: &str) -> bool {
    match (normalize_path(a), normalize_path(b)) {
        (Some(a), Some(b)) => fold(&a) == fold(&b),
        _ => false,
    }
}

/// Whether the path names a `.cmd` or `.bat`, which Windows runs only through cmd.
fn is_batch(path: &str) -> bool {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"))
}

/// Rules 1 to 3: the program is `ssh`, then exactly `-N`, `-o`, `ExitOnForwardFailure=yes` and the
/// alias, each compared exactly.
fn matches_directly(line: &str, ssh: &str, alias: &str) -> bool {
    let t = split_tokens(line);
    t.len() == 5
        && same_file(&t[0].text, ssh)
        && t[1].text == "-N"
        && t[2].text == "-o"
        && t[3].text == "ExitOnForwardFailure=yes"
        && t[4].text == alias
}

/// The text after `/c`, read as cmd reads it (rule 4): when it starts with a `"`, that quote and
/// the last `"` in the text are removed, except when the text holds exactly two quotes with at
/// least one blank and none of `&<>()@^|` between them, when both are kept.
fn as_cmd_reads(after_c: &str) -> String {
    let text = after_c.trim_start_matches(is_blank);
    if !text.starts_with('"') {
        return text.to_string();
    }
    let last = text.rfind('"').unwrap_or(0);
    let two_quotes = text.matches('"').count() == 2;
    let between = &text[1..last.max(1)];
    if two_quotes
        && between.contains(is_blank)
        && !between.contains(['&', '<', '>', '(', ')', '@', '^', '|'])
    {
        return text.to_string();
    }
    if last == 0 {
        text[1..].to_string()
    } else {
        format!("{}{}", &text[1..last], &text[last + 1..])
    }
}

/// Rule 4: `<System32>\cmd.exe` as the program, then optionally `/d`, then `/c`, each switch
/// compared without regard to case on its raw text, then the rest as cmd reads it.
fn matches_under_cmd(line: &str, system32_cmd: &str, ssh: &str, alias: &str) -> bool {
    let t = split_tokens(line);
    if !t
        .first()
        .is_some_and(|program| same_file(&program.text, system32_cmd))
    {
        return false;
    }
    let raw = |i: usize| t.get(i).map(|tok| &line[tok.start..tok.end]);
    let mut at = 1;
    if raw(at).is_some_and(|s| s.eq_ignore_ascii_case("/d")) {
        at += 1;
    }
    if !raw(at).is_some_and(|s| s.eq_ignore_ascii_case("/c")) {
        return false;
    }
    matches_directly(&as_cmd_reads(&line[t[at].end..]), ssh, alias)
}

/// The holder command and its matcher, for this machine's `<System32>` folder (keeper.md, The
/// holder command). The record names the holder process itself: `ssh.exe` for an exe, or the
/// `cmd.exe` that runs a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HolderMatcher {
    system32: String,
}

impl HolderMatcher {
    /// A matcher for `<System32>`, the Windows system folder as Windows reports it. A folder that
    /// is not fully qualified gives a matcher that matches no `.cmd` under cmd, and whose
    /// `self_check` fails.
    pub fn new(system32: impl AsRef<Path>) -> HolderMatcher {
        HolderMatcher {
            system32: system32.as_ref().to_string_lossy().into_owned(),
        }
    }

    /// `<System32>\cmd.exe`.
    pub fn cmd_exe(&self) -> PathBuf {
        PathBuf::from(self.cmd_path())
    }

    fn cmd_path(&self) -> String {
        format!("{}\\cmd.exe", self.system32.trim_end_matches(['\\', '/']))
    }

    /// The program run for this `-SshExe`: `<SshExe>` for an exe, or `<System32>\cmd.exe` for a
    /// `.cmd` or `.bat`. It is the launch request's `application`.
    pub fn program(&self, ssh_exe: &Path) -> PathBuf {
        if is_batch(&ssh_exe.to_string_lossy()) {
            self.cmd_exe()
        } else {
            ssh_exe.to_path_buf()
        }
    }
}

impl Matcher for HolderMatcher {
    /// For an exe, `"<SshExe>" -N -o ExitOnForwardFailure=yes <alias>`; for a `.cmd` or `.bat`,
    /// `"<System32>\cmd.exe" /d /c ""<SshExe>" -N -o ExitOnForwardFailure=yes <alias>"`.
    fn holder_command(&self, ssh_exe: &Path, alias: &str) -> String {
        let ssh = ssh_exe.to_string_lossy();
        let direct = format!("\"{ssh}\" {ARGS} {alias}");
        if is_batch(&ssh) {
            format!("\"{}\" /d /c \"{direct}\"", self.cmd_path())
        } else {
            direct
        }
    }

    /// Recognizes the holder command structurally, never byte for byte (keeper.md, The holder
    /// command, rules 1 to 5). A `-SshExe` that is not fully qualified matches nothing.
    fn is_holder(&self, command_line: &str, ssh_exe: &Path, alias: &str) -> bool {
        let ssh = ssh_exe.to_string_lossy();
        if normalize_path(&ssh).is_none() {
            return false;
        }
        matches_directly(command_line, &ssh, alias)
            || (is_batch(&ssh) && matches_under_cmd(command_line, &self.cmd_path(), &ssh, alias))
    }

    /// Runs the matcher against its own cases (keeper.md, The holder command): each row of its
    /// table and the splitter's three. The error names the first case answered wrongly.
    fn self_check(&self) -> Result<(), String> {
        check_cases(self, &self.system32)
    }
}

const ALIAS: &str = "agentspace-demo-0a1b2c3d";
const SSH: &str = r"C:\Tools\OpenSSH\ssh.exe";
const SSH_SPACES: &str = r"C:\Program Files\OpenSSH\ssh.exe";
const STUB: &str = r"C:\Tools\stub.cmd";
const STUB_SPACES: &str = r"C:\My Tools\stub.cmd";

/// One case of the self-check: its name, the command line, the `-SshExe` and the answer.
struct Case {
    name: &'static str,
    line: String,
    ssh: &'static str,
    alias: &'static str,
    matches: bool,
}

fn cases(m: &dyn Matcher, system32: &str) -> Vec<Case> {
    let cmd = format!("{}\\cmd.exe", system32.trim_end_matches(['\\', '/']));
    let other_cmd = format!("{}\\other\\cmd.exe", system32.trim_end_matches(['\\', '/']));
    let args = "-N -o ExitOnForwardFailure=yes";
    let mut v: Vec<Case> = Vec::new();
    let mut add = |name: &'static str,
                   line: String,
                   ssh: &'static str,
                   alias: &'static str,
                   matches: bool| {
        v.push(Case {
            name,
            line,
            ssh,
            alias,
            matches,
        });
    };
    add(
        "The keeper's own form for an exe",
        m.holder_command(Path::new(SSH), ALIAS),
        SSH,
        ALIAS,
        true,
    );
    add(
        "The keeper's own form for a .cmd",
        m.holder_command(Path::new(STUB), ALIAS),
        STUB,
        ALIAS,
        true,
    );
    add(
        "The keeper's own form for a .cmd with spaces in its path",
        m.holder_command(Path::new(STUB_SPACES), ALIAS),
        STUB_SPACES,
        ALIAS,
        true,
    );
    add(
        "An exe, quoted",
        format!("\"{SSH}\" {args} {ALIAS}"),
        SSH,
        ALIAS,
        true,
    );
    add(
        "An exe unquoted, in another case, with extra blanks and a tab, and a blank at the end",
        format!(
            "{}  -N\t-o   ExitOnForwardFailure=yes \t {ALIAS} ",
            SSH.to_uppercase()
        ),
        SSH,
        ALIAS,
        true,
    );
    add(
        "An exe with forward slashes",
        format!("{} {args} {ALIAS}", SSH.replace('\\', "/")),
        SSH,
        ALIAS,
        true,
    );
    add(
        "A .cmd under cmd with /c and no /d",
        format!("{cmd} /c \"\"{STUB}\" {args} {ALIAS}\""),
        STUB,
        ALIAS,
        true,
    );
    add(
        "A .cmd under CMD /D /C, in capitals",
        format!("{} /D /C \"\"{STUB}\" {args} {ALIAS}\"", cmd.to_uppercase()),
        STUB,
        ALIAS,
        true,
    );
    add(
        "A .cmd with spaces under cmd, two quotes kept",
        format!("{cmd} /c \"{STUB_SPACES}\" {args} {ALIAS}"),
        STUB_SPACES,
        ALIAS,
        true,
    );
    add(
        "An exe with spaces, quoted",
        format!("\"{SSH_SPACES}\" {args} {ALIAS}"),
        SSH_SPACES,
        ALIAS,
        true,
    );
    add(
        "An exe with spaces, unquoted",
        format!("{SSH_SPACES} {args} {ALIAS}"),
        SSH_SPACES,
        ALIAS,
        false,
    );
    add(
        "Another alias",
        format!("\"{SSH}\" {args} agentspace-demo-ffffffff"),
        SSH,
        ALIAS,
        false,
    );
    add(
        "A longer alias",
        format!("\"{SSH}\" {args} {ALIAS}x"),
        SSH,
        ALIAS,
        false,
    );
    add(
        "The alias in another case",
        format!("\"{SSH}\" {args} {}", ALIAS.to_uppercase()),
        SSH,
        ALIAS,
        false,
    );
    add(
        "The alias quoted with more text inside the quotes",
        format!("\"{SSH}\" {args} \"{ALIAS} -v\""),
        SSH,
        ALIAS,
        false,
    );
    add(
        "An extra argument after the alias",
        format!("\"{SSH}\" {args} {ALIAS} -v"),
        SSH,
        ALIAS,
        false,
    );
    add(
        "An extra argument before -N",
        format!("\"{SSH}\" -v {args} {ALIAS}"),
        SSH,
        ALIAS,
        false,
    );
    add(
        "-n for -N",
        format!("\"{SSH}\" -n -o ExitOnForwardFailure=yes {ALIAS}"),
        SSH,
        ALIAS,
        false,
    );
    add(
        "-o with its value missing",
        format!("\"{SSH}\" -N -o {ALIAS}"),
        SSH,
        ALIAS,
        false,
    );
    add(
        "Another program with the same arguments",
        format!(r#""C:\Tools\OpenSSH\scp.exe" {args} {ALIAS}"#),
        SSH,
        ALIAS,
        false,
    );
    add(
        "An exe under cmd /c",
        format!("{cmd} /c \"\"{SSH}\" {args} {ALIAS}\""),
        SSH,
        ALIAS,
        false,
    );
    add(
        "A .cmd under a cmd.exe that is not System32's",
        format!("{other_cmd} /c \"\"{STUB}\" {args} {ALIAS}\""),
        STUB,
        ALIAS,
        false,
    );
    add(
        "A .cmd under cmd /k",
        format!("{cmd} /k \"\"{STUB}\" {args} {ALIAS}\""),
        STUB,
        ALIAS,
        false,
    );
    add(
        "A .cmd under cmd /s /c",
        format!("{cmd} /s /c \"\"{STUB}\" {args} {ALIAS}\""),
        STUB,
        ALIAS,
        false,
    );
    add(
        "A .cmd under cmd with another alias",
        format!("{cmd} /c \"\"{STUB}\" {args} agentspace-demo-ffffffff\""),
        STUB,
        ALIAS,
        false,
    );
    add(
        "A .cmd under cmd with an extra argument inside the quotes",
        format!("{cmd} /c \"\"{STUB}\" {args} {ALIAS} -v\""),
        STUB,
        ALIAS,
        false,
    );
    add(
        "A .cmd under cmd with & calc after the quotes",
        format!("{cmd} /c \"\"{STUB}\" {args} {ALIAS}\" & calc"),
        STUB,
        ALIAS,
        false,
    );
    add(
        "A .cmd run directly",
        format!("\"{STUB}\" {args} {ALIAS}"),
        STUB,
        ALIAS,
        true,
    );
    add("An empty line", String::new(), SSH, ALIAS, false);
    add(
        "A relative -SshExe",
        format!("ssh.exe {args} {ALIAS}"),
        "ssh.exe",
        ALIAS,
        false,
    );
    v
}

/// The splitter's own cases (keeper.md, The holder command): the command line and the tokens it
/// gives.
fn splitter_cases() -> [(&'static str, &'static [&'static str]); 3] {
    [
        (
            r#"p "a b" c\\d e\"f g\\"h i""#,
            &["p", "a b", r"c\\d", "e\"f", r"g\h i"],
        ),
        (r#"p """a""""#, &["p", "\"a\""]),
        (r#""C:\x\"y  z "#, &[r"C:\x\", "y", "z"]),
    ]
}

/// Runs the matcher `m` against every case. Used by `HolderMatcher::self_check`, and by the tests
/// with a matcher that is wrong.
pub(crate) fn check_cases(m: &dyn Matcher, system32: &str) -> Result<(), String> {
    if normalize_path(system32).is_none() {
        return Err("The System32 folder is not a full path.".to_string());
    }
    for c in cases(m, system32) {
        if m.is_holder(&c.line, Path::new(c.ssh), c.alias) != c.matches {
            return Err(format!(
                "The holder matcher answers wrongly for: {}.",
                c.name
            ));
        }
    }
    for (line, want) in splitter_cases() {
        let got: Vec<String> = split_tokens(line).into_iter().map(|t| t.text).collect();
        if got != want {
            return Err(format!(
                "The command line splitter answers wrongly for: {line}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYS: &str = r"C:\Windows\System32";

    fn matcher() -> HolderMatcher {
        HolderMatcher::new(SYS)
    }

    fn texts(line: &str) -> Vec<String> {
        split_tokens(line).into_iter().map(|t| t.text).collect()
    }

    #[test]
    fn the_self_check_passes() {
        assert_eq!(matcher().self_check(), Ok(()));
        assert_eq!(HolderMatcher::new(r"D:\Win\System32\").self_check(), Ok(()));
        assert_eq!(HolderMatcher::new("D:/Win/System32").self_check(), Ok(()));
    }

    #[test]
    fn a_system32_folder_that_is_not_full_fails_the_self_check() {
        assert!(HolderMatcher::new("System32").self_check().is_err());
        assert!(HolderMatcher::new("").self_check().is_err());
    }

    /// Wraps the matcher and answers one thing wrongly.
    struct Wrong<'a> {
        inner: &'a HolderMatcher,
        flip: &'a dyn Fn(&str) -> bool,
    }

    impl Matcher for Wrong<'_> {
        fn holder_command(&self, ssh_exe: &Path, alias: &str) -> String {
            self.inner.holder_command(ssh_exe, alias)
        }

        fn is_holder(&self, line: &str, ssh_exe: &Path, alias: &str) -> bool {
            let right = self.inner.is_holder(line, ssh_exe, alias);
            if (self.flip)(line) { !right } else { right }
        }

        fn self_check(&self) -> Result<(), String> {
            check_cases(self, SYS)
        }
    }

    #[test]
    fn the_self_check_catches_a_matcher_that_answers_any_case_wrongly() {
        let m = matcher();
        let always = |_: &str| true;
        assert!(
            Wrong {
                inner: &m,
                flip: &always
            }
            .self_check()
            .is_err()
        );
        // Each kind of wrong answer, found by one line that holds the mark.
        for mark in [
            "calc", " /k ", "/s /c", "ffffffff", "-n ", "scp.exe", " -v", "other", " /C ",
        ] {
            let flip = |l: &str| l.contains(mark);
            let err = Wrong {
                inner: &m,
                flip: &flip,
            }
            .self_check();
            assert!(err.is_err(), "mark {mark}");
        }
        let never = |l: &str| !l.is_empty();
        assert!(
            Wrong {
                inner: &m,
                flip: &never
            }
            .self_check()
            .is_err()
        );
    }

    #[test]
    fn the_self_check_names_the_case_it_failed() {
        let m = matcher();
        let flip = |l: &str| l.contains("calc");
        let err = Wrong {
            inner: &m,
            flip: &flip,
        }
        .self_check()
        .unwrap_err();
        assert!(err.contains("& calc"), "{err}");
        assert!(err.starts_with(char::is_uppercase));
    }

    #[test]
    fn the_splitters_own_cases() {
        assert_eq!(
            texts(r#"p "a b" c\\d e\"f g\\"h i""#),
            ["p", "a b", r"c\\d", "e\"f", r"g\h i"]
        );
        assert_eq!(texts(r#"p """a""""#), ["p", "\"a\""]);
        assert_eq!(texts(r#""C:\x\"y  z "#), [r"C:\x\", "y", "z"]);
    }

    #[test]
    fn the_splitter_follows_the_quote_run_rules() {
        // Inside quoting, "" gives one literal quote and ends the quoting; """ keeps it.
        assert_eq!(texts(r#"p "a""b c"#), ["p", "a\"b", "c"]);
        assert_eq!(texts(r#"p "a"""b c"#), ["p", "a\"b c"]);
        // Outside quoting, "" is an empty quoted text and leaves quoting off.
        assert_eq!(texts(r#"p "" x"#), ["p", "", "x"]);
        assert_eq!(texts(r#"p a""b c"#), ["p", "ab", "c"]);
        // Backslashes before anything else are literal.
        assert_eq!(texts(r"p a\b \\c"), ["p", r"a\b", r"\\c"]);
        // The program runs to the closing quote, or to the end, with no escapes.
        assert_eq!(texts(r#""a b" c"#), ["a b", "c"]);
        assert_eq!(texts(r#""a b"#), ["a b"]);
        assert_eq!(texts("a\tb  c "), ["a", "b", "c"]);
        // An empty line gives no tokens (architect, 2026-11-19, for E0.06-T01-S07).
        assert!(texts("").is_empty());
        assert!(split_command_line("").is_empty());
        assert_eq!(texts(" a"), ["", "a"]);
        assert_eq!(texts(" "), [""]);
    }

    #[test]
    fn the_matcher_answers_an_empty_line_without_a_token_as_no() {
        let m = matcher();
        for ssh in [SSH, STUB] {
            assert!(!m.is_holder("", Path::new(ssh), ALIAS));
            assert!(!matches_under_cmd("", &m.cmd_path(), ssh, ALIAS));
        }
    }

    #[test]
    fn tokens_carry_their_raw_text() {
        let line = r#"C:\a /D "/c" rest"#;
        let t = split_tokens(line);
        let raw: Vec<&str> = t.iter().map(|k| &line[k.start..k.end]).collect();
        assert_eq!(raw, [r"C:\a", "/D", "\"/c\"", "rest"]);
        let line = "\"é\" ü";
        let t = split_tokens(line);
        assert_eq!(&line[t[1].start..t[1].end], "ü");
    }

    #[test]
    fn paths_are_normalized_as_windows_compares_them() {
        let n = normalize_path;
        assert_eq!(n(r"C:\a\b").as_deref(), Some(r"C:\a\b"));
        assert_eq!(n("C:/a//b/./c/../d/").as_deref(), Some(r"C:\a\b\d"));
        assert_eq!(n(r"C:\..\..\a").as_deref(), Some(r"C:\a"));
        assert_eq!(n(r"C:\").as_deref(), Some(r"C:\"));
        assert_eq!(n(r"\\?\C:\a").as_deref(), Some(r"C:\a"));
        assert_eq!(n(r"\\srv\share\a\..\b").as_deref(), Some(r"\\srv\share\b"));
        assert_eq!(n(r"\\srv\share\..\..\b").as_deref(), Some(r"\\srv\share\b"));
        assert_eq!(n(r"\\?\UNC\srv\share\a").as_deref(), Some(r"\\srv\share\a"));
        for bad in [
            "",
            "ssh.exe",
            r"..\ssh.exe",
            r"\ssh.exe",
            "C:ssh.exe",
            "C:",
            r"\\srv",
            r"\\",
            r"\\\x",
            r"\\srv\\x",
            r"\\.\C:\x",
            r"\\srv\.\x",
            r"\\..\share\x",
            r"\\?\Volume{0}\x",
        ] {
            assert_eq!(n(bad), None, "{bad}");
        }
        assert_eq!(n(r"\\srv\share").as_deref(), Some(r"\\srv\share"));
        assert_eq!(n(r"\\srv\share\").as_deref(), Some(r"\\srv\share"));
        assert_eq!(n(r"\\srv\share\a\..").as_deref(), Some(r"\\srv\share"));
        assert!(same_file(r"C:\A\B.EXE", "c:/a/./x/../b.exe"));
        assert!(same_file(r"\\?\C:\a\b.exe", r"C:\A\B.exe"));
        assert!(same_file(r"\\?\UNC\Srv\Share\b.exe", r"\\srv\share\B.EXE"));
        assert!(!same_file(r"C:\a\b.exe", r"C:\a\c.exe"));
        assert!(!same_file("ssh.exe", "ssh.exe"));
        assert!(!same_file(r"C:\a", r"D:\a"));
        // Only separators, `.`, `..` and case: a trailing dot or space stays (architect,
        // 2026-11-19, for E0.06-T01-S07).
        assert!(!same_file(r"C:\a\ssh.exe.", r"C:\a\ssh.exe"));
        assert!(!same_file(r"C:\a \ssh.exe", r"C:\a\ssh.exe"));
        assert!(!same_file(
            r"C:\PROGRA~1\ssh.exe",
            r"C:\Program Files\ssh.exe"
        ));
        // Case is folded one character to one, never `ß` to `SS`.
        assert!(same_file(r"C:\Ünï\ssh.exe", r"c:\üNÏ\SSH.EXE"));
        assert!(!same_file(r"C:\straße\ssh.exe", r"C:\STRASSE\ssh.exe"));
    }

    #[test]
    fn the_holder_command_for_an_exe_and_for_a_batch_file() {
        let m = matcher();
        assert_eq!(
            m.holder_command(Path::new(SSH), ALIAS),
            format!("\"{SSH}\" -N -o ExitOnForwardFailure=yes {ALIAS}")
        );
        assert_eq!(
            m.holder_command(Path::new(STUB), ALIAS),
            format!(
                "\"{SYS}\\cmd.exe\" /d /c \"\"{STUB}\" -N -o ExitOnForwardFailure=yes {ALIAS}\""
            )
        );
        assert_eq!(
            m.holder_command(Path::new(r"C:\x\STUB.BAT"), ALIAS),
            format!(
                "\"{SYS}\\cmd.exe\" /d /c \"\"C:\\x\\STUB.BAT\" -N -o ExitOnForwardFailure=yes {ALIAS}\""
            )
        );
    }

    #[test]
    fn the_program_run_is_the_exe_or_cmd() {
        let m = HolderMatcher::new(r"C:\Windows\System32\");
        assert_eq!(m.program(Path::new(SSH)), Path::new(SSH));
        assert_eq!(
            m.program(Path::new(STUB)),
            Path::new(r"C:\Windows\System32\cmd.exe")
        );
        assert_eq!(m.cmd_exe(), Path::new(r"C:\Windows\System32\cmd.exe"));
    }

    #[test]
    fn its_own_command_is_always_recognized() {
        let m = matcher();
        for ssh in [
            SSH,
            SSH_SPACES,
            STUB,
            STUB_SPACES,
            r"C:\a b\c d\x.BAT",
            r"\\srv\share\s s\ssh.exe",
        ] {
            let line = m.holder_command(Path::new(ssh), ALIAS);
            assert!(m.is_holder(&line, Path::new(ssh), ALIAS), "{line}");
            assert!(!m.is_holder(&line, Path::new(ssh), "agentspace-demo-ffffffff"));
        }
    }

    fn holder(line: &str, ssh: &str) -> bool {
        matcher().is_holder(line, Path::new(ssh), ALIAS)
    }

    #[test]
    fn a_line_that_is_the_command_by_another_spelling_matches() {
        let a = "-N -o ExitOnForwardFailure=yes";
        assert!(holder(&format!("\"{SSH}\" {a} {ALIAS}"), SSH));
        assert!(holder(&format!("{SSH} {a} {ALIAS}"), SSH));
        assert!(holder(
            &format!(
                "c:/tools/./openssh/../OPENSSH/Ssh.Exe  -N \t -o\tExitOnForwardFailure=yes {ALIAS} "
            ),
            SSH
        ));
        assert!(holder(&format!("\"{SSH}\" {a} \"{ALIAS}\""), SSH));
        // The ssh program with a different spelling of the same file, for a -SshExe too.
        assert!(holder(
            &format!("\"{SSH}\" {a} {ALIAS}"),
            "c:/tools/openssh/ssh.exe"
        ));
        let cmd = format!("{SYS}\\cmd.exe");
        assert!(holder(
            &format!("\"{cmd}\" /c \"\"{STUB}\" {a} {ALIAS}\""),
            STUB
        ));
        assert!(holder(&format!("{cmd} /d /c \"{STUB}\" {a} {ALIAS}"), STUB));
        assert!(holder(
            &format!("{cmd} /C /D \"{STUB}\" {a} {ALIAS}").replace("/C /D", "/D /C"),
            STUB
        ));
        assert!(holder(
            &format!("{cmd} /c   \"\"{STUB}\" {a} {ALIAS}\""),
            STUB
        ));
        assert!(holder(&format!("{cmd} /c {STUB} {a} {ALIAS}"), STUB));
        assert!(holder(&format!("{cmd} /c \"{STUB}\" {a} {ALIAS}"), STUB));
        assert!(holder(
            &format!(
                "{} /c \"\"{STUB}\" {a} {ALIAS}\"",
                SYS.to_lowercase() + "\\CMD.EXE"
            ),
            STUB
        ));
    }

    #[test]
    fn a_line_that_is_not_the_command_does_not_match() {
        let a = "-N -o ExitOnForwardFailure=yes";
        let cmd = format!("{SYS}\\cmd.exe");
        for (line, ssh) in [
            (format!("\"{SSH}\" {a}"), SSH),
            (format!("\"{SSH}\" {a} {ALIAS}\\"), SSH),
            (
                format!("\"{SSH}\" -N -o exitonforwardfailure=yes {ALIAS}"),
                SSH,
            ),
            (
                format!("\"{SSH}\" -N -oExitOnForwardFailure=yes {ALIAS}"),
                SSH,
            ),
            (
                format!("\"{SSH}\" -o ExitOnForwardFailure=yes -N {ALIAS}"),
                SSH,
            ),
            (
                format!("\"{SSH}\" {a} {ALIAS}"),
                r"C:\Tools\OpenSSH\other.exe",
            ),
            (format!("\"{SSH}\" {a} {ALIAS}"), "ssh.exe"),
            (format!("\"{SSH}\" {a} {ALIAS}"), r"\ssh.exe"),
            (format!("\"{SSH}\" {a} {ALIAS}"), ""),
            (format!("\"{SSH}x\" {a} {ALIAS}"), SSH),
            (format!(" \"{SSH}\" {a} {ALIAS}"), SSH),
            (format!("{cmd} /c \"\"{SSH}\" {a} {ALIAS}\""), SSH),
            (format!("{cmd} \"\"{STUB}\" {a} {ALIAS}\""), STUB),
            (format!("{cmd} /d \"\"{STUB}\" {a} {ALIAS}\""), STUB),
            (format!("{cmd} /c /d \"\"{STUB}\" {a} {ALIAS}\""), STUB),
            (format!("{cmd} /c"), STUB),
            (format!("{cmd} /c \"\""), STUB),
            (format!("{cmd} /c \"{STUB}\" {a} {ALIAS} \"&\" calc"), STUB),
            (format!("{cmd} /c \"\"{STUB}\" {a} {ALIAS}\" | more"), STUB),
            (format!("{cmd}x /c \"\"{STUB}\" {a} {ALIAS}\""), STUB),
            (format!("cmd.exe /c \"\"{STUB}\" {a} {ALIAS}\""), STUB),
            (
                format!("{cmd} /c \"\"{STUB}\" {a} {ALIAS}\" "),
                r"C:\Tools\other.cmd",
            ),
        ] {
            assert!(!holder(&line, ssh), "{line} for {ssh}");
        }
    }

    #[test]
    fn what_cmd_reads_follows_rule_4() {
        assert_eq!(as_cmd_reads("  \"\"a\" b\""), "\"a\" b");
        assert_eq!(as_cmd_reads("\"a b\" c"), "\"a b\" c");
        assert_eq!(as_cmd_reads("\"a\" b"), "a b");
        assert_eq!(as_cmd_reads("\"a b & c\" d"), "a b & c d");
        assert_eq!(as_cmd_reads("\"a b\" d \"x"), "a b\" d x");
        assert_eq!(as_cmd_reads("a b"), "a b");
        assert_eq!(as_cmd_reads("\"a"), "a");
        assert_eq!(as_cmd_reads("\""), "");
        assert_eq!(as_cmd_reads(""), "");
    }
}
