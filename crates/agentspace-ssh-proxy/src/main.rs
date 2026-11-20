//! The ProxyCommand, `agentspace-ssh-proxy.exe` (docs/operator.md, The ProxyCommand; D42).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The program reads its environment once,
//! here, and writes nothing to stdout but the tunnel. It MUST NOT print a runtime's panic text:
//! a panic is the "Could not reach the host" line and exit 1.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn main() {
    // A panic prints nothing: its text is a runtime's, and may carry a path or backend detail.
    std::panic::set_hook(Box::new(|_| {}));
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let env = agentspace_ssh_proxy::Env::from_process();
    let code = catch_unwind(AssertUnwindSafe(|| {
        agentspace_ssh_proxy::run(&args, env, Box::new(std::io::stderr()))
    }))
    .unwrap_or_else(|_| {
        let _ = writeln!(
            std::io::stderr(),
            "{}",
            agentspace_ssh_proxy::COULD_NOT_REACH
        );
        1
    });
    std::process::exit(code);
}
