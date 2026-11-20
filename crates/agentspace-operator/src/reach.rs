//! The program's environment, read once as it starts, and what the commands reach beyond the AWS
//! CLI, ssh and scp (docs/operator.md, Environment variables; The test seam).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::Arc;

use agentspace_ca::CaEnv;
use agentspace_operator_api::{Aws, Context, Failure, Remote, Say};

use crate::Reach;

/// Every variable the program reads, by name. The context's, `ca`'s, the seam's four, `PATH`
/// (the AWS CLI's fallback), and `AGENTSPACE_DIAGNOSTIC`.
const VARIABLES: [&str; 14] = [
    "USERPROFILE",
    "LOCALAPPDATA",
    "SystemRoot",
    "AGENTSPACE_NESTED",
    "AGENTSPACE_DIAGNOSTIC",
    "USERNAME",
    "COMPUTERNAME",
    "OneDrive",
    "OneDriveConsumer",
    "OneDriveCommercial",
    "AGENTSPACE_AWS_CMD",
    "AGENTSPACE_SSH_CMD",
    "AGENTSPACE_SCP_CMD",
    "AGENTSPACE_TIMEZONE",
];

/// The program's environment as it started: the variables it reads, copied once from a lookup
/// (the process's own, in `main`). A library reads none, so the seam's variables reach the
/// programs the operator starts from here, and a test supplies its own.
#[derive(Clone)]
pub struct Env(Arc<HashMap<String, OsString>>);

/// Names the variables it holds, never their values: a captured environment MUST NOT be printed
/// (AGENTS.md, Terry's real files in a review).
impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut names: Vec<&str> = self.0.keys().map(String::as_str).collect();
        names.sort_unstable();
        f.debug_tuple("Env").field(&names).finish()
    }
}

impl Env {
    /// Copies the variables the program reads from `lookup`, `PATH` included.
    pub fn snapshot(lookup: &dyn Fn(&str) -> Option<OsString>) -> Env {
        let mut map = HashMap::new();
        for name in VARIABLES.iter().chain(&["PATH"]) {
            if let Some(value) = lookup(name) {
                map.insert((*name).to_string(), value);
            }
        }
        Env(Arc::new(map))
    }

    /// One variable's value, as the lookup gave it.
    pub fn get(&self, name: &str) -> Option<OsString> {
        self.0.get(name).cloned()
    }

    /// A lookup over this environment, for the libraries that take one.
    pub fn lookup(&self) -> impl Fn(&str) -> Option<OsString> + Send + Sync + 'static {
        let map = Arc::clone(&self.0);
        move |name| map.get(name).cloned()
    }

    /// `ca`'s input: `USERNAME`, `COMPUTERNAME` and the three OneDrive folders.
    pub fn ca_env(&self) -> CaEnv {
        CaEnv::from_lookup(|name| self.get(name))
    }
}

/// The real [`Reach`]: the token push, `ca`, `sync` and `deploy`.
pub struct SystemReach {
    env: Env,
}

impl SystemReach {
    pub fn new(env: Env) -> Self {
        SystemReach { env }
    }
}

impl Reach for SystemReach {
    fn push(&self, ctx: &Context, say: &dyn Say, instance_id: &str, alias: &str) {
        // The push never fails a command, and runs unforced (operator.md, The token push).
        let _ = agentspace_push::push_using(
            ctx,
            say,
            instance_id,
            alias,
            false,
            Arc::new(self.env.lookup()),
        );
    }

    fn ca(&self, ctx: &Context, env: &CaEnv, say: &dyn Say) -> Result<(), Failure> {
        agentspace_ca::ca(ctx, env, say)
    }

    fn sync(
        &self,
        ctx: &Context,
        env: &CaEnv,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure> {
        agentspace_sync::sync(ctx, env, say, aws, remote)
    }

    fn deploy(
        &self,
        ctx: &Context,
        env: &CaEnv,
        say: &dyn Say,
        aws: &dyn Aws,
        remote: &dyn Remote,
    ) -> Result<(), Failure> {
        agentspace_sync::deploy(ctx, env, say, aws, remote)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(name: &str) -> Option<OsString> {
        match name {
            "USERNAME" => Some("someone".into()),
            "OneDrive" => Some("C:\\somewhere".into()),
            "AGENTSPACE_SSH_CMD" => Some("ssh-stand-in".into()),
            "PATH" => Some("a;b".into()),
            "UNRELATED" => Some("x".into()),
            _ => None,
        }
    }

    #[test]
    fn the_snapshot_holds_the_variables_the_program_reads_and_no_others() {
        let env = Env::snapshot(&lookup);
        assert_eq!(env.get("USERNAME"), Some("someone".into()));
        assert_eq!(env.get("AGENTSPACE_SSH_CMD"), Some("ssh-stand-in".into()));
        assert_eq!(env.get("PATH"), Some("a;b".into()));
        assert_eq!(env.get("UNRELATED"), None);
        assert_eq!(env.get("COMPUTERNAME"), None);
    }

    #[test]
    fn ca_env_is_filled_from_the_snapshot() {
        let ca = Env::snapshot(&lookup).ca_env();
        assert_eq!(ca.user_name.as_deref(), Some("someone"));
        assert_eq!(ca.computer_name, None);
        assert_eq!(ca.one_drive, Some("C:\\somewhere".into()));
        assert_eq!(ca.one_drive_consumer, None);
    }

    #[test]
    fn the_snapshot_formats_its_names_and_never_a_value() {
        let shown = format!("{:?}", Env::snapshot(&lookup));
        assert!(shown.contains("USERNAME"), "The names are shown");
        for value in ["someone", "somewhere", "ssh-stand-in", "a;b"] {
            assert!(!shown.contains(value), "A value was formatted");
        }
    }

    #[test]
    fn the_lookup_answers_from_the_snapshot_alone() {
        let env = Env::snapshot(&lookup);
        let look = env.lookup();
        assert_eq!(look("USERNAME"), Some("someone".into()));
        assert_eq!(look("UNRELATED"), None);
    }
}
