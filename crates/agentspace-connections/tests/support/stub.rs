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
//! - **`AGENTSPACE_CONN_STUB_CHAIN`**, after the levels above, makes the process start one more
//!   child: a program named by the first of the records in it, separated by U+001E. A record is
//!   its fields separated by U+001F: the child's label, `1` or `0` for whether the child exits as
//!   soon as it has started what it starts, the depth of plain descendants the child starts below
//!   itself, the child's program, and its arguments. The child gets the other records as its own
//!   `AGENTSPACE_CONN_STUB_CHAIN`, so the records start a chain of programs, each below the one
//!   before, and a program that exits leaves the one below it with a parent that has gone while
//!   the one above still holds it.
//! - **`AGENTSPACE_CONN_STUB_LABEL` and `AGENTSPACE_CONN_STUB_MARKS`**: a process that has a label
//!   writes its own process ID, as decimal digits, to `<marks folder>\<label>.pid` before it
//!   starts anything, so that a test can tell which process is which.
//! - **`AGENTSPACE_CONN_STUB_EXIT`** is `1` for a process that exits at once, after it has started
//!   what it starts and written their process IDs.
//! - **Every process exits by itself after 120 seconds** at most, so a test that fails cannot
//!   leave one running for long.

#![deny(unsafe_code)]

use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio, exit};
use std::thread::sleep;
use std::time::{Duration, Instant};

const CHILD: &str = "AGENTSPACE_CONN_STUB_CHILD";
const DEPTH: &str = "AGENTSPACE_CONN_STUB_DEPTH";
const PIDS: &str = "AGENTSPACE_CONN_STUB_PIDS";
const CHILD_EXE: &str = "AGENTSPACE_CONN_STUB_CHILD_EXE";
const CHILD_ARGS: &str = "AGENTSPACE_CONN_STUB_CHILD_ARGS";
const CHAIN: &str = "AGENTSPACE_CONN_STUB_CHAIN";
const LABEL: &str = "AGENTSPACE_CONN_STUB_LABEL";
const MARKS: &str = "AGENTSPACE_CONN_STUB_MARKS";
const EXIT: &str = "AGENTSPACE_CONN_STUB_EXIT";

const UNIT: char = '\u{1f}';
const RECORD: char = '\u{1e}';

const LIFETIME: Duration = Duration::from_secs(120);

/// Appends a started child's process ID to the PIDS file, when there is one.
fn note(child: &Child) {
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
}

fn quiet(command: &mut Command) -> &mut Command {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
}

fn main() {
    let started = Instant::now();
    let is_child = env::var_os(CHILD).is_some();
    let depth: u32 = env::var(DEPTH)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(if is_child { 0 } else { 1 });
    let exits = env::var(EXIT).is_ok_and(|v| v == "1");

    // The process's own mark, before it starts anything.
    if let Some(dir) = env::var_os(MARKS)
        && let Ok(label) = env::var(LABEL)
        && !label.is_empty()
    {
        let path = PathBuf::from(dir).join(format!("{label}.pid"));
        if fs::write(path, std::process::id().to_string()).is_err() {
            exit(6);
        }
    }

    // Held so each child's handle stays open as long as this process runs.
    let mut children: Vec<Child> = Vec::new();

    if depth > 0 {
        let program = match env::var_os(CHILD_EXE) {
            Some(p) => p.into(),
            None => match env::current_exe() {
                Ok(p) => p,
                Err(_) => exit(3),
            },
        };
        let mut command = Command::new(program);
        if let Ok(args) = env::var(CHILD_ARGS) {
            command.args(args.split(UNIT).filter(|a| !a.is_empty()));
        }
        command
            .env(CHILD, "1")
            .env(DEPTH, (depth - 1).to_string())
            .env_remove(CHILD_EXE)
            .env_remove(CHILD_ARGS)
            .env_remove(CHAIN)
            .env_remove(LABEL)
            .env_remove(EXIT);
        let child = match quiet(&mut command).spawn() {
            Ok(c) => c,
            Err(_) => exit(4),
        };
        note(&child);
        children.push(child);
    }

    if let Ok(chain) = env::var(CHAIN) {
        let mut records = chain.split(RECORD).filter(|r| !r.is_empty());
        if let Some(first) = records.next() {
            let rest: Vec<&str> = records.collect();
            let mut fields = first.split(UNIT);
            let label = fields.next().unwrap_or("");
            let child_exits = fields.next().unwrap_or("0");
            let child_depth = fields.next().unwrap_or("0");
            let Some(program) = fields.next() else {
                exit(7);
            };
            let mut command = Command::new(program);
            command.args(fields.filter(|a| !a.is_empty()));
            command
                .env(CHILD, "1")
                .env(DEPTH, child_depth)
                .env(EXIT, child_exits)
                .env(LABEL, label)
                .env_remove(CHILD_EXE)
                .env_remove(CHILD_ARGS);
            if rest.is_empty() {
                command.env_remove(CHAIN);
            } else {
                command.env(CHAIN, rest.join(&RECORD.to_string()));
            }
            let child = match quiet(&mut command).spawn() {
                Ok(c) => c,
                Err(_) => exit(4),
            };
            note(&child);
            children.push(child);
        }
    }

    if exits {
        return;
    }

    while started.elapsed() < LIFETIME {
        sleep(Duration::from_millis(200));
    }
}
