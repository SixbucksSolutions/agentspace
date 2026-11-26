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
        Sub::Create => &[
            "project", "platform", "profile", "plan", "board", "epic", "turn", "size", "ttl",
        ],
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

/// `create`'s pairs of flags that cannot be given together, each named in its message's order: the
/// board-side flag, or `--plan`, first (lease-api.md, Command lines, "Which of create's pairs and
/// partners is reported", rules 1 and 7; D75, D86).
const PAIRS: [(&str, &str); 5] = [
    ("plan", "size"),
    ("board", "plan"),
    ("board", "size"),
    ("epic", "plan"),
    ("epic", "size"),
];

/// The pair that a flag just read ends, when it ends one: of the pairs whose other flag already
/// stands on the line, the one whose earlier flag stands first (rule 3). `earlier` holds the flags
/// read so far, in line order.
fn pair_ended_by(
    name: &str,
    earlier: &[(String, OsString)],
) -> Option<(&'static str, &'static str)> {
    PAIRS
        .iter()
        .filter_map(|&(a, b)| {
            let other = if name == a {
                b
            } else if name == b {
                a
            } else {
                return None;
            };
            let at = earlier.iter().position(|(n, _)| n == other)?;
            Some((at, (a, b)))
        })
        .min_by_key(|&(at, _)| at)
        .map(|(_, pair)| pair)
}

/// A pair's usage error, with the `field` it carries before the whole line is read: `plan` for
/// `--plan` with `--size`, and otherwise the pair's board-side flag. A pair with `--epic` names
/// `board` instead once `--board` is found anywhere on the line (rule 5), which [`parse`] settles at
/// the end.
fn pair_error((a, b): (&str, &str)) -> LeaseError {
    let message = match (a, b) {
        ("plan", _) => "Options --plan and --size cannot be given together; the plan picks the \
                        size. Run 'agentspace lease create --help'."
            .to_owned(),
        (_, "plan") => format!(
            "Options --{a} and --plan cannot be given together; the epic carries the plan. Run \
             'agentspace lease create --help'."
        ),
        _ => format!(
            "Options --{a} and --size cannot be given together; the epic's plan picks the size. \
             Run 'agentspace lease create --help'."
        ),
    };
    usage(message, Some(a))
}

/// `create`'s missing partners, in rule 6's order, once the line has no other usage error:
/// `--board` without `--epic`, `--epic` without `--board`, and `--turn` without `--plan` or
/// `--epic` (lease-api.md, Command lines; Messages).
fn missing_partner(flags: &[(String, OsString)]) -> Option<LeaseError> {
    let has = |name: &str| flags.iter().any(|(n, _)| n == name);
    if has("board") && !has("epic") {
        return Some(usage(
            "Option --board needs --epic, the epic's ticket number. Run 'agentspace lease create \
             --help'."
                .to_owned(),
            Some("epic"),
        ));
    }
    if has("epic") && !has("board") {
        return Some(usage(
            "Option --epic needs --board, the board's ID. Run 'agentspace lease create --help'."
                .to_owned(),
            Some("board"),
        ));
    }
    if has("turn") && !has("plan") && !has("epic") {
        return Some(usage(
            "Option --turn needs a plan, from --plan or --epic. Run 'agentspace lease create \
             --help'."
                .to_owned(),
            Some("turn"),
        ));
    }
    None
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
    // Whether `--board` stands anywhere on `create`'s line, with a value or not, and whether the
    // error kept is a pair with `--epic`, whose `field` that decides (rule 5).
    let mut board_on_line = false;
    let mut first_is_epic_pair = false;

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
            if flag.name == "board" {
                board_on_line = true;
            }

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
            // A pair of `create`'s flags that cannot be given together, in either order, reported
            // where its later flag stands (lease-api.md, Command lines; D75, D86; pins P4, P6, P13
            // and P20). A later one with no value or an empty value was reported as that above.
            if let Some(pair) = pair_ended_by(flag.name, &flags) {
                if first_error.is_none() {
                    first_error = Some(pair_error(pair));
                    first_is_epic_pair = pair.0 == "epic";
                }
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
    // 3. The first of the other usage errors in line order. A pair with `--epic` names `board`
    // whenever `--board` is on the line, wherever it stands (rule 5).
    if let Some(mut error) = first_error {
        if first_is_epic_pair && board_on_line {
            error.field = Some("board".to_owned());
        }
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
            if sub == Sub::Create
                && let Some(error) = missing_partner(&flags)
            {
                return Parsed::Error(error);
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

    const TOGETHER: &str = "Options --plan and --size cannot be given together; the plan picks \
                            the size. Run 'agentspace lease create --help'.";

    #[test]
    fn plan_with_size_is_a_usage_error_for_plan_in_either_order() {
        for line in [
            &[
                "lease",
                "create",
                "--project",
                "x",
                "--plan",
                "p",
                "--size",
                "m",
            ][..],
            &[
                "lease",
                "create",
                "--SIZE=m",
                "--project",
                "x",
                "--Plan",
                "p",
            ][..],
        ] {
            let e = usage_of(line);
            assert_eq!(e.code, ErrorCode::Usage, "{line:?}");
            assert_eq!(e.field.as_deref(), Some("plan"), "{line:?}");
            assert_eq!(e.message, TOGETHER, "{line:?}");
        }
    }

    #[test]
    fn plan_with_size_is_reported_where_the_later_stands_and_in_line_order() {
        // An earlier error on the line comes first.
        let e = usage_of(&["lease", "create", "--bogus", "--plan", "p", "--size", "m"]);
        assert!(e.message.starts_with("Unknown option '--bogus'"));
        // An error after the later flag comes after it.
        let e = usage_of(&["lease", "create", "--plan", "p", "--size", "m", "--bogus"]);
        assert_eq!(e.message, TOGETHER);
        // A later one with no value, or an empty one, is reported as that.
        let e = usage_of(&["lease", "create", "--size", "m", "--plan"]);
        assert_eq!(
            (e.message.as_str(), e.field.as_deref()),
            (
                "Option --plan needs a value. Run 'agentspace lease create --help'.",
                Some("plan")
            )
        );
        let e = usage_of(&["lease", "create", "--plan", "p", "--size="]);
        assert_eq!(
            (e.message.as_str(), e.field.as_deref()),
            (
                "Option --size has an empty value. Run 'agentspace lease create --help'.",
                Some("size")
            )
        );
        // Help still wins.
        assert_eq!(
            run(&["lease", "create", "--plan", "p", "--size", "m", "-h"]),
            Parsed::Help(help::CREATE, Some(Sub::Create))
        );
    }

    #[test]
    fn plan_alone_is_a_flag_of_create_only() {
        match run(&["lease", "create", "--project", "x", "--plan", " p "]) {
            Parsed::Command(c) => assert_eq!(c.flags[1], ("plan".to_owned(), " p ".into())),
            other => panic!("{other:?}"),
        }
        let e = usage_of(&["lease", "list", "--plan", "p"]);
        assert!(
            e.message
                .starts_with("Unknown option '--plan' for agentspace lease list")
        );
        let e = usage_of(&[
            "lease",
            "create",
            "--project",
            "x",
            "--plan",
            "a",
            "--plan=b",
        ]);
        assert_eq!(e.message, "Option --plan is given twice. Give it once.");
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

    const B: &str = "0a1b2c3d-0a1b-4c2d-8e3f-0a1b2c3d4e5f";
    const BOARD_PLAN: &str = "Options --board and --plan cannot be given together; the epic \
                              carries the plan. Run 'agentspace lease create --help'.";
    const BOARD_SIZE: &str = "Options --board and --size cannot be given together; the epic's \
                              plan picks the size. Run 'agentspace lease create --help'.";
    const EPIC_PLAN: &str = "Options --epic and --plan cannot be given together; the epic \
                             carries the plan. Run 'agentspace lease create --help'.";
    const EPIC_SIZE: &str = "Options --epic and --size cannot be given together; the epic's \
                             plan picks the size. Run 'agentspace lease create --help'.";

    /// `create --project x` and then `rest`.
    fn create(rest: &[&str]) -> Vec<String> {
        let mut line = vec!["lease".to_owned(), "create".to_owned()];
        line.extend(["--project".to_owned(), "x".to_owned()]);
        line.extend(rest.iter().map(|s| (*s).to_owned()));
        line
    }

    fn usage_of_line(line: &[String]) -> LeaseError {
        let refs: Vec<&str> = line.iter().map(String::as_str).collect();
        usage_of(&refs)
    }

    fn message_and_field(e: &LeaseError) -> (&str, Option<&str>) {
        (e.message.as_str(), e.field.as_deref())
    }

    #[test]
    fn each_board_side_pair_in_either_order_names_its_board_side_flag() {
        let cases: [(&[&str], &str, &str); 8] = [
            (&["--board", B, "--plan", "p"], BOARD_PLAN, "board"),
            (&["--plan", "p", "--board", B], BOARD_PLAN, "board"),
            (&["--board", B, "--size", "m"], BOARD_SIZE, "board"),
            (&["--size", "m", "--BOARD", B], BOARD_SIZE, "board"),
            (&["--epic", "7", "--plan", "p"], EPIC_PLAN, "epic"),
            (&["--plan=p", "--epic", "7"], EPIC_PLAN, "epic"),
            (&["--epic", "7", "--size", "m"], EPIC_SIZE, "epic"),
            (&["--size", "m", "--Epic=7"], EPIC_SIZE, "epic"),
        ];
        for (rest, message, field) in cases {
            let e = usage_of_line(&create(rest));
            assert_eq!(e.code, ErrorCode::Usage, "{rest:?}");
            assert_eq!(message_and_field(&e), (message, Some(field)), "{rest:?}");
        }
    }

    #[test]
    fn a_pair_with_epic_names_board_whenever_board_is_on_the_line() {
        // Rule 5's example: reported at --plan, and --board stands later.
        let e = usage_of_line(&create(&["--epic", "7", "--plan", "p.json", "--board", B]));
        assert_eq!(message_and_field(&e), (EPIC_PLAN, Some("board")));
        let e = usage_of_line(&create(&["--epic", "7", "--size", "m", "--board", B]));
        assert_eq!(message_and_field(&e), (EPIC_SIZE, Some("board")));
        // A --board with no value is on the line too.
        let e = usage_of_line(&create(&["--epic", "7", "--size", "m", "--board"]));
        assert_eq!(message_and_field(&e), (EPIC_SIZE, Some("board")));
    }

    #[test]
    fn two_pairs_ending_at_one_flag_report_the_one_whose_earlier_flag_stands_first() {
        // Rule 3's example.
        let e = usage_of_line(&create(&["--board", B, "--epic", "7", "--plan", "p.json"]));
        assert_eq!(message_and_field(&e), (BOARD_PLAN, Some("board")));
        let e = usage_of_line(&create(&["--epic", "7", "--board", B, "--size", "m"]));
        assert_eq!(message_and_field(&e), (EPIC_SIZE, Some("board")));
        // Rule 4's example: the first pair in line order.
        let e = usage_of_line(&create(&["--plan", "p.json", "--board", B, "--size", "m"]));
        assert_eq!(message_and_field(&e), (BOARD_PLAN, Some("board")));
        // --plan with --size still names plan.
        let e = usage_of_line(&create(&["--plan", "p.json", "--size", "m", "--board", B]));
        assert_eq!(message_and_field(&e), (TOGETHER, Some("plan")));
    }

    #[test]
    fn a_pair_ranks_in_line_order_and_a_later_flag_with_no_value_is_that() {
        let e = usage_of_line(&create(&["--bogus", "--board", B, "--plan", "p"]));
        assert!(e.message.starts_with("Unknown option '--bogus'"));
        let e = usage_of_line(&create(&["--board", B, "--plan", "p", "--bogus"]));
        assert_eq!(e.message, BOARD_PLAN);
        let e = usage_of_line(&create(&["--plan", "p", "--board"]));
        assert_eq!(
            message_and_field(&e),
            (
                "Option --board needs a value. Run 'agentspace lease create --help'.",
                Some("board")
            )
        );
        let e = usage_of_line(&create(&["--size", "m", "--epic="]));
        assert_eq!(
            message_and_field(&e),
            (
                "Option --epic has an empty value. Run 'agentspace lease create --help'.",
                Some("epic")
            )
        );
        let e = usage_of_line(&create(&["--turn", "T", "--turn=U"]));
        assert_eq!(
            message_and_field(&e),
            ("Option --turn is given twice. Give it once.", Some("turn"))
        );
        // Help still wins over a pair.
        assert_eq!(
            run(&["lease", "create", "--board", B, "--plan", "p", "--help"]),
            Parsed::Help(help::CREATE, Some(Sub::Create))
        );
    }

    const NO_EPIC: &str = "Option --board needs --epic, the epic's ticket number. Run 'agentspace lease create --help'.";
    const NO_BOARD: &str =
        "Option --epic needs --board, the board's ID. Run 'agentspace lease create --help'.";
    const NO_PLAN: &str =
        "Option --turn needs a plan, from --plan or --epic. Run 'agentspace lease create --help'.";

    #[test]
    fn each_missing_partner_names_the_flag_that_is_missing_in_rule_6s_order() {
        let cases: [(&[&str], &str, &str); 6] = [
            (&["--board", B], NO_EPIC, "epic"),
            (&["--epic", "7"], NO_BOARD, "board"),
            (&["--turn", "T01"], NO_PLAN, "turn"),
            // Rule 6's example: --board without --epic before --turn without a plan.
            (&["--board", B, "--turn", "T01"], NO_EPIC, "epic"),
            (&["--turn", "T01", "--epic", "7"], NO_BOARD, "board"),
            (&["--turn", "T01", "--size", "m"], NO_PLAN, "turn"),
        ];
        for (rest, message, field) in cases {
            let e = usage_of_line(&create(rest));
            assert_eq!(e.code, ErrorCode::Usage, "{rest:?}");
            assert_eq!(message_and_field(&e), (message, Some(field)), "{rest:?}");
        }
        // A missing --project comes first.
        let e = usage_of(&["lease", "create", "--board", B]);
        assert_eq!(e.field.as_deref(), Some("project"));
        // A usage error of step 3 comes before any missing partner.
        let e = usage_of_line(&create(&["--board", B, "--bogus"]));
        assert!(e.message.starts_with("Unknown option '--bogus'"));
    }

    #[test]
    fn board_epic_and_turn_with_their_partners_are_a_command() {
        for rest in [
            &["--board", B, "--epic", "7"][..],
            &["--epic", "7", "--BOARD", B, "--turn", "T01"][..],
            &["--plan", "p", "--turn", "T01"][..],
            &["--turn= T01 ", "--plan", "p"][..],
        ] {
            let line = create(rest);
            let refs: Vec<&str> = line.iter().map(String::as_str).collect();
            match run(&refs) {
                Parsed::Command(c) => assert_eq!(c.sub, Sub::Create, "{rest:?}"),
                other => panic!("{rest:?}: {other:?}"),
            }
        }
        // The turn is kept as given, never trimmed.
        let line = create(&["--plan", "p", "--turn", " T01 "]);
        let refs: Vec<&str> = line.iter().map(String::as_str).collect();
        match run(&refs) {
            Parsed::Command(c) => assert!(c.flags.contains(&("turn".to_owned(), " T01 ".into()))),
            other => panic!("{other:?}"),
        }
        // They are flags of create alone.
        let e = usage_of(&["lease", "list", "--board", B]);
        assert!(
            e.message
                .starts_with("Unknown option '--board' for agentspace lease list")
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
