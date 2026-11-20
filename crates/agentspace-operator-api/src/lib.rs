//! The pinned boundary of the operator's crates (docs/operator.md; D42).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The architect committed this crate before v0.06-T01 from the pin on cards #0111 to #0114, so
//! that the T01 stories implement and call it at the same time. A change to it is the
//! architect's. It holds no logic but the one conversion the pin gives.

#![forbid(unsafe_code)]

use std::path::PathBuf;

/// What the program reads from its environment once, as it starts, so that a library reads none
/// but the test seam's (operator.md, The test seam).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    /// `USERPROFILE` (docs/state.md, The home folder).
    pub home: PathBuf,
    /// `LOCALAPPDATA`, or `None` when it is unset or empty (operator.md, Environment variables).
    pub local_app_data: Option<PathBuf>,
    /// `SystemRoot`, from which every `%SystemRoot%` path is built (operator.md, Programs it
    /// starts, A-24).
    pub system_root: PathBuf,
    /// Whether `AGENTSPACE_NESTED` was set and not empty as the program started.
    pub nested: bool,
}

/// A person's command, or the ProxyCommand's caller mode (operator.md, What a caller sees).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Person,
    Caller,
}

/// The caller's three progress lines, in the order they print (operator.md, What a caller sees).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallerLine {
    /// `Starting the host...`
    Starting,
    /// `Waiting for the host to be ready...`
    Waiting,
    /// `The host is ready.`
    Ready,
}

/// How a command speaks. Everything it prints goes to stderr (operator.md, Streams).
pub trait Say {
    /// The mode the command runs in.
    fn mode(&self) -> Mode;
    /// A person's progress line; in caller mode, `caller`'s line instead, at most once per run,
    /// and nothing for `None`. `-Quiet` suppresses both.
    fn progress(&self, person: &str, caller: Option<CallerLine>);
    /// A warning, which starts `agentspace: `; in caller mode, rewritten as What a caller sees
    /// gives it (`the instance` to `the host`). `-Quiet` suppresses it.
    fn warning(&self, person: &str);
    /// Backend detail, printed only in caller mode with `AGENTSPACE_DIAGNOSTIC` set.
    fn detail(&self, text: &str);
}

/// Which of the caller's failure lines replaces a person's (operator.md, What a caller sees).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallerFailure {
    /// The person's message, unchanged: #0034's sign-in message, a usage error, an `-Alias`
    /// refusal.
    Same,
    /// `agentspace: The host is not available, and could not be started.`
    NotAvailable,
    /// `agentspace: The host was not ready within 15 minutes; try again later.`
    NotReady,
    /// `agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to
    /// see why.`
    CouldNotReach,
}

/// A failed command: the one line it prints, and the caller's line in its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The person's whole message, starting `agentspace: `.
    pub message: String,
    /// The caller's line that replaces it in caller mode.
    pub caller: CallerFailure,
}

/// What the operator does with an AWS CLI call's output (operator.md, Calls by command).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// `--output json`, parsed and returned.
    Json,
    /// Streamed as the CLI writes it, its error text still scanned for the sign-in test
    /// (`cloudformation deploy`).
    PassThrough,
    /// Not read, and its exit code not checked (`ec2 wait instance-stopped`).
    Ignore,
    /// `--output json`, the exit code checked like any call's and failing under AWS failures
    /// exactly as the others do, the answer never parsed: `ec2 start-instances`,
    /// `ec2 stop-instances` and `ec2 modify-volume` (operator.md, The AWS side, What is read from
    /// each answer: "not read beyond their exit code").
    Checked,
}

/// One AWS CLI call. The runner adds `--region us-west-2` and, for [`Output::Json`],
/// `--output json`, in gap FB-07's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Call<'a> {
    pub service: &'a str,
    pub operation: &'a str,
    /// The operation's own arguments, in the order Calls by command gives them.
    pub args: &'a [&'a str],
    pub output: Output,
}

/// Why an AWS CLI call failed (operator.md, AWS failures).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AwsError {
    /// The sign-in has expired or is missing (#0034).
    SignIn,
    /// Any other failure: the CLI's own error text, trimmed.
    Failed(String),
    /// The program could not be run: the whole `agentspace: ` message.
    CannotRun(String),
}

/// The AWS CLI, or the program `AGENTSPACE_AWS_CMD` names (operator.md, The test seam).
pub trait Aws {
    /// The parsed answer for [`Output::Json`], `Value::Null` otherwise.
    fn call(&self, call: &Call) -> Result<serde_json::Value, AwsError>;
}

/// ssh and scp: Windows OpenSSH by full path, or the programs the test seam names
/// (operator.md, Programs it starts).
pub trait Remote {
    /// One ssh with `options`, then the remote command as one argument, the last; its stdout and
    /// stderr the operator's. Returns its exit code.
    fn ssh(&self, options: &[&str], remote_command: Option<&str>) -> Result<i32, Failure>;
    /// One scp with `args`; its stdout and stderr the operator's. Returns its exit code.
    fn scp(&self, args: &[&str]) -> Result<i32, Failure>;
}

/// #0034's message, exactly (operator.md, AWS failures).
pub const SIGN_IN_MESSAGE: &str = "agentspace: AWS sign-in has expired. Run 'aws login --remote' and open its link in a fresh Incognito window, then retry.";

impl From<AwsError> for Failure {
    fn from(e: AwsError) -> Self {
        match e {
            AwsError::SignIn => Failure {
                message: SIGN_IN_MESSAGE.to_string(),
                caller: CallerFailure::Same,
            },
            AwsError::Failed(text) => Failure {
                message: format!("agentspace: {text}"),
                caller: CallerFailure::CouldNotReach,
            },
            AwsError::CannotRun(message) => Failure {
                message,
                caller: CallerFailure::CouldNotReach,
            },
        }
    }
}
