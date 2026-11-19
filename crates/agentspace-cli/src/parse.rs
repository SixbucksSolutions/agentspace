//! The command line (lease-api.md, Command lines): tokens, flag values, help, usage errors and which
//! one is reported.

use std::ffi::OsString;

use crate::help;
use crate::osx::{from_units, units};
use crate::quote::quote_os;
use crate::{ErrorCode, LeaseError, ParsedCommand, Sub};

/// What a command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    /// Help text to print on stdout, exit 0, and the subcommand when help was asked for at the
    /// subcommand's level, which a failed write of it names (lease-api.md, Output).
    Help(&'static str, Option<Sub>),
    /// A line with no usage error.
    Command(ParsedCommand),
    /// The usage error to report.
    Error(LeaseError),
}

/// Where the reading has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Top,
    Lease,
    Sub(Sub),
}

const MINUS: u32 = b'-' as u32;
const EQUALS: u32 = b'=' as u32;

/// An argument, kept as the units the operating system gave.
struct Arg<'a> {
    os: &'a OsString,
    units: Vec<u32>,
}

impl<'a> Arg<'a> {
    fn new(os: &'a OsString) -> Arg<'a> {
        Arg {
            os,
            units: units(os),
        }
    }

    fn starts_with_dash(&self) -> bool {
        self.units.first() == Some(&MINUS)
    }

    fn starts_with_double_dash(&self) -> bool {
        self.units.starts_with(&[MINUS, MINUS])
    }

    fn eq_ignore_case(&self, text: &str) -> bool {
        self.units.len() == text.len()
            && self
                .units
                .iter()
                .zip(text.bytes())
                .all(|(&u, b)| u < 128 && (u as u8).eq_ignore_ascii_case(&b))
    }

    /// `-h` or `--help`, in any case, and nothing else.
    fn is_help(&self) -> bool {
        self.eq_ignore_case("-h") || self.eq_ignore_case("--help")
    }
}

/// The flags each subcommand has, by lowercase name.
fn flags_of(sub: Sub) -> &'static [&'static str] {
    match sub {
        Sub::Create => &["project", "platform", "profile", "size", "ttl"],
        Sub::Get | Sub::Release => &[],
        Sub::Renew => &["ttl"],
        Sub::List => &["project", "state"],
    }
}

/// A flag of the subcommand, as it stands in one argument.
struct FlagArg {
    name: &'static str,
    /// The units after the first `=`, when the argument has one.
    equals_value: Option<Vec<u32>>,
}

/// Reads an argument as a flag of the subcommand: `--` and the flag's name in ASCII letters, in any
/// case, then optionally `=` and a value. Anything else is not a flag of it.
fn as_flag(arg: &Arg<'_>, sub: Sub) -> Option<FlagArg> {
    if !arg.starts_with_double_dash() {
        return None;
    }
    let rest = &arg.units[2..];
    let eq = rest.iter().position(|&u| u == EQUALS);
    let name_units = &rest[..eq.unwrap_or(rest.len())];
    if name_units.is_empty()
        || !name_units
            .iter()
            .all(|&u| u < 128 && (u as u8).is_ascii_alphabetic())
    {
        return None;
    }
    let lower: String = name_units
        .iter()
        .map(|&u| char::from(u as u8).to_ascii_lowercase())
        .collect();
    let name = flags_of(sub).iter().find(|&&f| f == lower)?;
    Some(FlagArg {
        name,
        equals_value: eq.map(|i| rest[i + 1..].to_vec()),
    })
}

fn usage(message: String, field: Option<&str>) -> LeaseError {
    LeaseError {
        code: ErrorCode::Usage,
        message,
        field: field.map(str::to_owned),
        lease_id: None,
        state: None,
    }
}

fn unknown_option(arg: &Arg<'_>, level: Level) -> LeaseError {
    let value = quote_os(arg.os);
    usage(
        match level {
            Level::Top => {
                format!("Unknown option {value}. Run 'agentspace --help' for its options.")
            }
            Level::Lease => {
                format!("Unknown option {value}. Run 'agentspace lease --help' for its options.")
            }
            Level::Sub(sub) => format!(
                "Unknown option {value} for agentspace lease {n}. Run 'agentspace lease {n} --help'.",
                n = sub.name()
            ),
        },
        None,
    )
}

fn unexpected_argument(arg: &Arg<'_>, sub: Sub) -> LeaseError {
    usage(
        format!(
            "Unexpected argument {} for agentspace lease {n}. Run 'agentspace lease {n} --help'.",
            quote_os(arg.os),
            n = sub.name()
        ),
        None,
    )
}

fn sub_from(arg: &Arg<'_>) -> Option<Sub> {
    [Sub::Create, Sub::Get, Sub::Renew, Sub::Release, Sub::List]
        .into_iter()
        .find(|s| arg.eq_ignore_case(s.name()))
}

/// Reads the command line, which is the arguments after the program's name.
pub fn parse(args: &[OsString]) -> Parsed {
    let args: Vec<Arg<'_>> = args.iter().map(Arg::new).collect();

    let mut level = Level::Top;
    let mut help_level: Option<Level> = None;
    let mut unknown_word: Option<LeaseError> = None;
    let mut first_error: Option<LeaseError> = None;
    let mut flags: Vec<(String, OsString)> = Vec::new();
    let mut id: Option<OsString> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        i += 1;

        if arg.is_help() {
            // The first request for help fixes the level that is printed.
            help_level.get_or_insert(level);
            continue;
        }

        if arg.starts_with_dash() {
            let flag = match level {
                Level::Sub(sub) => as_flag(arg, sub).map(|f| (sub, f)),
                _ => None,
            };
            let Some((sub, flag)) = flag else {
                first_error.get_or_insert_with(|| unknown_option(arg, level));
                continue;
            };

            // The flag's value: after an equals sign always, else the next argument unless it
            // starts with `--` or asks for help (which is not consumed, and is read next).
            let value: Option<Vec<u32>> = match flag.equals_value {
                Some(value) => Some(value),
                None => match args.get(i) {
                    Some(next) if !next.is_help() && !next.starts_with_double_dash() => {
                        i += 1;
                        Some(next.units.clone())
                    }
                    _ => None,
                },
            };
            let sub_name = sub.name();
            let Some(value) = value else {
                first_error.get_or_insert_with(|| {
                    usage(
                        format!(
                            "Option --{} needs a value. Run 'agentspace lease {sub_name} --help'.",
                            flag.name
                        ),
                        Some(flag.name),
                    )
                });
                continue;
            };
            if value.is_empty() {
                first_error.get_or_insert_with(|| {
                    usage(
                        format!(
                            "Option --{} has an empty value. Run 'agentspace lease {sub_name} --help'.",
                            flag.name
                        ),
                        Some(flag.name),
                    )
                });
                continue;
            }
            if flag.name != "state" && flags.iter().any(|(n, _)| n == flag.name) {
                first_error.get_or_insert_with(|| {
                    usage(
                        format!("Option --{} is given twice. Give it once.", flag.name),
                        Some(flag.name),
                    )
                });
                continue;
            }
            flags.push((flag.name.to_owned(), from_units(&value)));
            continue;
        }

        match level {
            Level::Top => {
                if arg.eq_ignore_case("lease") {
                    level = Level::Lease;
                } else {
                    unknown_word = Some(usage(
                        format!(
                            "Unknown command {}. Run 'agentspace --help' for the commands.",
                            quote_os(arg.os)
                        ),
                        None,
                    ));
                    break;
                }
            }
            Level::Lease => match sub_from(arg) {
                Some(sub) => level = Level::Sub(sub),
                None => {
                    unknown_word = Some(usage(
                        format!(
                            "Unknown subcommand {}. Run 'agentspace lease --help' for the subcommands.",
                            quote_os(arg.os)
                        ),
                        None,
                    ));
                    break;
                }
            },
            Level::Sub(sub) => {
                let takes_id = matches!(sub, Sub::Get | Sub::Renew | Sub::Release);
                if takes_id && id.is_none() {
                    id = Some(arg.os.clone());
                } else {
                    first_error.get_or_insert_with(|| unexpected_argument(arg, sub));
                }
            }
        }
    }

    // 1. Help anywhere before an unknown command or subcommand, whatever else is on the line.
    if let Some(at) = help_level {
        return match at {
            Level::Top => Parsed::Help(help::TOP, None),
            Level::Lease => Parsed::Help(help::LEASE, None),
            Level::Sub(sub) => Parsed::Help(
                match sub {
                    Sub::Create => help::CREATE,
                    Sub::Get => help::GET,
                    Sub::Renew => help::RENEW,
                    Sub::Release => help::RELEASE,
                    Sub::List => help::LIST,
                },
                Some(sub),
            ),
        };
    }
    // 2. An unknown command or subcommand, as soon as it was read.
    if let Some(error) = unknown_word {
        return Parsed::Error(error);
    }
    // 3. The first of the other usage errors in line order.
    if let Some(error) = first_error {
        return Parsed::Error(error);
    }
    // 4. A missing command, subcommand, lease ID or `--project`.
    match level {
        Level::Top => Parsed::Error(usage(
            "No command given. Run 'agentspace --help' for the commands.".to_owned(),
            None,
        )),
        Level::Lease => Parsed::Error(usage(
            "No subcommand given. Run 'agentspace lease --help' for the subcommands.".to_owned(),
            None,
        )),
        Level::Sub(sub) => {
            let takes_id = matches!(sub, Sub::Get | Sub::Renew | Sub::Release);
            if takes_id && id.is_none() {
                return Parsed::Error(usage(
                    format!(
                        "No lease ID given. Run 'agentspace lease {} --help'.",
                        sub.name()
                    ),
                    Some("id"),
                ));
            }
            if sub == Sub::Create && !flags.iter().any(|(n, _)| n == "project") {
                return Parsed::Error(usage(
                    "Option --project is required. Run 'agentspace lease create --help'."
                        .to_owned(),
                    Some("project"),
                ));
            }
            Parsed::Command(ParsedCommand { sub, flags, id })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(list: &[&str]) -> Parsed {
        parse(&list.iter().map(OsString::from).collect::<Vec<_>>())
    }

    fn usage_of(list: &[&str]) -> LeaseError {
        match run(list) {
            Parsed::Error(e) => e,
            other => panic!("expected an error for {list:?}, got {other:?}"),
        }
    }

    #[test]
    fn help_levels() {
        assert_eq!(run(&["--help"]), Parsed::Help(help::TOP, None));
        assert_eq!(run(&["--help", "lease"]), Parsed::Help(help::TOP, None));
        assert_eq!(run(&["Lease", "-H"]), Parsed::Help(help::LEASE, None));
        assert_eq!(
            run(&["lease", "create", "--ttl", "-h"]),
            Parsed::Help(help::CREATE, Some(Sub::Create))
        );
        assert_eq!(
            run(&["lease", "list", "--bogus", "--HELP"]),
            Parsed::Help(help::LIST, Some(Sub::List))
        );
        assert_eq!(
            run(&["lease", "--help", "frob"]),
            Parsed::Help(help::LEASE, None)
        );
        assert_eq!(
            run(&["lease", "--help", "create"]),
            Parsed::Help(help::LEASE, None)
        );
        assert_eq!(
            run(&["lease", "get", "x", "--help"]),
            Parsed::Help(help::GET, Some(Sub::Get))
        );
    }

    #[test]
    fn the_equals_form_makes_help_a_value() {
        match run(&["lease", "create", "--ttl=--help", "--project=-h"]) {
            Parsed::Command(c) => {
                assert_eq!(c.flags[0], ("ttl".to_owned(), OsString::from("--help")));
                assert_eq!(c.flags[1], ("project".to_owned(), OsString::from("-h")));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_value_may_start_with_one_dash_and_flags_match_in_any_case() {
        match run(&["lease", "renew", "--TTL", "-5m", "ID"]) {
            Parsed::Command(c) => {
                assert_eq!(c.sub, Sub::Renew);
                assert_eq!(c.id, Some(OsString::from("ID")));
                assert_eq!(c.flags, vec![("ttl".to_owned(), OsString::from("-5m"))]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn state_may_repeat_and_keeps_line_order() {
        match run(&[
            "lease",
            "list",
            "--state",
            "a",
            "--project",
            "p",
            "--state=b,c",
        ]) {
            Parsed::Command(c) => assert_eq!(
                c.flags,
                vec![
                    ("state".to_owned(), OsString::from("a")),
                    ("project".to_owned(), OsString::from("p")),
                    ("state".to_owned(), OsString::from("b,c")),
                ]
            ),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unknown_words_stop_reading_and_come_first() {
        assert_eq!(
            usage_of(&["--project", "x", "lease", "create"]).message,
            "Unknown command 'x'. Run 'agentspace --help' for the commands."
        );
        assert_eq!(
            usage_of(&["lease", "--bogus", "frob"]).message,
            "Unknown subcommand 'frob'. Run 'agentspace lease --help' for the subcommands."
        );
        assert!(matches!(run(&["frob", "--help"]), Parsed::Error(_)));
    }

    #[test]
    fn the_five_cases_with_a_field() {
        let cases: [(&[&str], &str); 7] = [
            (&["lease", "create", "--project", "x", "--ttl"], "ttl"),
            (&["lease", "create", "--project", "x", "--ttl", ""], "ttl"),
            (
                &["lease", "create", "--project", "x", "--ttl=1", "--TTL=2"],
                "ttl",
            ),
            (&["lease", "get"], "id"),
            (&["lease", "create"], "project"),
            (&["lease", "list", "--state", "x", "--state"], "state"),
            (&["lease", "list", "--project", "a", "--project"], "project"),
        ];
        for (line, field) in cases {
            assert_eq!(usage_of(line).field.as_deref(), Some(field), "{line:?}");
        }
        let without: [&[&str]; 5] = [
            &["--bogus"],
            &["lease", "get", "a", "b"],
            &["lease", "list", "x"],
            &["lease"],
            &[],
        ];
        for line in without {
            assert_eq!(usage_of(line).field, None, "{line:?}");
        }
    }

    #[test]
    fn a_later_flag_without_a_value_is_not_reported_as_twice() {
        let line = [
            "lease",
            "create",
            "--project",
            "x",
            "--project",
            "--size",
            "m",
        ];
        assert!(usage_of(&line).message.contains("needs a value"));
        let line = ["lease", "create", "--project", "x", "--project="];
        assert!(usage_of(&line).message.contains("empty value"));
    }

    #[test]
    fn tokens_that_are_not_flags_are_unknown_options() {
        for tok in ["-ttl", "-p", "-", "--", "--help=yes", "--ttl:", "--=x"] {
            let e = usage_of(&["lease", "create", "--project", "x", tok]);
            assert!(
                e.message.starts_with(&format!("Unknown option '{tok}'")),
                "{tok}"
            );
        }
    }

    #[test]
    fn an_empty_argument_is_a_word() {
        assert_eq!(
            usage_of(&[""]).message,
            "Unknown command ''. Run 'agentspace --help' for the commands."
        );
        assert_eq!(
            usage_of(&["lease", "get", "x", ""]).message,
            "Unexpected argument '' for agentspace lease get. Run 'agentspace lease get --help'."
        );
    }

    #[cfg(unix)]
    #[test]
    fn arguments_that_are_not_unicode_are_read_as_written() {
        use std::os::unix::ffi::OsStringExt;
        let bad = OsString::from_vec(vec![0xff, b'x']);
        let line = vec![OsString::from("lease"), OsString::from("get"), bad.clone()];
        match parse(&line) {
            Parsed::Command(c) => assert_eq!(c.id, Some(bad.clone())),
            other => panic!("{other:?}"),
        }
        match parse(&[bad]) {
            Parsed::Error(e) => assert_eq!(
                e.message,
                "Unknown command '\\udcffx'. Run 'agentspace --help' for the commands."
            ),
            other => panic!("{other:?}"),
        }
    }
}
