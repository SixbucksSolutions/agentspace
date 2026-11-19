//! Adapters into the library's own API, for the tests that call it in process (docs/operator.md,
//! What a caller sees, Environment variables). Each is a thin function, so every library call
//! goes through this file and a change to the API changes one place.
//!
//! The adapters touch no real file and no real environment variable: the context and the
//! diagnostic flag are built from an injected lookup, and a printer writes to a buffer.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![allow(dead_code)]

use std::ffi::OsString;
use std::io::Write;
use std::sync::{Arc, Mutex};

pub use agentspace_operator::{CallerFailure, CallerLine, Context, Failure, Mode, Printer, Say};

/// A writer that keeps what is written, so a test reads what a printer said to stderr.
#[derive(Clone, Default)]
pub struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Capture {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap_or_else(|e| e.into_inner())).into_owned()
    }
}

/// A printer for `mode`, with `-Quiet` as `quiet` and AGENTSPACE_DIAGNOSTIC as `diagnostic`,
/// and what it says.
pub fn printer(mode: Mode, quiet: bool, diagnostic: bool) -> (Printer, Capture) {
    let capture = Capture::default();
    let say = Printer::new(mode, quiet, diagnostic, Box::new(capture.clone()));
    (say, capture)
}

/// A person's command.
pub fn person(quiet: bool) -> (Printer, Capture) {
    printer(Mode::Person, quiet, false)
}

/// The ProxyCommand's caller mode.
pub fn caller(quiet: bool, diagnostic: bool) -> (Printer, Capture) {
    printer(Mode::Caller, quiet, diagnostic)
}

pub fn progress(say: &Printer, person: &str, caller: Option<CallerLine>) {
    say.progress(person, caller);
}

pub fn warning(say: &Printer, person: &str) {
    say.warning(person);
}

pub fn detail(say: &Printer, text: &str) {
    say.detail(text);
}

pub fn mode(say: &Printer) -> Mode {
    say.mode()
}

/// A failure printed as the program prints it.
pub fn failure(say: &Printer, message: &str, caller: CallerFailure) {
    say.failure(&Failure {
        message: message.to_string(),
        caller,
    });
}

fn lookup_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let pairs: Vec<(String, String)> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |name: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| OsString::from(v))
    }
}

/// The context the program builds from `pairs` as its whole environment.
pub fn context(pairs: &[(&str, &str)]) -> Result<Context, Failure> {
    let lookup = lookup_of(pairs);
    agentspace_operator::context_from(&lookup)
}

/// Whether AGENTSPACE_DIAGNOSTIC counts as set, from `pairs` as the whole environment.
pub fn diagnostic(pairs: &[(&str, &str)]) -> bool {
    let lookup = lookup_of(pairs);
    agentspace_operator::diagnostic_from(&lookup)
}
