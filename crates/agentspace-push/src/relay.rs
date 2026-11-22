//! The push's ssh's stderr in the ProxyCommand's caller mode (operator.md, What the laptop sends,
//! step 2; What a caller sees; #0167, P14).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - [`SshStderr`] is the caller's choice of where the ssh's stderr goes: [`SshStderr::Inherit`],
//!   the operator's own, for a person's `up`, `up -Host` and `sync`; or [`SshStderr::Relay`], a
//!   pipe whose lines reach a sink through [`StderrFilter`], for the ProxyCommand.
//! - [`StderrFilter`] is the line rule alone, a pure function over byte chunks: every line passes
//!   with the line ending it came with, but a line that is exactly [`HOST_READY_LINE`], compared
//!   without its LF or CR LF, is dropped. A last line with no ending passes as it is. Bytes are
//!   compared and never decoded. Under `AGENTSPACE_DIAGNOSTIC` nothing is dropped.
//! - [`Relay`] reads the pipe on a thread of its own while the push runs, and no longer: on
//!   [`Relay::drain`] it reads what the pipe already holds, passes it on and stops, without waiting
//!   for more; on [`Relay::stop`] it stops reading at once. It never blocks on the pipe: it reads
//!   only what the pipe says it holds, so a full pipe never stops the ssh and neither call waits on
//!   a process that keeps the pipe open.

use std::io::Write;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// The line a caller-mode push drops from its ssh's stderr: its nested ProxyCommand's own
/// `The host is ready.`, so that a connection prints that line once, its own (#0167).
pub const HOST_READY_LINE: &[u8] = b"The host is ready.";

/// Where the relayed lines go: the ProxyCommand's stderr, shared with its own printer so that each
/// write reaches it whole.
pub type StderrSink = Arc<Mutex<dyn Write + Send>>;

/// Where the push's ssh's stderr goes (operator.md, What the laptop sends, step 2). The caller
/// chooses it; nothing else in the push depends on it.
#[derive(Clone, Default)]
pub enum SshStderr {
    /// The operator's own stderr, inherited: a person's `up`, `up -Host` and `sync`. The default.
    #[default]
    Inherit,
    /// The ProxyCommand's caller mode: a pipe, each line of which is written to `sink` as it
    /// arrives, through [`StderrFilter`]. `diagnostic` is whether `AGENTSPACE_DIAGNOSTIC` was set
    /// to a non-empty value as the ProxyCommand started; then nothing is dropped.
    Relay { sink: StderrSink, diagnostic: bool },
}

/// Names the choice and never the sink.
impl std::fmt::Debug for SshStderr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SshStderr::Inherit => f.write_str("Inherit"),
            SshStderr::Relay { diagnostic, .. } => f
                .debug_struct("Relay")
                .field("diagnostic", diagnostic)
                .finish_non_exhaustive(),
        }
    }
}

/// The caller-mode line rule over the bytes of the push's ssh's stderr, chunk by chunk.
///
/// [`feed`](StderrFilter::feed) takes the bytes as they were read, wherever the reads split the
/// lines, and gives back the bytes to pass on now. A line that can no longer be
/// [`HOST_READY_LINE`] passes as soon as that is known; one that still could is held until its LF.
/// [`finish`](StderrFilter::finish) ends the stream: a last line with no ending is given back as it
/// is, with none added, unless it is exactly [`HOST_READY_LINE`].
#[derive(Debug, Clone, Default)]
pub struct StderrFilter {
    /// `AGENTSPACE_DIAGNOSTIC`: every byte passes as it comes.
    pass_all: bool,
    /// The current line's bytes, while it could still be the ready line.
    held: Vec<u8>,
    /// The current line cannot be the ready line: its bytes pass as they come, up to its LF.
    passing: bool,
}

impl StderrFilter {
    /// A filter that drops the ready line, or, with `diagnostic`, drops nothing.
    pub fn new(diagnostic: bool) -> StderrFilter {
        StderrFilter {
            pass_all: diagnostic,
            ..StderrFilter::default()
        }
    }

    /// The bytes of `chunk` to pass on now, in order and unchanged.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<u8> {
        if self.pass_all {
            return chunk.to_vec();
        }
        let mut out = Vec::with_capacity(chunk.len());
        for &byte in chunk {
            if self.passing {
                out.push(byte);
                if byte == b'\n' {
                    self.passing = false;
                }
                continue;
            }
            if byte == b'\n' {
                if !ends_as_ready_line(&self.held) {
                    out.extend_from_slice(&self.held);
                    out.push(b'\n');
                }
                self.held.clear();
                continue;
            }
            self.held.push(byte);
            if !could_be_ready_line(&self.held) {
                out.append(&mut self.held);
                self.passing = true;
            }
        }
        out
    }

    /// The end of the stream: the last line's held bytes, when it had no ending, as they are. A
    /// last line that is exactly [`HOST_READY_LINE`] is dropped like any other.
    pub fn finish(&mut self) -> Vec<u8> {
        let held = std::mem::take(&mut self.held);
        self.passing = false;
        if !self.pass_all && held == HOST_READY_LINE {
            return Vec::new();
        }
        held
    }
}

/// Whether a line, without its LF, is the ready line: exactly it, or it with the CR of a CR LF.
fn ends_as_ready_line(line: &[u8]) -> bool {
    line.strip_suffix(b"\r").unwrap_or(line) == HOST_READY_LINE
}

/// Whether a line's bytes so far, with no LF yet, may still turn out to be the ready line.
fn could_be_ready_line(held: &[u8]) -> bool {
    HOST_READY_LINE.starts_with(held)
        || held
            .strip_suffix(b"\r")
            .is_some_and(|line| line == HOST_READY_LINE)
}

/// A pipe as the relay reads it. Neither call blocks: the relay reads only what is there.
pub(crate) trait Source: Send + 'static {
    /// How many bytes can be read now without blocking, or `None` when the pipe has ended: every
    /// writer has closed it and nothing is left.
    fn available(&mut self) -> Option<usize>;
    /// Reads at most `buf.len()` bytes, which MUST be no more than [`Source::available`] gave.
    /// `0` when the pipe has ended or the read failed.
    fn read(&mut self, buf: &mut [u8]) -> usize;
}

/// How long the relay waits before it looks at an empty pipe again.
const POLL: Duration = Duration::from_millis(10);
/// The most it reads at once.
const CHUNK: usize = 8192;

const RUN: u8 = 0;
const DRAIN: u8 = 1;
const STOP: u8 = 2;

struct Shared {
    phase: AtomicU8,
    sink: StderrSink,
}

/// The thread that reads the ssh's stderr pipe and passes its lines on, while the push runs.
/// Dropped without [`Relay::drain`], it stops as [`Relay::stop`] does.
pub(crate) struct Relay {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Relay {
    /// Starts reading `source` into `sink`. `None` when its thread cannot be started.
    pub(crate) fn start(source: impl Source, sink: StderrSink, diagnostic: bool) -> Option<Relay> {
        let shared = Arc::new(Shared {
            phase: AtomicU8::new(RUN),
            sink,
        });
        let theirs = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .spawn(move || pump(source, &theirs, StderrFilter::new(diagnostic)))
            .ok()?;
        Some(Relay {
            shared,
            thread: Some(thread),
        })
    }

    /// The ssh has exited and its stdout has ended, or been given up on: reads what the pipe holds
    /// now, passes it on, a last line with no ending included, and returns once that is done. It
    /// never waits for more to arrive.
    pub(crate) fn drain(mut self) {
        self.shared.phase.store(DRAIN, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    /// The tree has been stopped at the limit: reads nothing more and passes nothing more read,
    /// a line still held included. Returns at once, without waiting for the thread.
    pub(crate) fn stop(self) {
        // Dropping it stops it.
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.shared.phase.store(STOP, Ordering::SeqCst);
        }
    }
}

/// The relay's thread.
fn pump(mut source: impl Source, shared: &Shared, mut filter: StderrFilter) {
    let mut buf = vec![0u8; CHUNK];
    // In the drain: how many bytes the pipe held when it began, and are still to be read.
    let mut left: Option<usize> = None;
    loop {
        match shared.phase.load(Ordering::SeqCst) {
            STOP => return,
            DRAIN if left.is_none() => left = Some(source.available().unwrap_or(0)),
            _ => {}
        }
        let want = match left {
            Some(0) => break,
            Some(n) => n,
            None => match source.available() {
                None => break,
                Some(0) => {
                    std::thread::sleep(POLL);
                    continue;
                }
                Some(n) => n,
            },
        };
        let got = source.read(&mut buf[..want.min(CHUNK)]);
        if got == 0 {
            break;
        }
        if let Some(n) = left.as_mut() {
            *n = n.saturating_sub(got);
        }
        if !emit(shared, &filter.feed(&buf[..got])) {
            return;
        }
    }
    emit(shared, &filter.finish());
}

/// Writes `bytes` to the sink, unless the relay has been stopped. `false` when it has.
fn emit(shared: &Shared, bytes: &[u8]) -> bool {
    let mut sink = shared.sink.lock().unwrap_or_else(|e| e.into_inner());
    if shared.phase.load(Ordering::SeqCst) == STOP {
        return false;
    }
    if !bytes.is_empty() {
        let _ = sink.write_all(bytes);
        let _ = sink.flush();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::time::Instant;

    /// Feeds `chunks` in order, then finishes.
    fn filtered(diagnostic: bool, chunks: &[&[u8]]) -> Vec<u8> {
        let mut filter = StderrFilter::new(diagnostic);
        let mut out = Vec::new();
        for chunk in chunks {
            out.extend(filter.feed(chunk));
        }
        out.extend(filter.finish());
        out
    }

    /// Feeds `bytes` one byte at a time, then finishes.
    fn bytewise(diagnostic: bool, bytes: &[u8]) -> Vec<u8> {
        let chunks: Vec<&[u8]> = bytes.chunks(1).collect();
        filtered(diagnostic, &chunks)
    }

    #[test]
    fn the_exact_line_is_dropped_with_an_lf_or_a_cr_lf_and_every_other_line_passes_as_it_came() {
        let input = b"Warning: Permanently added 'x' (ED25519) to the list of known hosts.\r\n\
The host is ready.\r\n\
ssh: one\n\
The host is ready.\n\
\n\
\r\n\
two\r\n";
        let want = b"Warning: Permanently added 'x' (ED25519) to the list of known hosts.\r\n\
ssh: one\n\
\n\
\r\n\
two\r\n";
        assert_eq!(filtered(false, &[input]), want);
        assert_eq!(bytewise(false, input), want);
    }

    #[test]
    fn lines_split_across_reads_are_judged_whole() {
        let want = b"a\r\nb\n";
        assert_eq!(
            filtered(
                false,
                &[b"a\r", b"\nThe host", b" is re", b"ady.\r", b"\nb\n"]
            ),
            want
        );
        assert_eq!(
            filtered(false, &[b"The host is ready.", b"\r", b"\n", b"a\r\nb\n"]),
            want
        );
        // A line that starts as the ready line and then is not passes whole, its start included.
        assert_eq!(
            filtered(false, &[b"The host is", b" ready", b".. no\n"]),
            b"The host is ready.. no\n"
        );
    }

    #[test]
    fn a_near_miss_passes_trailing_spaces_and_other_endings_included() {
        for line in [
            &b"The host is ready. \n"[..],
            b" The host is ready.\n",
            b"The host is ready.\r\r\n",
            b"The host is ready\n",
            b"the host is ready.\n",
            b"The host is ready.x\n",
            b"agentspace: The host is ready.\n",
            b"The host is ready.\tx\n",
        ] {
            assert_eq!(filtered(false, &[line]), line, "{line:?}");
            assert_eq!(bytewise(false, line), line, "{line:?}");
        }
    }

    #[test]
    fn a_last_line_with_no_ending_passes_as_it_is() {
        assert_eq!(filtered(false, &[b"a\nlast"]), b"a\nlast");
        assert_eq!(filtered(false, &[b"a\nThe host"]), b"a\nThe host");
        // A CR alone is not a line ending: the line is not exactly the ready line.
        assert_eq!(
            filtered(false, &[b"The host is ready.\r"]),
            b"The host is ready.\r"
        );
        // Exactly the ready line, at the end with no ending, is dropped like any other.
        assert_eq!(filtered(false, &[b"a\nThe host is ready."]), b"a\n");
        assert_eq!(filtered(false, &[]), b"");
    }

    #[test]
    fn bytes_that_are_not_utf8_pass_unchanged() {
        let input: &[u8] = b"\xff\xfe bad \xc3\x28\r\nThe host is ready.\n\x80\nend \xe2\x82";
        let want: &[u8] = b"\xff\xfe bad \xc3\x28\r\n\x80\nend \xe2\x82";
        assert_eq!(filtered(false, &[input]), want);
        assert_eq!(bytewise(false, input), want);
        // UTF-16 text of the ready line is other bytes, and passes.
        let wide: Vec<u8> = "The host is ready.\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(filtered(false, &[&wide]), wide);
    }

    #[test]
    fn a_line_passes_as_soon_as_it_cannot_be_the_ready_line() {
        let mut filter = StderrFilter::new(false);
        assert_eq!(filter.feed(b"ssh: conn"), b"ssh: conn");
        assert_eq!(filter.feed(b"ect"), b"ect");
        assert_eq!(filter.feed(b"\r\nThe host"), b"\r\n");
        assert_eq!(filter.feed(b" is ready.\r\n"), b"");
    }

    #[test]
    fn with_diagnostic_nothing_is_dropped() {
        let input = b"The host is ready.\r\nThe host is ready.\nx\nThe host is ready.";
        assert_eq!(filtered(true, &[input]), input);
        assert_eq!(bytewise(true, input), input);
        let mut filter = StderrFilter::new(true);
        assert_eq!(filter.feed(b"The host"), b"The host");
    }

    // The relay, over a stand-in pipe that the test writes to and ends.

    #[derive(Default)]
    struct Pipe {
        bytes: VecDeque<u8>,
        ended: bool,
        reads: usize,
        peeks: usize,
    }

    #[derive(Clone, Default)]
    struct FakePipe(Arc<Mutex<Pipe>>);

    impl FakePipe {
        fn write(&self, bytes: &[u8]) {
            self.0.lock().unwrap().bytes.extend(bytes);
        }
        fn end(&self) {
            self.0.lock().unwrap().ended = true;
        }
        fn reads(&self) -> usize {
            self.0.lock().unwrap().reads
        }
        /// Waits up to 5 seconds for the relay to have looked at the pipe `n` times.
        fn wait_for_peeks(&self, n: usize) -> bool {
            let until = Instant::now() + Duration::from_secs(5);
            while Instant::now() < until {
                if self.0.lock().unwrap().peeks >= n {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            false
        }
    }

    impl Source for FakePipe {
        fn available(&mut self) -> Option<usize> {
            let mut pipe = self.0.lock().unwrap();
            pipe.peeks += 1;
            if pipe.bytes.is_empty() && pipe.ended {
                None
            } else {
                Some(pipe.bytes.len())
            }
        }
        fn read(&mut self, buf: &mut [u8]) -> usize {
            let mut pipe = self.0.lock().unwrap();
            assert!(
                buf.len() <= pipe.bytes.len(),
                "A read asked for more than the pipe held"
            );
            pipe.reads += 1;
            for slot in buf.iter_mut() {
                *slot = pipe.bytes.pop_front().unwrap();
            }
            buf.len()
        }
    }

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
        fn bytes(&self) -> Vec<u8> {
            self.0.lock().unwrap().clone()
        }
        fn sink(&self) -> StderrSink {
            Arc::new(Mutex::new(self.clone()))
        }
        /// Waits up to 5 seconds for the capture to hold `want`.
        fn wait_for(&self, want: &[u8]) -> bool {
            let until = Instant::now() + Duration::from_secs(5);
            while Instant::now() < until {
                if self.bytes() == want {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            false
        }
    }

    #[test]
    fn lines_are_passed_on_as_they_arrive_while_the_push_runs() {
        let pipe = FakePipe::default();
        let out = Capture::default();
        let relay = Relay::start(pipe.clone(), out.sink(), false).unwrap();
        pipe.write(b"first\r\n");
        assert!(
            out.wait_for(b"first\r\n"),
            "The first line was not passed on"
        );
        pipe.write(b"The host is ready.\r\nsecond\n");
        assert!(out.wait_for(b"first\r\nsecond\n"));
        pipe.end();
        relay.drain();
        assert_eq!(out.bytes(), b"first\r\nsecond\n");
    }

    #[test]
    fn the_drain_passes_what_the_pipe_holds_and_never_waits_for_more() {
        // A process that outlived the ssh keeps the pipe open: it never ends.
        let pipe = FakePipe::default();
        let out = Capture::default();
        let relay = Relay::start(pipe.clone(), out.sink(), false).unwrap();
        // The relay has found the pipe empty and waits before it looks again, so the bytes are
        // almost always still in the pipe when the drain begins, and the drain reads them.
        assert!(pipe.wait_for_peeks(1), "The relay never looked at the pipe");
        pipe.write(b"held\nThe host is ready.\nssh: last error, with no ending");
        let started = Instant::now();
        relay.drain();
        let took = started.elapsed();
        assert!(took < Duration::from_secs(1), "The drain took {took:?}");
        assert_eq!(out.bytes(), b"held\nssh: last error, with no ending");
        // What arrives after the drain is not read, and not passed on.
        let reads = pipe.reads();
        pipe.write(b"later\n");
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(out.bytes(), b"held\nssh: last error, with no ending");
        assert_eq!(pipe.reads(), reads, "The pipe was read after the drain");
    }

    #[test]
    fn the_drain_of_an_empty_open_pipe_returns_at_once() {
        let pipe = FakePipe::default();
        let out = Capture::default();
        let relay = Relay::start(pipe.clone(), out.sink(), false).unwrap();
        // The drain runs on a thread of the test's, so a drain that waits for the pipe to end
        // fails here rather than hanging: the pipe is then ended, which lets it return.
        let (drained, at_drain) = std::sync::mpsc::channel();
        let drainer = std::thread::spawn(move || {
            relay.drain();
            let _ = drained.send(());
        });
        let waited = at_drain.recv_timeout(Duration::from_secs(1)).is_err();
        if waited {
            pipe.end();
        }
        drainer.join().unwrap();
        assert!(!waited, "The drain waited for the pipe to end");
        assert!(out.bytes().is_empty());
    }

    #[test]
    fn the_stop_reads_and_passes_nothing_more_and_drops_a_held_line() {
        let pipe = FakePipe::default();
        let out = Capture::default();
        let relay = Relay::start(pipe.clone(), out.sink(), false).unwrap();
        pipe.write(b"before\nThe host is");
        assert!(out.wait_for(b"before\n"));
        // Let the relay take the held start of a line too.
        let until = Instant::now() + Duration::from_secs(5);
        while !pipe.0.lock().unwrap().bytes.is_empty() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        // The stop runs on a thread of the test's, so a stop that waits for the pipe to end fails
        // here rather than hanging: the pipe is then ended, which lets it return.
        let (stopped, at_stop) = std::sync::mpsc::channel();
        let started = Instant::now();
        let stopper = std::thread::spawn(move || {
            relay.stop();
            let _ = stopped.send(());
        });
        let waited = at_stop.recv_timeout(Duration::from_secs(1)).is_err();
        if waited {
            pipe.end();
        }
        stopper.join().unwrap();
        assert!(!waited, "The stop waited for the pipe to end");
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "The stop waited"
        );
        pipe.write(b" ready.\nafter\n");
        pipe.end();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(out.bytes(), b"before\n");
        assert_eq!(
            pipe.0.lock().unwrap().bytes.len(),
            b" ready.\nafter\n".len(),
            "The pipe was read after the stop"
        );
    }

    #[test]
    fn the_relay_keeps_reading_however_much_arrives() {
        let pipe = FakePipe::default();
        let out = Capture::default();
        let relay = Relay::start(pipe.clone(), out.sink(), false).unwrap();
        let mut want = Vec::new();
        for i in 0..20_000 {
            let line = format!("line {i}\r\n");
            pipe.write(line.as_bytes());
            pipe.write(b"The host is ready.\r\n");
            want.extend(line.as_bytes());
        }
        pipe.end();
        assert!(out.wait_for(&want), "Not every line was passed on");
        relay.drain();
        assert_eq!(out.bytes(), want);
    }

    #[test]
    fn with_diagnostic_the_relay_drops_nothing() {
        let pipe = FakePipe::default();
        let out = Capture::default();
        let relay = Relay::start(pipe.clone(), out.sink(), true).unwrap();
        pipe.write(b"The host is ready.\r\nx\n");
        pipe.end();
        assert!(out.wait_for(b"The host is ready.\r\nx\n"));
        relay.drain();
    }

    #[test]
    fn the_choice_names_no_sink() {
        let out = Capture::default();
        let relay = SshStderr::Relay {
            sink: out.sink(),
            diagnostic: true,
        };
        assert_eq!(format!("{relay:?}"), "Relay { diagnostic: true, .. }");
        assert_eq!(format!("{:?}", SshStderr::default()), "Inherit");
    }
}
