//! Log lines (docs/keeper.md, Log lines; docs/state.md, The keeper's logs).

use agentspace_keeper_api::Clock;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// `<state folder>\board-holder.log`.
pub(crate) const LOG: &str = "board-holder.log";
/// `<state folder>\board-holder-ssh.log`, and the name it is rotated to.
pub(crate) const SSH_LOG: &str = "board-holder-ssh.log";
pub(crate) const SSH_LOG_ROTATED: &str = "board-holder-ssh.log.1";
/// A holder's log larger than this is rotated (keeper.md, The holder's log).
pub(crate) const ROTATE_ABOVE: u64 = 1_048_576;
/// An open of the log that fails is tried up to 5 times, 50 ms apart.
const OPEN_TRIES: u32 = 5;
const OPEN_GAP: Duration = Duration::from_millis(50);

/// A field's value, with every whitespace character written `_`, and `-` for nothing.
fn field(value: &str) -> String {
    if value.is_empty() {
        return "-".to_string();
    }
    value
        .chars()
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .collect()
}

/// A reason: each run of control characters (U+0000 to U+001F, and U+007F) written as one space,
/// then trimmed; an empty reason is `Unknown.`
pub(crate) fn reason_text(reason: &str) -> String {
    let mut out = String::with_capacity(reason.len());
    let mut in_run = false;
    for c in reason.chars() {
        let control = matches!(c, '\u{0}'..='\u{1f}' | '\u{7f}');
        if control {
            if !in_run {
                out.push(' ');
            }
            in_run = true;
        } else {
            out.push(c);
            in_run = false;
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        "Unknown.".to_string()
    } else {
        trimmed.to_string()
    }
}

/// One pass line: `<time> <action> lease=<lease> alias=<alias> pid=<pid>[ reason=<reason>]`,
/// without its LF. An empty `lease`, `alias` or `pid` is written `-`.
pub(crate) fn pass_line(
    time: &str,
    action: &str,
    lease: &str,
    alias: &str,
    pid: &str,
    reason: Option<&str>,
) -> String {
    let mut line = format!(
        "{time} {action} lease={} alias={} pid={}",
        field(lease),
        field(alias),
        field(pid)
    );
    if let Some(r) = reason {
        line.push_str(" reason=");
        line.push_str(&reason_text(r));
    }
    line
}

/// Appends lines to `board-holder.log`, and writes each line it logged to stdout as well.
pub(crate) struct Logger<'a> {
    path: PathBuf,
    clock: &'a dyn Clock,
    stdout: &'a mut dyn Write,
}

impl<'a> Logger<'a> {
    pub(crate) fn new(state_dir: &Path, clock: &'a dyn Clock, stdout: &'a mut dyn Write) -> Self {
        Logger {
            path: state_dir.join(LOG),
            clock,
            stdout,
        }
    }

    /// Logs one line. The file is opened for this line alone, for appending, so the line lands
    /// whole at the end of the file even with two writers: the system finds the end and writes
    /// in one step. An open that fails is tried up to 5 times, 50 ms apart. A line that is not
    /// written to the file is dropped, and is not written to stdout either.
    pub(crate) fn log(&mut self, line: &str) -> Result<(), io::Error> {
        let mut bytes = Vec::with_capacity(line.len() + 1);
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
        let mut tries = 0;
        let mut file = loop {
            tries += 1;
            match open_append(&self.path) {
                Ok(f) => break f,
                Err(e) if tries >= OPEN_TRIES => return Err(e),
                Err(_) => self.clock.sleep(OPEN_GAP),
            }
        };
        file.write_all(&bytes)?;
        // Stdout is not the record; a failure to write it changes nothing.
        let _ = self.stdout.write_all(&bytes);
        let _ = self.stdout.flush();
        Ok(())
    }
}

/// Opens the log for appending, created if missing, sharing read and write (state.md, The
/// keeper's logs).
fn open_append(path: &Path) -> io::Result<fs::File> {
    let mut options = OpenOptions::new();
    options.append(true).create(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ | FILE_SHARE_WRITE.
        options.share_mode(0x1 | 0x2);
    }
    options.open(path)
}

/// Rotates `board-holder-ssh.log` when it is larger than 1 MiB: renames it to
/// `board-holder-ssh.log.1`, replacing any older one. Says whether it was rotated. A failure to
/// rotate is no rotation, and is not reported.
pub(crate) fn rotate_ssh_log(state_dir: &Path) -> bool {
    let from = state_dir.join(SSH_LOG);
    let due = fs::metadata(&from).is_ok_and(|m| m.is_file() && m.len() > ROTATE_ABOVE);
    due && fs::rename(&from, state_dir.join(SSH_LOG_ROTATED)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pass_line_has_its_fields_in_order() {
        assert_eq!(
            pass_line("2026-11-18T06:01:36Z", "idle", "", "", "", None),
            "2026-11-18T06:01:36Z idle lease=- alias=- pid=-"
        );
        assert_eq!(
            pass_line(
                "2026-11-18T05:07:00Z",
                "started",
                "0b6f4c1e-8d1a-4f0e-9c3b-2a5d7e9f1c44",
                "agentspace-swimcoach-0b6f4c1e",
                "24112",
                None
            ),
            "2026-11-18T05:07:00Z started lease=0b6f4c1e-8d1a-4f0e-9c3b-2a5d7e9f1c44 alias=agentspace-swimcoach-0b6f4c1e pid=24112"
        );
        assert_eq!(
            pass_line(
                "2026-11-18T06:02:06Z",
                "skipped",
                "",
                "",
                "",
                Some("locked")
            ),
            "2026-11-18T06:02:06Z skipped lease=- alias=- pid=- reason=locked"
        );
    }

    /// A clock for a logger whose opens never fail.
    struct Still;

    impl Clock for Still {
        fn utc_now_ms(&self) -> i64 {
            0
        }
        fn monotonic(&self) -> Duration {
            Duration::ZERO
        }
        fn sleep(&self, _: Duration) {}
    }

    #[test]
    fn each_line_lands_whole_with_several_writers_at_once() {
        // Each line MUST land whole at the end of the file even when two processes write at once
        // (keeper.md, Log lines). Each writer opens the file for each line, as two passes do.
        let dir = tempfile::Builder::new()
            .prefix("agentspace-keeper-unit-log-")
            .tempdir()
            .expect("A throwaway folder can be made");
        const WRITERS: usize = 4;
        const LINES: usize = 200;
        // Long enough that a line written in two steps would be seen torn.
        let tail = "x".repeat(300);
        std::thread::scope(|scope| {
            for writer in 0..WRITERS {
                let (path, tail) = (dir.path(), tail.as_str());
                scope.spawn(move || {
                    let mut sink = Vec::new();
                    let mut log = Logger::new(path, &Still, &mut sink);
                    for n in 0..LINES {
                        log.log(&format!("writer={writer} line={n} {tail}"))
                            .expect("The line is logged");
                    }
                });
            }
        });
        let text = std::fs::read_to_string(dir.path().join(LOG)).expect("The log is UTF-8");
        assert!(text.ends_with('\n'));
        let mut seen: Vec<&str> = text.lines().collect();
        assert_eq!(seen.len(), WRITERS * LINES);
        seen.sort_unstable();
        let mut want: Vec<String> = (0..WRITERS)
            .flat_map(|w| (0..LINES).map(move |n| (w, n)))
            .map(|(w, n)| format!("writer={w} line={n} {tail}"))
            .collect();
        want.sort_unstable();
        assert_eq!(seen, want);
    }

    #[test]
    fn whitespace_in_a_value_is_an_underscore() {
        assert_eq!(
            pass_line("T", "kept", "a b", "c\td\u{a0}e", "1", None),
            "T kept lease=a_b alias=c_d_e pid=1"
        );
    }

    #[test]
    fn a_reason_folds_control_runs_trims_and_is_never_empty() {
        assert_eq!(reason_text("a\r\n\tb\u{7f}c"), "a b c");
        assert_eq!(reason_text("  \n x \r\n"), "x");
        assert_eq!(reason_text(""), "Unknown.");
        assert_eq!(reason_text("\r\n"), "Unknown.");
        assert_eq!(reason_text("a  b"), "a  b");
    }
}
