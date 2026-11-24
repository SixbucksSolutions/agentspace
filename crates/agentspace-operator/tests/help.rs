//! Help at every level, byte for byte from docs/operator.md, read at test time (docs/operator.md,
//! Command line: help is checked first; gap FB-11 says which argument is the command; gap FA-13
//! says a line feed ends every line).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

#[path = "common/cli.rs"]
mod cli;

use cli::*;

#[test]
fn the_extractor_finds_every_level_in_the_doc() {
    // Proves the extractor can find something before any test trusts it (Terry's global rule).
    let blocks = help_blocks();
    let mut found: Vec<&str> = blocks.keys().map(String::as_str).collect();
    found.sort();
    let mut want: Vec<&str> = COMMANDS.to_vec();
    want.push("top");
    want.sort();
    assert_eq!(found, want, "one help block per level of docs/operator.md");
    for (level, text) in blocks {
        assert!(
            text.starts_with("Usage: agentspace-operator"),
            "{level}: {text:?}"
        );
        assert!(text.ends_with('\n') && !text.contains('\r'), "{level}");
        assert!(text.lines().count() >= 5, "{level} is more than a stub");
    }
    assert!(help_text("top").contains("Commands:\n  deploy     Create or update a host's stack"));
    assert!(help_text("ca").contains("%USERPROFILE%\\.agentspace\\ca"));
    // v0.07's additions are in the extracted text, so a byte comparison below shows them.
    assert!(help_text("top").contains("[-Host <project> | -All]"));
    assert!(help_text("top").contains("-InstanceType <type>"));
    assert!(help_text("bootstrap").starts_with("Usage: agentspace-operator bootstrap "));
    assert!(help_text("provision").starts_with("Usage: agentspace-operator provision -Host "));
    // v0.09's deploy and top-level help: `-InstanceType` is for every host, the box included.
    assert!(
        help_text("deploy").contains(
            "-InstanceType <type>  The instance type to give the host, the box included,"
        )
    );
    assert!(help_text("top").contains("For deploy: the instance type to give the host\n"));
}

#[test]
fn top_level_help_by_either_spelling() {
    assert_help(&["-h"], "top");
    assert_help(&["--help"], "top");
}

#[test]
fn each_commands_help_by_either_spelling() {
    for command in COMMANDS {
        assert_help(&[command, "-h"], command);
        assert_help(&[command, "--help"], command);
        // Help is the argument, wherever it stands.
        assert_help(&["-h", command], command);
        assert_help(&["--help", command], command);
    }
}

#[test]
fn help_and_commands_are_matched_without_regard_to_case() {
    for spelling in ["-H", "--HELP", "--Help", "-h"] {
        assert_help(&[spelling], "top");
        assert_help(&["status", spelling], "status");
    }
    for command in [
        "UP",
        "Up",
        "uP",
        "SYNC",
        "Deploy",
        "CA",
        "Down",
        "STATUS",
        "Grow",
        "BOOTSTRAP",
        "Provision",
    ] {
        assert_help(&[command, "-h"], &command.to_lowercase());
    }
}

#[test]
fn each_help_text_is_the_doc_text_with_a_line_feed_after_every_line() {
    // The explicit form of FA-13, beside the byte comparison.
    for level in std::iter::once("top").chain(COMMANDS) {
        let args: Vec<&str> = if level == "top" {
            vec!["--help"]
        } else {
            vec![level, "--help"]
        };
        let r = run(&args);
        assert_eq!(r.code, Some(0));
        let out = r.out();
        assert!(
            out.ends_with("\n"),
            "{level}: the last line ends in a line feed"
        );
        assert!(
            !out.ends_with("\n\n"),
            "{level}: nothing after the last line's line feed"
        );
        assert!(!out.contains('\r'), "{level}: no carriage return");
        let lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(lines.last(), Some(&""), "{level}");
        assert_eq!(out, help_text(level), "{level}");
    }
}

#[test]
fn help_wins_over_every_usage_error_and_refusal_on_the_line() {
    // Help is checked first, and the line then gets help "whatever else it holds".
    assert_line_help("sync -Alias x --help", "sync");
    assert_line_help("deploy -Alias bad! -GiB abc -Quiet -Quiet -h", "deploy");
    assert_line_help("status -GiB abc -h", "status");
    assert_line_help("up -Alias -Fx -h", "up");
    assert_line_help("grow -GiB -h", "grow");
    assert_line_help("-GiB abc -Quiet -Quiet --HELP", "top");
    assert_line_help("bogus -x extra -GiB -h", "top");
}

#[test]
fn help_wins_over_every_new_usage_error_and_host_check() {
    // The host checks come after help as well (operator.md, Command line: help is checked first).
    assert_line_help("provision -h", "provision");
    assert_line_help("provision --help -Host", "provision");
    assert_line_help("ca -Host y -h", "ca");
    assert_line_help("up -All -h", "up");
    assert_line_help("status -Host y -All --help", "status");
    assert_line_help("deploy -InstanceType m9g.xlarge -h", "deploy");
    assert_line_help("grow -InstanceType bad -Host -All -h", "grow");
    assert_line_help("bootstrap -Host bad! -h", "bootstrap");
    assert_line_help("-Host bad! -All -All -InstanceType -h", "top");
}

#[test]
fn the_new_parameters_follow_the_same_rule_for_which_argument_is_the_command() {
    // FB-11 for -Host and -InstanceType: the argument after either is its value unless it is
    // help; -All takes no value, so what follows it can be the command.
    assert_line_help("-Host up -h", "top");
    assert_line_help("-Host -h up", "up");
    assert_line_help("-InstanceType deploy -h", "top");
    assert_line_help("-InstanceType -h deploy", "deploy");
    assert_line_help("-Host x deploy -h", "deploy");
    assert_line_help("-host provision -H", "top");
    assert_line_help("-HOST -H provision", "provision");
    assert_line_help("-All up -h", "up");
    assert_line_help("-All -Host x bootstrap -h", "bootstrap");
    assert_line_help("-instancetype m9g.xlarge -Host x deploy --help", "deploy");
    // A value that looks like a parameter is still a value.
    assert_line_help("-Host -All status -h", "status");
    assert_line_help("-InstanceType -Quiet grow -h", "grow");
    // As the last argument's value position, help is still asked for.
    assert_line_help("status -Host -h", "status");
    assert_line_help("deploy -InstanceType -h", "deploy");
}

#[test]
fn an_unknown_command_gets_the_top_levels_help() {
    assert_line_help("bogus -h", "top");
    assert_line_help("st --help", "top");
    assert_line_help("-Quiet -h", "top");
    assert_help(&["", "-h"], "top");
}

#[test]
fn the_first_command_on_the_line_names_the_help() {
    // A second positional is no command for help; the line is read left to right.
    assert_line_help("up status -h", "up");
    assert_line_help("status up --help", "status");
    assert_line_help("bogus status -h", "top");
    assert_line_help("-Quiet grow -h", "grow");
    assert_line_help("-GiB 5 grow -h", "grow");
    assert_line_help("-Quiet -GiB 5 -Alias x deploy -h", "deploy");
}

#[test]
fn fb11_a_value_never_names_a_command_for_help() {
    // Gap FB-11's two examples, exactly.
    assert_line_help("-Alias up -h", "top");
    assert_line_help("-Alias -h up", "up");
}

#[test]
fn fb11_the_rule_in_its_other_shapes() {
    // The argument after -GiB or -Alias is its value unless it is help.
    assert_line_help("-GiB up -h", "top");
    assert_line_help("-GiB -h up", "up");
    assert_line_help("-GiB 5 -Alias status -h", "top");
    assert_line_help("-gib status --help", "top");
    assert_line_help("-ALIAS -H status", "status");
    // A value that looks like a parameter is still a value, so the command after it is the command.
    assert_line_help("-Alias -Quiet up -h", "up");
    assert_line_help("-GiB -GiB up -h", "up");
    assert_line_help("-Alias -GiB sync --help", "sync");
    // A value before the command, a command, then another value's worth of arguments.
    assert_line_help("-Alias deploy grow -h", "grow");
    // Help as the very value position still asks for help, and the command may come before it.
    assert_line_help("up -Alias -h", "up");
    assert_line_help("down -GiB -h", "down");
}

#[test]
fn an_unknown_parameter_counts_as_a_parameter_when_finding_the_command_for_help() {
    // Architect's ruling for S01: `-Foo up -h` prints up's help; the unknown parameter is never
    // reported.
    assert_line_help("-Foo up -h", "up");
    assert_line_help("-x grow --help", "grow");
    assert_line_help("-Foo -Bar status -h", "status");
    assert_line_help("up -Foo -h", "up");
    assert_line_help("-Foo -h", "top");
    // A bare dash starts with `-` too.
    assert_line_help("- sync -h", "sync");
    // But as a parameter's value it is a value, and the command after it is the command.
    assert_line_help("-Alias -Foo up -h", "up");
}

#[test]
fn help_is_matched_without_regard_to_case_in_a_value_position() {
    assert_line_help("-Alias -H up", "up");
    assert_line_help("-GiB --HeLp grow", "grow");
}

#[test]
fn help_starts_no_program_and_writes_nothing() {
    // The three seam variables name nothing (Home::command); a program started would show.
    for level in ["top", "up", "sync", "grow", "bootstrap", "provision"] {
        let args: Vec<&str> = if level == "top" {
            vec!["-h"]
        } else {
            vec![level, "-h"]
        };
        let r = run(&args);
        assert_eq!(r.err(), "");
        assert!(r.leftovers.is_empty(), "{:?}", r.leftovers);
    }
}

#[test]
fn quiet_does_not_silence_help() {
    assert_line_help("-Quiet -h", "top");
    assert_line_help("up -Quiet --help", "up");
    assert_line_help("-quiet status -h", "status");
}
