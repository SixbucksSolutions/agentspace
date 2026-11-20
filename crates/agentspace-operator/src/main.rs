//! `agentspace-operator`: the operator's program (docs/operator.md, Command line).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The program MUST exit 0 or 1 and nothing
//! else, and MUST NOT print a runtime's error text.

fn main() {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    // Unlocked: each write takes the stdout lock for itself. A lock held for the whole run would
    // block every other thread's write to stdout, and `deploy`'s passthrough copies the AWS CLI's
    // output on a thread of its own while this one waits for it (#0139).
    let mut stdout = std::io::stdout();
    let code = agentspace_operator::run(
        &args,
        &agentspace_operator::process_env,
        &mut stdout,
        Box::new(std::io::stderr()),
    );
    std::process::exit(code);
}
