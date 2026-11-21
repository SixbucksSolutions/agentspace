//! Starting a program that outlives the call: the detached start of a provisioning run
//! (docs/driver.md, Starting a provisioning run; docs/state.md, The provisioning run's start; D48,
//! D55).
//!
//! RFC 2119 keywords in this crate's comments, and the capitals are load-bearing.
//!
//! # The public API (pinned by E0.07-T02-S02, #0147)
//!
//! | Item | What it is |
//! |---|---|
//! | `Detach` | The trait: `start` creates `program` with `args`, in the working folder `cwd`, with the variables `env` on top of the logon environment, and returns once the process is created, never waiting for it. |
//! | `WmiDetach` | Windows only. It creates the process through WMI's `Win32_Process.Create`, so no job object of the caller or of any process above it holds it, with no window and no standard handles. |
//! | `NoDetach` | Every system. Its `start` always refuses. |
//! | `wmi::create` | Windows only. The `Win32_Process.Create` call itself, which agentspace-launch's starter shares (D42). |
//!
//! # What `start` refuses
//!
//! **Before any call into Windows, `start` MUST refuse, with an `Err` and never a panic:**
//! - a program, an argument or a working folder that holds a NUL, and a program or an argument
//!   that holds a double quote, a carriage return or a line feed, none of which a Windows command
//!   line can carry;
//! - a variable whose name is empty or holds a NUL or `=`, and a value that holds a NUL.
//!
//! The messages name the argument by its place and the variable by its name, and never print an
//! argument's or a value's text.
//!
//! # The command line
//!
//! The program and every argument are quoted for Windows' command line, each between double
//! quotes, as `CommandLineToArgvW` reads them: in an argument, the backslashes at its end are
//! doubled, so that its closing quote stays a quote (no argument holds a quote, so no other
//! backslash precedes one). The program is the command line's first word, which Windows reads up
//! to the next quote with no escapes, so its backslashes are kept as they are.
//!
//! # The environment
//!
//! **The process's environment is the user's logon environment with each `(name, value)` of `env`
//! applied to it, and a pair whose value is empty makes that variable absent**, whether or not the
//! logon environment holds it (state.md, The provisioning run's start: a value the logon
//! environment holds and the starting program does not is removed).
//!
//! **How (architect, 2026-11-20, for v0.07-T02):** `Win32_ProcessStartup.EnvironmentVariables`
//! replaces the process's environment rather than adding to it: given one variable, WMI starts the
//! process with that variable alone, and `OS`, `SystemRoot`, `PATH` and the rest unset. It also
//! keeps an entry `NAME=` as a variable set and empty, and never expands `%...%` in a value. So,
//! when `env` holds a pair, `WmiDetach` builds the whole block itself:
//! 1. **The logon environment**, from `CreateEnvironmentBlock` with this process's token and
//!    `bInherit` FALSE, so nothing of the starting process's own environment reaches the process,
//!    and no other profile is loaded. An entry with no `=` after its first character, so no name,
//!    is dropped.
//! 2. **Each pair in turn**, its name compared without regard to case, as Windows compares
//!    variable names: every entry of that name is removed, and the pair is added when its value
//!    is not empty. Of two pairs with one name, the later wins.
//! 3. **The block, whole, as `EnvironmentVariables`.**
//!
//! **With `env` empty, no block is passed at all**, and the process gets WMI's own logon
//! environment, as the keeper's launcher always has (D42).

#![deny(unsafe_code)]
// Off Windows only `NoDetach` is built, and the checks and the command line go unused.
#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
#[allow(unsafe_code)]
pub mod wmi;

#[cfg(windows)]
#[allow(unsafe_code)]
mod logon;

use std::ffi::{OsStr, OsString};
use std::path::Path;

/// Starting a program that outlives the call.
pub trait Detach {
    /// Creates `program` with `args`, in the working folder `cwd`, with each `(name, value)` of
    /// `env` set on top of the logon environment, and returns once the process is created; it
    /// never waits for it. A pair whose value is empty makes that variable absent. An `Err` is the
    /// reason, in agentspace's words.
    fn start(
        &self,
        program: &Path,
        args: &[&str],
        cwd: &Path,
        env: &[(&str, &OsStr)],
    ) -> Result<(), String>;
}

/// The detached start on a system that has none: `start` always refuses.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoDetach;

/// What `NoDetach` answers.
const ONLY_ON_WINDOWS: &str = "A detached start is only on Windows.";

impl Detach for NoDetach {
    fn start(&self, _: &Path, _: &[&str], _: &Path, _: &[(&str, &OsStr)]) -> Result<(), String> {
        Err(ONLY_ON_WINDOWS.to_string())
    }
}

/// The detached start through WMI's `Win32_Process.Create`: outside every job object of the
/// caller and above it, with no window and no standard handles, in the logon environment with
/// `env` applied to it (the crate's docs, The environment).
#[cfg(windows)]
#[derive(Debug, Default, Clone, Copy)]
pub struct WmiDetach;

#[cfg(windows)]
impl Detach for WmiDetach {
    fn start(
        &self,
        program: &Path,
        args: &[&str],
        cwd: &Path,
        env: &[(&str, &OsStr)],
    ) -> Result<(), String> {
        let call = Call::new(program, args, cwd, env)?;
        let environment = if call.pairs.is_empty() {
            Vec::new()
        } else {
            environment(logon::environment()?, &call.pairs)
        };
        // The call initializes COM on a thread of its own, so a caller's own COM apartment, of
        // whatever kind, never meets it.
        let thread = std::thread::Builder::new()
            .name("agentspace-detach-create".to_string())
            .spawn(move || wmi::create(&call.command_line, &call.directory, &environment))
            .map_err(|e| format!("A thread for Win32_Process.Create could not be started: {e}"))?;
        let answer = thread
            .join()
            .map_err(|_| "Win32_Process.Create ended without an answer.".to_string())?;
        match answer {
            Ok(0) => Ok(()),
            Ok(code) => Err(returned(code)),
            Err(error) => Err(format!("Win32_Process.Create could not be called: {error}")),
        }
    }
}

/// A nonzero `ReturnValue` of `Win32_Process.Create`, with what WMI documents it to mean.
fn returned(code: u32) -> String {
    let meaning = match code {
        2 => Some("access was denied"),
        3 => Some("the privilege it needs is missing"),
        8 => Some("an unknown failure"),
        9 => Some("the path was not found"),
        21 => Some("a parameter was not valid"),
        _ => None,
    };
    match meaning {
        Some(meaning) => format!("Win32_Process.Create returned {code}: {meaning}."),
        None => format!("Win32_Process.Create returned {code}."),
    }
}

/// The block `WmiDetach` passes: `logon`'s entries with each pair applied, in order (the crate's
/// docs, The environment). An entry with no name is dropped; every entry named as a pair is,
/// without regard to case, removed; and a pair whose value is not empty is added at the end.
fn environment(logon: Vec<OsString>, pairs: &[(String, OsString)]) -> Vec<OsString> {
    let mut block: Vec<OsString> = logon
        .into_iter()
        .filter(|entry| name_of(entry).is_some())
        .collect();
    for (name, value) in pairs {
        let wanted = name.to_uppercase();
        block.retain(|entry| name_of(entry).is_none_or(|n| n.to_uppercase() != wanted));
        if !value.is_empty() {
            let mut entry = OsString::from(name);
            entry.push("=");
            entry.push(value);
            block.push(entry);
        }
    }
    block
}

/// The name of a `NAME=VALUE` entry: the text before its first `=` after the first character, so
/// that `=C:=C:\` is named `=C:`. An entry with no `=` there has no name.
fn name_of(entry: &OsStr) -> Option<String> {
    let text = entry.to_string_lossy();
    let at = text.char_indices().skip(1).find(|(_, c)| *c == '=')?.0;
    Some(text[..at].to_string())
}

/// One checked call: what `wmi::create` is given, but the logon environment.
struct Call {
    command_line: OsString,
    directory: OsString,
    pairs: Vec<(String, OsString)>,
}

impl Call {
    /// The call for these inputs, or the refusal of the first one a command line or WMI cannot
    /// carry (the crate's docs list them).
    fn new(
        program: &Path,
        args: &[&str],
        cwd: &Path,
        env: &[(&str, &OsStr)],
    ) -> Result<Call, String> {
        if holds_nul(program.as_os_str()) {
            return Err("The program's path holds a NUL.".to_string());
        }
        if breaks_a_command_line(program.as_os_str()) {
            return Err(
                "The program's path holds a double quote or a line break, which a Windows command \
                 line cannot carry."
                    .to_string(),
            );
        }
        for (index, arg) in args.iter().enumerate() {
            let place = index + 1;
            if arg.contains('\0') {
                return Err(format!("Argument {place} holds a NUL."));
            }
            if breaks_a_command_line(OsStr::new(arg)) {
                return Err(format!(
                    "Argument {place} holds a double quote or a line break, which a Windows \
                     command line cannot carry."
                ));
            }
        }
        if holds_nul(cwd.as_os_str()) {
            return Err("The working folder's path holds a NUL.".to_string());
        }
        let mut pairs = Vec::with_capacity(env.len());
        for (name, value) in env {
            if name.is_empty() {
                return Err("An environment variable's name is empty.".to_string());
            }
            if name.contains('\0') || name.contains('=') {
                return Err(format!(
                    "The environment variable name {} holds a NUL or an equals sign.",
                    name.escape_debug()
                ));
            }
            if holds_nul(value) {
                return Err(format!(
                    "The value of the environment variable {name} holds a NUL."
                ));
            }
            pairs.push((name.to_string(), value.to_os_string()));
        }
        Ok(Call {
            command_line: command_line(program, args),
            directory: cwd.as_os_str().to_os_string(),
            pairs,
        })
    }
}

/// `"<program>" "<arg 1>" "<arg 2>" ...`, read back by `CommandLineToArgvW` as exactly the
/// program and the arguments (the crate's docs, The command line). No input holds a quote.
fn command_line(program: &Path, args: &[&str]) -> OsString {
    let mut line = OsString::from("\"");
    line.push(program.as_os_str());
    line.push("\"");
    for arg in args {
        line.push(" ");
        line.push(quote(arg));
    }
    line
}

/// One argument between double quotes, its trailing backslashes doubled.
fn quote(arg: &str) -> String {
    let trailing = arg.len() - arg.trim_end_matches('\\').len();
    let mut quoted = String::with_capacity(arg.len() + trailing + 2);
    quoted.push('"');
    quoted.push_str(arg);
    quoted.extend(std::iter::repeat_n('\\', trailing));
    quoted.push('"');
    quoted
}

fn holds_nul(text: &OsStr) -> bool {
    text.as_encoded_bytes().contains(&0)
}

/// A double quote, a carriage return or a line feed.
fn breaks_a_command_line(text: &OsStr) -> bool {
    text.as_encoded_bytes()
        .iter()
        .any(|b| matches!(b, b'"' | b'\r' | b'\n'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(program: &str, args: &[&str], env: &[(&str, &str)]) -> Result<Call, String> {
        let env: Vec<(&str, &OsStr)> = env.iter().map(|(n, v)| (*n, OsStr::new(*v))).collect();
        Call::new(Path::new(program), args, Path::new(r"C:\w"), &env)
    }

    #[test]
    fn no_detach_always_refuses() {
        let why = NoDetach
            .start(Path::new("p"), &[], Path::new("."), &[])
            .unwrap_err();
        assert_eq!(why, "A detached start is only on Windows.");
    }

    #[test]
    fn the_program_and_every_argument_are_quoted() {
        let c = call(
            r"C:\Program Files\x\run.exe",
            &["provision", "-Host", "my project", ""],
            &[],
        )
        .unwrap();
        assert_eq!(
            c.command_line,
            r#""C:\Program Files\x\run.exe" "provision" "-Host" "my project" """#
        );
    }

    #[test]
    fn backslashes_at_an_arguments_end_are_doubled_and_others_kept() {
        assert_eq!(quote(r"C:\a\b"), r#""C:\a\b""#);
        assert_eq!(quote(r"C:\a\"), r#""C:\a\\""#);
        assert_eq!(quote(r"\\server\share\\"), r#""\\server\share\\\\""#);
        assert_eq!(quote(r"\"), r#""\\""#);
        assert_eq!(quote(""), r#""""#);
    }

    #[test]
    fn a_quote_or_a_line_break_in_the_program_or_an_argument_is_refused() {
        for bad in ["a\"b", "a\rb", "a\nb"] {
            let why = call(bad, &[], &[]).err().unwrap();
            assert!(why.starts_with("The program's path holds"), "{why}");
            let why = call("p", &["ok", bad], &[]).err().unwrap();
            assert!(why.starts_with("Argument 2 holds a double quote"), "{why}");
            assert!(!why.contains(bad));
        }
    }

    #[test]
    fn a_nul_in_the_program_an_argument_or_the_folder_is_refused() {
        assert!(call("a\0b", &[], &[]).is_err());
        assert!(call("p", &["a\0b"], &[]).is_err());
        assert!(Call::new(Path::new("p"), &[], Path::new("C:\\a\0b"), &[]).is_err());
    }

    #[test]
    fn a_bad_variable_name_or_value_is_refused_without_its_value() {
        let why = call("p", &[], &[("", "v")]).err().unwrap();
        assert_eq!(why, "An environment variable's name is empty.");
        for name in ["A=B", "=C:", "A\0B"] {
            let why = call("p", &[], &[(name, "v")]).err().unwrap();
            assert!(why.contains("holds a NUL or an equals sign"), "{why}");
        }
        let why = call("p", &[], &[("NAME", "sec\0ret")]).err().unwrap();
        assert_eq!(
            why,
            "The value of the environment variable NAME holds a NUL."
        );
    }

    fn block(entries: &[&str]) -> Vec<OsString> {
        entries.iter().map(OsString::from).collect()
    }

    fn pairs(env: &[(&str, &str)]) -> Vec<(String, OsString)> {
        env.iter()
            .map(|(n, v)| (n.to_string(), OsString::from(v)))
            .collect()
    }

    #[test]
    fn a_pair_is_added_on_top_of_the_logon_environment() {
        let logon = block(&["OS=Windows_NT", r"SystemRoot=C:\W", "PATH=a;b"]);
        let got = environment(logon, &pairs(&[("AGENTSPACE_STATE_DIR", r"C:\s x")]));
        assert_eq!(
            got,
            block(&[
                "OS=Windows_NT",
                r"SystemRoot=C:\W",
                "PATH=a;b",
                r"AGENTSPACE_STATE_DIR=C:\s x"
            ])
        );
    }

    #[test]
    fn a_pair_replaces_a_logon_variable_of_its_name_whatever_its_case() {
        let logon = block(&["OS=Windows_NT", "UserProfile=C:\\old", "PATH=a"]);
        let got = environment(logon, &pairs(&[("USERPROFILE", r"C:\new")]));
        assert_eq!(
            got,
            block(&["OS=Windows_NT", "PATH=a", r"USERPROFILE=C:\new"])
        );
    }

    #[test]
    fn an_empty_value_removes_the_variable_and_adds_nothing() {
        let logon = block(&["OS=Windows_NT", "agentspace_ssh_dir=C:\\x", "PATH=a"]);
        let got = environment(
            logon,
            &pairs(&[("AGENTSPACE_SSH_DIR", ""), ("AGENTSPACE_PROXY_COMMAND", "")]),
        );
        assert_eq!(got, block(&["OS=Windows_NT", "PATH=a"]));
    }

    #[test]
    fn of_two_pairs_with_one_name_the_later_wins() {
        let got = environment(block(&["OS=x"]), &pairs(&[("A", "1"), ("a", "2")]));
        assert_eq!(got, block(&["OS=x", "a=2"]));
        let got = environment(block(&["OS=x"]), &pairs(&[("A", "1"), ("A", "")]));
        assert_eq!(got, block(&["OS=x"]));
    }

    #[test]
    fn a_logon_entry_with_no_name_is_dropped_and_a_drive_entry_kept() {
        let got = environment(
            block(&["OS=x", "junk", "=", r"=C:=C:\w"]),
            &pairs(&[("A", "1")]),
        );
        assert_eq!(got, block(&["OS=x", r"=C:=C:\w", "A=1"]));
    }

    /// The real logon environment: it holds what every process needs, and none of this process's
    /// own variables that the logon environment lacks. The test names the variables it checks,
    /// and never prints a value.
    #[cfg(windows)]
    #[test]
    fn the_logon_environment_holds_the_systems_variables() {
        let names: Vec<String> = logon::environment()
            .unwrap()
            .iter()
            .filter_map(|e| name_of(e))
            .map(|n| n.to_uppercase())
            .collect();
        for wanted in ["SYSTEMROOT", "PATH", "OS", "USERPROFILE"] {
            assert!(names.iter().any(|n| n == wanted), "{wanted} is missing");
        }
        // Set by cargo for this test process only, so never in a logon environment.
        assert!(std::env::var_os("CARGO_PKG_NAME").is_some());
        assert!(!names.iter().any(|n| n == "CARGO_PKG_NAME"));
    }

    #[test]
    fn a_nonzero_return_value_is_named() {
        assert_eq!(
            returned(9),
            "Win32_Process.Create returned 9: the path was not found."
        );
        assert_eq!(returned(77), "Win32_Process.Create returned 77.");
    }
}
