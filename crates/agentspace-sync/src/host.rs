//! How the operator reaches a host that is not the box (docs/operator.md, How the operator reaches a
//! host; docs/state.md, A host's folder, The operator's key and certificate, A host's operator
//! alias; D47, D55).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **The box is reached through the user's own `agentspace` entry**, and nothing here runs for it:
//!   no key, certificate or alias is written for the box.
//! - **[`prepare`] runs once per command for a host that is not the box, after the CA check and
//!   before the command's first ssh or scp to it**, in this order: the operator's key for the host,
//!   made when `id_ed25519` is missing and while `hosts.lock` is held; a fresh one-hour certificate
//!   signed with the CA; the host's operator alias, written only when missing or different, after
//!   the `Include` line.
//! - **An existing key is never replaced**, and a file at the alias's path that is not this host's
//!   alias is left as it is and fails the command.
//! - **The private key is never printed, logged or put in a message.** A message names a path, and
//!   no program gets more of the key than its path.
//! - **The alias's ProxyCommand names `agentspace-ssh-proxy.exe` in the operator's own folder**,
//!   and a program that is not there fails the command before any ssh starts.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use agentspace_alias::{
    AliasFailureKind, HostAliasFile, default_proxy_command, ensure_include, host_alias_file_bytes,
    host_alias_name, host_file_text, known_hosts_text, write_host_alias,
};
use agentspace_credential::{ensure_operator_key, sign_operator_certificate};
use agentspace_hosts::host_key_path;
use agentspace_operator_api::{CallerFailure, Context, Failure};

use crate::Seams;
use crate::files::{Places, cannot_write};

/// The program the alias's ProxyCommand runs, found in the operator's own folder.
const PROXY_PROGRAM: &str = "agentspace-ssh-proxy.exe";

/// What the alias file says about itself, after its owner mark.
const COMMENT: &str = "written by agentspace; it MUST NOT be edited";

fn failure(message: String) -> Failure {
    Failure {
        message,
        caller: CallerFailure::CouldNotReach,
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The ssh folder: `AGENTSPACE_SSH_DIR` when the seams carry one, else `%USERPROFILE%\.ssh`.
fn ssh_dir(places: &Places, seams: &Seams) -> PathBuf {
    match &seams.ssh_dir {
        Some(dir) => dir.clone(),
        None => places.home().join(".ssh"),
    }
}

/// Makes the operator's key, certificate and alias for `project`'s host (operator.md, How the
/// operator reaches a host, steps 2 to 4). The CA check, step 1, is the caller's.
pub(crate) fn prepare(
    ctx: &Context,
    places: &Places,
    project: &str,
    seams: &Seams,
) -> Result<(), Failure> {
    let home = places.home();
    let key_path = host_key_path(home, project);

    // Step 2: the key. It is looked for without the lock, so that a command that writes no registry
    // never waits on a provisioning run's `hosts.lock` (architect, 2026-11-21, for v0.07-T03, on
    // #0152's review). Only when it is missing is `hosts.lock` taken, the key looked for again under
    // it, and `ensure_operator_key` called while it is held, so that two commands never make two
    // keys. The lock is never held across anything else.
    if !key_path.is_file() {
        let _held = crate::registry::lock(home)?;
        ensure_operator_key(home, project).map_err(|e| {
            failure(format!(
                "agentspace: Could not make the operator's key {}: {}",
                key_path.display(),
                e.message
            ))
        })?;
    }

    // Step 3: a fresh certificate, replacing the last in one rename.
    sign_operator_certificate(home, project, unix_now()).map_err(|e| {
        failure(format!(
            "agentspace: Could not sign the operator's certificate for '{project}': {}",
            e.message
        ))
    })?;

    // Step 4: the alias, after the program it names is known to be there.
    write_alias(ctx, places, project, seams)
}

fn write_alias(
    ctx: &Context,
    places: &Places,
    project: &str,
    seams: &Seams,
) -> Result<(), Failure> {
    let home = places.home();
    let proxy_dir = seams.operator_dir.clone().unwrap_or_default();
    let proxy = proxy_dir.join(PROXY_PROGRAM);
    if !proxy.is_file() {
        return Err(failure(format!(
            "agentspace: {} is missing; the operator's alias for a host needs it beside the operator.",
            proxy.display()
        )));
    }
    let alias = host_alias_name(project);
    let command = default_proxy_command(
        &proxy.display().to_string(),
        &alias,
        Some(&ctx.system_root.display().to_string()),
    )
    .map_err(|e| failure(format!("agentspace: {e}.")))?;
    let home_text = home.display().to_string();
    let key = host_file_text(&home_text, project, "id_ed25519");
    let cert = host_file_text(&home_text, project, "id_ed25519-cert.pub");
    let known_hosts = known_hosts_text(&home_text);
    let bytes = host_alias_file_bytes(&HostAliasFile {
        project,
        comment: COMMENT,
        key_path: &key,
        cert_path: &cert,
        known_hosts: &known_hosts,
        proxy_command: &command,
    })
    .map_err(|e| failure(format!("agentspace: {e}.")))?;

    let ssh = ssh_dir(places, seams);
    let alias_path = ssh.join("agentspace.d").join(&alias);
    // An `Include` line that cannot be made is a configuration a person has to fix when its kind is
    // `Config`, and a file that cannot be written otherwise (operator.md, How the operator reaches
    // a host, step 4; architect, 2026-11-21, for v0.07-T03, on #0152's review).
    let config = ssh.join("config");
    ensure_include(&ssh, unix_now()).map_err(|e| match e.kind {
        AliasFailureKind::Config => failure(format!(
            "agentspace: {} cannot take agentspace's Include line: {}.",
            config.display(),
            e.message
        )),
        _ => cannot_write(&config),
    })?;
    write_host_alias(&ssh, project, &bytes).map_err(|e| match e.kind {
        AliasFailureKind::Foreign => failure(format!(
            "agentspace: {} is not agentspace's alias for '{project}'; it was left as it is.",
            alias_path.display()
        )),
        _ => cannot_write(&alias_path),
    })
}

#[allow(clippy::mixed_attributes_style)]
#[cfg(test)]
mod tests {
    // The tests start the machine's own `ssh-keygen.exe` against a throwaway CA.
    #![cfg(windows)]
    use super::*;
    use crate::tests::{Scratch, ctx_in};
    use agentspace_box::SystemClock;
    use agentspace_ca::CaEnv;
    use agentspace_operator_api::{CallerLine, Mode, Say};
    use std::time::{Duration, Instant};

    struct Quiet;

    impl Say for Quiet {
        fn mode(&self) -> Mode {
            Mode::Person
        }
        fn progress(&self, _: &str, _: Option<CallerLine>) {}
        fn warning(&self, _: &str) {}
        fn detail(&self, _: &str) {}
    }

    /// A context in a throwaway home, with the machine's own system folder so that `ca` finds
    /// `ssh-keygen.exe`, and a CA made there. `None` when the machine has no system folder.
    fn lab(dir: &Scratch) -> Option<Context> {
        let root = std::env::var_os("SystemRoot")?;
        let mut ctx = ctx_in(dir.path());
        ctx.system_root = PathBuf::from(root);
        std::fs::create_dir_all(&ctx.home).ok()?;
        // A machine that has the system folder has `ssh-keygen.exe`, so a CA that cannot be made is a
        // failure of the test, never a skip.
        if let Err(failure) = agentspace_ca::ca(&ctx, &CaEnv::default(), &Quiet) {
            panic!(
                "{}",
                failure
                    .message
                    .replace(&ctx.home.display().to_string(), "<home>")
            );
        }
        Some(ctx)
    }

    fn with_seams<T>(dir: &Scratch, proxy: bool, run: impl FnOnce(&Seams) -> T) -> T {
        let operator_dir = dir.path().join("operator-program");
        std::fs::create_dir_all(&operator_dir).unwrap();
        if proxy {
            std::fs::write(operator_dir.join(PROXY_PROGRAM), "x").unwrap();
        }
        let clock = SystemClock::new();
        let find_go = || None;
        let ca_check = |_: &Context, _: &CaEnv| Ok(String::new());
        let push = |_: &Context, _: &dyn Say, _: &str, _: &str, _: Option<&str>| {};
        run(&Seams {
            clock: &clock,
            ca_check: &ca_check,
            find_go: &find_go,
            localswim_checkout: dir.path().join("localswim"),
            push: &push,
            folder: crate::folder::CommandFolder::new(),
            ssh_dir: Some(dir.path().join("ssh")),
            operator_dir: Some(operator_dir),
        })
    }

    #[test]
    fn a_host_gets_its_key_a_fresh_certificate_and_its_alias_and_a_second_run_changes_no_alias() {
        let dir = Scratch::new("host-prepare");
        let Some(ctx) = lab(&dir) else { return };
        let places = Places::new(&ctx);
        let host_dir = agentspace_hosts::host_dir(places.home(), "demo");
        let alias_path = dir
            .path()
            .join("ssh")
            .join("agentspace.d")
            .join("agentspace-host.demo");
        with_seams(&dir, true, |seams| {
            prepare(&ctx, &places, "demo", seams).unwrap()
        });
        assert!(host_dir.join("id_ed25519").is_file());
        assert!(host_dir.join("id_ed25519-cert.pub").is_file());
        let alias = std::fs::read_to_string(&alias_path).unwrap();
        let lines: Vec<&str> = alias.lines().collect();
        assert_eq!(lines[0], format!("# agentspace host demo; {COMMENT}"));
        assert_eq!(lines[1], "Host agentspace-host.demo");
        assert!(alias.contains("  User ubuntu\n"));
        assert!(alias.contains("  HostKeyAlias agentspace-host.demo\n"));
        assert!(alias.contains("  StrictHostKeyChecking accept-new\n"));
        assert!(!alias.contains("RemoteForward"));
        assert!(
            alias.contains("agentspace-ssh-proxy.exe\" agentspace-host.demo\n"),
            "{alias}"
        );
        let config = std::fs::read_to_string(dir.path().join("ssh").join("config")).unwrap();
        assert!(config.contains("Include agentspace.d/*"));
        let key = std::fs::read(host_dir.join("id_ed25519")).unwrap();

        // A second command keeps the key and the alias, and signs again.
        with_seams(&dir, true, |seams| {
            prepare(&ctx, &places, "demo", seams).unwrap()
        });
        assert_eq!(std::fs::read(host_dir.join("id_ed25519")).unwrap(), key);
        assert_eq!(std::fs::read_to_string(&alias_path).unwrap(), alias);
        assert!(host_dir.join("id_ed25519-cert.pub").is_file());
    }

    #[test]
    fn a_proxy_program_that_is_missing_fails_before_the_alias_is_written() {
        let dir = Scratch::new("host-no-proxy");
        let Some(ctx) = lab(&dir) else { return };
        let places = Places::new(&ctx);
        let failure =
            with_seams(&dir, false, |seams| prepare(&ctx, &places, "demo", seams)).unwrap_err();
        let want = dir.path().join("operator-program").join(PROXY_PROGRAM);
        assert_eq!(
            failure.message,
            format!(
                "agentspace: {} is missing; the operator's alias for a host needs it beside the operator.",
                want.display()
            )
        );
        assert!(!dir.path().join("ssh").join("agentspace.d").exists());
    }

    #[test]
    fn an_include_line_below_a_host_block_fails_with_the_config_message_and_its_why() {
        let dir = Scratch::new("host-include-config");
        let Some(ctx) = lab(&dir) else { return };
        let places = Places::new(&ctx);
        let ssh = dir.path().join("ssh");
        std::fs::create_dir_all(&ssh).unwrap();
        let config = ssh.join("config");
        std::fs::write(
            &config,
            "Host other\n  User someone\nInclude agentspace.d/*\n",
        )
        .unwrap();
        let failure =
            with_seams(&dir, true, |seams| prepare(&ctx, &places, "demo", seams)).unwrap_err();
        let prefix = format!(
            "agentspace: {} cannot take agentspace's Include line: ",
            config.display()
        );
        assert!(failure.message.starts_with(&prefix), "{}", failure.message);
        assert!(failure.message.ends_with('.'));
        assert!(
            failure.message.len() > prefix.len() + 1,
            "The reason MUST be there"
        );
        assert!(
            !ssh.join("agentspace.d")
                .join("agentspace-host.demo")
                .exists()
        );
    }

    #[test]
    fn an_include_line_that_cannot_be_written_for_io_fails_with_cannot_write_the_config() {
        let dir = Scratch::new("host-include-io");
        let Some(ctx) = lab(&dir) else { return };
        let places = Places::new(&ctx);
        let ssh = dir.path().join("ssh");
        std::fs::create_dir_all(&ssh).unwrap();
        // A file where the aliases' folder belongs is an Io failure of agentspace-alias.
        std::fs::write(ssh.join("agentspace.d"), "x").unwrap();
        let failure =
            with_seams(&dir, true, |seams| prepare(&ctx, &places, "demo", seams)).unwrap_err();
        assert_eq!(
            failure.message,
            format!("agentspace: Cannot write {}.", ssh.join("config").display())
        );
    }

    /// Holds `hosts.lock` for `hold`, in a thread that has taken it before this returns.
    fn hold_lock(home: &std::path::Path, hold: Duration) -> std::thread::JoinHandle<()> {
        let held = agentspace_hosts::lock(home).unwrap();
        std::thread::spawn(move || {
            std::thread::sleep(hold);
            drop(held);
        })
    }

    #[test]
    fn a_key_that_exists_is_found_without_waiting_for_a_held_hosts_lock() {
        let dir = Scratch::new("host-key-no-lock");
        let Some(ctx) = lab(&dir) else { return };
        let places = Places::new(&ctx);
        with_seams(&dir, true, |seams| {
            prepare(&ctx, &places, "demo", seams).unwrap()
        });
        let holder = hold_lock(places.home(), Duration::from_secs(8));
        let start = Instant::now();
        let result = with_seams(&dir, true, |seams| prepare(&ctx, &places, "demo", seams));
        let took = start.elapsed();
        holder.join().unwrap();
        if let Err(failure) = &result {
            panic!(
                "{}",
                failure
                    .message
                    .replace(&dir.path().display().to_string(), "<dir>")
            );
        }
        assert!(
            took < Duration::from_secs(6),
            "A command whose key exists MUST NOT wait for hosts.lock"
        );
    }

    #[test]
    fn a_key_that_is_missing_waits_for_the_hosts_lock_and_then_takes_it() {
        let dir = Scratch::new("host-key-lock");
        let Some(ctx) = lab(&dir) else { return };
        let places = Places::new(&ctx);
        let holder = hold_lock(places.home(), Duration::from_secs(2));
        let start = Instant::now();
        let result = with_seams(&dir, true, |seams| prepare(&ctx, &places, "demo", seams));
        let took = start.elapsed();
        holder.join().unwrap();
        if let Err(failure) = &result {
            panic!(
                "{}",
                failure
                    .message
                    .replace(&dir.path().display().to_string(), "<dir>")
            );
        }
        assert!(
            took >= Duration::from_millis(1500),
            "A command with no key MUST wait for hosts.lock"
        );
        assert!(agentspace_hosts::host_key_path(places.home(), "demo").is_file());
    }

    #[test]
    fn a_file_that_is_not_this_hosts_alias_is_left_and_fails_the_command() {
        let dir = Scratch::new("host-foreign");
        let Some(ctx) = lab(&dir) else { return };
        let places = Places::new(&ctx);
        let folder = dir.path().join("ssh").join("agentspace.d");
        std::fs::create_dir_all(&folder).unwrap();
        let alias_path = folder.join("agentspace-host.demo");
        std::fs::write(&alias_path, "# a person's own file\n").unwrap();
        let failure =
            with_seams(&dir, true, |seams| prepare(&ctx, &places, "demo", seams)).unwrap_err();
        assert_eq!(
            failure.message,
            format!(
                "agentspace: {} is not agentspace's alias for 'demo'; it was left as it is.",
                alias_path.display()
            )
        );
        assert_eq!(
            std::fs::read_to_string(&alias_path).unwrap(),
            "# a person's own file\n"
        );
    }
}
