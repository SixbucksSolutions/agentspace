//! The guard's log, `%USERPROFILE%\.agentspace\guard.log` (docs/operator.md, guard, Its log;
//! docs/state.md, The guard's log; D56, D63).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! **Each line is written by opening the file for append, writing the whole line in one write and
//! closing it**, sharing read, write and delete, so a line lands whole at the end of the file even
//! when two processes write at once. The bytes are UTF-8 without a byte order mark, and each line
//! ends in one LF. The file is created at its first line, after its folder. An open or a write that
//! fails is tried up to 5 times, 50 ms apart, and a line that still cannot be written is dropped:
//! the first dropped line's error is kept, for the run's message `Cannot write the guard's log
//! <path>: <error>.` and its exit status. A dropped line MUST NOT change anything the run decided
//! or did.

use std::cell::RefCell;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use agentspace_hosts::{agentspace_dir, guard_log_path};
use agentspace_state::{format_time, now};

/// How many times an open or a write is tried (operator.md, guard, Its log).
const TRIES: u32 = 5;
/// The pause between two tries.
const RETRY_EVERY: Duration = Duration::from_millis(50);

/// The prefix of every message of agentspace's own.
const PREFIX: &str = "agentspace: ";

/// A message as the log and a host's stderr line carry it: without its `agentspace: ` prefix.
pub(crate) fn bare(message: &str) -> &str {
    message.strip_prefix(PREFIX).unwrap_or(message)
}

/// A `reason` as the log writes it: each control character, a line break included, written as `?`
/// (operator.md, guard, Its log).
pub(crate) fn reason(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// An I/O error's text as a sentence fragment: the system's own text, with no control character,
/// no `(os error <digits>)` suffix and no trailing period.
pub(crate) fn error_text(error: &std::io::Error) -> String {
    const OPEN: &str = " (os error ";
    let text: String = error
        .to_string()
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let text = text.trim();
    let open = text.strip_suffix(')').and_then(|body| {
        let open = body.rfind(OPEN)?;
        let code = &body[open + OPEN.len()..];
        let digits = code.strip_prefix('-').unwrap_or(code);
        (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then_some(open)
    });
    let text = open.map_or(text, |open| &text[..open]);
    text.trim().trim_end_matches('.').to_string()
}

/// One line of the log: `<time> host=<project> <text>`, or `<time> <text>` for a line that is not
/// about one host, with `time` in UTC to the whole second.
pub(crate) fn format_line(time: i64, project: Option<&str>, text: &str) -> String {
    match project {
        Some(project) => format!("{} host={project} {text}", format_time(time)),
        None => format!("{} {text}", format_time(time)),
    }
}

/// The guard's log, which makes no file until its first line.
pub(crate) struct GuardLog {
    home: PathBuf,
    path: PathBuf,
    /// The error text of the first line that could not be written.
    dropped: RefCell<Option<String>>,
}

impl GuardLog {
    pub(crate) fn new(home: &Path) -> GuardLog {
        GuardLog {
            home: home.to_path_buf(),
            path: guard_log_path(home),
            dropped: RefCell::new(None),
        }
    }

    /// The log's full path.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The error text of the first line that was dropped, or `None` when every line was written.
    pub(crate) fn dropped(&self) -> Option<String> {
        self.dropped.borrow().clone()
    }

    /// A line about one host: `<time> host=<project> <text>`, the time read now.
    pub(crate) fn host(&self, project: &str, text: &str) {
        self.write(&format_line(now(), Some(project), text));
    }

    /// A line about the run: `<time> <text>`, the time read now.
    pub(crate) fn run(&self, text: &str) {
        self.write(&format_line(now(), None, text));
    }

    /// `host=<project> failed step=<step> reason=<reason>`: `message` is the failure's message, with
    /// or without its `agentspace: ` prefix.
    pub(crate) fn host_failed(&self, project: &str, step: &str, message: &str) {
        self.host(
            project,
            &format!("failed step={step} reason={}", reason(bare(message))),
        );
    }

    /// `failed step=<step> reason=<reason>`, with no `host=` field.
    pub(crate) fn run_failed(&self, step: &str, message: &str) {
        self.run(&format!(
            "failed step={step} reason={}",
            reason(bare(message))
        ));
    }

    /// Writes one whole line, LF-ended, trying up to [`TRIES`] times. A line that cannot be
    /// written is dropped, and the first drop's error is kept.
    fn write(&self, line: &str) {
        let mut bytes = Vec::with_capacity(line.len() + 1);
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
        let mut last = None;
        for attempt in 0..TRIES {
            match append(&self.home, &self.path, &bytes) {
                Ok(()) => return,
                Err(e) => last = Some(e),
            }
            if attempt + 1 < TRIES {
                std::thread::sleep(RETRY_EVERY);
            }
        }
        let mut dropped = self.dropped.borrow_mut();
        if dropped.is_none() {
            *dropped = Some(last.map_or_else(
                || "the line could not be written".to_string(),
                |e| error_text(&e),
            ));
        }
    }
}

/// Opens the log for append sharing read, write and delete, creating it, and the folder before it,
/// when missing, and writes `bytes` in one write.
fn append(home: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.append(true).create(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE.
        options.share_mode(1 | 2 | 4);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(first) => {
            // A missing folder is made, and the open tried once more; any other failure is the
            // try's.
            if path.parent().is_some_and(|dir| !dir.exists()) {
                std::fs::create_dir_all(agentspace_dir(home))?;
                options.open(path)?
            } else {
                return Err(first);
            }
        }
    };
    file.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn home() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn a_line_is_time_host_and_text() {
        assert_eq!(
            format_line(1_795_075_954, Some("example"), "stopped since=x"),
            "2026-11-19T08:12:34Z host=example stopped since=x"
        );
        assert_eq!(
            format_line(1_795_075_954, None, "failed step=leases reason=r"),
            "2026-11-19T08:12:34Z failed step=leases reason=r"
        );
    }

    #[test]
    fn a_reason_has_no_prefix_and_no_control_character() {
        assert_eq!(
            bare("agentspace: Could not stop i-0."),
            "Could not stop i-0."
        );
        assert_eq!(bare("Plain"), "Plain");
        assert_eq!(reason("a\r\nb\tc\u{7}d"), "a??b?c?d");
        assert_eq!(reason("fine: é ü"), "fine: é ü");
    }

    #[test]
    fn an_io_error_text_is_a_fragment() {
        let e = std::io::Error::from_raw_os_error(5);
        let text = error_text(&e);
        assert!(!text.contains("os error"), "{text}");
        assert!(!text.ends_with('.'));
        assert!(!text.is_empty());
        let plain = std::io::Error::other("it broke.\n");
        assert_eq!(error_text(&plain), "it broke");
    }

    #[test]
    fn the_log_is_made_at_its_first_line_with_its_folder() {
        let t = home();
        let log = GuardLog::new(t.path());
        assert!(!log.path().exists(), "Nothing is made before a line");
        log.host("example", "stopped since=2026-11-18T08:00:00Z");
        let bytes = fs::read(log.path()).unwrap();
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.ends_with('\n') && !text.contains('\r'));
        assert_eq!(text.lines().count(), 1);
        assert!(
            text.lines()
                .next()
                .unwrap()
                .ends_with(" host=example stopped since=2026-11-18T08:00:00Z")
        );
        assert_eq!(log.dropped(), None);
    }

    #[test]
    fn lines_are_appended_and_the_file_is_never_truncated() {
        let t = home();
        let log = GuardLog::new(t.path());
        log.run_failed("describe", "agentspace: Throttled\nagain");
        GuardLog::new(t.path()).host_failed("example", "stop", "agentspace: Denied");
        let text = fs::read_to_string(log.path()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with("Z failed step=describe reason=Throttled?again"));
        assert!(lines[1].ends_with("Z host=example failed step=stop reason=Denied"));
    }

    #[test]
    fn a_line_is_written_while_a_reader_holds_the_file_open() {
        let t = home();
        let log = GuardLog::new(t.path());
        log.run("failed step=lock reason=first");
        // A reader sharing read, write and delete, as the log's own readers do.
        #[cfg(windows)]
        let reader = {
            use std::os::windows::fs::OpenOptionsExt;
            OpenOptions::new()
                .read(true)
                .share_mode(7)
                .open(log.path())
                .unwrap()
        };
        #[cfg(not(windows))]
        let reader = fs::File::open(log.path()).unwrap();
        log.run("failed step=lock reason=second");
        drop(reader);
        assert_eq!(fs::read_to_string(log.path()).unwrap().lines().count(), 2);
        assert_eq!(log.dropped(), None);
    }

    #[test]
    fn a_line_that_cannot_be_written_is_dropped_and_the_first_error_kept() {
        let t = home();
        // A folder at the log's path: every try fails.
        fs::create_dir_all(guard_log_path(t.path())).unwrap();
        let log = GuardLog::new(t.path());
        log.run("failed step=lock reason=one");
        let first = log.dropped().expect("The line is dropped");
        log.run("failed step=lock reason=two");
        assert_eq!(log.dropped(), Some(first.clone()));
        assert!(!first.is_empty() && !first.contains('\n'));
        assert!(guard_log_path(t.path()).is_dir(), "Left as it was");
    }

    #[test]
    fn the_tries_are_five_and_fifty_milliseconds_apart() {
        assert_eq!(TRIES, 5);
        assert_eq!(RETRY_EVERY, Duration::from_millis(50));
        let t = home();
        fs::create_dir_all(guard_log_path(t.path())).unwrap();
        let log = GuardLog::new(t.path());
        let start = std::time::Instant::now();
        log.run("x");
        // Four pauses between five tries.
        assert!(start.elapsed() >= Duration::from_millis(200));
    }
}
