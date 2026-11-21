//! Place, renew, check a release and release (docs/state.md, "Creation, in order", "Renewal" and
//! "Release, and what it leaves"; docs/driver.md, "The operations").
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **Place MUST decide support first, then check the CA, refuse an existing key, make and sign
//!   the key, make the transport, make sure of the `Include` line, and write the alias file last.**
//! - **A failed place MUST remove every file and folder it made,** the alias file only when its
//!   owner mark names this lease, and MUST keep the `Include` line, any backup and `agentspace.d`.
//! - **Release MUST remove the alias file before the lease folder,** and MUST work for a lease
//!   that was just placed and has no record.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use agentspace_alias::{
    AliasFailure, AliasFailureKind, AliasFile, alias_file_bytes, alias_name, check_alias_owner,
    default_proxy_command, ensure_include, find_proxy_program, is_valid_alias, lease_file_text,
    remove_alias, write_alias,
};
use agentspace_credential::{
    CredFailure, RenewGuard, issue, remove_lease_folder, renew as renew_guard,
};
use agentspace_driver_api::{Context, Failure, LeaseRef, PlaceRequest, Placed, Renewal};

use crate::reasons::reasons;
use crate::text::{
    FOREIGN_ALIAS, TRANSPORT_MISSING, alias_check_failed, alias_leftover, alias_setup_failed,
    capitalize, delete_failed, failure, lease_file_name, lease_folder_name, still_there,
    transport_failed,
};

/// The certificate's one principal, the returned user and the alias's login user (D3).
const USER: &str = "ubuntu";

/// The comment of an alias file's owner mark line (state.md, "The alias file").
const COMMENT: &str = "written by agentspace. Do not edit.";

/// The three files of a lease's credential, in the order they are removed.
const CREDENTIAL_FILES: [&str; 3] = ["id_ed25519-cert.pub", "id_ed25519.pub", "id_ed25519"];

/// Windows' `SystemRoot`, for the `cmd.exe /d /c` form of the default ProxyCommand, from the
/// context only (`Context::system_root`'s doc comment; the architect's ruling of 2026-11-19, which
/// answered this story's question). Unset or empty is `None`, so that form then fails with
/// "Windows' system folder is not known"; the `.exe` form needs none.
fn system_root(ctx: &Context) -> Option<&str> {
    ctx.system_root.as_deref().filter(|r| !r.is_empty())
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

fn lease_dir(home: &Path, id: &str) -> PathBuf {
    home.join(".agentspace").join("leases").join(id)
}

fn cred_failure(f: CredFailure) -> Failure {
    failure(f.message, f.leftovers)
}

/// The lease's ProxyCommand: the override when there is one, and else the default line from the
/// program beside the running program, which is never read for the instance-ID cache (D21).
fn proxy_command(ctx: &Context, alias: &str, system_root: Option<&str>) -> Result<String, String> {
    // An alias that is not a valid name is an alias failure (driver.md), not a transport one.
    if !is_valid_alias(alias) {
        return Err(alias_setup_failed("the host alias is not a valid name"));
    }
    if let Some(command) = ctx.proxy_override.as_deref().filter(|c| !c.is_empty()) {
        return Ok(command.to_string());
    }
    let Some(program) = find_proxy_program(&ctx.own_exe) else {
        return Err(TRANSPORT_MISSING.to_string());
    };
    default_proxy_command(&program.to_string_lossy(), alias, system_root)
        .map_err(|e| transport_failed(&e.0))
}

/// The alias file's bytes for a lease, or the message of the failure that stops the place.
fn alias_bytes(
    ctx: &Context,
    req: &PlaceRequest,
    alias: &str,
    system_root: Option<&str>,
) -> Result<Vec<u8>, String> {
    let proxy = proxy_command(ctx, alias, system_root)?;
    let home = std::path::absolute(&ctx.home).unwrap_or_else(|_| ctx.home.clone());
    let home = home.to_string_lossy();
    let key_path = lease_file_text(&home, req.id, "id_ed25519");
    let cert_path = lease_file_text(&home, req.id, "id_ed25519-cert.pub");
    alias_file_bytes(&AliasFile {
        id: req.id,
        comment: COMMENT,
        alias,
        user: USER,
        key_path: &key_path,
        cert_path: &cert_path,
        host_key_alias: "agentspace",
        known_hosts: None,
        proxy_command: &proxy,
    })
    .map_err(|e| alias_setup_failed(&e.0))
}

/// Removes the lease's alias file when one is there and its owner mark names this lease. A foreign
/// file is kept, and so is anything that is not a file. Returns the file's name when it could not
/// be removed.
fn clean_alias(ctx: &Context, alias: &str, id: &str) -> Option<String> {
    let path = ctx.ssh_dir.join("agentspace.d").join(alias);
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_file() => match remove_alias(&ctx.ssh_dir, alias, id) {
            Ok(()) => None,
            Err(f) if f.kind == AliasFailureKind::Foreign => None,
            Err(_) => Some(alias_leftover(&ctx.home, &ctx.ssh_dir, alias)),
        },
        _ => None,
    }
}

/// Removes what a place made in the lease's folder: the whole folder when the place made it, and
/// only the three credential files when the folder was there before. Returns what stays.
fn clean_credential(ctx: &Context, id: &str, folder_existed: bool) -> Vec<String> {
    if !folder_existed {
        return match remove_lease_folder(&ctx.home, id) {
            Ok(()) => Vec::new(),
            Err(f) if f.leftovers.is_empty() => vec![lease_folder_name(id)],
            Err(f) => f.leftovers,
        };
    }
    let dir = lease_dir(&ctx.home, id);
    let mut left = Vec::new();
    for name in CREDENTIAL_FILES {
        match fs::remove_file(dir.join(name)) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(_) => left.push(lease_file_name(id, name)),
        }
    }
    left
}

/// The failure of a place that went wrong after its credential was issued: removes what the place
/// made, and names what it could not remove.
fn undo_place(
    ctx: &Context,
    id: &str,
    alias: &str,
    folder_existed: bool,
    message: String,
) -> Failure {
    let mut left = Vec::new();
    left.extend(clean_alias(ctx, alias, id));
    left.extend(clean_credential(ctx, id, folder_existed));
    failure(message, left)
}

/// Why the alias step failed, as lease-api.md's "Alias cannot be set up" message.
fn alias_step_message(f: &AliasFailure) -> String {
    alias_setup_failed(&f.message)
}

pub fn place(ctx: &Context, req: &PlaceRequest) -> Result<Placed, Failure> {
    let unmet = reasons(req.platform, req.profile, req.size);
    if !unmet.is_empty() {
        return Ok(Placed::Unsatisfiable { reasons: unmet });
    }

    // Steps 1 to 4: the CA is checked and never made, an existing key is refused, and the key is
    // made, checked and signed. The credential crate removes what it made when it fails.
    let folder_existed = fs::symlink_metadata(lease_dir(&ctx.home, req.id)).is_ok();
    let issued = issue(&ctx.home, req.id, USER, req.ttl_seconds).map_err(cred_failure)?;

    let alias = alias_name(req.project, req.id);

    // The transport and the alias file's bytes come before the `Include` line: a transport that
    // cannot be found, a line break or an invalid alias name leaves none of it.
    let bytes = match alias_bytes(ctx, req, &alias, system_root(ctx)) {
        Ok(bytes) => bytes,
        Err(message) => return Err(undo_place(ctx, req.id, &alias, folder_existed, message)),
    };

    // Step 5, then step 6.
    if let Err(f) = ensure_include(&ctx.ssh_dir, now_unix()) {
        let message = alias_step_message(&f);
        return Err(undo_place(ctx, req.id, &alias, folder_existed, message));
    }
    if let Err(f) = write_alias(&ctx.ssh_dir, &alias, req.id, &bytes) {
        let message = alias_step_message(&f);
        return Err(undo_place(ctx, req.id, &alias, folder_existed, message));
    }

    Ok(Placed::Ready {
        host_alias: alias,
        user: USER.to_string(),
        expires_at: issued.expires_at,
    })
}

/// A renewal that holds its credential crate guard. Dropping it drops the guard, which rolls the
/// renewal back, so it MUST NOT be defeated.
struct Held {
    guard: RenewGuard,
}

impl Renewal for Held {
    fn expires_at(&self) -> i64 {
        self.guard.expires_at()
    }

    fn commit(self: Box<Self>) {
        let Held { guard } = *self;
        guard.commit();
    }

    fn rollback(self: Box<Self>) -> Result<(), Failure> {
        let Held { guard } = *self;
        guard.rollback().map_err(cred_failure)
    }
}

pub fn renew(
    ctx: &Context,
    lease: &LeaseRef,
    ttl_seconds: u32,
) -> Result<Box<dyn Renewal>, Failure> {
    let guard = renew_guard(&ctx.home, lease.id, USER, ttl_seconds).map_err(cred_failure)?;
    Ok(Box::new(Held { guard }))
}

/// The failure of a refused or failed alias check, in lease-api.md's words.
fn check_failure(f: &AliasFailure) -> Failure {
    let message = if f.kind == AliasFailureKind::Foreign {
        FOREIGN_ALIAS.to_string()
    } else {
        alias_check_failed(&f.message)
    };
    failure(message, Vec::new())
}

pub fn check_release(ctx: &Context, lease: &LeaseRef) -> Result<(), Failure> {
    let alias = alias_name(lease.project, lease.id);
    check_alias_owner(&ctx.ssh_dir, &alias, lease.id).map_err(|f| check_failure(&f))
}

pub fn release(ctx: &Context, lease: &LeaseRef) -> Result<(), Failure> {
    let alias = alias_name(lease.project, lease.id);
    // Step 1 again, so that a refused alias deletes nothing, the lease folder included.
    check_alias_owner(&ctx.ssh_dir, &alias, lease.id).map_err(|f| check_failure(&f))?;

    // The alias file goes first, so that it never points at a key that is gone. When it cannot be
    // removed the lease folder is left alone, and both are named.
    if let Err(f) = remove_alias(&ctx.ssh_dir, &alias, lease.id) {
        if f.kind == AliasFailureKind::Foreign {
            return Err(check_failure(&f));
        }
        let mut left = vec![alias_leftover(&ctx.home, &ctx.ssh_dir, &alias)];
        if fs::symlink_metadata(lease_dir(&ctx.home, lease.id)).is_ok() {
            left.push(lease_folder_name(lease.id));
        }
        // The `<why>` already names each leftover, so the message is not given a second list.
        return Err(Failure {
            message: delete_failed(&still_there(&left)),
            leftovers: left,
        });
    }

    remove_lease_folder(&ctx.home, lease.id).map_err(|f| {
        // The crate's message is a phrase that fits the template's `<why>`: either a lowercase
        // phrase, or one that starts with a home-relative leftover (`~/...`) and names every
        // leftover itself, so the message is not given a second list. An ID it cannot use as a
        // folder's name is a whole sentence of its own.
        if f.message
            .starts_with(|c: char| c.is_lowercase() || c == '~')
        {
            Failure {
                message: delete_failed(&f.message),
                leftovers: f.leftovers,
            }
        } else {
            failure(capitalize(&f.message), f.leftovers)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const ID: &str = "3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90";
    const OTHER: &str = "aaaaaaaa-5b7d-4a60-8c11-0d2e4f6a8b90";

    fn ctx(root: &Path, over: Option<&str>) -> Context {
        Context {
            home: root.join("home"),
            ssh_dir: root.join("home").join(".ssh"),
            proxy_override: over.map(str::to_string),
            system_root: None,
            own_exe: root.join("bin").join("agentspace.exe"),
        }
    }

    fn req<'a>(id: &'a str) -> PlaceRequest<'a> {
        PlaceRequest {
            id,
            project: "swim",
            platform: "linux/arm64",
            profile: "agent-linux",
            size: "m",
            ttl_seconds: 3600,
        }
    }

    #[test]
    fn the_default_transport_needs_the_program_beside_the_running_one() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), None);
        let e = proxy_command(&c, "agentspace-swim-3f2a9c1e", None).unwrap_err();
        assert_eq!(e, TRANSPORT_MISSING);
    }

    #[test]
    fn an_exe_program_gives_the_plain_line_and_a_cmd_program_needs_the_system_folder() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let c = ctx(dir.path(), None);
        fs::write(bin.join("agentspace-ssh-proxy.cmd"), b"").unwrap();
        let e = proxy_command(&c, "agentspace-swim-3f2a9c1e", None).unwrap_err();
        assert_eq!(
            e,
            "The lease's way to reach its host cannot be found because Windows' system folder is not known. No lease was recorded."
        );
        let line = proxy_command(&c, "agentspace-swim-3f2a9c1e", Some("C:\\Windows\\")).unwrap();
        assert!(line.starts_with("C:\\Windows\\System32\\cmd.exe /d /c \""));
        assert!(line.ends_with(" agentspace-swim-3f2a9c1e"));
        fs::write(bin.join("agentspace-ssh-proxy.exe"), b"").unwrap();
        let line = proxy_command(&c, "agentspace-swim-3f2a9c1e", None).unwrap();
        assert!(
            line.starts_with('"')
                && line.ends_with("agentspace-ssh-proxy.exe\" agentspace-swim-3f2a9c1e")
        );
    }

    #[test]
    fn an_override_replaces_the_program_and_an_empty_one_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), Some("my-proxy %h"));
        assert_eq!(proxy_command(&c, "a", None).unwrap(), "my-proxy %h");
        let c = ctx(dir.path(), Some(""));
        assert_eq!(
            proxy_command(&c, "agentspace-a-12345678", None).unwrap_err(),
            TRANSPORT_MISSING
        );
    }

    #[test]
    fn an_override_with_a_line_break_is_refused_in_the_alias_words() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), Some("a\nb"));
        let e = alias_bytes(&c, &req(ID), "agentspace-swim-3f2a9c1e", None).unwrap_err();
        assert_eq!(
            e,
            "The host alias could not be added to the OpenSSH client configuration: the ProxyCommand holds a line break. No lease was recorded."
        );
    }

    #[test]
    fn the_alias_bytes_carry_the_user_the_paths_and_the_owner_mark() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), Some("proxy 100%"));
        let bytes = alias_bytes(&c, &req(ID), "agentspace-swim-3f2a9c1e", None).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with(&format!("# agentspace lease {ID}; ")));
        assert!(text.contains("\n  User ubuntu\n"));
        assert!(text.contains("\n  ProxyCommand proxy 100%%\n"));
        assert!(text.contains(&format!("/.agentspace/leases/{ID}/id_ed25519\"")));
    }

    #[test]
    fn an_unsupported_request_answers_before_anything_is_touched() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), None);
        let mut r = req(ID);
        r.platform = "windows/amd64";
        r.size = "xl";
        let got = place(&c, &r).unwrap();
        let Placed::Unsatisfiable { reasons } = got else {
            panic!("not unsatisfiable");
        };
        let codes: Vec<&str> = reasons.iter().map(|r| r.code).collect();
        assert_eq!(codes, ["unsupported_platform", "unsupported_size"]);
        assert!(!c.home.exists());
    }

    #[test]
    fn no_ca_fails_with_the_no_ca_wording_and_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), Some("p"));
        fs::create_dir_all(&c.home).unwrap();
        let f = place(&c, &req(ID)).unwrap_err();
        assert_eq!(
            f.message,
            "agentspace's certificate authority has not been set up, so no certificate can be issued. Nothing was changed; the operator sets it up."
        );
        assert!(f.leftovers.is_empty());
        assert_eq!(fs::read_dir(&c.home).unwrap().count(), 0);
    }

    fn write_alias_for(c: &Context, alias: &str, id: &str) -> PathBuf {
        let dir = c.ssh_dir.join("agentspace.d");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(alias);
        fs::write(&path, format!("# agentspace lease {id}; test\nHost x\n")).unwrap();
        path
    }

    #[test]
    fn cleaning_removes_an_alias_that_names_the_lease_and_keeps_a_foreign_one() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), None);
        let mine = write_alias_for(&c, "agentspace-swim-3f2a9c1e", ID);
        assert_eq!(clean_alias(&c, "agentspace-swim-3f2a9c1e", ID), None);
        assert!(!mine.exists());
        let foreign = write_alias_for(&c, "agentspace-swim-3f2a9c1e", OTHER);
        assert_eq!(clean_alias(&c, "agentspace-swim-3f2a9c1e", ID), None);
        assert!(foreign.exists());
        // Nothing there, a folder there, and a folder where agentspace.d should be are all left.
        assert_eq!(clean_alias(&c, "agentspace-swim-00000000", ID), None);
    }

    #[test]
    fn a_file_where_the_alias_folder_has_to_be_is_not_a_leftover() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), None);
        fs::create_dir_all(&c.ssh_dir).unwrap();
        fs::write(c.ssh_dir.join("agentspace.d"), b"x").unwrap();
        assert_eq!(clean_alias(&c, "agentspace-swim-3f2a9c1e", ID), None);
    }

    #[test]
    fn cleaning_a_folder_that_existed_removes_only_the_credential_files() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), None);
        let lease = lease_dir(&c.home, ID);
        fs::create_dir_all(&lease).unwrap();
        for name in CREDENTIAL_FILES {
            fs::write(lease.join(name), b"x").unwrap();
        }
        fs::write(lease.join("other"), b"y").unwrap();
        assert!(clean_credential(&c, ID, true).is_empty());
        assert!(lease.join("other").exists());
        assert!(!lease.join("id_ed25519").exists());
        // A folder the place made goes whole.
        assert!(clean_credential(&c, ID, false).is_empty());
        assert!(!lease.exists());
    }

    #[test]
    fn release_of_nothing_succeeds_and_a_foreign_alias_is_refused_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), None);
        let lease = LeaseRef {
            id: ID,
            project: "swim",
        };
        assert!(check_release(&c, &lease).is_ok());
        assert!(release(&c, &lease).is_ok());
        assert!(release(&c, &lease).is_ok());

        let folder = lease_dir(&c.home, ID);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("id_ed25519"), b"k").unwrap();
        let foreign = write_alias_for(&c, "agentspace-swim-3f2a9c1e", OTHER);
        let f = check_release(&c, &lease).unwrap_err();
        assert_eq!(f.message, FOREIGN_ALIAS);
        let f = release(&c, &lease).unwrap_err();
        assert_eq!(f.message, FOREIGN_ALIAS);
        assert!(foreign.exists());
        assert!(folder.join("id_ed25519").exists());
    }

    #[test]
    fn release_removes_the_alias_then_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), None);
        let lease = LeaseRef {
            id: ID,
            project: "swim",
        };
        let folder = lease_dir(&c.home, ID);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("id_ed25519"), b"k").unwrap();
        let alias = write_alias_for(&c, "agentspace-swim-3f2a9c1e", ID);
        release(&c, &lease).unwrap();
        assert!(!alias.exists());
        assert!(!folder.exists());
        // The `agentspace.d` folder stays.
        assert!(c.ssh_dir.join("agentspace.d").is_dir());
    }

    #[test]
    fn a_folder_at_the_alias_place_is_refused_by_the_check() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), None);
        let lease = LeaseRef {
            id: ID,
            project: "swim",
        };
        fs::create_dir_all(
            c.ssh_dir
                .join("agentspace.d")
                .join("agentspace-swim-3f2a9c1e"),
        )
        .unwrap();
        let f = check_release(&c, &lease).unwrap_err();
        assert_eq!(f.message, FOREIGN_ALIAS);
    }

    #[test]
    fn a_file_where_the_alias_folder_has_to_be_names_it_in_the_release_check() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), None);
        let lease = LeaseRef {
            id: ID,
            project: "swim",
        };
        fs::create_dir_all(&c.ssh_dir).unwrap();
        fs::write(c.ssh_dir.join("agentspace.d"), b"x").unwrap();
        let f = check_release(&c, &lease).unwrap_err();
        assert_eq!(
            f.message,
            "The lease's host alias file could not be checked: the folder for host aliases is a file, not a folder. Nothing was deleted."
        );
    }

    #[test]
    fn a_file_where_the_leases_folder_has_to_be_fails_the_release_with_the_why() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path(), None);
        let lease = LeaseRef {
            id: ID,
            project: "swim",
        };
        fs::create_dir_all(c.home.join(".agentspace")).unwrap();
        fs::write(c.home.join(".agentspace").join("leases"), b"x").unwrap();
        let f = release(&c, &lease).unwrap_err();
        assert_eq!(
            f.message,
            "The lease's local key, certificate or host alias could not be deleted: the folder for leases is a file, not a folder. Run release again to finish."
        );
    }

    #[test]
    fn the_system_folder_comes_from_the_context_and_empty_is_unset() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = ctx(dir.path(), None);
        assert_eq!(system_root(&c), None);
        c.system_root = Some(String::new());
        assert_eq!(system_root(&c), None);
        c.system_root = Some("C:\\Windows".to_string());
        assert_eq!(system_root(&c), Some("C:\\Windows"));
    }

    #[test]
    fn the_driver_names_itself_only_to_the_cli() {
        use agentspace_driver_api::Driver;
        assert_eq!(crate::Ec2Driver::new().name(), "ec2");
    }
}
