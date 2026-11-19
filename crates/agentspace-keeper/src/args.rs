//! The command line (docs/keeper.md, Command line; Usage errors).

use std::ffi::OsString;
use std::path::PathBuf;

/// The program's name in its usage lines and messages, whatever path started it.
pub const PROGRAM: &str = "agentspace-keeper";

/// The two usage lines a usage error writes after its message, each ending in LF.
pub const USAGE: &str = "Usage: agentspace-keeper [-SshExe <full path>] [-Now <YYYY-MM-DDTHH:MM:SSZ>]\n       agentspace-keeper -Watch [-SshExe <full path>] [-IntervalSeconds <n>] [-MaxPasses <n>] [-BackoffBaseSeconds <n>] [-BackoffMaxSeconds <n>] [-ReleaseGraceSeconds <n>]\n";

/// What `-SshExe` defaults to after the value of `SystemRoot`.
const SSH_UNDER_SYSTEM_ROOT: &str = r"\System32\OpenSSH\ssh.exe";

/// The arguments `-Watch` uses, each a whole number from 1 to 86400 (keeper.md, Command line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchOptions {
    /// `-IntervalSeconds`, 30 by default.
    pub interval_seconds: u32,
    /// `-MaxPasses`, no limit by default.
    pub max_passes: Option<u32>,
    /// `-BackoffBaseSeconds`, 30 by default.
    pub backoff_base_seconds: u32,
    /// `-BackoffMaxSeconds`, 300 by default.
    pub backoff_max_seconds: u32,
    /// `-ReleaseGraceSeconds`, 95 by default.
    pub release_grace_seconds: u32,
    /// `-FastFailureSeconds`, hidden, 60 by default.
    pub fast_failure_seconds: u32,
}

impl Default for WatchOptions {
    fn default() -> Self {
        WatchOptions {
            interval_seconds: 30,
            max_passes: None,
            backoff_base_seconds: 30,
            backoff_max_seconds: 300,
            release_grace_seconds: 95,
            fast_failure_seconds: 60,
        }
    }
}

/// A command line that passed every usage check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// `-SshExe`, or its default from `SystemRoot`; it has passed check 5.
    pub ssh_exe: PathBuf,
    /// `-Now`, as a Unix time in whole seconds.
    pub now: Option<i64>,
    /// `Some` when `-Watch` was given.
    pub watch: Option<WatchOptions>,
}

/// The arguments, in the argument table's order. The numbers come last, in the order check 2
/// takes them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Name {
    SshExe,
    Now,
    Watch,
    IntervalSeconds,
    MaxPasses,
    BackoffBaseSeconds,
    BackoffMaxSeconds,
    ReleaseGraceSeconds,
    FastFailureSeconds,
}

const NAMES: [Name; 9] = [
    Name::SshExe,
    Name::Now,
    Name::Watch,
    Name::IntervalSeconds,
    Name::MaxPasses,
    Name::BackoffBaseSeconds,
    Name::BackoffMaxSeconds,
    Name::ReleaseGraceSeconds,
    Name::FastFailureSeconds,
];

impl Name {
    /// The name as keeper.md spells it, without its dash.
    fn spelling(self) -> &'static str {
        match self {
            Name::SshExe => "SshExe",
            Name::Now => "Now",
            Name::Watch => "Watch",
            Name::IntervalSeconds => "IntervalSeconds",
            Name::MaxPasses => "MaxPasses",
            Name::BackoffBaseSeconds => "BackoffBaseSeconds",
            Name::BackoffMaxSeconds => "BackoffMaxSeconds",
            Name::ReleaseGraceSeconds => "ReleaseGraceSeconds",
            Name::FastFailureSeconds => "FastFailureSeconds",
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    /// Matched exactly, without regard to case. A `:` at the end is not ignored, which keeper.md
    /// allows either way.
    fn find(raw: &str) -> Option<Name> {
        let rest = raw.strip_prefix('-')?;
        NAMES
            .into_iter()
            .find(|n| n.spelling().eq_ignore_ascii_case(rest))
    }
}

/// A next argument that is missing, empty, all whitespace, or starts with `-` is no value.
fn is_value(next: Option<&String>) -> bool {
    match next {
        None => false,
        Some(v) => !v.trim().is_empty() && !v.starts_with('-'),
    }
}

/// A whole number from 1 to 86400, in decimal digits only; leading zeros are allowed.
fn whole_number(value: &str) -> Option<u32> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let digits = value.trim_start_matches('0');
    if digits.len() > 5 {
        return None;
    }
    let n: u32 = if digits.is_empty() {
        0
    } else {
        digits.parse().ok()?
    };
    (1..=86_400).contains(&n).then_some(n)
}

/// Whether a path is fully qualified as Windows reads one: a drive letter, a colon and a
/// separator (`C:\x`), or two separators (`\\server\share`, `\\?\C:\x`). `\x` and `C:x` are not.
///
/// The keeper is a Windows program, and these are the rules its paths follow on every system.
/// On a system that is not Windows a path that system calls absolute is also accepted, so that
/// the same code runs there against real files in a throwaway folder (D42: T01 is tested on
/// Linux).
pub(crate) fn is_fully_qualified(path: &str) -> bool {
    let b = path.as_bytes();
    let sep = |c: u8| c == b'\\' || c == b'/';
    let windows = (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && sep(b[2]))
        || (b.len() >= 2 && sep(b[0]) && sep(b[1]));
    windows || (cfg!(not(windows)) && std::path::Path::new(path).is_absolute())
}

/// Whether a path names a `.cmd` or `.bat`, by its extension without regard to case.
pub(crate) fn is_batch(path: &str) -> bool {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".cmd") || lower.ends_with(".bat")
}

/// Usage check 5, on `-SshExe` or its default: the message of the first that fails.
pub(crate) fn check_ssh_exe(path: &str) -> Option<String> {
    if !is_fully_qualified(path) {
        return Some(format!("SshExe '{path}' is not a full path."));
    }
    if path.contains('"') {
        return Some(format!("SshExe '{path}' holds a quote."));
    }
    if is_batch(path) && path.contains('%') {
        return Some(format!(
            "SshExe '{path}' is a batch whose path holds a percent sign."
        ));
    }
    None
}

/// Reads the command line (keeper.md, Command line), running the usage checks in their order.
///
/// `args` are the arguments after the program's own name. `var` reads one variable of the
/// keeper's own environment; only `SystemRoot` is read here, for `-SshExe`'s default. An error is
/// the usage error's message, without the `agentspace-keeper: ` before it.
pub fn parse_args(
    args: &[String],
    var: &dyn Fn(&str) -> Option<OsString>,
) -> Result<Options, String> {
    // Check 1: one scan, left to right, ending at the first failure.
    let mut given: [Option<Option<String>>; 9] = Default::default();
    let mut i = 0;
    while i < args.len() {
        let raw = &args[i];
        let Some(name) = Name::find(raw) else {
            return Err(format!("Unknown argument '{raw}'."));
        };
        if given[name.index()].is_some() {
            return Err(format!("-{} is given twice.", name.spelling()));
        }
        if name == Name::Watch {
            given[name.index()] = Some(None);
            i += 1;
            continue;
        }
        if !is_value(args.get(i + 1)) {
            return Err(format!("-{} needs a value.", name.spelling()));
        }
        given[name.index()] = Some(Some(args[i + 1].clone()));
        i += 2;
    }
    let value = |n: Name| given[n.index()].clone().flatten();
    let watch = given[Name::Watch.index()].is_some();

    // Check 2: the numbers, one at a time, in the argument table's order.
    let mut numbers = [None; 9];
    for name in &NAMES[Name::IntervalSeconds.index()..] {
        let Some(v) = value(*name) else { continue };
        if !watch {
            return Err(format!("-{} is for -Watch.", name.spelling()));
        }
        match whole_number(&v) {
            Some(n) => numbers[name.index()] = Some(n),
            None => {
                return Err(format!(
                    "-{} '{v}' is not a whole number from 1 to 86400.",
                    name.spelling()
                ));
            }
        }
    }

    // Check 3.
    let now_raw = value(Name::Now);
    if watch && now_raw.is_some() {
        return Err("-Now is not allowed with -Watch.".to_string());
    }

    // Check 4, defaults included.
    let defaults = WatchOptions::default();
    let pick = |n: Name, d: u32| numbers[n.index()].unwrap_or(d);
    let options = WatchOptions {
        interval_seconds: pick(Name::IntervalSeconds, defaults.interval_seconds),
        max_passes: numbers[Name::MaxPasses.index()],
        backoff_base_seconds: pick(Name::BackoffBaseSeconds, defaults.backoff_base_seconds),
        backoff_max_seconds: pick(Name::BackoffMaxSeconds, defaults.backoff_max_seconds),
        release_grace_seconds: pick(Name::ReleaseGraceSeconds, defaults.release_grace_seconds),
        fast_failure_seconds: pick(Name::FastFailureSeconds, defaults.fast_failure_seconds),
    };
    if options.backoff_max_seconds < options.backoff_base_seconds {
        return Err("-BackoffMaxSeconds is less than -BackoffBaseSeconds.".to_string());
    }

    // Check 5, on the default too when -SshExe is not given.
    let ssh_exe = match value(Name::SshExe) {
        Some(v) => v,
        None => {
            let root = var("SystemRoot").unwrap_or_default();
            format!("{}{SSH_UNDER_SYSTEM_ROOT}", root.to_string_lossy())
        }
    };
    if let Some(message) = check_ssh_exe(&ssh_exe) {
        return Err(message);
    }

    // Check 6.
    let now = match now_raw {
        None => None,
        Some(v) => match agentspace_state::parse_time(&v) {
            Some(t) => Some(t),
            None => {
                return Err(format!(
                    "-Now '{v}' is not a time in the form YYYY-MM-DDTHH:MM:SSZ."
                ));
            }
        },
    };

    Ok(Options {
        ssh_exe: PathBuf::from(ssh_exe),
        now,
        watch: watch.then_some(options),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> Option<OsString> {
        (name == "SystemRoot").then(|| OsString::from(r"C:\Windows"))
    }

    fn no_root(_: &str) -> Option<OsString> {
        None
    }

    fn parse(args: &[&str]) -> Result<Options, String> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse_args(&args, &root)
    }

    fn err(args: &[&str]) -> String {
        parse(args).unwrap_err()
    }

    #[test]
    fn no_arguments_give_one_pass_with_the_default_ssh() {
        let o = parse(&[]).unwrap();
        assert_eq!(
            o.ssh_exe,
            PathBuf::from(r"C:\Windows\System32\OpenSSH\ssh.exe")
        );
        assert_eq!(o.now, None);
        assert_eq!(o.watch, None);
    }

    #[test]
    fn names_ignore_case_and_take_their_values() {
        let o = parse(&["-sshexe", r"D:\x\ssh.exe", "-NOW", "2026-11-18T05:07:00Z"]).unwrap();
        assert_eq!(o.ssh_exe, PathBuf::from(r"D:\x\ssh.exe"));
        assert_eq!(o.now, agentspace_state::parse_time("2026-11-18T05:07:00Z"));
    }

    #[test]
    fn watch_takes_its_numbers_and_defaults() {
        let o = parse(&["-Watch"]).unwrap();
        assert_eq!(o.watch, Some(WatchOptions::default()));
        let o = parse(&[
            "-Watch",
            "-IntervalSeconds",
            "007",
            "-MaxPasses",
            "86400",
            "-BackoffBaseSeconds",
            "1",
            "-BackoffMaxSeconds",
            "1",
            "-ReleaseGraceSeconds",
            "5",
            "-FastFailureSeconds",
            "2",
        ])
        .unwrap();
        assert_eq!(
            o.watch,
            Some(WatchOptions {
                interval_seconds: 7,
                max_passes: Some(86400),
                backoff_base_seconds: 1,
                backoff_max_seconds: 1,
                release_grace_seconds: 5,
                fast_failure_seconds: 2,
            })
        );
    }

    #[test]
    fn check_one_reports_unknown_twice_and_missing_values_left_to_right() {
        assert_eq!(err(&["-Bogus"]), "Unknown argument '-Bogus'.");
        assert_eq!(err(&["positional"]), "Unknown argument 'positional'.");
        for help in ["-h", "--help", "-?", "-Launch", "-Watch:"] {
            assert_eq!(err(&[help]), format!("Unknown argument '{help}'."));
        }
        assert_eq!(
            err(&["-Watch", "-Watch", "-Bogus"]),
            "-Watch is given twice."
        );
        assert_eq!(err(&["-watch", "-WATCH"]), "-Watch is given twice.");
        assert_eq!(
            err(&["-IntervalSeconds", "abc", "-Bogus"]),
            "Unknown argument '-Bogus'."
        );
        assert_eq!(err(&["-sshexe"]), "-SshExe needs a value.");
        assert_eq!(err(&["-Now", ""]), "-Now needs a value.");
        assert_eq!(err(&["-Now", " \t"]), "-Now needs a value.");
        assert_eq!(err(&["-SshExe", "-Watch"]), "-SshExe needs a value.");
        // A value is skipped, never read as a name.
        assert_eq!(err(&["-Now", "x", "-Now", "y"]), "-Now is given twice.");
        assert_eq!(
            err(&["-SshExe", r"C:\a", "-SshExe", r"C:\b"]),
            "-SshExe is given twice."
        );
    }

    #[test]
    fn check_two_takes_the_numbers_in_the_table_order() {
        assert_eq!(
            err(&["-IntervalSeconds", "abc"]),
            "-IntervalSeconds is for -Watch."
        );
        assert_eq!(
            err(&["-FastFailureSeconds", "5", "-MaxPasses", "x"]),
            "-MaxPasses is for -Watch."
        );
        assert_eq!(
            err(&["-Watch", "-MaxPasses", "abc", "-IntervalSeconds", "0"]),
            "-IntervalSeconds '0' is not a whole number from 1 to 86400."
        );
        for bad in [
            "0",
            "86401",
            "+5",
            "5 ",
            "1,000",
            "1.0",
            "000000000086401",
            "٣",
        ] {
            assert_eq!(
                err(&["-Watch", "-ReleaseGraceSeconds", bad]),
                format!("-ReleaseGraceSeconds '{bad}' is not a whole number from 1 to 86400.")
            );
        }
        assert!(parse(&["-Watch", "-MaxPasses", "0000000000086400"]).is_ok());
    }

    #[test]
    fn checks_three_to_six_come_in_order() {
        assert_eq!(
            err(&["-Watch", "-Now", "bad", "-SshExe", "rel"]),
            "-Now is not allowed with -Watch."
        );
        assert_eq!(
            err(&["-Watch", "-BackoffBaseSeconds", "301", "-SshExe", "rel"]),
            "-BackoffMaxSeconds is less than -BackoffBaseSeconds."
        );
        assert_eq!(
            err(&["-Watch", "-BackoffMaxSeconds", "29"]),
            "-BackoffMaxSeconds is less than -BackoffBaseSeconds."
        );
        assert_eq!(
            err(&["-SshExe", "ssh.exe", "-Now", "bad"]),
            "SshExe 'ssh.exe' is not a full path."
        );
        assert_eq!(
            err(&["-Now", "bad"]),
            "-Now 'bad' is not a time in the form YYYY-MM-DDTHH:MM:SSZ."
        );
        for bad in [
            "2026-02-30T00:00:00Z",
            "2026-11-18T24:00:00Z",
            "2026-11-18T05:07:00",
        ] {
            assert_eq!(
                err(&["-Now", bad]),
                format!("-Now '{bad}' is not a time in the form YYYY-MM-DDTHH:MM:SSZ.")
            );
        }
    }

    #[test]
    fn check_five_refuses_relative_quoted_and_percent_batch_paths() {
        for rel in [r"\x\ssh.exe", r"C:x\ssh.exe", "ssh.exe", r"x\ssh.exe"] {
            assert_eq!(
                err(&["-SshExe", rel]),
                format!("SshExe '{rel}' is not a full path.")
            );
        }
        assert_eq!(
            err(&["-SshExe", r#"C:\a"b\ssh.exe"#]),
            r#"SshExe 'C:\a"b\ssh.exe' holds a quote."#
        );
        assert_eq!(
            err(&["-SshExe", r"C:\a%b\stub.CMD"]),
            r"SshExe 'C:\a%b\stub.CMD' is a batch whose path holds a percent sign."
        );
        assert_eq!(
            err(&["-SshExe", r"C:\a%b\stub.bat"]),
            r"SshExe 'C:\a%b\stub.bat' is a batch whose path holds a percent sign."
        );
        assert!(parse(&["-SshExe", r"C:\a%b\ssh.exe"]).is_ok());
        assert!(parse(&["-SshExe", r"\\server\share\ssh.exe"]).is_ok());
        assert!(parse(&["-SshExe", "C:/x/ssh.exe"]).is_ok());
    }

    #[test]
    fn the_default_ssh_is_checked_like_a_given_one() {
        let e = parse_args(&[], &no_root).unwrap_err();
        assert_eq!(e, r"SshExe '\System32\OpenSSH\ssh.exe' is not a full path.");
        let empty = |_: &str| Some(OsString::new());
        assert_eq!(
            parse_args(&[], &empty).unwrap_err(),
            r"SshExe '\System32\OpenSSH\ssh.exe' is not a full path."
        );
    }

    #[test]
    fn the_usage_lines_are_keeper_mds() {
        let lines: Vec<&str> = USAGE.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            "Usage: agentspace-keeper [-SshExe <full path>] [-Now <YYYY-MM-DDTHH:MM:SSZ>]"
        );
        assert!(lines[1].starts_with("       agentspace-keeper -Watch [-SshExe <full path>]"));
        assert!(!USAGE.contains("FastFailureSeconds"));
    }
}
