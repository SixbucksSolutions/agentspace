//! TEST-ONLY stand-in for an `ssh.exe` or an `agentspace-ssh-proxy.exe`. It MUST NOT be installed
//! or shipped.
//!
//! The release tests copy it, under the image name a case needs, into a throwaway folder, and start
//! the copy with the arguments of the command line the case stands for. The stub ignores its
//! arguments, never prints, and waits until it is ended. Everything else it does, it is told by
//! environment variables, which the test sets on the process it starts.
//!
//! - **`ASDRV_STUB_CHILDREN`** is how many children the stub starts. Child `i` (from 0) is the
//!   program in `ASDRV_STUB_CHILD<i>_EXE`, with the arguments in `ASDRV_STUB_CHILD<i>_ARGS`,
//!   separated by U+001F. A child gets no `ASDRV_STUB_` variable, so a copy of the stub only waits.
//! - **`ASDRV_STUB_PIDS`** names a file. The stub appends each child's process ID to it, as
//!   decimal digits and one line feed, in one write per child.
//! - **`ASDRV_STUB_WATCH`** names a path, and **`ASDRV_STUB_LOG`** a file. The stub polls whether
//!   the path exists as fast as it can, and writes `exists` or `missing` as a line to the file
//!   when it starts and each time the answer changes.
//! - **Every process exits by itself after 120 seconds** at most, so a test that fails cannot
//!   leave one running for long.

#![deny(unsafe_code)]

use std::env;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio, exit};
use std::thread::sleep;
use std::time::{Duration, Instant};

const LIFETIME: Duration = Duration::from_secs(120);

fn append(path: &Path, line: &str) -> bool {
    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(mut file) => file.write_all(line.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

fn main() {
    let started = Instant::now();
    let count: usize = env::var("ASDRV_STUB_CHILDREN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    // Held so each child's handle stays open as long as this process runs.
    let mut children: Vec<Child> = Vec::new();
    for i in 0..count {
        let Some(program) = env::var_os(format!("ASDRV_STUB_CHILD{i}_EXE")) else {
            exit(3);
        };
        let mut command = Command::new(program);
        if let Ok(args) = env::var(format!("ASDRV_STUB_CHILD{i}_ARGS")) {
            command.args(args.split('\u{1f}').filter(|a| !a.is_empty()));
        }
        for (name, _) in env::vars_os() {
            if name.to_string_lossy().starts_with("ASDRV_STUB_") {
                command.env_remove(name);
            }
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = match command.spawn() {
            Ok(c) => c,
            Err(_) => exit(4),
        };
        if let Some(path) = env::var_os("ASDRV_STUB_PIDS")
            && !append(&PathBuf::from(path), &format!("{}\n", child.id()))
        {
            exit(5);
        }
        children.push(child);
    }

    let watch = env::var_os("ASDRV_STUB_WATCH").map(PathBuf::from);
    let log = env::var_os("ASDRV_STUB_LOG").map(PathBuf::from);
    if let (Some(watch), Some(log)) = (watch, log) {
        let word = |exists: bool| if exists { "exists\n" } else { "missing\n" };
        let mut last = watch.exists();
        if !append(&log, word(last)) {
            exit(6);
        }
        while started.elapsed() < LIFETIME {
            let now = watch.exists();
            if now != last {
                last = now;
                if !append(&log, word(now)) {
                    exit(6);
                }
            }
            sleep(Duration::from_millis(5));
        }
        return;
    }

    while started.elapsed() < LIFETIME {
        sleep(Duration::from_millis(200));
    }
}
