//! The installer's arguments and usage errors (keeper.md, The installer; operator.md, Argument style).

/// What the caller asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// No argument: a real install.
    Install,
    /// `-DryRun`: print the task's definition and change nothing.
    DryRun,
    /// `-Uninstall`: end and unregister the task.
    Uninstall,
}

/// The second line of every usage error.
pub const USAGE: &str = "Usage: agentspace-keeper-install [-DryRun | -Uninstall]";

/// The prefix of the first line of every usage error.
pub const PROGRAM_PREFIX: &str = "agentspace-keeper-install: ";

const DRY_RUN: &str = "DryRun";
const UNINSTALL: &str = "Uninstall";

/// The arguments after the program's name. `Err` holds the usage error's message, the text after
/// `agentspace-keeper-install: `. The checks run in the spec's order, and the first that fails is
/// the one reported: an argument that is neither name, or a name given twice, taken left to right,
/// then `-DryRun` with `-Uninstall`. A name is matched exactly and without regard to case, and
/// takes no value.
pub fn parse_args(args: &[String]) -> Result<Mode, String> {
    let mut dry_run = false;
    let mut uninstall = false;
    for raw in args {
        let name = match name_of(raw) {
            Some(name) => name,
            None => return Err(format!("Unknown argument '{raw}'.")),
        };
        let seen = if name == DRY_RUN {
            &mut dry_run
        } else {
            &mut uninstall
        };
        if *seen {
            return Err(format!("-{name} is given twice."));
        }
        *seen = true;
    }
    match (dry_run, uninstall) {
        (true, true) => Err("-DryRun and -Uninstall cannot be given together.".to_string()),
        (true, false) => Ok(Mode::DryRun),
        (false, true) => Ok(Mode::Uninstall),
        (false, false) => Ok(Mode::Install),
    }
}

/// The argument's name as the spec spells it, if `raw` is one of the two switches.
fn name_of(raw: &str) -> Option<&'static str> {
    let bare = raw.strip_prefix('-')?;
    [DRY_RUN, UNINSTALL]
        .into_iter()
        .find(|name| bare.eq_ignore_ascii_case(name))
}

/// The two lines a usage error writes to stderr, without line endings.
pub fn usage_error_lines(message: &str) -> [String; 2] {
    [format!("{PROGRAM_PREFIX}{message}"), USAGE.to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn the_three_modes() {
        assert_eq!(parse_args(&args(&[])), Ok(Mode::Install));
        assert_eq!(parse_args(&args(&["-DryRun"])), Ok(Mode::DryRun));
        assert_eq!(parse_args(&args(&["-Uninstall"])), Ok(Mode::Uninstall));
    }

    #[test]
    fn names_ignore_case() {
        assert_eq!(parse_args(&args(&["-dRyRuN"])), Ok(Mode::DryRun));
        assert_eq!(parse_args(&args(&["-UNINSTALL"])), Ok(Mode::Uninstall));
    }

    #[test]
    fn an_unknown_argument_is_named_as_given() {
        for raw in [
            "-h",
            "--help",
            "-?",
            "DryRun",
            "--DryRun",
            "-DryRun:1",
            "",
            "-Dry",
        ] {
            assert_eq!(
                parse_args(&args(&[raw])),
                Err(format!("Unknown argument '{raw}'."))
            );
        }
    }

    #[test]
    fn the_first_unknown_argument_left_to_right_is_reported() {
        assert_eq!(
            parse_args(&args(&["-DryRun", "-x", "-y"])),
            Err("Unknown argument '-x'.".to_string())
        );
    }

    #[test]
    fn a_name_twice_is_reported_with_the_spec_spelling() {
        assert_eq!(
            parse_args(&args(&["-dryrun", "-DRYRUN"])),
            Err("-DryRun is given twice.".to_string())
        );
        assert_eq!(
            parse_args(&args(&["-uninstall", "-Uninstall"])),
            Err("-Uninstall is given twice.".to_string())
        );
    }

    #[test]
    fn check_one_comes_before_check_two() {
        assert_eq!(
            parse_args(&args(&["-DryRun", "-Uninstall", "-DryRun"])),
            Err("-DryRun is given twice.".to_string())
        );
        assert_eq!(
            parse_args(&args(&["-DryRun", "-Uninstall", "-x"])),
            Err("Unknown argument '-x'.".to_string())
        );
    }

    #[test]
    fn both_names_together_are_refused_in_either_order() {
        let message = "-DryRun and -Uninstall cannot be given together.";
        assert_eq!(
            parse_args(&args(&["-DryRun", "-Uninstall"])),
            Err(message.to_string())
        );
        assert_eq!(
            parse_args(&args(&["-Uninstall", "-DryRun"])),
            Err(message.to_string())
        );
    }

    #[test]
    fn a_usage_error_is_two_lines() {
        assert_eq!(
            usage_error_lines("Unknown argument '-x'."),
            [
                "agentspace-keeper-install: Unknown argument '-x'.".to_string(),
                "Usage: agentspace-keeper-install [-DryRun | -Uninstall]".to_string()
            ]
        );
    }
}
