//! `agentspace-operator`: the operator's program (docs/operator.md, Command line).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The program MUST exit 0 or 1 and nothing
//! else, and MUST NOT print a runtime's error text.

fn main() {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let mut stdout = std::io::stdout().lock();
    let code = agentspace_operator::run(
        &args,
        &agentspace_operator::process_env,
        &mut stdout,
        Box::new(std::io::stderr()),
    );
    std::process::exit(code);
}
