//! Reading a process's command line: its tokens, an ssh client's destination, a proxy client's alias.
//!
//! RFC 2119 keywords; the capitals are load-bearing. The rules are docs/driver.md, "Which processes
//! are the alias's clients" and "An ssh client's destination", and docs/keeper.md, "The holder
//! command", rule 1. Everything here is pure Rust, so it is tested on every platform, and nothing
//! here panics on any input.

/// Option letters of `ssh` that take a value. Letters are compared with their case.
const VALUE_LETTERS: &str = "BbcDEeFIiJLlmOoPpQRSWw";

/// Whether a character separates tokens outside quotes: a space or a tab.
fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// Splits a command line into tokens as Windows' `CommandLineToArgvW` does.
///
/// The first token, the program, runs to the next `"` when the line starts with one (or to the end
/// when there is none), and otherwise to the next space or tab, with no escapes. Every later token
/// follows the argument rules: a space or tab outside quotes ends it; 2n backslashes before a `"`
/// give n backslashes and the `"` opens or closes quoting; 2n+1 give n backslashes and a literal
/// `"`; backslashes before anything else are literal. Quotes that follow one another directly are
/// read as one run, by a count (keeper.md, The holder command, rule 1). An empty line gives no
/// tokens; a line that starts with a blank gives an empty program, as `CommandLineToArgvW` does.
#[must_use]
pub fn split_command_line(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut tokens = Vec::new();

    if n == 0 {
        return tokens;
    }

    // The program: no escapes.
    let mut program = String::new();
    if chars[i] == '"' {
        i += 1;
        while i < n && chars[i] != '"' {
            program.push(chars[i]);
            i += 1;
        }
        if i < n {
            i += 1; // The closing quote.
        }
    } else {
        while i < n && !is_blank(chars[i]) {
            program.push(chars[i]);
            i += 1;
        }
    }
    tokens.push(program);

    // The arguments.
    loop {
        while i < n && is_blank(chars[i]) {
            i += 1;
        }
        if i >= n {
            break;
        }
        let mut token = String::new();
        let mut quoting = false;
        while i < n {
            let c = chars[i];
            if c == '\\' {
                let mut backslashes = 0;
                while i < n && chars[i] == '\\' {
                    backslashes += 1;
                    i += 1;
                }
                if i < n && chars[i] == '"' {
                    token.extend(std::iter::repeat_n('\\', backslashes / 2));
                    if backslashes % 2 == 1 {
                        token.push('"');
                        i += 1;
                    }
                    // With an even count the quote is left for the run below.
                } else {
                    token.extend(std::iter::repeat_n('\\', backslashes));
                }
            } else if c == '"' {
                // A run of quotes that follow one another directly, read by a count.
                let mut count = u8::from(quoting);
                while i < n && chars[i] == '"' {
                    i += 1;
                    count += 1;
                    if count == 3 {
                        token.push('"');
                        count = 0;
                    }
                }
                quoting = count == 1;
            } else if is_blank(c) && !quoting {
                break;
            } else {
                token.push(c);
                i += 1;
            }
        }
        tokens.push(token);
    }
    tokens
}

/// The host of an `ssh` client's destination, read as OpenSSH's `ssh` reads its arguments.
///
/// `args` is the whole token vector, with the program as `args[0]`, which is skipped. Returns `None`
/// for a line with no destination or no host (driver.md, An ssh client's destination, rules 1 to 6).
/// The host is the destination's host alone: `ubuntu@<a>` and `ssh://ubuntu@<a>:22` give `<a>`.
#[must_use]
pub fn ssh_destination(args: &[String]) -> Option<String> {
    let mut i = 1;
    let destination: &str = loop {
        let token = args.get(i)?;
        if token == "--" {
            // Rule 3: the token after it is the destination.
            break args.get(i + 1)?;
        }
        if token.len() > 1 && token.starts_with('-') {
            // Rule 2: a run of option letters, read from left to right.
            let letters = &token[1..];
            for (offset, letter) in letters.char_indices() {
                if letter.is_ascii() && VALUE_LETTERS.contains(letter) {
                    if offset + letter.len_utf8() < letters.len() {
                        // The rest of the token is the value.
                    } else {
                        // The whole next token is the value, whatever it holds.
                        args.get(i + 1)?;
                        i += 1;
                    }
                    break;
                }
                // Any other letter, listed or not, takes no value.
            }
            i += 1;
            continue;
        }
        // Rule 4: the first token that is neither option letters nor an option's value. A lone `-`
        // is such a token.
        break token;
    };
    host_of(destination)
}

/// The host of a destination: rule 5, then rule 6's "no host".
fn host_of(destination: &str) -> Option<String> {
    let host = if let Some(rest) = destination.strip_prefix("ssh://") {
        let (authority, tail) = match rest.find('/') {
            Some(slash) => (&rest[..slash], &rest[slash..]),
            None => (rest, ""),
        };
        if !tail.is_empty() && tail != "/" {
            return None;
        }
        let hostport = match authority.rfind('@') {
            Some(at) => &authority[at + 1..],
            None => authority,
        };
        if let Some(inside) = hostport.strip_prefix('[') {
            // A bracket that is never closed leaves the line with no host.
            &inside[..inside.find(']')?]
        } else {
            match hostport.find(':') {
                Some(colon) => &hostport[..colon],
                None => hostport,
            }
        }
    } else {
        match destination.rfind('@') {
            Some(at) => &destination[at + 1..],
            None => destination,
        }
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

/// The alias an `agentspace-ssh-proxy.exe` client names: the one token after the program.
///
/// `args` is the whole token vector, with the program as `args[0]`. Returns `None` when no token or
/// more than one follows the program.
#[must_use]
pub fn proxy_alias(args: &[String]) -> Option<String> {
    if args.len() == 2 {
        Some(args[1].clone())
    } else {
        None
    }
}

/// Whether `found` is `alias`, compared whole and without regard to ASCII case.
#[must_use]
pub fn is_alias(found: &str, alias: &str) -> bool {
    found.eq_ignore_ascii_case(alias)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "agentspace-foo-1234abcd";

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    fn dest(line: &str) -> Option<String> {
        ssh_destination(&split_command_line(line))
    }

    /// Whether a line is a client of `alias` by its program name and the rules of this module.
    fn is_client(line: &str, alias: &str) -> bool {
        let args = split_command_line(line);
        let Some(program) = args.first() else {
            return false;
        };
        let name = program
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match name.as_str() {
            "ssh.exe" => ssh_destination(&args).is_some_and(|h| is_alias(&h, alias)),
            "agentspace-ssh-proxy.exe" => proxy_alias(&args).is_some_and(|h| is_alias(&h, alias)),
            _ => false,
        }
    }

    // The splitter.

    #[test]
    fn splitter_documented_cases() {
        assert_eq!(
            split_command_line(r#"p "a b" c\\d e\"f g\\"h i""#),
            v(&["p", "a b", r"c\\d", "e\"f", r"g\h i"])
        );
        assert_eq!(split_command_line(r#"p """a""""#), v(&["p", "\"a\""]));
        assert_eq!(
            split_command_line("\"C:\\x\\\"y  z "),
            v(&["C:\\x\\", "y", "z"])
        );
    }

    #[test]
    fn splitter_an_empty_line_gives_no_tokens_and_a_blank_one_an_empty_program() {
        assert!(split_command_line("").is_empty());
        assert_eq!(split_command_line("   \t "), v(&[""]));
    }

    #[test]
    fn splitter_program_rules() {
        assert_eq!(
            split_command_line(r#""C:\a b\ssh.exe" x"#),
            v(&[r"C:\a b\ssh.exe", "x"])
        );
        // A quoted program with no closing quote runs to the end.
        assert_eq!(
            split_command_line(r#""C:\a b\ssh.exe x"#),
            v(&[r"C:\a b\ssh.exe x"])
        );
        // An unquoted program has no escapes: a quote inside it is literal.
        assert_eq!(split_command_line(r#"a"b c"#), v(&["a\"b", "c"]));
        // Backslashes in the program are literal, even before the closing quote.
        assert_eq!(split_command_line(r#""C:\x\" y"#), v(&[r"C:\x\", "y"]));
        // Blanks, tabs and runs of them separate tokens.
        assert_eq!(split_command_line("p \t a\t\tb  "), v(&["p", "a", "b"]));
        // A line that starts with a blank has an empty program, and its first word is an argument
        // (keeper.md, The holder command, rule 1: the program runs to the next space or tab).
        assert_eq!(
            split_command_line("  p \t a\t\tb  "),
            v(&["", "p", "a", "b"])
        );
        assert_eq!(
            dest(&format!(" ssh.exe -J {A} other")).as_deref(),
            Some("ssh.exe")
        );
    }

    #[test]
    fn splitter_quote_runs_by_count() {
        // Inside quoting: two quotes give one literal quote and end the quoting.
        assert_eq!(split_command_line(r#"p "a""b c""#), v(&["p", "a\"b", "c"]));
        // Inside quoting: three give one literal and keep the quoting.
        assert_eq!(split_command_line(r#"p "a"""b c""#), v(&["p", "a\"b c"]));
        // Outside quoting: two are an empty quoted text.
        assert_eq!(split_command_line(r#"p "" q"#), v(&["p", "", "q"]));
        assert_eq!(split_command_line(r#"p a""b"#), v(&["p", "ab"]));
        // Outside quoting: three give one literal quote and leave quoting off.
        assert_eq!(split_command_line(r#"p """ q"#), v(&["p", "\"", "q"]));
        // A quote opens quoting, and the next closes it.
        assert_eq!(split_command_line(r#"p a" "b"#), v(&["p", "a b"]));
        // An unclosed quote runs to the end.
        assert_eq!(split_command_line(r#"p "a b"#), v(&["p", "a b"]));
    }

    #[test]
    fn splitter_backslashes() {
        assert_eq!(split_command_line(r"p a\b\\c"), v(&["p", r"a\b\\c"]));
        // 2n backslashes before a quote give n and the quote opens quoting.
        assert_eq!(split_command_line(r#"p a\\"b c""#), v(&["p", r"a\b c"]));
        assert_eq!(split_command_line(r#"p a\\\\"b c""#), v(&["p", r"a\\b c"]));
        // 2n+1 give n and a literal quote.
        assert_eq!(split_command_line(r#"p a\"b c"#), v(&["p", "a\"b", "c"]));
        assert_eq!(split_command_line(r#"p a\\\"b"#), v(&["p", "a\\\"b"]));
        // A literal quote inside quoting keeps the quoting.
        assert_eq!(split_command_line(r#"p "a\"b c""#), v(&["p", "a\"b c"]));
        // Trailing backslashes are literal.
        assert_eq!(split_command_line(r"p a\\"), v(&["p", r"a\\"]));
    }

    #[test]
    fn splitter_non_ascii_text_is_kept() {
        assert_eq!(
            split_command_line("p \u{e9}t\u{e9} \"x \u{4e2d}\""),
            v(&["p", "\u{e9}t\u{e9}", "x \u{4e2d}"])
        );
    }

    // The examples table of "An ssh client's destination".

    #[test]
    fn table_rows_that_are_clients() {
        let rows = [
            format!("ssh.exe {A}"),
            format!(
                r#""C:\Windows\System32\OpenSSH\ssh.exe" -T -o ProxyCommand="x y" -o ClearAllForwardings=yes {A}"#
            ),
            format!("ssh.exe -N -o ExitOnForwardFailure=yes {A}"),
            format!("ssh.exe ubuntu@{A}"),
            format!("ssh.exe ssh://ubuntu@{A}:22"),
            format!("ssh.exe -l ubuntu -p 22 {A} uname -a"),
            format!("ssh.exe -s {A} sftp"),
            "ssh.exe -vvv -oPort=22 AGENTSPACE-FOO-1234ABCD".to_string(),
            format!("ssh.exe -- {A}"),
            format!("ssh.exe {A} -v"),
            format!("agentspace-ssh-proxy.exe {A}"),
        ];
        for row in &rows {
            assert!(
                is_client(row, A),
                "Expected a client for a row of {} bytes",
                row.len()
            );
        }
    }

    #[test]
    fn table_rows_that_are_not_clients() {
        let rows = [
            format!("ssh.exe -J {A} other"),
            format!("ssh.exe -F {A} other"),
            format!("ssh.exe -i{A} other"),
            format!("ssh.exe other {A}"),
            format!("ssh.exe other -- {A}"),
            format!("ssh.exe {A}0"),
            "ssh.exe agentspace-foo-1234".to_string(),
            "ssh.exe agentspace-bar-1234abcd".to_string(),
            format!("ssh.exe ssh://{A}/path"),
            "ssh.exe -p".to_string(),
            format!("scp.exe {A}:file ."),
            format!("agentspace-ssh-proxy.exe {A} extra"),
            "agentspace-ssh-proxy.exe".to_string(),
        ];
        for row in &rows {
            assert!(
                !is_client(row, A),
                "Expected no client for a row of {} bytes",
                row.len()
            );
        }
    }

    // The destination's host and its grammar.

    #[test]
    fn destination_is_the_host_not_the_raw_text() {
        assert_eq!(dest(&format!("ssh.exe {A}")).as_deref(), Some(A));
        assert_eq!(dest(&format!("ssh.exe ubuntu@{A}")).as_deref(), Some(A));
        assert_eq!(
            dest(&format!("ssh.exe ssh://ubuntu@{A}:22")).as_deref(),
            Some(A)
        );
        // The text after the last `@` is the host.
        assert_eq!(dest("ssh.exe a@b@c").as_deref(), Some("c"));
    }

    #[test]
    fn destination_no_destination_or_no_host() {
        assert_eq!(dest("ssh.exe"), None);
        assert_eq!(dest(""), None);
        assert_eq!(dest("ssh.exe -N -v"), None);
        assert_eq!(dest("ssh.exe --"), None);
        assert_eq!(dest("ssh.exe -p"), None);
        assert_eq!(dest("ssh.exe user@"), None);
        assert_eq!(dest("ssh.exe \"\""), None);
        assert_eq!(ssh_destination(&[]), None);
        assert_eq!(ssh_destination(&v(&["ssh.exe", "-p"])), None);
    }

    #[test]
    fn destination_skips_only_the_program() {
        // The program is args[0] and is never read as the destination.
        assert_eq!(ssh_destination(&v(&["host"])), None);
        assert_eq!(
            ssh_destination(&v(&["ssh.exe", "host"])).as_deref(),
            Some("host")
        );
    }

    #[test]
    fn destination_every_value_letter_joined_and_apart() {
        for letter in "BbcDEeFIiJLlmOoPpQRSWw".chars() {
            // Apart: the whole next token is the value, a leading `-` and the alias included.
            assert_eq!(
                dest(&format!("ssh.exe -{letter} {A} dest")).as_deref(),
                Some("dest"),
                "Letter {letter} apart"
            );
            assert_eq!(
                dest(&format!("ssh.exe -{letter} -- dest")).as_deref(),
                Some("dest"),
                "Letter {letter} taking `--`"
            );
            assert_eq!(
                dest(&format!("ssh.exe -{letter} -x dest")).as_deref(),
                Some("dest"),
                "Letter {letter} taking `-x`"
            );
            // Joined: the rest of the token is the value.
            assert_eq!(
                dest(&format!("ssh.exe -{letter}{A} dest")).as_deref(),
                Some("dest"),
                "Letter {letter} joined"
            );
            // With nothing after it, the line has no destination.
            assert_eq!(
                dest(&format!("ssh.exe -{letter}")),
                None,
                "Letter {letter} last"
            );
            // After the destination it is never read.
            assert_eq!(
                dest(&format!("ssh.exe dest -{letter}")).as_deref(),
                Some("dest"),
                "Letter {letter} after the destination"
            );
        }
    }

    #[test]
    fn destination_every_flag_letter_takes_no_value() {
        for letter in "1246AaCfGgKkMNnqsTtVvXxYy".chars() {
            assert_eq!(
                dest(&format!("ssh.exe -{letter} dest")).as_deref(),
                Some("dest"),
                "Letter {letter}"
            );
            // A run of the same flag, and the alias read as the destination after it.
            assert_eq!(
                dest(&format!("ssh.exe -{letter}{letter} {A}")).as_deref(),
                Some(A),
                "Run of letter {letter}"
            );
            assert_eq!(
                dest(&format!("ssh.exe -{letter}")),
                None,
                "Letter {letter} alone"
            );
        }
    }

    #[test]
    fn destination_an_unknown_letter_is_a_flag() {
        for letter in "dHhjUuZz37_@".chars() {
            assert_eq!(
                dest(&format!("ssh.exe -{letter} {A}")).as_deref(),
                Some(A),
                "Letter {letter}"
            );
            assert_eq!(
                dest(&format!("ssh.exe -v{letter} {A}")).as_deref(),
                Some(A),
                "Letter {letter} after a flag"
            );
        }
        // A letter that is not ASCII is unknown too.
        assert_eq!(dest("ssh.exe -\u{e9} dest").as_deref(), Some("dest"));
    }

    #[test]
    fn destination_letters_are_compared_with_their_case() {
        // `C` takes no value and `c` does; `q` takes none and `Q` does; `s` takes none and `S` does.
        assert_eq!(dest("ssh.exe -C dest").as_deref(), Some("dest"));
        assert_eq!(dest("ssh.exe -c dest other").as_deref(), Some("other"));
        assert_eq!(dest("ssh.exe -q dest").as_deref(), Some("dest"));
        assert_eq!(dest("ssh.exe -Q dest other").as_deref(), Some("other"));
        assert_eq!(dest("ssh.exe -s dest").as_deref(), Some("dest"));
        assert_eq!(dest("ssh.exe -S dest other").as_deref(), Some("other"));
        assert_eq!(dest("ssh.exe -x dest").as_deref(), Some("dest"));
        assert_eq!(dest("ssh.exe -X dest").as_deref(), Some("dest"));
        // `-P` takes a value, from OpenSSH 9.4 on: `-P <alias>` is a miss on the safe side.
        assert_eq!(dest(&format!("ssh.exe -P {A}")), None);
    }

    #[test]
    fn destination_combined_runs() {
        // A run ending in a value letter takes the next token.
        assert_eq!(dest("ssh.exe -vvp 22 dest").as_deref(), Some("dest"));
        assert_eq!(dest("ssh.exe -4Np 22 dest").as_deref(), Some("dest"));
        // A value letter mid-run takes the rest of the token, so later letters are not read.
        assert_eq!(dest("ssh.exe -vp22 dest").as_deref(), Some("dest"));
        assert_eq!(dest(&format!("ssh.exe -pv {A}")).as_deref(), Some(A));
        assert_eq!(dest(&format!("ssh.exe -oPort=22 {A}")).as_deref(), Some(A));
        assert_eq!(
            dest(&format!("ssh.exe -vvv -oPort=22 {A}")).as_deref(),
            Some(A)
        );
        // A run ending in a value letter with nothing after it has no destination.
        assert_eq!(dest("ssh.exe -vvp"), None);
        assert_eq!(dest("ssh.exe -4Np"), None);
        // A flag after a value letter in the same token is the value's, not read.
        assert_eq!(dest("ssh.exe -iN dest").as_deref(), Some("dest"));
        // Several option tokens before the destination.
        assert_eq!(
            dest(&format!("ssh.exe -T -J jump -o A=b -p 22 -l ubuntu {A} ls")).as_deref(),
            Some(A)
        );
    }

    #[test]
    fn destination_the_alias_as_an_options_value_is_not_the_destination() {
        assert_eq!(
            dest(&format!("ssh.exe -J {A} other")).as_deref(),
            Some("other")
        );
        assert_eq!(
            dest(&format!("ssh.exe -F {A} other")).as_deref(),
            Some("other")
        );
        assert_eq!(
            dest(&format!("ssh.exe -i{A} other")).as_deref(),
            Some("other")
        );
        assert_eq!(
            dest(&format!("ssh.exe -o {A} other")).as_deref(),
            Some("other")
        );
        assert_eq!(dest(&format!("ssh.exe -J {A}")), None);
    }

    #[test]
    fn destination_double_dash() {
        assert_eq!(dest(&format!("ssh.exe -- {A}")).as_deref(), Some(A));
        assert_eq!(dest(&format!("ssh.exe -v -- {A} -x")).as_deref(), Some(A));
        // The token after `--` is the destination whatever it holds, a leading `-` included.
        assert_eq!(dest("ssh.exe -- -v").as_deref(), Some("-v"));
        assert_eq!(
            dest(&format!("ssh.exe other -- {A}")).as_deref(),
            Some("other")
        );
        assert_eq!(dest("ssh.exe --"), None);
        // `--` as the value of a value letter is that letter's value, not the end of options.
        assert_eq!(dest(&format!("ssh.exe -o -- {A}")).as_deref(), Some(A));
    }

    #[test]
    fn destination_a_lone_dash_is_the_destination() {
        assert_eq!(dest("ssh.exe -").as_deref(), Some("-"));
        assert_eq!(dest(&format!("ssh.exe - {A}")).as_deref(), Some("-"));
        assert_eq!(dest(&format!("ssh.exe -v - {A}")).as_deref(), Some("-"));
    }

    #[test]
    fn destination_options_after_it_are_never_compared() {
        assert_eq!(dest(&format!("ssh.exe {A} -v")).as_deref(), Some(A));
        assert_eq!(dest(&format!("ssh.exe {A} -p")).as_deref(), Some(A));
        assert_eq!(
            dest(&format!("ssh.exe other {A}")).as_deref(),
            Some("other")
        );
        assert_eq!(
            dest(&format!("ssh.exe other -- {A}")).as_deref(),
            Some("other")
        );
        assert_eq!(dest(&format!("ssh.exe {A} uname -a")).as_deref(), Some(A));
        assert_eq!(dest(&format!("ssh.exe {A} -J other")).as_deref(), Some(A));
    }

    #[test]
    fn destination_ssh_url_forms() {
        // Compared with its case: only a lowercase `ssh://` is a URL.
        assert_eq!(dest("ssh.exe ssh://host").as_deref(), Some("host"));
        assert_eq!(dest("ssh.exe ssh://host/").as_deref(), Some("host"));
        assert_eq!(dest("ssh.exe ssh://host:22").as_deref(), Some("host"));
        assert_eq!(dest("ssh.exe ssh://host:22/").as_deref(), Some("host"));
        assert_eq!(dest("ssh.exe ssh://user@host").as_deref(), Some("host"));
        assert_eq!(dest("ssh.exe ssh://user@host:22").as_deref(), Some("host"));
        assert_eq!(dest("ssh.exe ssh://user@host:22/").as_deref(), Some("host"));
        // The last `@` ends the user.
        assert_eq!(dest("ssh.exe ssh://a@b@host:22").as_deref(), Some("host"));
        // Brackets: the host is the text inside them.
        assert_eq!(dest("ssh.exe ssh://[::1]").as_deref(), Some("::1"));
        assert_eq!(dest("ssh.exe ssh://[::1]:22").as_deref(), Some("::1"));
        assert_eq!(
            dest("ssh.exe ssh://user@[fe80::1]:22/").as_deref(),
            Some("fe80::1")
        );
        assert_eq!(dest("ssh.exe ssh://[host]").as_deref(), Some("host"));
        assert_eq!(dest("ssh.exe ssh://[a:b]x").as_deref(), Some("a:b"));
        // A bracket never closed, or an empty host, is no host.
        assert_eq!(dest("ssh.exe ssh://[::1"), None);
        assert_eq!(dest("ssh.exe ssh://[]"), None);
        assert_eq!(dest("ssh.exe ssh://"), None);
        assert_eq!(dest("ssh.exe ssh:///"), None);
        assert_eq!(dest("ssh.exe ssh://user@"), None);
        assert_eq!(dest("ssh.exe ssh://:22"), None);
        // Any path but a single trailing `/` is no host.
        assert_eq!(dest("ssh.exe ssh://host/path"), None);
        assert_eq!(dest("ssh.exe ssh://host//"), None);
        assert_eq!(dest("ssh.exe ssh://user@host:22/path"), None);
        assert_eq!(dest("ssh.exe ssh://[::1]/x"), None);
        assert_eq!(dest("ssh.exe ssh://host/?q"), None);
        // The URL's case is compared: `SSH://` is an ordinary destination, whose host follows its
        // last `@` or is the whole text.
        assert_eq!(dest("ssh.exe SSH://host").as_deref(), Some("SSH://host"));
        assert_eq!(dest("ssh.exe SSH://user@host").as_deref(), Some("host"));
        // A `:` and a `[` are plain text outside a URL.
        assert_eq!(dest("ssh.exe host:22").as_deref(), Some("host:22"));
        assert_eq!(dest("ssh.exe [::1]").as_deref(), Some("[::1]"));
    }

    #[test]
    fn destination_the_alias_inside_a_url_with_a_path_is_a_miss() {
        assert!(!is_client(&format!("ssh.exe ssh://{A}/path"), A));
        assert!(is_client(&format!("ssh.exe ssh://{A}/"), A));
        assert!(is_client(&format!("ssh.exe ssh://ubuntu@{A}:22/"), A));
        assert!(is_client(&format!("ssh.exe ssh://[{A}]:22"), A));
    }

    #[test]
    fn destination_reads_quoted_tokens_as_split() {
        // A quoted option value with spaces is one token.
        assert_eq!(
            dest(&format!(r#"ssh.exe -o "ProxyCommand=x y" {A}"#)).as_deref(),
            Some(A)
        );
        // A quoted destination is the same token as an unquoted one.
        assert_eq!(dest(&format!(r#"ssh.exe "{A}""#)).as_deref(), Some(A));
        assert_eq!(
            dest(&format!(r#"ssh.exe "ubuntu@{A}""#)).as_deref(),
            Some(A)
        );
    }

    // The proxy client.

    #[test]
    fn proxy_alias_is_exactly_one_token_after_the_program() {
        assert_eq!(proxy_alias(&v(&["p.exe", A])).as_deref(), Some(A));
        assert_eq!(proxy_alias(&v(&["p.exe"])), None);
        assert_eq!(proxy_alias(&[]), None);
        assert_eq!(proxy_alias(&v(&["p.exe", A, "extra"])), None);
        assert_eq!(proxy_alias(&v(&["p.exe", "", ""])), None);
        // The one token is returned whatever it holds; the comparison is `is_alias`'s.
        assert_eq!(proxy_alias(&v(&["p.exe", "-v"])).as_deref(), Some("-v"));
        assert_eq!(proxy_alias(&v(&["p.exe", ""])).as_deref(), Some(""));
    }

    #[test]
    fn proxy_alias_through_the_splitter() {
        let args = split_command_line(&format!(r#""C:\a b\agentspace-ssh-proxy.exe" {A}"#));
        assert_eq!(proxy_alias(&args).as_deref(), Some(A));
        let args = split_command_line(&format!(r#""C:\a b\agentspace-ssh-proxy.exe" "{A}""#));
        assert_eq!(proxy_alias(&args).as_deref(), Some(A));
        let args = split_command_line(&format!(r#"agentspace-ssh-proxy.exe {A}  "#));
        assert_eq!(proxy_alias(&args).as_deref(), Some(A));
    }

    // The comparison.

    #[test]
    fn alias_is_compared_whole_and_without_regard_to_ascii_case() {
        assert!(is_alias(A, A));
        assert!(is_alias("AGENTSPACE-FOO-1234ABCD", A));
        assert!(is_alias("Agentspace-Foo-1234AbCd", A));
        assert!(!is_alias("agentspace-foo-1234abcd0", A));
        assert!(!is_alias("agentspace-foo-1234", A));
        assert!(!is_alias("xagentspace-foo-1234abcd", A));
        assert!(!is_alias("agentspace-bar-1234abcd", A));
        assert!(!is_alias("", A));
        assert!(!is_alias(A, ""));
        assert!(is_alias("", ""));
        // Only ASCII case is ignored.
        assert!(!is_alias(
            "agentspace-f\u{f6}o-1234abcd",
            "agentspace-f\u{d6}o-1234abcd"
        ));
    }
}
