//! The command line (docs/operator.md, Command line).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! [`parse`] checks, in this order, and the first failure ends the command: help; the arguments
//! read from left to right, the first usage error found among them reported (a missing command is
//! found only once the whole line is read); then the two `-Alias` checks; then the five host checks.

use crate::help_text;

/// The commands, in the order operator.md lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Deploy,
    Up,
    Sync,
    Bootstrap,
    Provision,
    Ca,
    Down,
    Status,
    Grow,
    Guard,
}

impl Command {
    /// Every command, in operator.md's order.
    pub const ALL: [Command; 10] = [
        Command::Deploy,
        Command::Up,
        Command::Sync,
        Command::Bootstrap,
        Command::Provision,
        Command::Ca,
        Command::Down,
        Command::Status,
        Command::Grow,
        Command::Guard,
    ];

    /// The command's name as operator.md spells it: lowercase.
    pub fn name(self) -> &'static str {
        match self {
            Command::Deploy => "deploy",
            Command::Up => "up",
            Command::Sync => "sync",
            Command::Bootstrap => "bootstrap",
            Command::Provision => "provision",
            Command::Ca => "ca",
            Command::Down => "down",
            Command::Status => "status",
            Command::Grow => "grow",
            Command::Guard => "guard",
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
        HelpLevel::Command(Command::Bootstrap) => help_text::BOOTSTRAP,
        HelpLevel::Command(Command::Provision) => help_text::PROVISION,
        HelpLevel::Command(Command::Ca) => help_text::CA,
        HelpLevel::Command(Command::Down) => help_text::DOWN,
        HelpLevel::Command(Command::Status) => help_text::STATUS,
        HelpLevel::Command(Command::Grow) => help_text::GROW,
        HelpLevel::Command(Command::Guard) => help_text::GUARD,
    }
}

/// A command line that passed every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub command: Command,
    /// `-GiB`, default 2. Only `grow` uses it, and for `grow` it is 1 or more.
    pub gib: i32,
    /// `-Alias`, default `agentspace`. Only `up` uses it.
    pub alias: String,
    /// Whether `-Alias` was given.
    pub alias_given: bool,
    /// `-Host`, in its normalized form (lowercase, trimmed). `swimcoach` names the box, and is the
    /// same as no `-Host` (P1).
    pub host: Option<String>,
    /// `-All`: every host in the host registry.
    pub all: bool,
    /// `-InstanceType`. Only `deploy` takes it, for every host, the box included.
    pub instance_type: Option<String>,
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

const COMMANDS_TEXT: &str =
    "The commands are deploy, up, sync, bootstrap, provision, ca, down, status, grow and guard.";
const ALIAS_APPLIES: &str = "agentspace: -Alias applies to up alone.";
const ALL_APPLIES: &str = "agentspace: -All applies to status, down and sync alone.";
const HOST_AND_ALL: &str = "agentspace: -Host and -All cannot be given together.";
const PROVISION_NEEDS_HOST: &str = "agentspace: provision needs -Host <project>.";
const INSTANCE_TYPE_APPLIES: &str = "agentspace: -InstanceType applies to deploy alone.";
const ALIAS_INVALID: &str = "agentspace: -Alias MUST be an SSH host alias of letters, digits, '.', '_' and '-' that does not start with '-'.";

/// Which parameter an argument names, matched without regard to case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Param {
    Gib,
    Alias,
    Host,
    InstanceType,
    Quiet,
    All,
}

impl Param {
    fn from_arg(arg: &str) -> Option<Param> {
        if arg.eq_ignore_ascii_case("-GiB") {
            Some(Param::Gib)
        } else if arg.eq_ignore_ascii_case("-Alias") {
            Some(Param::Alias)
        } else if arg.eq_ignore_ascii_case("-Host") {
            Some(Param::Host)
        } else if arg.eq_ignore_ascii_case("-InstanceType") {
            Some(Param::InstanceType)
        } else if arg.eq_ignore_ascii_case("-Quiet") {
            Some(Param::Quiet)
        } else if arg.eq_ignore_ascii_case("-All") {
            Some(Param::All)
        } else {
            None
        }
    }

    /// The parameter as operator.md spells it, whatever case was given.
    fn name(self) -> &'static str {
        match self {
            Param::Gib => "-GiB",
            Param::Alias => "-Alias",
            Param::Host => "-Host",
            Param::InstanceType => "-InstanceType",
            Param::Quiet => "-Quiet",
            Param::All => "-All",
        }
    }

    fn takes_value(self) -> bool {
        !matches!(self, Param::Quiet | Param::All)
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
/// right, the argument after `-GiB`, `-Alias`, `-Host` or `-InstanceType` is that parameter's value
/// unless it is help, an
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

/// `^[a-z0-9][a-z0-9-]*\.[a-z0-9][a-z0-9-]*\z`, case-sensitively: an EC2 instance type's form.
fn valid_instance_type(value: &str) -> bool {
    let part = |p: &str| {
        let b = p.as_bytes();
        let plain = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit();
        b.first().is_some_and(|&c| plain(c)) && b.iter().all(|&c| plain(c) || c == b'-')
    };
    // The class `[a-z0-9-]` holds no `.`, so the one dot is the one the pattern names.
    match value.split_once('.') {
        Some((family, size)) => part(family) && part(size),
        None => false,
    }
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
    let mut gib_text: Option<String> = None;
    let mut alias: Option<String> = None;
    let mut host: Option<String> = None;
    let mut instance_type: Option<String> = None;
    let mut quiet = false;
    let mut all = false;
    let mut seen_gib = false;
    let mut seen_alias = false;
    let mut seen_host = false;
    let mut seen_instance_type = false;

    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        i += 1;
        if let Some(p) = Param::from_arg(arg) {
            let seen = match p {
                Param::Gib => &mut seen_gib,
                Param::Alias => &mut seen_alias,
                Param::Host => &mut seen_host,
                Param::InstanceType => &mut seen_instance_type,
                Param::Quiet => &mut quiet,
                Param::All => &mut all,
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
                        Some(n) => {
                            gib = Some(n);
                            gib_text = Some(value.clone());
                        }
                        None => {
                            return Parsed::Failure(format!(
                                "agentspace: -GiB MUST be a whole number, not '{}'.",
                                shown(value)
                            ));
                        }
                    },
                    Param::Host => match agentspace_values::project(value) {
                        Ok(project) => host = Some(project),
                        Err(_) => {
                            return Parsed::Failure(format!(
                                "agentspace: -Host MUST be a project name of 1 to 32 letters, digits and '-' that starts and ends with a letter or digit, not '{}'.",
                                shown(value)
                            ));
                        }
                    },
                    Param::InstanceType => {
                        if !valid_instance_type(value) {
                            return Parsed::Failure(format!(
                                "agentspace: -InstanceType MUST be an EC2 instance type such as m9g.xlarge, not '{}'.",
                                shown(value)
                            ));
                        }
                        instance_type = Some(value.clone());
                    }
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

    // The five host checks, in their order (D49, D55; v0.09 deleted the sixth, D67, D71).
    if host.is_some() && matches!(command, Command::Ca | Command::Guard) {
        return Parsed::Failure(format!(
            "agentspace: -Host does not apply to {}.",
            command.name()
        ));
    }
    if all && !matches!(command, Command::Status | Command::Down | Command::Sync) {
        return Parsed::Failure(ALL_APPLIES.to_string());
    }
    if host.is_some() && all {
        return Parsed::Failure(HOST_AND_ALL.to_string());
    }
    if command == Command::Provision && host.is_none() {
        return Parsed::Failure(PROVISION_NEEDS_HOST.to_string());
    }
    if instance_type.is_some() && command != Command::Deploy {
        return Parsed::Failure(INSTANCE_TYPE_APPLIES.to_string());
    }

    // Then `grow`'s `-GiB` check (D61, #0164; operator.md, Command line): a value of 0 or less is a
    // usage error, found after the host checks and before anything runs. Every other command
    // checks the form alone.
    if command == Command::Grow
        && let Some((n, text)) = gib.as_ref().zip(gib_text.as_deref())
        && *n < 1
    {
        return Parsed::Failure(format!(
            "agentspace: -GiB MUST be 1 or more for grow, not '{}'.",
            shown(text)
        ));
    }

    Parsed::Run(Request {
        command,
        gib: gib.unwrap_or(DEFAULT_GIB),
        alias,
        alias_given,
        host,
        all,
        instance_type,
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
            "agentspace: Unknown command 'st'. The commands are deploy, up, sync, bootstrap, provision, ca, down, status, grow and guard."
        );
    }

    #[test]
    fn usage_errors_have_their_texts() {
        assert_eq!(
            fails(&[]),
            "agentspace: No command given. The commands are deploy, up, sync, bootstrap, provision, ca, down, status, grow and guard."
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
        assert!(matches!(p(&["status", "-GiB", "-1"]), Parsed::Run(r) if r.gib == -1));
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
                matches!(p(&["status", "-GiB", text]), Parsed::Run(r) if r.gib == n),
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
            "agentspace: Unknown command 'foo'. The commands are deploy, up, sync, bootstrap, provision, ca, down, status, grow and guard."
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

    fn run(line: &[&str]) -> Request {
        match p(line) {
            Parsed::Run(r) => r,
            other => panic!("expected a run for {line:?}, got {other:?}"),
        }
    }

    #[test]
    fn host_all_and_instance_type_are_read_and_the_host_is_normalized() {
        let r = run(&["status", "-host", "  Agentspace "]);
        assert_eq!(r.host.as_deref(), Some("agentspace"));
        assert!(!r.all);
        assert!(run(&["sync", "-ALL"]).all);
        let r = run(&[
            "deploy",
            "-Host",
            "agentspace",
            "-InstanceType",
            "m9g.2xlarge",
        ]);
        assert_eq!(r.instance_type.as_deref(), Some("m9g.2xlarge"));
        assert_eq!(run(&["bootstrap"]).command, Command::Bootstrap);
        assert_eq!(
            run(&["provision", "-Host", "x"]).command,
            Command::Provision
        );
        // `-Host swimcoach` and `-Host` with an `-Alias` are read as given.
        let r = run(&["up", "-Host", "swimcoach", "-Alias", "mine"]);
        assert_eq!(r.host.as_deref(), Some("swimcoach"));
        assert!(r.alias_given);
    }

    #[test]
    fn a_value_after_host_or_instance_type_is_taken_whatever_it_looks_like() {
        assert_eq!(
            fails(&["status", "-Host", "-All"]),
            "agentspace: -Host MUST be a project name of 1 to 32 letters, digits and '-' that starts and ends with a letter or digit, not '-All'."
        );
        assert_eq!(
            fails(&["deploy", "-InstanceType", "-Host"]),
            "agentspace: -InstanceType MUST be an EC2 instance type such as m9g.xlarge, not '-Host'."
        );
    }

    #[test]
    fn the_new_usage_errors_have_their_texts() {
        assert_eq!(fails(&["up", "-Host"]), "agentspace: -Host needs a value.");
        assert_eq!(
            fails(&["up", "-InstanceType"]),
            "agentspace: -InstanceType needs a value."
        );
        assert_eq!(
            fails(&["up", "-host", "a", "-HOST", "b"]),
            "agentspace: -Host is given more than once."
        );
        assert_eq!(
            fails(&["status", "-All", "-all"]),
            "agentspace: -All is given more than once."
        );
        for bad in ["", "-a", "a-", "a b!", "a_b", &"a".repeat(33)] {
            assert_eq!(
                fails(&["status", "-Host", bad]),
                format!(
                    "agentspace: -Host MUST be a project name of 1 to 32 letters, digits and '-' that starts and ends with a letter or digit, not '{bad}'."
                ),
                "{bad:?}"
            );
        }
        assert_eq!(
            run(&["status", "-Host", &"a".repeat(32)])
                .host
                .unwrap()
                .len(),
            32
        );
        for bad in [
            "m9g",
            "M9g.xlarge",
            "m9g.",
            ".xlarge",
            "m9g.x.y",
            "-m9g.x",
            "m9g.-x",
            "m9g.x ",
            "",
        ] {
            assert_eq!(
                fails(&["deploy", "-Host", "p", "-InstanceType", bad]),
                format!(
                    "agentspace: -InstanceType MUST be an EC2 instance type such as m9g.xlarge, not '{bad}'."
                ),
                "{bad:?}"
            );
        }
        for good in ["m9g.2xlarge", "m7i.metal-24xl", "t3.micro", "a.b"] {
            assert!(
                run(&["deploy", "-Host", "p", "-InstanceType", good])
                    .instance_type
                    .is_some()
            );
        }
    }

    #[test]
    fn the_host_checks_run_in_their_order_after_the_alias_checks() {
        assert_eq!(
            fails(&["ca", "-Host", "y"]),
            "agentspace: -Host does not apply to ca."
        );
        // `-Alias` comes first, then the host checks.
        assert_eq!(
            fails(&["ca", "-Alias", "x", "-Host", "y"]),
            "agentspace: -Alias applies to up alone."
        );
        // 1 before 2: `ca -Host y -All`.
        assert_eq!(
            fails(&["ca", "-Host", "y", "-All"]),
            "agentspace: -Host does not apply to ca."
        );
        // 2 before 3: `-All` with a command it does not apply to is found before `-Host` with it.
        for command in ["up", "deploy", "grow", "ca", "bootstrap", "provision"] {
            assert_eq!(
                fails(&[command, "-All"]),
                "agentspace: -All applies to status, down and sync alone.",
                "{command}"
            );
        }
        assert_eq!(
            fails(&["up", "-All", "-Host", "y"]),
            "agentspace: -All applies to status, down and sync alone."
        );
        assert_eq!(
            fails(&["status", "-Host", "y", "-All"]),
            "agentspace: -Host and -All cannot be given together."
        );
        assert_eq!(
            fails(&["provision"]),
            "agentspace: provision needs -Host <project>."
        );
        assert_eq!(
            fails(&["provision", "-All"]),
            "agentspace: -All applies to status, down and sync alone."
        );
        assert_eq!(
            fails(&["up", "-InstanceType", "m9g.xlarge"]),
            "agentspace: -InstanceType applies to deploy alone."
        );
        assert_eq!(
            fails(&["provision", "-InstanceType", "m9g.xlarge"]),
            "agentspace: provision needs -Host <project>."
        );
    }

    /// v0.09 deleted the sixth host check (D67, D71): `deploy -InstanceType` is read for every
    /// host, the box included, with `-Host swimcoach` or with no `-Host`.
    #[test]
    fn deploy_instance_type_is_accepted_for_every_host_the_box_included() {
        let r = run(&["deploy", "-InstanceType", "m9g.xlarge"]);
        assert_eq!(r.instance_type.as_deref(), Some("m9g.xlarge"));
        assert_eq!(r.host, None);
        let r = run(&[
            "deploy",
            "-Host",
            "SwimCoach",
            "-InstanceType",
            "m9g.xlarge",
        ]);
        assert_eq!(r.instance_type.as_deref(), Some("m9g.xlarge"));
        assert_eq!(r.host.as_deref(), Some("swimcoach"));
        let r = run(&["deploy", "-Host", "p", "-InstanceType", "m9g.xlarge"]);
        assert_eq!(r.instance_type.as_deref(), Some("m9g.xlarge"));
        assert_eq!(run(&["deploy"]).instance_type, None);
        // The `-Alias` checks and host check 2 still refuse a `deploy` line that holds
        // `-InstanceType`, in their order.
        assert_eq!(
            fails(&[
                "deploy",
                "-Quiet",
                "-Alias",
                "x",
                "-InstanceType",
                "m9g.xlarge"
            ]),
            ALIAS_APPLIES
        );
        assert_eq!(
            fails(&["deploy", "-All", "-InstanceType", "m9g.xlarge"]),
            ALL_APPLIES
        );
        assert_eq!(
            fails(&[
                "deploy",
                "-Host",
                "y",
                "-All",
                "-InstanceType",
                "m9g.xlarge"
            ]),
            ALL_APPLIES
        );
    }

    #[test]
    fn a_usage_error_in_an_argument_comes_before_the_host_checks() {
        assert_eq!(
            fails(&["ca", "-Host", "y", "-GiB", "abc"]),
            "agentspace: -GiB MUST be a whole number, not 'abc'."
        );
        assert_eq!(
            fails(&["status", "-Host", "bad!", "-GiB", "abc"]),
            "agentspace: -Host MUST be a project name of 1 to 32 letters, digits and '-' that starts and ends with a letter or digit, not 'bad!'."
        );
        assert!(fails(&["-Host", "y"]).starts_with("agentspace: No command given."));
    }

    #[test]
    fn help_reads_host_and_instance_type_values_as_values() {
        assert_eq!(p(&["-Host", "up", "-h"]), Parsed::Help(HelpLevel::Top));
        assert_eq!(
            p(&["-InstanceType", "-h", "deploy"]),
            Parsed::Help(HelpLevel::Command(Command::Deploy))
        );
        assert_eq!(
            p(&["-All", "status", "-h"]),
            Parsed::Help(HelpLevel::Command(Command::Status))
        );
        assert_eq!(
            p(&["bootstrap", "-Host", "x", "--help"]),
            Parsed::Help(HelpLevel::Command(Command::Bootstrap))
        );
    }

    /// Every `**Help` marker of docs/operator.md's Commands section is followed by one fenced
    /// block: the top level's marker is `**Help** (...)`, every command's `**Help:**`.
    #[test]
    fn each_help_text_is_the_block_in_operator_md_byte_for_byte() {
        let doc = include_str!("../../../docs/operator.md").replace("\r\n", "\n");
        let commands = doc.split_once("\n## Commands\n").unwrap().1;
        let commands = commands.split_once("\n## ").unwrap().0;
        let mut blocks = Vec::new();
        let mut lines = commands.lines();
        while let Some(line) = lines.next() {
            if line.starts_with("**Help") {
                assert_eq!(lines.by_ref().find(|l| !l.is_empty()), Some("```"));
                let mut block = String::new();
                for body in lines.by_ref().take_while(|l| *l != "```") {
                    block.push_str(body);
                    block.push('\n');
                }
                blocks.push(block);
            }
        }
        let mut levels = vec![HelpLevel::Top];
        levels.extend(Command::ALL.map(HelpLevel::Command));
        assert_eq!(blocks.len(), levels.len(), "One block per level");
        // Doc order: the top level, then deploy, up, sync, bootstrap, provision, ca, down, status,
        // grow, which is `Command::ALL`'s order.
        for (block, level) in blocks.iter().zip(levels) {
            assert_eq!(help(level), block, "{level:?}");
        }
    }

    #[test]
    fn guard_is_a_command_and_takes_no_host_or_all() {
        assert_eq!(run(&["GUARD", "-quiet"]).command, Command::Guard);
        assert!(run(&["guard", "-Quiet"]).quiet);
        assert_eq!(
            fails(&["guard", "-Host", "y"]),
            "agentspace: -Host does not apply to guard."
        );
        // Check 1 comes before check 3.
        assert_eq!(
            fails(&["guard", "-Host", "x", "-All"]),
            "agentspace: -Host does not apply to guard."
        );
        assert_eq!(
            fails(&["guard", "-All"]),
            "agentspace: -All applies to status, down and sync alone."
        );
        assert_eq!(
            fails(&["guard", "-Alias", "x"]),
            "agentspace: -Alias applies to up alone."
        );
        assert_eq!(
            fails(&["guard", "-InstanceType", "m9g.xlarge"]),
            "agentspace: -InstanceType applies to deploy alone."
        );
        // `guard` ignores the value of -GiB, and checks its form alone.
        assert_eq!(run(&["guard", "-GiB", "0"]).gib, 0);
        assert!(fails(&["guard", "-GiB", "abc"]).starts_with("agentspace: -GiB MUST be a whole"));
        assert_eq!(
            p(&["guard", "-h"]),
            Parsed::Help(HelpLevel::Command(Command::Guard))
        );
    }

    #[test]
    fn grow_refuses_a_gib_of_0_or_less_after_the_host_checks() {
        for text in ["0", "-1", "+0", "-0", "-2147483648", "000"] {
            assert_eq!(
                fails(&["grow", "-GiB", text]),
                format!("agentspace: -GiB MUST be 1 or more for grow, not '{text}'."),
                "{text}"
            );
        }
        assert_eq!(run(&["grow", "-GiB", "1"]).gib, 1);
        assert_eq!(run(&["grow", "-GiB", "+7"]).gib, 7);
        assert_eq!(run(&["grow"]).gib, DEFAULT_GIB);
        // Every other command ignores the value.
        assert_eq!(run(&["status", "-GiB", "0"]).gib, 0);
        assert_eq!(run(&["down", "-GiB", "-3"]).gib, -3);
        // The form is checked first, and the host checks come before the value check.
        assert!(fails(&["grow", "-GiB", "abc"]).starts_with("agentspace: -GiB MUST be a whole"));
        assert_eq!(
            fails(&["grow", "-All", "-GiB", "0"]),
            "agentspace: -All applies to status, down and sync alone."
        );
        assert_eq!(
            fails(&["grow", "-Alias", "x", "-GiB", "0"]),
            "agentspace: -Alias applies to up alone."
        );
        assert_eq!(
            fails(&["grow", "-InstanceType", "m9g.xlarge", "-GiB", "0"]),
            "agentspace: -InstanceType applies to deploy alone."
        );
        // Before the host registry is read, so a host that does not exist is not reached.
        assert_eq!(
            fails(&["grow", "-Host", "nobody", "-GiB", "0"]),
            "agentspace: -GiB MUST be 1 or more for grow, not '0'."
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
