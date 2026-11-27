//! How the operator reaches a host that is not the box (docs/operator.md, How the operator reaches
//! a host; docs/state.md, A host's folder, The operator's key and certificate, A host's operator
//! alias; D47, D55).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! [`sign_in`] is the four steps, in order, once per command: the CA check, the operator's key for
//! the host (made under `hosts.lock`, only when missing), a fresh one-hour certificate, and the
//! host's operator alias with the `Include` line. The private key MUST NOT be printed or put in
//! any message: no message here holds more of it than its path.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use agentspace_alias::{
    AliasFailureKind, HostAliasFile, default_proxy_command, ensure_include, host_alias_file_bytes,
    host_alias_name, host_file_text, known_hosts_text, write_host_alias,
};
use agentspace_ca::CaEnv;
use agentspace_credential::{ensure_operator_key, sign_operator_certificate};
use agentspace_hosts::{HostKey, host_key_path_keyed};
use agentspace_operator_api::{CallerFailure, Context, Failure};

use crate::cache::registry_failure;

/// The program a host's operator alias names, beside the operator (operator.md, How each program
/// finds the others). Only the `.exe` is looked for.
const PROXY_PROGRAM: &str = "agentspace-ssh-proxy.exe";

/// The comment of the alias file's owner mark line: agentspace-alias's, the text's one home, so that
/// the operator and `sync` write the same bytes and neither rewrites the other's alias (operator.md,
/// How the operator reaches a host: "both commands write the same bytes"; architect, 2026-11-21).
const COMMENT: &str = agentspace_alias::HOST_ALIAS_COMMENT;

/// Where the operator writes a host's alias, and where it finds the program the alias names.
#[derive(Debug, Clone)]
pub struct Access {
    /// The ssh folder: `AGENTSPACE_SSH_DIR` made full, or `<home>\.ssh`.
    pub ssh_dir: PathBuf,
    /// The running executable, as a full path, or `None` when the system could not say.
    pub own_exe: Option<PathBuf>,
}

fn could_not_reach(message: String) -> Failure {
    Failure {
        message,
        caller: CallerFailure::CouldNotReach,
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

/// The full path of the ProxyCommand program beside the operator, whether or not it is there.
fn proxy_program_path(access: &Access) -> PathBuf {
    access
        .own_exe
        .as_deref()
        .and_then(Path::parent)
        .map_or_else(
            || PathBuf::from(PROXY_PROGRAM),
            |dir| dir.join(PROXY_PROGRAM),
        )
}

/// The operator's key, certificate and alias for the `linux/arm64` host of `project`, which is not
/// the box: [`sign_in_keyed`] with that host's key, byte for byte as before v0.12.
pub fn sign_in(
    ctx: &Context,
    ca_env: &CaEnv,
    access: &Access,
    project: &str,
) -> Result<(), Failure> {
    sign_in_keyed(ctx, ca_env, access, &HostKey::linux_arm64(project))
}

/// The operator's key, certificate and alias for the host `key`, which is not the box. Every name
/// is the host's own, from its key (operator.md, A host's platform and its names; D91): its folder,
/// and so its key's and certificate's paths, and its alias, are its host name's, which is the
/// project on `linux/arm64`, so that platform's names are unchanged.
///
/// 1. Checks the CA as `sync`'s step 1 does, with `ca`'s own message.
/// 2. Makes the operator's key when it is missing, while it holds `hosts.lock`: `agentspace: Could
///    not make the operator's key <path>: <why>`.
/// 3. Signs a fresh one-hour certificate: `agentspace: Could not sign the operator's certificate
///    for '<host name>': <why>`.
/// 4. Writes the host's alias when it is missing or its bytes differ, and the `Include` line when it
///    is missing, with the messages of operator.md's step 4.
pub fn sign_in_keyed(
    ctx: &Context,
    ca_env: &CaEnv,
    access: &Access,
    key: &HostKey,
) -> Result<(), Failure> {
    agentspace_ca::check_for_sync(ctx, ca_env)?;
    // The host's name names its folder, its certificate and its alias (D91).
    let host_name = key.folder_name();
    let project = host_name.as_str();

    // Step 2: the key. It is looked for without the lock, so that a command whose key exists never
    // waits on a provisioning run's `hosts.lock` (architect, 2026-11-21, for v0.07-T03). Only when it
    // is missing is `hosts.lock` taken, and `ensure_operator_key`, which looks for the key again,
    // called while it is held, so two commands never make two keys. The lock is held for that alone.
    let key_path = host_key_path_keyed(&ctx.home, key);
    if !key_path.is_file() {
        let held = agentspace_hosts::lock(&ctx.home).map_err(|e| registry_failure(&ctx.home, e))?;
        // Pin P12 of v0.12-T03: once E0.12-T03-S03's second part gives agentspace-credential its
        // pair by host name, this one call becomes
        // `ensure_operator_key_for(&ctx.home, &key.host_name())`.
        let made = ensure_operator_key(&ctx.home, project);
        drop(held);
        made.map_err(|f| {
            could_not_reach(format!(
                "agentspace: Could not make the operator's key {}: {}",
                key_path.display(),
                f.message
            ))
        })?;
    }

    // Step 3: a fresh certificate, replacing the last one.
    // Pin P12 of v0.12-T03: once E0.12-T03-S03's second part gives agentspace-credential its pair
    // by host name, this one call becomes
    // `sign_operator_certificate_for(&ctx.home, &key.host_name(), principal, unix_now())`, with
    // `principal` the host's seam's login user (operator.md, The platform seam, the login user
    // row), `seam.login_user()`, the seam from `agentspace_platforms::for_platform(&key.platform)`.
    sign_operator_certificate(&ctx.home, project, unix_now()).map_err(|f| {
        could_not_reach(format!(
            "agentspace: Could not sign the operator's certificate for '{project}': {}",
            f.message
        ))
    })?;

    // Step 4: the alias, and the `Include` line before it.
    write_alias(ctx, access, project)
}

fn write_alias(ctx: &Context, access: &Access, project: &str) -> Result<(), Failure> {
    let alias = host_alias_name(project);
    let program = proxy_program_path(access);
    if !program.is_file() {
        return Err(could_not_reach(format!(
            "agentspace: {} is missing; the operator's alias for a host needs it beside the operator.",
            program.display()
        )));
    }
    let alias_path = access.ssh_dir.join("agentspace.d").join(&alias);
    let cannot_write =
        |path: &Path| could_not_reach(format!("agentspace: Cannot write {}.", path.display()));

    let proxy = default_proxy_command(&program.to_string_lossy(), &alias, None)
        .map_err(|_| cannot_write(&alias_path))?;
    let home = std::path::absolute(&ctx.home).unwrap_or_else(|_| ctx.home.clone());
    let home = home.to_string_lossy();
    let key_path = host_file_text(&home, project, "id_ed25519");
    let cert_path = host_file_text(&home, project, "id_ed25519-cert.pub");
    let known_hosts = known_hosts_text(&home);
    let bytes = host_alias_file_bytes(&HostAliasFile {
        project,
        comment: COMMENT,
        key_path: &key_path,
        cert_path: &cert_path,
        known_hosts: &known_hosts,
        proxy_command: &proxy,
    })
    .map_err(|_| cannot_write(&alias_path))?;

    let config = access.ssh_dir.join("config");
    // An `Include` line that cannot be added fails by its kind: `Config`, a configuration a person
    // has to fix, with its reason, and any other with `Cannot write <config>.` (operator.md, How the
    // operator reaches a host, step 4; architect, 2026-11-21, for v0.07-T03, on #0152's review).
    // The words and the `<config>` path are agentspace-sync's for the same failure.
    ensure_include(&access.ssh_dir, unix_now()).map_err(|f| match f.kind {
        AliasFailureKind::Config => could_not_reach(format!(
            "agentspace: {} cannot take agentspace's Include line: {}.",
            config.display(),
            f.message
        )),
        _ => cannot_write(&config),
    })?;
    write_host_alias(&access.ssh_dir, project, &bytes).map_err(|f| match f.kind {
        AliasFailureKind::Foreign => could_not_reach(format!(
            "agentspace: {} is not agentspace's alias for '{project}'; it was left as it is.",
            alias_path.display()
        )),
        _ => cannot_write(&alias_path),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempHome;

    #[test]
    fn a_program_that_is_not_beside_the_operator_fails_the_alias_with_its_path() {
        let home = TempHome::new("access-no-proxy");
        let ctx = home.ctx();
        let folder = home.path().join("bin");
        std::fs::create_dir_all(&folder).unwrap();
        let access = Access {
            ssh_dir: home.path().join("ssh"),
            own_exe: Some(folder.join("agentspace-operator.exe")),
        };
        let failure = write_alias(&ctx, &access, "agentspace").unwrap_err();
        assert_eq!(
            failure.message,
            format!(
                "agentspace: {} is missing; the operator's alias for a host needs it beside the operator.",
                folder.join("agentspace-ssh-proxy.exe").display()
            )
        );
        assert!(!access.ssh_dir.exists(), "Nothing was written");
    }

    #[test]
    fn the_alias_is_written_with_the_include_line_and_a_foreign_file_is_left() {
        let home = TempHome::new("access-alias");
        let ctx = home.ctx();
        let folder = home.path().join("bin");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("agentspace-ssh-proxy.exe"), b"x").unwrap();
        let access = Access {
            ssh_dir: home.path().join("ssh"),
            own_exe: Some(folder.join("agentspace-operator.exe")),
        };
        write_alias(&ctx, &access, "agentspace").unwrap();
        let alias = access
            .ssh_dir
            .join("agentspace.d")
            .join("agentspace-host.agentspace");
        let text = std::fs::read_to_string(&alias).unwrap();
        assert!(text.starts_with("# agentspace host agentspace; "), "{text}");
        assert!(text.contains("Host agentspace-host.agentspace\n"));
        assert!(text.contains("StrictHostKeyChecking accept-new\n"));
        assert!(!text.contains("RemoteForward"));
        let config = std::fs::read_to_string(access.ssh_dir.join("config")).unwrap();
        assert!(config.contains("Include"), "{config}");

        // A second write changes nothing, and a foreign file is left as it is.
        write_alias(&ctx, &access, "agentspace").unwrap();
        assert_eq!(std::fs::read_to_string(&alias).unwrap(), text);
        std::fs::write(&alias, b"someone else's\n").unwrap();
        let failure = write_alias(&ctx, &access, "agentspace").unwrap_err();
        assert_eq!(
            failure.message,
            format!(
                "agentspace: {} is not agentspace's alias for 'agentspace'; it was left as it is.",
                alias.display()
            )
        );
        assert_eq!(std::fs::read(&alias).unwrap(), b"someone else's\n");
    }

    #[test]
    fn another_platforms_alias_is_named_by_its_host_name_and_names_its_folder() {
        let home = TempHome::new("access-alias-platform");
        let ctx = home.ctx();
        let folder = home.path().join("bin");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("agentspace-ssh-proxy.exe"), b"x").unwrap();
        let access = Access {
            ssh_dir: home.path().join("ssh"),
            own_exe: Some(folder.join("agentspace-operator.exe")),
        };
        let key = HostKey::new("agentspace", "windows/amd64");
        write_alias(&ctx, &access, &key.folder_name()).unwrap();
        let alias = access
            .ssh_dir
            .join("agentspace.d")
            .join(key.operator_alias());
        let text = std::fs::read_to_string(&alias).unwrap();
        assert!(text.contains("Host agentspace-host.agentspace.windows-amd64\n"));
        assert!(text.contains("  User agent\n"), "{text}");
        assert!(text.contains("/.agentspace/hosts/agentspace.windows-amd64/id_ed25519\""));
        // The linux/arm64 host's alias of the same project is another file, left unmade.
        assert!(
            !access
                .ssh_dir
                .join("agentspace.d")
                .join("agentspace-host.agentspace")
                .exists()
        );
    }
}
