//! The push's calls on a system that is not Windows. These are NOT THE CONTRACT'S: they exist so
//! that the crate builds and its portable logic is tested on Linux (operator.md, The token push is
//! the laptop's, and the laptop is Windows).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

use std::collections::VecDeque;
use std::io::Read;
use std::process::{Child, ChildStderr};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

/// The ssh's stderr pipe, for the caller-mode relay: a thread of its own reads it, blocking, and
/// hands each chunk over, so that the relay itself never blocks on it.
pub struct StderrPipe {
    chunks: mpsc::Receiver<Vec<u8>>,
    held: VecDeque<u8>,
    ended: bool,
}

/// The relay's reader of the ssh's stderr, or `None` when its thread cannot be started.
pub fn stderr_pipe(mut pipe: ChildStderr) -> Option<StderrPipe> {
    let (sent, chunks) = mpsc::channel();
    std::thread::Builder::new()
        .spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match pipe.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => {
                        if sent.send(buf[..n].to_vec()).is_err() {
                            return;
                        }
                    }
                }
            }
        })
        .ok()?;
    Some(StderrPipe {
        chunks,
        held: VecDeque::new(),
        ended: false,
    })
}

impl crate::relay::Source for StderrPipe {
    fn available(&mut self) -> Option<usize> {
        loop {
            match self.chunks.try_recv() {
                Ok(chunk) => self.held.extend(chunk),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.ended = true;
                    break;
                }
            }
        }
        if self.ended && self.held.is_empty() {
            None
        } else {
            Some(self.held.len())
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> usize {
        let n = buf.len().min(self.held.len());
        for (slot, byte) in buf.iter_mut().zip(self.held.drain(..n)) {
            *slot = byte;
        }
        n
    }
}

/// There is no DPAPI here: every blob is refused.
pub fn unprotect(_blob: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    None
}

/// Waits until the process has exited or `deadline` passes, by polling. `true` when it has exited.
pub fn wait_for_exit(child: &mut Child, deadline: Instant) -> bool {
    loop {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Stops the ssh alone. The Windows half stops its whole tree; this does not. It waits a moment for
/// the killed process, so that it does not linger as a zombie.
pub fn terminate_tree(child: &mut Child) {
    let _ = child.kill();
    let _ = wait_for_exit(child, Instant::now() + Duration::from_secs(1));
}
