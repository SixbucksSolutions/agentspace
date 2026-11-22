//! The command line rules of docs/driver.md, "Which processes are the alias's clients" and "An ssh
//! client's destination", through the crate's public functions: a line is split as
//! `CommandLineToArgvW` splits it, then read by the rule of its program.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. These tests are pure: they start no
//! process and read no file.

use agentspace_connections::argv::{is_alias, proxy_alias, split_command_line, ssh_destination};

/// The alias of driver.md's examples table.
const ALIAS: &str = "agentspace-foo-1234abcd";

/// Replaces `<a>` in an example by the alias.
fn fill(text: &str) -> String {
    text.replace("<a>", ALIAS)
}

fn split(line: &str) -> Vec<String> {
    split_command_line(&fill(line))
}

/// Whether `ssh.exe` with this command line is a client of `ALIAS`, by rules 3 and the grammar.
fn ssh_client(line: &str) -> bool {
    let args = split(line);
    ssh_destination(&args).is_some_and(|host| is_alias(&host, ALIAS))
}

/// Whether `agentspace-ssh-proxy.exe` with this command line is a client of `ALIAS`.
fn proxy_client(line: &str) -> bool {
    let args = split(line);
    proxy_alias(&args).is_some_and(|found| is_alias(&found, ALIAS))
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

// ---- driver.md's examples table, one test per row ----------------------------------------------

#[test]
fn row_plain_destination_is_a_client() {
    assert!(ssh_client("ssh.exe <a>"));
}

#[test]
fn row_quoted_program_with_proxy_command_is_a_client() {
    let line = r#""C:\Windows\System32\OpenSSH\ssh.exe" -T -o ProxyCommand="x y" -o ClearAllForwardings=yes <a>"#;
    assert!(ssh_client(line));
    let args = split(line);
    assert_eq!(args[0], r"C:\Windows\System32\OpenSSH\ssh.exe");
    assert_eq!(args[3], "ProxyCommand=x y");
    assert_eq!(ssh_destination(&args), Some(ALIAS.to_string()));
}

#[test]
fn row_keepers_holder_is_a_client() {
    assert!(ssh_client("ssh.exe -N -o ExitOnForwardFailure=yes <a>"));
}

#[test]
fn row_user_at_alias_and_ssh_url_are_clients() {
    assert!(ssh_client("ssh.exe ubuntu@<a>"));
    assert!(ssh_client("ssh.exe ssh://ubuntu@<a>:22"));
    // The host, not the raw destination.
    assert_eq!(
        ssh_destination(&split("ssh.exe ubuntu@<a>")),
        Some(ALIAS.to_string())
    );
    assert_eq!(
        ssh_destination(&split("ssh.exe ssh://ubuntu@<a>:22")),
        Some(ALIAS.to_string())
    );
}

#[test]
fn row_options_with_values_before_the_destination_are_clients() {
    assert!(ssh_client("ssh.exe -l ubuntu -p 22 <a> uname -a"));
    assert!(ssh_client("ssh.exe -s <a> sftp"));
}

#[test]
fn row_upper_case_alias_with_clustered_options_is_a_client() {
    let line = "ssh.exe -vvv -oPort=22 AGENTSPACE-FOO-1234ABCD";
    assert!(ssh_client(line));
    assert_eq!(
        ssh_destination(&split(line)),
        Some("AGENTSPACE-FOO-1234ABCD".to_string())
    );
}

#[test]
fn row_double_dash_ends_the_options() {
    assert!(ssh_client("ssh.exe -- <a>"));
}

#[test]
fn row_option_after_the_destination_is_a_client() {
    assert!(ssh_client("ssh.exe <a> -v"));
}

#[test]
fn row_alias_as_an_options_value_is_not_a_client() {
    assert!(!ssh_client("ssh.exe -J <a> other"));
    assert!(!ssh_client("ssh.exe -F <a> other"));
    assert!(!ssh_client("ssh.exe -i<a> other"));
    // The destination is the token after the value.
    assert_eq!(
        ssh_destination(&split("ssh.exe -J <a> other")),
        Some("other".to_string())
    );
    assert_eq!(
        ssh_destination(&split("ssh.exe -i<a> other")),
        Some("other".to_string())
    );
}

#[test]
fn row_alias_in_the_remote_command_is_not_a_client() {
    assert!(!ssh_client("ssh.exe other <a>"));
    assert!(!ssh_client("ssh.exe other -- <a>"));
    assert_eq!(
        ssh_destination(&split("ssh.exe other <a>")),
        Some("other".to_string())
    );
}

#[test]
fn row_longer_shorter_and_other_aliases_are_not_clients() {
    assert!(!ssh_client("ssh.exe <a>0"));
    assert!(!ssh_client("ssh.exe agentspace-foo-1234"));
    assert!(!ssh_client("ssh.exe agentspace-foo-5678abcd"));
    assert!(!ssh_client("ssh.exe agentspace-bar-1234abcd"));
}

#[test]
fn row_ssh_url_with_a_path_has_no_host() {
    assert!(!ssh_client("ssh.exe ssh://<a>/path"));
    assert_eq!(ssh_destination(&split("ssh.exe ssh://<a>/path")), None);
}

#[test]
fn row_option_needing_a_value_leaves_no_destination() {
    assert!(!ssh_client("ssh.exe -p"));
    assert_eq!(ssh_destination(&split("ssh.exe -p")), None);
}

#[test]
fn row_scp_is_not_an_ssh_exe() {
    // Read by the ssh rule, the line has the alias as a host followed by `:file`, so it is not a
    // match even before the image name is looked at.
    assert!(!ssh_client("scp.exe <a>:file ."));
}

#[test]
fn row_proxy_with_exactly_the_alias_is_a_client() {
    assert!(proxy_client("agentspace-ssh-proxy.exe <a>"));
    assert_eq!(
        proxy_alias(&split("agentspace-ssh-proxy.exe <a>")),
        Some(ALIAS.to_string())
    );
}

#[test]
fn row_proxy_with_extra_tokens_or_none_is_not_a_client() {
    assert!(!proxy_client("agentspace-ssh-proxy.exe <a> extra"));
    assert!(!proxy_client("agentspace-ssh-proxy.exe"));
    assert_eq!(
        proxy_alias(&split("agentspace-ssh-proxy.exe <a> extra")),
        None
    );
    assert_eq!(proxy_alias(&split("agentspace-ssh-proxy.exe")), None);
}

// ---- The alias comparison ----------------------------------------------------------------------

#[test]
fn is_alias_is_whole_and_ignores_ascii_case() {
    assert!(is_alias(ALIAS, ALIAS));
    assert!(is_alias("AGENTSPACE-FOO-1234ABCD", ALIAS));
    assert!(is_alias(ALIAS, "AgentSpace-Foo-1234ABCD"));
    assert!(!is_alias("agentspace-foo-1234abcd0", ALIAS));
    assert!(!is_alias("agentspace-foo-1234abc", ALIAS));
    assert!(!is_alias("agentspace-foo-1234", ALIAS));
    assert!(!is_alias("", ALIAS));
    assert!(!is_alias(" agentspace-foo-1234abcd", ALIAS));
}

#[test]
fn is_alias_does_not_fold_non_ascii_case() {
    // Only ASCII case is ignored: U+212A (Kelvin sign) is not an ASCII letter and is not `k`.
    assert!(!is_alias(
        "agentspace-\u{212a}oo-1234abcd",
        "agentspace-koo-1234abcd"
    ));
}

// ---- The grammar's rules, one by one ----------------------------------------------------------

#[test]
fn a_value_taking_letter_takes_the_rest_of_its_token_or_the_next_token() {
    assert_eq!(
        ssh_destination(&split("ssh.exe -p22 host")),
        Some("host".to_string())
    );
    assert_eq!(
        ssh_destination(&split("ssh.exe -vp22 host")),
        Some("host".to_string())
    );
    assert_eq!(
        ssh_destination(&split("ssh.exe -oPort=22 host")),
        Some("host".to_string())
    );
    // The next token is the value, whatever it holds, a leading dash included.
    assert_eq!(
        ssh_destination(&split("ssh.exe -o -v host")),
        Some("host".to_string())
    );
    assert_eq!(
        ssh_destination(&split("ssh.exe -vo Port=22 host")),
        Some("host".to_string())
    );
}

#[test]
fn every_value_taking_letter_swallows_the_next_token() {
    for letter in "BbcDEeFIiJLlmOoPpQRSWw".chars() {
        let line = format!("ssh.exe -{letter} value host");
        assert_eq!(
            ssh_destination(&split(&line)),
            Some("host".to_string()),
            "Option letter {letter}"
        );
        let line = format!("ssh.exe -{letter}");
        assert_eq!(
            ssh_destination(&split(&line)),
            None,
            "Option letter {letter} with no value"
        );
    }
}

#[test]
fn every_letter_that_takes_no_value_leaves_the_next_token_as_the_destination() {
    for letter in "1246AaCfGgKkMNnqsTtVvXxYy".chars() {
        let line = format!("ssh.exe -{letter} host");
        assert_eq!(
            ssh_destination(&split(&line)),
            Some("host".to_string()),
            "Option letter {letter}"
        );
    }
}

#[test]
fn option_letters_are_compared_with_their_case() {
    // Lower-case `c` takes a value, and upper-case `C` takes none.
    assert_eq!(
        ssh_destination(&split("ssh.exe -C host")),
        Some("host".to_string())
    );
    assert_eq!(
        ssh_destination(&split("ssh.exe -c host other")),
        Some("other".to_string())
    );
    // Lower-case `e` takes a value, and so does upper-case `E`; `d` is in neither list.
    assert_eq!(
        ssh_destination(&split("ssh.exe -d host")),
        Some("host".to_string())
    );
}

#[test]
fn a_letter_in_neither_list_takes_no_value() {
    assert_eq!(
        ssh_destination(&split("ssh.exe -Z host")),
        Some("host".to_string())
    );
    assert_eq!(
        ssh_destination(&split("ssh.exe -zv host")),
        Some("host".to_string())
    );
}

#[test]
fn a_lone_dash_is_a_destination() {
    assert_eq!(
        ssh_destination(&split("ssh.exe - other")),
        Some("-".to_string())
    );
}

#[test]
fn double_dash_ends_the_options_even_for_a_dash_led_destination() {
    assert_eq!(
        ssh_destination(&split("ssh.exe -- -x")),
        Some("-x".to_string())
    );
    assert_eq!(ssh_destination(&split("ssh.exe --")), None);
}

#[test]
fn no_token_after_the_destination_is_compared() {
    assert_eq!(
        ssh_destination(&split("ssh.exe host -J <a> -o x <a>")),
        Some("host".to_string())
    );
}

#[test]
fn a_line_with_no_tokens_after_the_program_has_no_destination() {
    assert_eq!(ssh_destination(&split("ssh.exe")), None);
    assert_eq!(ssh_destination(&strings(&[])), None);
}

#[test]
fn the_program_is_never_read_as_a_destination() {
    assert_eq!(ssh_destination(&strings(&["ssh.exe"])), None);
    assert_eq!(proxy_alias(&strings(&["agentspace-ssh-proxy.exe"])), None);
    assert_eq!(proxy_alias(&strings(&[])), None);
}

#[test]
fn the_host_is_the_text_after_the_last_at_sign() {
    assert_eq!(
        ssh_destination(&split("ssh.exe a@b@host")),
        Some("host".to_string())
    );
    assert_eq!(
        ssh_destination(&split("ssh.exe user@host")),
        Some("host".to_string())
    );
}

#[test]
fn an_ssh_url_gives_its_host_without_user_or_port() {
    assert_eq!(
        ssh_destination(&split("ssh.exe ssh://host")),
        Some("host".to_string())
    );
    assert_eq!(
        ssh_destination(&split("ssh.exe ssh://host:2222")),
        Some("host".to_string())
    );
    assert_eq!(
        ssh_destination(&split("ssh.exe ssh://user@host:2222")),
        Some("host".to_string())
    );
    // A single `/` after the authority is allowed, and nothing more.
    assert_eq!(
        ssh_destination(&split("ssh.exe ssh://host/")),
        Some("host".to_string())
    );
    assert_eq!(ssh_destination(&split("ssh.exe ssh://host//")), None);
    assert_eq!(ssh_destination(&split("ssh.exe ssh://host/x")), None);
    // The user is everything up to the last `@` of the authority.
    assert_eq!(
        ssh_destination(&split("ssh.exe ssh://a@b@host:22")),
        Some("host".to_string())
    );
    // A host in brackets is the text inside them.
    assert_eq!(
        ssh_destination(&split("ssh.exe ssh://user@[::1]:22")),
        Some("::1".to_string())
    );
    assert_eq!(
        ssh_destination(&split("ssh.exe ssh://[::1]")),
        Some("::1".to_string())
    );
}

#[test]
fn the_ssh_scheme_is_compared_with_its_case() {
    // `SSH://` is not the scheme, so the destination is read as a plain one: the host is the text
    // after the last `@`, or the whole destination.
    assert_eq!(
        ssh_destination(&split("ssh.exe SSH://host")),
        Some("SSH://host".to_string())
    );
}

#[test]
fn an_ssh_url_naming_the_alias_is_a_client_by_its_host() {
    assert!(ssh_client("ssh.exe ssh://<a>"));
    assert!(ssh_client("ssh.exe ssh://<a>/"));
    assert!(ssh_client("ssh.exe ssh://u@<a>:22/"));
    assert!(!ssh_client("ssh.exe ssh://u@<a>0:22"));
    assert!(!ssh_client("ssh.exe ssh://<a>/x"));
}

// ---- split_command_line, keeper.md's holder command, rule 1 -----------------------------------

#[test]
fn split_plain_tokens() {
    assert_eq!(
        split_command_line("a.exe x y"),
        strings(&["a.exe", "x", "y"])
    );
    assert_eq!(
        split_command_line("a.exe  x\ty   z "),
        strings(&["a.exe", "x", "y", "z"])
    );
}

#[test]
fn split_program_is_read_without_escapes() {
    assert_eq!(
        split_command_line(r#""C:\Program Files\a b.exe" x"#),
        strings(&[r"C:\Program Files\a b.exe", "x"])
    );
    assert_eq!(
        split_command_line(r"C:\dir\a.exe x"),
        strings(&[r"C:\dir\a.exe", "x"])
    );
    // A program that starts with a quote and has no closing one runs to the end.
    assert_eq!(
        split_command_line(r#""C:\dir\a b.exe"#),
        strings(&[r"C:\dir\a b.exe"])
    );
    // Backslashes at the end of a quoted program are not escapes.
    assert_eq!(
        split_command_line(r#""C:\dir\" x"#),
        strings(&[r"C:\dir\", "x"])
    );
}

#[test]
fn split_quoted_arguments() {
    assert_eq!(
        split_command_line(r#"a.exe "x y" z"#),
        strings(&["a.exe", "x y", "z"])
    );
    assert_eq!(
        split_command_line(r#"a.exe x" "y z"#),
        strings(&["a.exe", "x y", "z"])
    );
    assert_eq!(
        split_command_line(r#"a.exe "" z"#),
        strings(&["a.exe", "", "z"])
    );
    assert_eq!(
        split_command_line(r#"a.exe -o ProxyCommand="x y" h"#),
        strings(&["a.exe", "-o", "ProxyCommand=x y", "h"])
    );
}

#[test]
fn split_backslashes_before_a_quote() {
    // 2n backslashes and a quote: n backslashes, and the quote opens or closes quoting.
    assert_eq!(
        split_command_line(r#"a.exe x\\"y z""#),
        strings(&["a.exe", r"x\y z"])
    );
    // 2n+1 backslashes and a quote: n backslashes and a literal quote.
    assert_eq!(
        split_command_line(r#"a.exe x\"y z"#),
        strings(&["a.exe", r#"x"y"#, "z"])
    );
    assert_eq!(
        split_command_line(r#"a.exe x\\\"y"#),
        strings(&["a.exe", r#"x\"y"#])
    );
    // Backslashes before anything else are literal.
    assert_eq!(
        split_command_line(r"a.exe C:\dir\file x\\y"),
        strings(&["a.exe", r"C:\dir\file", r"x\\y"])
    );
}

#[test]
fn split_runs_of_quotes_are_read_by_a_count() {
    // Inside quoting, `""` gives one literal quote and ends the quoting.
    assert_eq!(
        split_command_line(r#"a.exe "a""b c"#),
        strings(&["a.exe", "a\"b", "c"])
    );
    // Inside quoting, `"""` gives one literal quote and keeps the quoting.
    assert_eq!(
        split_command_line(r#"a.exe "a"""b c""#),
        strings(&["a.exe", "a\"b c"])
    );
    // Outside quoting, `"""` gives one literal quote and leaves quoting off.
    assert_eq!(
        split_command_line(r#"a.exe x"""y z"#),
        strings(&["a.exe", "x\"y", "z"])
    );
}
