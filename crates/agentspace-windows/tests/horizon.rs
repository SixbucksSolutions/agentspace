//! The horizon command (docs/operator.md, The horizon on a Windows host; The platform seam, the
//! horizon row; pins P12 and P17): the digits as one word of its own, then one LF or nothing, and
//! never a CR, written with `[IO.File]::WriteAllText` to a temporary file and moved over
//! `lease-horizon`.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The bytes are shown by running the command under the laptop's own Windows PowerShell, with
//! `C:\ProgramData\agentspace` replaced by a throwaway folder under the build's own scratch
//! folder. A command that names a system folder in a way that cannot be replaced is refused before
//! it runs, so nothing is ever written outside the throwaway folder.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use common::{PREFIX, base64_encode, platform, utf16le};

const SYSTEM_FOLDER: &str = "C:\\ProgramData\\agentspace";
const HORIZONS: [u64; 4] = [1_795_000_123, 7, 0, u64::MAX];

fn command(horizon: u64) -> String {
    platform()
        .horizon_command(horizon)
        .expect("the horizon command")
}

// What the command's text says

#[test]
fn the_horizon_command_is_not_the_encoded_form() {
    let command = command(1_795_000_123);
    assert!(!command.starts_with("powershell"));
    assert!(!command.contains("-EncodedCommand"));
    assert!(!command.starts_with(PREFIX));
}

#[test]
fn the_horizons_digits_are_one_word_of_their_own() {
    for horizon in HORIZONS {
        let command = command(horizon);
        assert!(has_word(&command, &horizon.to_string()), "{horizon}");
    }
}

#[test]
fn a_planted_command_with_the_digits_glued_to_text_has_no_word_of_their_own() {
    assert!(!has_word("$t=\"x1795000123\"", "1795000123"));
    assert!(!has_word("$t=\"17950001234\"", "1795000123"));
    assert!(has_word("$t = '1795000123'", "1795000123"));
}

#[test]
fn the_horizon_is_written_with_write_all_text() {
    assert!(command(1_795_000_123).contains("[IO.File]::WriteAllText"));
}

#[test]
fn no_cmdlet_that_ends_its_text_with_cr_lf_writes_it() {
    let command = command(1_795_000_123);
    for cmdlet in ["Set-Content", "Out-File", "Add-Content", "Tee-Object", ">>"] {
        assert!(!command.contains(cmdlet), "{cmdlet}");
    }
    assert!(!command.contains(" > "), "redirection");
}

#[test]
fn no_cr_is_spelled_in_the_command() {
    let command = command(1_795_000_123);
    assert!(!command.contains('\r'), "a raw CR");
    let folded = command.to_lowercase();
    for spelling in [
        "`r",
        "\\r",
        "[char]13",
        "[char]0x0d",
        "[char]0xd",
        "0x0d",
        "newline",
        "`r`n",
    ] {
        assert!(!folded.contains(spelling), "{spelling}");
    }
}

#[test]
fn the_command_names_the_horizon_file_in_programdata_agentspace() {
    let command = command(1_795_000_123);
    assert!(command.contains(SYSTEM_FOLDER));
    assert!(command.contains("lease-horizon"));
}

#[test]
fn the_command_makes_the_folder_and_moves_a_temporary_file_over_the_horizon() {
    let folded = command(1_795_000_123).to_lowercase();
    assert!(
        folded.contains("new-item") || folded.contains("createdirectory"),
        "makes the folder"
    );
    assert!(
        folded.contains("move-item")
            || folded.contains("file]::move")
            || folded.contains("file]::replace")
            || folded.contains(".moveto("),
        "moves the temporary file over the horizon"
    );
}

// What the command writes: shown by running it in a throwaway folder

#[test]
fn it_writes_the_digits_then_an_lf_or_the_digits_alone() {
    for horizon in HORIZONS {
        let run = Run::new("bytes");
        let outcome = run.run(&command(horizon));
        assert_eq!(outcome.code, Some(0), "{horizon}");
        let bytes = std::fs::read(run.horizon_file()).expect("the horizon file");
        check_horizon_bytes(&bytes, &horizon.to_string())
            .unwrap_or_else(|why| panic!("{horizon}: {why}"));
    }
}

#[test]
fn it_never_writes_a_cr() {
    let run = Run::new("nocr");
    assert_eq!(run.run(&command(1_795_000_123)).code, Some(0));
    let bytes = std::fs::read(run.horizon_file()).expect("the horizon file");
    assert!(!bytes.contains(&b'\r'), "{bytes:?}");
}

#[test]
fn it_writes_no_byte_order_mark_and_nothing_else() {
    let run = Run::new("bom");
    assert_eq!(run.run(&command(1_795_000_123)).code, Some(0));
    let bytes = std::fs::read(run.horizon_file()).expect("the horizon file");
    assert!(bytes.starts_with(b"1795000123"), "{bytes:?}");
    assert!(bytes.len() <= "1795000123\n".len(), "{bytes:?}");
}

#[test]
fn the_checker_refuses_a_cr_lf_ending() {
    // Planted wrong bytes: what Set-Content would write.
    assert!(check_horizon_bytes(b"1795000123\r\n", "1795000123").is_err());
    assert!(check_horizon_bytes(b"1795000123\r", "1795000123").is_err());
    assert!(check_horizon_bytes(b"1795000123\n\n", "1795000123").is_err());
    assert!(check_horizon_bytes(b"\xEF\xBB\xBF1795000123\n", "1795000123").is_err());
    assert!(check_horizon_bytes(b"17950001234\n", "1795000123").is_err());
    assert!(check_horizon_bytes(b"1795000123\n", "1795000123").is_ok());
    assert!(check_horizon_bytes(b"1795000123", "1795000123").is_ok());
}

#[test]
fn it_makes_the_folder_when_it_is_missing() {
    let run = Run::new("mkdir");
    assert!(!run.folder().exists());
    assert_eq!(run.run(&command(1_795_000_123)).code, Some(0));
    assert!(run.folder().is_dir());
}

#[test]
fn it_replaces_an_existing_horizon_whole() {
    let run = Run::new("replace");
    std::fs::create_dir_all(run.folder()).unwrap();
    std::fs::write(
        run.horizon_file(),
        b"1700000000\r\nOLD-AND-LONGER-CONTENT\r\n",
    )
    .unwrap();
    assert_eq!(run.run(&command(1_795_000_123)).code, Some(0));
    let bytes = std::fs::read(run.horizon_file()).expect("the horizon file");
    check_horizon_bytes(&bytes, "1795000123").unwrap();
}

#[test]
fn it_leaves_no_temporary_file_behind() {
    let run = Run::new("tempfile");
    assert_eq!(run.run(&command(1_795_000_123)).code, Some(0));
    let left: Vec<String> = std::fs::read_dir(run.folder())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(left, ["lease-horizon"]);
}

#[test]
fn it_exits_non_zero_and_leaves_the_horizon_alone_when_the_folder_cannot_be_made() {
    let run = Run::new("blocked");
    // A file where the folder belongs: the folder cannot be made.
    std::fs::write(run.folder(), b"in the way").unwrap();
    let outcome = run.run(&command(1_795_000_123));
    assert_ne!(outcome.code, Some(0));
    assert_eq!(std::fs::read(run.folder()).unwrap(), b"in the way");
}

#[test]
fn a_zero_exit_means_the_file_is_there_whole() {
    let run = Run::new("exit");
    let outcome = run.run(&command(42));
    assert_eq!(outcome.code, Some(0));
    assert!(run.horizon_file().is_file());
}

// Helpers

/// Whether `text` holds `word` bounded on both sides by something that is not part of a word.
fn has_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    text.match_indices(word).any(|(at, _)| {
        let before = at.checked_sub(1).map(|i| bytes[i]);
        let after = bytes.get(at + word.len()).copied();
        let part_of_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'.';
        !before.is_some_and(part_of_word) && !after.is_some_and(part_of_word)
    })
}

/// The bytes operator.md allows: the digits then one LF, or the digits alone.
fn check_horizon_bytes(bytes: &[u8], digits: &str) -> Result<(), String> {
    let mut with_lf = digits.as_bytes().to_vec();
    with_lf.push(b'\n');
    if bytes == digits.as_bytes() || bytes == with_lf.as_slice() {
        Ok(())
    } else {
        Err(format!(
            "{} bytes that are not the digits and an LF",
            bytes.len()
        ))
    }
}

struct Outcome {
    code: Option<i32>,
}

/// One throwaway home and one throwaway `ProgramData`, under the build's scratch folder.
struct Run {
    base: PathBuf,
}

impl Run {
    fn new(name: &str) -> Run {
        let base = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("s06test-horizon-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("home")).unwrap();
        std::fs::create_dir_all(base.join("tmp")).unwrap();
        std::fs::create_dir_all(base.join("pd")).unwrap();
        Run { base }
    }

    fn folder(&self) -> PathBuf {
        self.base.join("pd").join("agentspace")
    }

    fn horizon_file(&self) -> PathBuf {
        self.folder().join("lease-horizon")
    }

    /// Runs the command's script under Windows PowerShell with the system folder replaced.
    fn run(&self, script: &str) -> Outcome {
        let replaced = self.isolate(script);
        let line = base64_encode(&utf16le(&replaced));
        let result = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
            ])
            .args(["-EncodedCommand", &line])
            .env("USERPROFILE", self.base.join("home"))
            .env("HOME", self.base.join("home"))
            .env("LOCALAPPDATA", self.base.join("home"))
            .env("APPDATA", self.base.join("home"))
            .env("TMP", self.base.join("tmp"))
            .env("TEMP", self.base.join("tmp"))
            .env("ProgramData", self.base.join("pd"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();
        match result {
            Ok(output) => Outcome {
                code: output.status.code(),
            },
            Err(error) => panic!(
                "Windows PowerShell could not be started: {:?}",
                error.kind()
            ),
        }
    }

    /// Replaces the system folder by the throwaway one, and refuses a command that still names
    /// a system folder some other way.
    fn isolate(&self, script: &str) -> String {
        let target = self.folder().to_string_lossy().to_string();
        let mut replaced = replace_ignoring_case(script, SYSTEM_FOLDER, &target);
        replaced = replace_ignoring_case(&replaced, "C:/ProgramData/agentspace", &target);
        let mut rest = replace_ignoring_case(&replaced, "$env:ProgramData", "");
        rest = replace_ignoring_case(&rest, "%ProgramData%", "");
        let folded = rest.to_lowercase();
        for risk in [
            "programdata",
            "commonapplicationdata",
            "getfolderpath",
            "c:\\windows",
        ] {
            assert!(
                !folded.contains(risk),
                "the command names a system folder that cannot be replaced ({risk}); not run"
            );
        }
        replaced
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn replace_ignoring_case(text: &str, from: &str, to: &str) -> String {
    let folded = text.to_lowercase();
    let needle = from.to_lowercase();
    // ASCII-only needles keep byte offsets of the folded text equal to the original's.
    assert!(folded.len() == text.len() || !needle.is_ascii());
    let mut out = String::new();
    let mut last = 0;
    for (at, _) in folded.match_indices(&needle) {
        if at < last || folded.len() != text.len() {
            continue;
        }
        out.push_str(&text[last..at]);
        out.push_str(to);
        last = at + from.len();
    }
    out.push_str(&text[last..]);
    out
}
