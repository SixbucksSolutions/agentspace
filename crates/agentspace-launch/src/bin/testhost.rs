//! `agentspace-launch-testhost`: test-only, never installed. The keeper's stand-in for this
//! crate's tests (D42: the keeper's own program is the launcher):
//! - `-Launch <request>` runs `agentspace_launch::run_launcher` and exits with its code, as the
//!   keeper's `main` does.
//! - `--sleep <secs>` sleeps that many whole seconds and exits 0: a stub holder.
//!
//! Anything else exits 2.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::time::Duration;

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if let Some(request) = agentspace_launch::launcher_request(&args) {
        std::process::exit(agentspace_launch::run_launcher(&request));
    }
    if let [flag, secs] = &args[..]
        && flag == "--sleep"
        && let Some(secs) = secs.to_str().and_then(|s| s.parse::<u64>().ok())
    {
        std::thread::sleep(Duration::from_secs(secs));
        std::process::exit(0);
    }
    std::process::exit(2);
}
