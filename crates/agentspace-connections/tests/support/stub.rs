//! TEST-ONLY stand-in for an `ssh.exe` or an `agentspace-ssh-proxy.exe`. It MUST NOT be installed
//! or shipped.
//!
//! The tests copy it, under the image name a case needs, into a throwaway folder, and start the
//! copy with the arguments of the command line the case stands for. The stub ignores its
//! arguments. It never prints.
//!
//! - **A stub that is not marked a child** starts a chain of descendants, one child at each level:
//!   a child is a copy of the stub's own program, marked a child by `AGENTSPACE_CONN_STUB_CHILD`,
//!   or the program named by `AGENTSPACE_CONN_STUB_CHILD_EXE` for the first level only. Its
//!   arguments are the ones in `AGENTSPACE_CONN_STUB_CHILD_ARGS`, separated by U+001F.
//! - **`AGENTSPACE_CONN_STUB_DEPTH`** is how many levels of descendants the process starts below
//!   itself. It is 1 for a stub that is not marked a child and has none set, and 0 for a child
//!   that has none set. A child gets its parent's depth less one.
//! - **`AGENTSPACE_CONN_STUB_PIDS`** names a file. Each process that starts a child appends that
//!   child's process ID to it, as decimal digits and one line feed, in one write.
//! - **Every process exits by itself after 120 seconds** at most, so a test that fails cannot
//!   leave one running for long.

#![deny(unsafe_code)]

use std::env;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::process::{Command, Stdio, exit};
use std::thread::sleep;
use std::time::{Duration, Instant};

const CHILD: &str = "AGENTSPACE_CONN_STUB_CHILD";
const DEPTH: &str = "AGENTSPACE_CONN_STUB_DEPTH";
const PIDS: &str = "AGENTSPACE_CONN_STUB_PIDS";
const CHILD_EXE: &str = "AGENTSPACE_CONN_STUB_CHILD_EXE";
const CHILD_ARGS: &str = "AGENTSPACE_CONN_STUB_CHILD_ARGS";

const LIFETIME: Duration = Duration::from_secs(120);

fn main() {
    let started = Instant::now();
    let is_child = env::var_os(CHILD).is_some();
    let depth: u32 = env::var(DEPTH)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(if is_child { 0 } else { 1 });

    // Held so the child's handle stays open as long as this process runs.
    let _child = if depth > 0 {
        let program = match env::var_os(CHILD_EXE) {
            Some(p) => p.into(),
            None => match env::current_exe() {
                Ok(p) => p,
                Err(_) => exit(3),
            },
        };
        let mut command = Command::new(program);
        if let Ok(args) = env::var(CHILD_ARGS) {
            command.args(args.split('\u{1f}').filter(|a| !a.is_empty()));
        }
        command
            .env(CHILD, "1")
            .env(DEPTH, (depth - 1).to_string())
            .env_remove(CHILD_EXE)
            .env_remove(CHILD_ARGS)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = match command.spawn() {
            Ok(c) => c,
            Err(_) => exit(4),
        };
        if let Some(path) = env::var_os(PIDS) {
            let line = format!("{}\n", child.id());
            match OpenOptions::new().create(true).append(true).open(path) {
                Ok(mut file) => {
                    if file.write_all(line.as_bytes()).is_err() {
                        exit(5);
                    }
                }
                Err(_) => exit(5),
            }
        }
        Some(child)
    } else {
        None
    };

    while started.elapsed() < LIFETIME {
        sleep(Duration::from_millis(200));
    }
}
