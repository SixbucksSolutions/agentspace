//! The command line (docs/operator.md, Command line).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! [`parse`] checks, in this order, and the first failure ends the command: help; the arguments
//! read from left to right, the first usage error found among them reported (a missing command is
//! found only once the whole line is read); then the two `-Alias` checks.

use crate::help_text;

/// The commands, in the order operator.md lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Deploy,
    Up,
    Sync,
    Ca,
    Down,
    Status,
    Grow,
}

impl Command {
    /// Every command, in operator.md's order.
    pub const ALL: [Command; 7] = [
        Command::Deploy,
        Command::Up,
        Command::Sync,
        Command::Ca,
        Command::Down,
        Command::Status,
        Command::Grow,
    ];

    /// The command's name as operator.md spells it: lowercase.
    pub fn name(self) -> &'static str {
        match self {
            Command::Deploy => "deploy",
            Command::Up => "up",
            Command::Sync => "sync",
            Command::Ca => "ca",
            Command::Down => "down",
            Command::Status => "status",
            Command::Grow => "grow",
        }
    }

    /// Matches a whole command name without regard to case; an abbreviation is unknown.
    pub fn from_arg(arg: &str) -> Option<Command> {
        Command::ALL
            .into_iter()
            .find(|c| c.name().eq_ignore_ascii_case(arg))
    }
}

/// Which help text an argument list asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpLevel {
    /// The top level's.
    Top,
    /// One command's.
    Command(Command),
}

/// The text printed to stdout for `level`, byte for byte as docs/operator.md gives it, with a line
/// feed after every line.
pub fn help(level: HelpLevel) -> &'static str {
    match level {
        HelpLevel::Top => help_text::TOP,
        HelpLevel::Command(Command::Deploy) => help_text::DEPLOY,
        HelpLevel::Command(Command::Up) => help_text::UP,
        HelpLevel::Command(Command::Sync) => help_text::SYNC,
        HelpLevel::Command(Command::Ca) => help_text::CA,
        HelpLevel::Command(Command::Down) => help_text::DOWN,
        HelpLevel::Command(Command::Status) => help_text::STATUS,
        HelpLevel::Command(Command::Grow) => help_text::GROW,
    }
}

/// A command line that passed every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub command: Command,
    /// `-GiB`, default 2. Only `grow` uses it.
    pub gib: i32,
    /// `-Alias`, default `agentspace`. Only `up` uses it.
    pub alias: String,
    /// Whether `-Alias` was given.
    pub alias_given: bool,
    pub quiet: bool,
}

/// What a command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    /// Print this help to stdout, exit 0, and do nothing else.
    Help(HelpLevel),
    /// Run this request.
    Run(Request),
    /// A usage error or an `-Alias` refusal: the whole `agentspace: ` line, without its line feed.
    /// Nothing is written to stdout, nothing is started, and the exit code is 1.
    Failure(String),
}

/// The default `-GiB`.
pub const DEFAULT_GIB: i32 = 2;
/// The default `-Alias`.
pub const DEFAULT_ALIAS: &str = "agentspace";

const COMMANDS_TEXT: &str = "The commands are deploy, up, sync, ca, down, status and grow.";
const ALIAS_APPLIES: &str = "agentspace: -Alias applies to up alone.";
const ALIAS_INVALID: &str = "agentspace: -Alias MUST be an SSH host alias of letters, digits, '.', '_' and '-' that does not start with '-'.";

/// Which parameter an argument names, matched without regard to case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Param {
    Gib,
    Alias,
    Quiet,
}

impl Param {
    fn from_arg(arg: &str) -> Option<Param> {
        if arg.eq_ignore_ascii_case("-GiB") {
            Some(Param::Gib)
        } else if arg.eq_ignore_ascii_case("-Alias") {
            Some(Param::Alias)
        } else if arg.eq_ignore_ascii_case("-Quiet") {
            Some(Param::Quiet)
        } else {
            None
        }
    }

    /// The parameter as operator.md spells it, whatever case was given.
    fn name(self) -> &'static str {
        match self {
            Param::Gib => "-GiB",
            Param::Alias => "-Alias",
            Param::Quiet => "-Quiet",
        }
    }

    fn takes_value(self) -> bool {
        !matches!(self, Param::Quiet)
    }
}

fn is_help(arg: &str) -> bool {
    arg.eq_ignore_ascii_case("-h") || arg.eq_ignore_ascii_case("--help")
}

/// The argument as given, with each control character written as `?`, so that a message stays one
/// line.
fn shown(arg: &str) -> String {
    arg.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// Which help a line that holds a help argument gets (gap FB-11): the line is read from left to
/// right, the argument after `-GiB` or `-Alias` is that parameter's value unless it is help, an
/// argument that starts with `-` and is not a value counts as a parameter, known or not, and the
/// command is the first argument that is none of a parameter, a parameter's value and help.
fn help_level(args: &[String]) -> HelpLevel {
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if is_help(arg) {
            i += 1;
            continue;
        }
        if let Some(p) = Param::from_arg(arg) {
            i += 1;
            if p.takes_value() && i < args.len() && !is_help(&args[i]) {
                i += 1;
            }
            continue;
        }
        // An argument that starts with `-` and is not a parameter's value counts as a parameter,
        // known or not, so it is skipped: `-Foo up -h` prints `up`'s help.
        if arg.starts_with('-') {
            i += 1;
            continue;
        }
        return match Command::from_arg(arg) {
            Some(c) => HelpLevel::Command(c),
            None => HelpLevel::Top,
        };
    }
    HelpLevel::Top
}

/// `-GiB`'s form: an optional `+` or `-`, one or more ASCII digits, within the range of a 32-bit
/// signed integer.
fn parse_gib(value: &str) -> Option<i32> {
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // `i64` holds the range with room for the sign. A longer run of digits has leading zeros or is
    // out of range, and the trimmed form tells which.
    let trimmed = digits.trim_start_matches('0');
    if trimmed.len() > 10 {
        return None;
    }
    let magnitude: i64 = if trimmed.is_empty() {
        0
    } else {
        trimmed.parse().ok()?
    };
    let signed = if value.starts_with('-') {
        -magnitude
    } else {
        magnitude
    };
    i32::try_from(signed).ok()
}

/// `^[A-Za-z0-9._-]+\z`, case-sensitively, and not starting with `-`.
fn valid_alias(alias: &str) -> bool {
    !alias.is_empty()
        && !alias.starts_with('-')
        && alias
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Reads a command line, the program's own name excluded.
pub fn parse(args: &[String]) -> Parsed {
    if args.iter().any(|a| is_help(a)) {
        return Parsed::Help(help_level(args));
    }

    let mut command: Option<Command> = None;
    let mut gib: Option<i32> = None;
    let mut alias: Option<String> = None;
    let mut quiet = false;
    let mut seen_gib = false;
    let mut seen_alias = false;

    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        i += 1;
        if let Some(p) = Param::from_arg(arg) {
            let seen = match p {
                Param::Gib => &mut seen_gib,
                Param::Alias => &mut seen_alias,
                Param::Quiet => &mut quiet,
            };
            // At each argument a repeated name is found before a missing value, so
            // `-GiB 1 -GiB` reports the repeat.
            if *seen {
                return Parsed::Failure(format!(
                    "agentspace: {} is given more than once.",
                    p.name()
                ));
            }
            *seen = true;
            if p.takes_value() {
                let Some(value) = args.get(i) else {
                    return Parsed::Failure(format!("agentspace: {} needs a value.", p.name()));
                };
                i += 1;
                match p {
                    Param::Gib => match parse_gib(value) {
                        Some(n) => gib = Some(n),
                        None => {
                            return Parsed::Failure(format!(
                                "agentspace: -GiB MUST be a whole number, not '{}'.",
                                shown(value)
                            ));
                        }
                    },
                    _ => alias = Some(value.clone()),
                }
            }
        } else if arg.starts_with('-') {
            return Parsed::Failure(format!("agentspace: Unknown parameter '{}'.", shown(arg)));
        } else if command.is_some() {
            return Parsed::Failure(format!("agentspace: Unexpected argument '{}'.", shown(arg)));
        } else {
            match Command::from_arg(arg) {
                Some(c) => command = Some(c),
                None => {
                    return Parsed::Failure(format!(
                        "agentspace: Unknown command '{}'. {COMMANDS_TEXT}",
                        shown(arg)
                    ));
                }
            }
        }
    }

    let Some(command) = command else {
        return Parsed::Failure(format!("agentspace: No command given. {COMMANDS_TEXT}"));
    };

    // The two -Alias checks, in their order, before anything runs.
    if alias.is_some() && command != Command::Up {
        return Parsed::Failure(ALIAS_APPLIES.to_string());
    }
    let alias_given = alias.is_some();
    let alias = alias.unwrap_or_else(|| DEFAULT_ALIAS.to_string());
    if !valid_alias(&alias) {
        return Parsed::Failure(ALIAS_INVALID.to_string());
    }

    Parsed::Run(Request {
        command,
        gib: gib.unwrap_or(DEFAULT_GIB),
        alias,
        alias_given,
        quiet,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(line: &[&str]) -> Parsed {
        let args: Vec<String> = line.iter().map(|s| s.to_string()).collect();
        parse(&args)
    }

    fn fails(line: &[&str]) -> String {
        match p(line) {
            Parsed::Failure(m) => m,
            other => panic!("expected a failure for {line:?}, got {other:?}"),
        }
    }

    #[test]
    fn commands_match_whole_without_regard_to_case() {
        assert!(matches!(p(&["STATUS"]), Parsed::Run(r) if r.command == Command::Status));
        assert_eq!(
            fails(&["st"]),
            "agentspace: Unknown command 'st'. The commands are deploy, up, sync, ca, down, status and grow."
        );
    }

    #[test]
    fn usage_errors_have_their_texts() {
        assert_eq!(
            fails(&[]),
            "agentspace: No command given. The commands are deploy, up, sync, ca, down, status and grow."
        );
        assert_eq!(
            fails(&["up", "down"]),
            "agentspace: Unexpected argument 'down'."
        );
        assert_eq!(
            fails(&["up", "-Zed"]),
            "agentspace: Unknown parameter '-Zed'."
        );
        assert_eq!(
            fails(&["up", "-quiet", "-QUIET"]),
            "agentspace: -Quiet is given more than once."
        );
        assert_eq!(
            fails(&["grow", "-gib", "1", "-GIB", "2"]),
            "agentspace: -GiB is given more than once."
        );
        assert_eq!(fails(&["grow", "-gib"]), "agentspace: -GiB needs a value.");
        assert_eq!(
            fails(&["up", "-alias"]),
            "agentspace: -Alias needs a value."
        );
    }

    #[test]
    fn a_value_is_taken_whatever_it_looks_like() {
        assert_eq!(
            fails(&["up", "-Alias", "-Fx"]),
            ALIAS_INVALID,
            "the value -Fx is the alias, and its leading dash is refused"
        );
        assert!(matches!(p(&["grow", "-GiB", "-1"]), Parsed::Run(r) if r.gib == -1));
    }

    #[test]
    fn gib_form_and_range() {
        for (text, n) in [
            ("0", 0),
            ("+5", 5),
            ("-5", -5),
            ("007", 7),
            ("2147483647", i32::MAX),
            ("-2147483648", i32::MIN),
            ("-0000000000000000000000002147483648", i32::MIN),
        ] {
            assert!(
                matches!(p(&["grow", "-GiB", text]), Parsed::Run(r) if r.gib == n),
                "{text}"
            );
        }
        for text in [
            "",
            "+",
            "-",
            "abc",
            "0x10",
            "1e1",
            "2.5",
            " 1",
            "1 ",
            "2147483648",
            "-2147483649",
            "99999999999999999999999999",
            "１",
            "+-1",
        ] {
            assert_eq!(
                fails(&["status", "-GiB", text]),
                format!("agentspace: -GiB MUST be a whole number, not '{text}'."),
                "{text}"
            );
        }
    }

    #[test]
    fn control_characters_are_shown_as_question_marks() {
        assert_eq!(
            fails(&["up", "a\nb\u{7f}"]),
            "agentspace: Unexpected argument 'a?b?'."
        );
    }

    #[test]
    fn the_first_usage_error_from_the_left_wins() {
        assert_eq!(
            fails(&["status", "-GiB", "abc", "-Alias", "bad!"]),
            "agentspace: -GiB MUST be a whole number, not 'abc'."
        );
        assert_eq!(
            fails(&["foo", "-GiB", "abc"]),
            "agentspace: Unknown command 'foo'. The commands are deploy, up, sync, ca, down, status and grow."
        );
        assert_eq!(
            fails(&["-GiB", "abc"]),
            "agentspace: -GiB MUST be a whole number, not 'abc'."
        );
        // The -Alias checks run after the arguments are read.
        assert_eq!(
            fails(&["status", "-Alias", "x", "-Zed"]),
            "agentspace: Unknown parameter '-Zed'."
        );
        assert!(fails(&["-Alias", "x"]).starts_with("agentspace: No command"));
    }

    #[test]
    fn the_alias_checks_run_in_order() {
        assert_eq!(fails(&["status", "-Alias", "x"]), ALIAS_APPLIES);
        assert_eq!(fails(&["status", "-Alias", "bad!"]), ALIAS_APPLIES);
        for bad in [
            "a;b", "a b", "a&b", "", "a|b", "a\"b", "-Fx", "a/b", "a%b", "\u{212a}", "\u{130}",
            "a\n",
        ] {
            assert_eq!(fails(&["up", "-Alias", bad]), ALIAS_INVALID, "{bad:?}");
        }
        assert!(
            matches!(p(&["up", "-Alias", "dev.box_1-x"]), Parsed::Run(r) if r.alias == "dev.box_1-x" && r.alias_given)
        );
        assert!(matches!(p(&["up"]), Parsed::Run(r) if r.alias == "agentspace" && !r.alias_given));
    }

    #[test]
    fn help_is_checked_first_and_wherever_it_stands() {
        assert_eq!(
            p(&["sync", "-Alias", "x", "--help"]),
            Parsed::Help(HelpLevel::Command(Command::Sync))
        );
        assert_eq!(p(&["-H"]), Parsed::Help(HelpLevel::Top));
        assert_eq!(
            p(&["nonsense", "-GiB", "abc", "-h"]),
            Parsed::Help(HelpLevel::Top)
        );
        assert_eq!(
            p(&["UP", "extra", "-Zed", "-h"]),
            Parsed::Help(HelpLevel::Command(Command::Up))
        );
    }

    #[test]
    fn help_names_the_command_by_gap_fb_11() {
        assert_eq!(p(&["-Alias", "up", "-h"]), Parsed::Help(HelpLevel::Top));
        assert_eq!(
            p(&["-Alias", "-h", "up"]),
            Parsed::Help(HelpLevel::Command(Command::Up))
        );
        assert_eq!(p(&["-GiB", "grow", "-h"]), Parsed::Help(HelpLevel::Top));
        assert_eq!(
            p(&["-Quiet", "grow", "-h"]),
            Parsed::Help(HelpLevel::Command(Command::Grow))
        );
    }

    #[test]
    fn an_unknown_parameter_is_skipped_when_finding_the_command_for_help() {
        assert_eq!(
            p(&["-Foo", "up", "-h"]),
            Parsed::Help(HelpLevel::Command(Command::Up))
        );
        assert_eq!(
            p(&["-h", "-Foo", "grow"]),
            Parsed::Help(HelpLevel::Command(Command::Grow))
        );
        // As a value, `-Foo` is not skipped as a parameter: the next argument is the command.
        assert_eq!(
            p(&["-Alias", "-Foo", "up", "-h"]),
            Parsed::Help(HelpLevel::Command(Command::Up))
        );
        assert_eq!(p(&["-Foo", "-h"]), Parsed::Help(HelpLevel::Top));
        assert_eq!(p(&["-Foo", "nonsense", "-h"]), Parsed::Help(HelpLevel::Top));
        // Without help, the unknown parameter is still a usage error.
        assert_eq!(
            fails(&["-Foo", "up"]),
            "agentspace: Unknown parameter '-Foo'."
        );
    }

    #[test]
    fn a_repeated_name_is_found_before_a_missing_value() {
        assert_eq!(
            fails(&["status", "-GiB", "1", "-GiB"]),
            "agentspace: -GiB is given more than once."
        );
        assert_eq!(
            fails(&["up", "-Alias", "a", "-alias"]),
            "agentspace: -Alias is given more than once."
        );
    }

    #[test]
    fn every_help_text_ends_with_a_line_feed_and_has_no_carriage_return() {
        let mut levels = vec![HelpLevel::Top];
        levels.extend(Command::ALL.map(HelpLevel::Command));
        for level in levels {
            let text = help(level);
            assert!(text.ends_with("\n") && !text.ends_with("\n\n"), "{level:?}");
            assert!(!text.contains('\r'), "{level:?}");
            assert!(text.starts_with("Usage: agentspace-operator "), "{level:?}");
        }
    }
}
