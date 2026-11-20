//! Usage errors and modes of the keeper installer (keeper.md, The installer), from the spec alone.

use agentspace_keeper_install::{Mode, parse_args};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

/// The usage error's message, or None when the arguments parsed.
fn error_of(list: &[&str]) -> Option<String> {
    parse_args(&args(list)).err()
}

fn mode_of(list: &[&str]) -> Mode {
    match parse_args(&args(list)) {
        Ok(mode) => mode,
        Err(message) => panic!("Arguments {list:?} were refused: {message}"),
    }
}

#[test]
fn no_arguments_is_install() {
    assert!(matches!(mode_of(&[]), Mode::Install));
}

#[test]
fn dry_run_alone_is_dry_run() {
    assert!(matches!(mode_of(&["-DryRun"]), Mode::DryRun));
}

#[test]
fn uninstall_alone_is_uninstall() {
    assert!(matches!(mode_of(&["-Uninstall"]), Mode::Uninstall));
}

#[test]
fn names_match_without_regard_to_case() {
    for raw in ["-dryrun", "-DRYRUN", "-dRyRuN"] {
        assert!(matches!(mode_of(&[raw]), Mode::DryRun), "{raw}");
    }
    for raw in ["-uninstall", "-UNINSTALL", "-uNiNsTaLl"] {
        assert!(matches!(mode_of(&[raw]), Mode::Uninstall), "{raw}");
    }
}

#[test]
fn names_match_exactly_so_prefixes_and_values_are_unknown() {
    for raw in [
        "-Dry",
        "-DryRu",
        "-DryRuns",
        "-Un",
        "-Uninstal",
        "-DryRun:true",
        "-DryRun=1",
        "DryRun",
        "--DryRun",
        "--Uninstall",
        "Uninstall",
        "-",
        "--",
        "",
        " -DryRun",
        "-DryRun ",
    ] {
        assert_eq!(
            error_of(&[raw]),
            Some(format!("Unknown argument '{raw}'.")),
            "{raw:?}"
        );
    }
}

#[test]
fn help_spellings_are_unknown_arguments() {
    for raw in ["-h", "--help", "-?", "-H", "--HELP", "-help", "/?"] {
        assert_eq!(
            error_of(&[raw]),
            Some(format!("Unknown argument '{raw}'.")),
            "{raw}"
        );
    }
}

#[test]
fn positional_argument_is_unknown_and_keeps_its_raw_text() {
    assert_eq!(
        error_of(&["Some Thing"]),
        Some("Unknown argument 'Some Thing'.".to_string())
    );
}

#[test]
fn unknown_argument_reports_the_raw_text_not_a_lowercased_one() {
    assert_eq!(
        error_of(&["-DryRunX"]),
        Some("Unknown argument '-DryRunX'.".to_string())
    );
    assert_eq!(
        error_of(&["-WaTcH"]),
        Some("Unknown argument '-WaTcH'.".to_string())
    );
}

#[test]
fn first_unknown_argument_left_to_right_is_reported() {
    assert_eq!(
        error_of(&["-DryRun", "-First", "-Second"]),
        Some("Unknown argument '-First'.".to_string())
    );
    assert_eq!(
        error_of(&["-Zed", "-Alpha"]),
        Some("Unknown argument '-Zed'.".to_string())
    );
}

#[test]
fn a_name_given_twice_is_reported_with_the_spec_spelling() {
    assert_eq!(
        error_of(&["-DryRun", "-DryRun"]),
        Some("-DryRun is given twice.".to_string())
    );
    assert_eq!(
        error_of(&["-Uninstall", "-Uninstall"]),
        Some("-Uninstall is given twice.".to_string())
    );
}

#[test]
fn a_name_given_twice_is_matched_without_regard_to_case() {
    assert_eq!(
        error_of(&["-dryrun", "-DRYRUN"]),
        Some("-DryRun is given twice.".to_string())
    );
    assert_eq!(
        error_of(&["-UNINSTALL", "-uninstall"]),
        Some("-Uninstall is given twice.".to_string())
    );
}

#[test]
fn a_name_given_three_times_is_still_given_twice() {
    assert_eq!(
        error_of(&["-DryRun", "-DryRun", "-DryRun"]),
        Some("-DryRun is given twice.".to_string())
    );
}

#[test]
fn dry_run_with_uninstall_cannot_be_given_together_in_either_order() {
    let expected = Some("-DryRun and -Uninstall cannot be given together.".to_string());
    assert_eq!(error_of(&["-DryRun", "-Uninstall"]), expected);
    assert_eq!(error_of(&["-Uninstall", "-DryRun"]), expected);
    assert_eq!(error_of(&["-dryrun", "-UNINSTALL"]), expected);
}

#[test]
fn check_one_comes_before_check_two() {
    // An unknown argument after both names still wins over the pair.
    assert_eq!(
        error_of(&["-DryRun", "-Uninstall", "-Bogus"]),
        Some("Unknown argument '-Bogus'.".to_string())
    );
    assert_eq!(
        error_of(&["-Bogus", "-DryRun", "-Uninstall"]),
        Some("Unknown argument '-Bogus'.".to_string())
    );
    // A repeated name wins over the pair, wherever the repeat is.
    assert_eq!(
        error_of(&["-DryRun", "-Uninstall", "-DryRun"]),
        Some("-DryRun is given twice.".to_string())
    );
    assert_eq!(
        error_of(&["-DryRun", "-DryRun", "-Uninstall"]),
        Some("-DryRun is given twice.".to_string())
    );
    assert_eq!(
        error_of(&["-Uninstall", "-DryRun", "-Uninstall"]),
        Some("-Uninstall is given twice.".to_string())
    );
}

#[test]
fn checks_run_left_to_right_between_unknown_and_repeat() {
    assert_eq!(
        error_of(&["-DryRun", "-DryRun", "-Bogus"]),
        Some("-DryRun is given twice.".to_string())
    );
    assert_eq!(
        error_of(&["-Bogus", "-DryRun", "-DryRun"]),
        Some("Unknown argument '-Bogus'.".to_string())
    );
}

#[test]
fn a_usage_message_has_no_prefix_and_no_usage_line() {
    // The message is the text after `agentspace-keeper-install: `, one line.
    for list in [
        vec!["-x"],
        vec!["-DryRun", "-DryRun"],
        vec!["-DryRun", "-Uninstall"],
    ] {
        let message = error_of(&list).expect("A usage error");
        assert!(
            !message.starts_with("agentspace-keeper-install"),
            "{message}"
        );
        assert!(!message.contains('\n'), "{message}");
        assert!(!message.contains("Usage:"), "{message}");
    }
}
