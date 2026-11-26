//! A proxy in the environment changes nothing (docs/lease-api.md, "The board read": the
//! connection is made to 127.0.0.1, directly, never through a proxy, whatever `HTTP_PROXY`,
//! `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY` say, in any case).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The test never calls `set_var`: it
//! re-runs this test executable as a child with the variables given through `Command::env`, and
//! the child does the read (`proxy_child_read`, which does nothing unless the marker variable is
//! set) and prints one line, `RESULT ...`, which carries a variant and a message, never a token.
//! The child's home, `LOCALAPPDATA`, `APPDATA`, `TMP` and `TEMP` are the test's throwaway ones.
//! It is the test executable, not a toolchain program, that runs under the moved home.

mod common;

use agentspace_board::{BoardError, read_epic};
use agentspace_push::StdFiles;
use common::*;
use std::io::Read;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const MARKER: &str = "AGENTSPACE_BOARD_TEST_CHILD_ROOT";

/// The child's half. It does the read in a throwaway home the parent made, and says what came
/// back. Without the marker variable it is a test that passes at once.
#[test]
fn proxy_child_read() {
    let Some(root) = std::env::var_os(MARKER) else {
        return;
    };
    let ctx = ctx_for(Path::new(&root));
    let result = read_epic(&ctx, &StdFiles, &FakeDpapi, BOARD, 52);
    match result {
        Ok(epic) => println!("RESULT ok revision={} archived={}", epic.revision, epic.archived),
        Err(BoardError::UnknownBoard) => println!("RESULT UnknownBoard"),
        Err(BoardError::UnknownTicket) => println!("RESULT UnknownTicket"),
        Err(BoardError::TooLarge) => println!("RESULT TooLarge"),
        Err(BoardError::Internal(msg)) => println!("RESULT Internal {msg}"),
    }
}

/// A listener the test owns, standing for a proxy: it counts the connections made to it.
struct Trap {
    port: u16,
    count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Trap {
    fn start() -> Trap {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert_ne!(port, 8811);
        listener.set_nonblocking(true).unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (count, stop) = (Arc::clone(&count), Arc::clone(&stop));
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((conn, _)) => {
                            count.fetch_add(1, Ordering::Relaxed);
                            drop(conn);
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(5)),
                    }
                }
            })
        };
        Trap {
            port,
            count,
            stop,
            thread: Some(thread),
        }
    }

    fn connections(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }
}

impl Drop for Trap {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Runs the child with `proxy_url` in every proxy variable, and returns its `RESULT` line.
fn run_child(world: &World, vars: &[(&str, &str)], remove: &[&str]) -> String {
    let exe = std::env::current_exe().unwrap();
    let local = world.local();
    let tmp = world.root().join("tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    let mut cmd = Command::new(exe);
    cmd.args(["--exact", "proxy_child_read", "--nocapture", "--test-threads=1"])
        .env(MARKER, world.root())
        .env("USERPROFILE", world.root().join("home"))
        .env("HOME", world.root().join("home"))
        .env("LOCALAPPDATA", &local)
        .env("APPDATA", world.root().join("AppData").join("Roaming"))
        .env("TMP", &tmp)
        .env("TEMP", &tmp)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // The caller's own variables of localswim's are removed by name, never read.
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().to_ascii_uppercase().starts_with("LOCALSWIM_") {
            cmd.env_remove(name);
        }
    }
    for name in remove {
        cmd.env_remove(name);
    }
    for (name, value) in vars {
        cmd.env(name, value);
    }
    let mut child = cmd.spawn().expect("the test executable starts");
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        match child.try_wait().unwrap() {
            Some(_) => break,
            None if Instant::now() > deadline => {
                // The child is the test's own process, ended by its handle.
                let _ = child.kill();
                let _ = child.wait();
                panic!("the child read did not end within 90 seconds");
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    // The child's output is checked for the token before anything in it is quoted.
    no_token(world.standin.token(), &out);
    // The libtest line `test proxy_child_read ... ` is written with no line end, so the child's
    // line follows it on the same line.
    out.lines()
        .find_map(|l| l.find("RESULT ").map(|at| l[at..].to_string()))
        .unwrap_or_else(|| panic!("the child printed no RESULT line"))
}

#[test]
fn proxies_in_the_environment_change_nothing() {
    let trap = Trap::start();
    let world = World::new(Behavior::standard());
    let proxy = format!("http://127.0.0.1:{}", trap.port);
    let mut vars: Vec<(&str, &str)> = Vec::new();
    for name in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        vars.push((name, proxy.as_str()));
    }
    // NO_PROXY is removed, so a client that honors proxies has no way round the trap.
    let line = run_child(&world, &vars, &["NO_PROXY", "no_proxy"]);
    assert_eq!(line, "RESULT ok revision=7 archived=false", "the read through the environment");
    assert_eq!(trap.connections(), 0, "the read connected to the proxy");
    let reqs = world.requests();
    assert_eq!(reqs.len(), 1, "the read reached the stand-in directly");
    assert_ticket_view(&reqs[0], BOARD, 52);
}

#[test]
fn a_no_proxy_that_names_nothing_useful_changes_nothing_either() {
    // Every proxy variable set, NO_PROXY set to a host that is not the board's: a client that
    // honors proxies would use the proxy for 127.0.0.1.
    let trap = Trap::start();
    let world = World::new(Behavior::standard());
    let proxy = format!("http://127.0.0.1:{}", trap.port);
    let vars = [
        ("HTTP_PROXY", proxy.as_str()),
        ("HTTPS_PROXY", proxy.as_str()),
        ("ALL_PROXY", proxy.as_str()),
        ("NO_PROXY", "example.invalid"),
    ];
    let line = run_child(&world, &vars, &[]);
    assert_eq!(line, "RESULT ok revision=7 archived=false");
    assert_eq!(trap.connections(), 0);
    assert_eq!(world.requests().len(), 1);
}

#[test]
fn a_proxy_variable_naming_a_dead_port_does_not_break_the_read() {
    let world = World::new(Behavior::standard());
    let dead = format!("http://127.0.0.1:{}", dead_port());
    let vars = [("HTTP_PROXY", dead.as_str()), ("ALL_PROXY", dead.as_str())];
    let line = run_child(&world, &vars, &["NO_PROXY", "no_proxy"]);
    assert_eq!(line, "RESULT ok revision=7 archived=false");
    assert_eq!(world.requests().len(), 1);
}
