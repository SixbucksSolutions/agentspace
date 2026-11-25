//! `agentspace-operator`: the operator's program (docs/operator.md, Command line).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The program MUST exit 0 or 1 and nothing
//! else, and MUST NOT print a runtime's error text.

fn main() {
    // First, before anything is started or opened: a process at error mode 0 that starts a file
    // that is not a program waits for a person at a modal dialog (#0225, D81); its children inherit
    // this mode.
    agentspace_launch::quiet_hard_errors();
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    // A program that uses PassThrough MUST NOT hold its stdout or stderr lock across a command; the
    // passed-through output is written from other threads (architect, 2026-11-20, for
    // E0.06-T05-S02, #0139). So this stays unlocked, and each write takes the lock for itself:
    // `deploy` copies the AWS CLI's output on a thread of its own while this one waits for it.
    let mut stdout = std::io::stdout();
    let code = agentspace_operator::run(
        &args,
        &agentspace_operator::process_env,
        &mut stdout,
        Box::new(std::io::stderr()),
    );
    std::process::exit(code);
}
