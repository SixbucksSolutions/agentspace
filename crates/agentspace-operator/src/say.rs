//! The pinned [`Say`] for a person's command and for caller mode (docs/operator.md, Messages and
//! What a caller sees).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Everything goes to the one writer the printer is given, which the program makes stderr. Write
//! errors are ignored: a closed stderr MUST NOT panic the program or change its exit code.

use std::io::Write;
use std::sync::Mutex;

use agentspace_operator_api::{CallerFailure, CallerLine, Failure, Mode, Say};

/// What the caller's progress lines say.
fn caller_text(line: CallerLine) -> &'static str {
    match line {
        CallerLine::Starting => "Starting the host...",
        CallerLine::Waiting => "Waiting for the host to be ready...",
        CallerLine::Ready => "The host is ready.",
    }
}

fn caller_bit(line: CallerLine) -> u8 {
    match line {
        CallerLine::Starting => 1,
        CallerLine::Waiting => 2,
        CallerLine::Ready => 4,
    }
}

/// The caller's failure lines, from operator.md's table.
pub const NOT_AVAILABLE: &str = "agentspace: The host is not available, and could not be started.";
pub const NOT_READY: &str =
    "agentspace: The host was not ready within 15 minutes; try again later.";
pub const COULD_NOT_REACH: &str = "agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.";

/// The warning as a caller sees it: `the instance` written `the host`, and `The instance` written
/// `The host`.
pub fn caller_warning(person: &str) -> String {
    person
        .replace("the instance", "the host")
        .replace("The instance", "The host")
}

struct Inner {
    out: Box<dyn Write + Send>,
    /// The caller lines already printed, one bit each.
    printed: u8,
}

/// Speaks for a person's command ([`Printer::person`]) or the ProxyCommand's caller mode
/// ([`Printer::caller`]), to any writer.
pub struct Printer {
    mode: Mode,
    quiet: bool,
    /// `AGENTSPACE_DIAGNOSTIC` set and not empty, read once by the caller of the constructor.
    diagnostic: bool,
    inner: Mutex<Inner>,
}

impl Printer {
    /// A printer for `mode` that writes to `out`. `quiet` is `-Quiet`; `diagnostic` is whether
    /// `AGENTSPACE_DIAGNOSTIC` was set and not empty as the program started.
    pub fn new(mode: Mode, quiet: bool, diagnostic: bool, out: Box<dyn Write + Send>) -> Printer {
        Printer {
            mode,
            quiet,
            diagnostic,
            inner: Mutex::new(Inner { out, printed: 0 }),
        }
    }

    /// A person's command, writing to stderr.
    pub fn person(quiet: bool) -> Printer {
        Printer::new(Mode::Person, quiet, false, Box::new(std::io::stderr()))
    }

    /// The ProxyCommand's caller mode, writing to stderr.
    pub fn caller(quiet: bool, diagnostic: bool) -> Printer {
        Printer::new(Mode::Caller, quiet, diagnostic, Box::new(std::io::stderr()))
    }

    fn write_line(&self, text: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let text = text.strip_suffix('\n').unwrap_or(text);
        let _ = inner.out.write_all(format!("{text}\n").as_bytes());
        let _ = inner.out.flush();
    }

    /// Prints a failure's one line, whatever `-Quiet` says. In caller mode it prints the caller's
    /// line in place of the person's, as operator.md's table gives it. The caller adds detail with
    /// [`Say::detail`] after it.
    pub fn failure(&self, failure: &Failure) {
        match (self.mode, failure.caller) {
            (Mode::Person, _) | (Mode::Caller, CallerFailure::Same) => {
                self.write_line(&failure.message)
            }
            (Mode::Caller, CallerFailure::NotAvailable) => self.write_line(NOT_AVAILABLE),
            (Mode::Caller, CallerFailure::NotReady) => self.write_line(NOT_READY),
            (Mode::Caller, CallerFailure::CouldNotReach) => self.write_line(COULD_NOT_REACH),
        }
    }
}

impl Say for Printer {
    fn mode(&self) -> Mode {
        self.mode
    }

    fn progress(&self, person: &str, caller: Option<CallerLine>) {
        if self.quiet {
            return;
        }
        match self.mode {
            Mode::Person => self.write_line(person),
            Mode::Caller => {
                let Some(line) = caller else { return };
                {
                    let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
                    let bit = caller_bit(line);
                    if inner.printed & bit != 0 {
                        return;
                    }
                    inner.printed |= bit;
                }
                self.write_line(caller_text(line));
            }
        }
    }

    fn warning(&self, person: &str) {
        if self.quiet {
            return;
        }
        match self.mode {
            Mode::Person => self.write_line(person),
            Mode::Caller => self.write_line(&caller_warning(person)),
        }
    }

    fn detail(&self, text: &str) {
        if self.mode == Mode::Caller && self.diagnostic {
            self.write_line(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Write for Capture {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Capture {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    fn printer(mode: Mode, quiet: bool, diagnostic: bool) -> (Printer, Capture) {
        let cap = Capture::default();
        (
            Printer::new(mode, quiet, diagnostic, Box::new(cap.clone())),
            cap,
        )
    }

    #[test]
    fn a_person_sees_every_line() {
        let (say, cap) = printer(Mode::Person, false, true);
        say.progress(
            "Starting i-0123456789abcdef0...",
            Some(CallerLine::Starting),
        );
        say.progress(
            "Starting i-0123456789abcdef0...",
            Some(CallerLine::Starting),
        );
        say.progress("Waiting for Session Manager...", None);
        say.warning("agentspace: The instance said no.");
        say.detail("backend text");
        assert_eq!(
            cap.text(),
            "Starting i-0123456789abcdef0...\nStarting i-0123456789abcdef0...\nWaiting for Session Manager...\nagentspace: The instance said no.\n"
        );
    }

    #[test]
    fn quiet_suppresses_progress_and_warnings_in_both_modes() {
        for mode in [Mode::Person, Mode::Caller] {
            let (say, cap) = printer(mode, true, true);
            say.progress("x", Some(CallerLine::Ready));
            say.warning("agentspace: x");
            assert_eq!(cap.text(), "", "{mode:?}");
        }
    }

    #[test]
    fn a_failure_prints_whatever_quiet_says() {
        let (say, cap) = printer(Mode::Person, true, false);
        say.failure(&Failure {
            message: "agentspace: Down.".to_string(),
            caller: CallerFailure::CouldNotReach,
        });
        assert_eq!(cap.text(), "agentspace: Down.\n");
    }

    #[test]
    fn a_caller_sees_each_line_once_and_nothing_for_none() {
        let (say, cap) = printer(Mode::Caller, false, false);
        say.progress(
            "Starting i-0123456789abcdef0...",
            Some(CallerLine::Starting),
        );
        say.progress(
            "Waiting for i-0123456789abcdef0 to finish stopping...",
            None,
        );
        say.progress(
            "Starting i-0123456789abcdef0...",
            Some(CallerLine::Starting),
        );
        say.progress("Waiting for Session Manager...", Some(CallerLine::Waiting));
        say.progress("Patching Ubuntu...", Some(CallerLine::Waiting));
        say.progress("Ready: i-0123456789abcdef0", Some(CallerLine::Ready));
        say.progress("Ready: i-0123456789abcdef0", Some(CallerLine::Ready));
        assert_eq!(
            cap.text(),
            "Starting the host...\nWaiting for the host to be ready...\nThe host is ready.\n"
        );
    }

    #[test]
    fn a_caller_warning_says_host_for_instance() {
        let (say, cap) = printer(Mode::Caller, false, false);
        say.warning("agentspace: The instance has no token, so the instance gets none.");
        assert_eq!(
            cap.text(),
            "agentspace: The host has no token, so the host gets none.\n"
        );
    }

    #[test]
    fn detail_prints_only_in_caller_mode_with_diagnostic() {
        let (say, cap) = printer(Mode::Caller, false, true);
        say.detail("backend text");
        assert_eq!(cap.text(), "backend text\n");
        let (say, cap) = printer(Mode::Caller, false, false);
        say.detail("backend text");
        assert_eq!(cap.text(), "");
        let (say, cap) = printer(Mode::Person, false, true);
        say.detail("backend text");
        assert_eq!(cap.text(), "");
    }

    #[test]
    fn a_caller_failure_prints_the_callers_line() {
        let cases = [
            (CallerFailure::Same, "agentspace: Person's own."),
            (CallerFailure::NotAvailable, NOT_AVAILABLE),
            (CallerFailure::NotReady, NOT_READY),
            (CallerFailure::CouldNotReach, COULD_NOT_REACH),
        ];
        for (caller, line) in cases {
            let (say, cap) = printer(Mode::Caller, false, false);
            say.failure(&Failure {
                message: "agentspace: Person's own.".to_string(),
                caller,
            });
            assert_eq!(cap.text(), format!("{line}\n"));
        }
    }
}
