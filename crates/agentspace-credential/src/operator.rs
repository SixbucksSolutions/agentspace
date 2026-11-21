//! The operator's key and certificate for a host (docs/state.md, "A host's folder" and "The
//! operator's key and certificate"; docs/decisions.md, D47 and D55).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **The files are in `<home>\.agentspace\hosts\<project>\`**: `id_ed25519`, `id_ed25519.pub` and
//!   `id_ed25519-cert.pub`. `home` is the caller's, `USERPROFILE`, as for a lease.
//! - **This module takes no lock.** The caller holds the host registry lock around
//!   [`ensure_operator_key`], so two commands never make two keys (state.md, "The operator's key
//!   and certificate"). [`sign_operator_certificate`] needs no lock: each call signs a copy under
//!   its own name and moves it over the certificate in one rename.
//! - **An existing key MUST NOT be replaced.** A key that does not load with an empty passphrase, or
//!   is not ed25519, is a failure, and a person removes it.
//! - **The key is made, signed and its rules set by the lease code** (`new_key`,
//!   `sign_certificate_commented`, `load_ca`, and `platform::create_private_file`), never by a copy
//!   of it.
//! - **A message MUST be in agentspace's own words:** no path outside the home, no user name, and no
//!   other program's or library's error text. A file is named in its home-relative form,
//!   `~/.agentspace/hosts/<project>/id_ed25519`.
//! - **Nothing here writes to stdout or stderr, and nothing panics on bad input.**

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::lease::{
    CERT_FILE, CredFailure, KEY_FILE, PUB_FILE, agentspace_dir, ca_failure, fail, file_gone,
    load_ca, new_hex, with_line_ending,
};
use crate::platform;
use crate::{
    check_unencrypted, has_passphrase, new_key, parse_private, sign_certificate_commented,
};

/// The one principal of the operator's certificate.
const PRINCIPAL: &str = "ubuntu";
/// How long the operator's certificate is valid, in seconds: one hour, exactly.
const TTL_SECONDS: u32 = 3600;

const BAD_PROJECT: &str =
    "The host's project cannot be used to name its folder. Nothing was changed.";
const KEY_NOT_MADE: &str =
    "The operator's key for the host could not be made. Nothing was changed; try again.";
const KEY_NOT_MADE_LEFT: &str = "The operator's key for the host could not be made, and some of the files it made could not be deleted.";
const KEY_UNPROTECTED: &str = "The operator's private key could not be kept from other users, so it was not kept. Nothing was changed.";
const KEY_UNPROTECTED_LEFT: &str = "The operator's private key could not be kept from other users, and some of the files it made could not be deleted.";
const LEFTOVER_STUCK: &str = "A file left by an earlier attempt to make the operator's key could not be deleted, so no key was made. Nothing was changed.";
const SIGN_FAILED: &str =
    "The operator's certificate for the host could not be signed. Nothing was changed; try again.";

/// A project as the host registry names it, `^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$`, which is also
/// one plain path component.
fn valid_project(project: &str) -> bool {
    let bytes = project.as_bytes();
    let plain = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    !bytes.is_empty()
        && bytes.len() <= 32
        && bytes.iter().all(|b| plain(b) || *b == b'-')
        && bytes.first().is_some_and(plain)
        && bytes.last().is_some_and(plain)
}

fn host_dir(home: &Path, project: &str) -> PathBuf {
    agentspace_dir(home).join("hosts").join(project)
}

/// A file of the host's folder in its home-relative form.
fn relative(project: &str, name: &str) -> String {
    format!("~/.agentspace/hosts/{project}/{name}")
}

/// What is at a path, without following a link: whether it exists, and whether it is a plain file.
enum Found {
    Missing,
    File,
    Other,
}

fn found(path: &Path) -> Result<Found, ()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => Ok(Found::File),
        Ok(_) => Ok(Found::Other),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Found::Missing),
        Err(_) => Err(()),
    }
}

/// Checks that an existing `id_ed25519` loads with an empty passphrase and is ed25519, and names the
/// file in the message when it does not.
fn check_existing(project: &str, key_path: &Path) -> Result<(), CredFailure> {
    let name = relative(project, KEY_FILE);
    let bytes = fs::read(key_path).map_err(|_| {
        fail(&format!(
            "The operator's key {name} could not be read, so it was not replaced. A person removes it."
        ))
    })?;
    match parse_private(&bytes) {
        Ok(key) if has_passphrase(&key) => Err(fail(&format!(
            "The operator's key {name} does not load with an empty passphrase, so it was not replaced. A person removes it."
        ))),
        Ok(key) if key.algorithm() != ssh_key::Algorithm::Ed25519 => Err(fail(&format!(
            "The operator's key {name} is not an ed25519 key, so it was not replaced. A person removes it."
        ))),
        Ok(_) => Ok(()),
        Err(ssh_key::Error::AlgorithmUnknown | ssh_key::Error::AlgorithmUnsupported { .. }) => {
            Err(fail(&format!(
                "The operator's key {name} is not an ed25519 key, so it was not replaced. A person removes it."
            )))
        }
        Err(_) => Err(fail(&format!(
            "The operator's key {name} does not load with an empty passphrase, so it was not replaced. A person removes it."
        ))),
    }
}

/// Makes the operator's ed25519 key for the host `project`, in
/// `<home>\.agentspace\hosts\<project>\`, if `id_ed25519` is missing, and says whether it made it.
///
/// 1. Creates the host's folder, and the folders above it, when they are missing.
/// 2. Looks for `id_ed25519`. When it is there it is checked, never replaced: one that does not
///    load with an empty passphrase, or is not ed25519, is a failure that names the file. Returns
///    `Ok(false)` otherwise.
/// 3. Deletes a leftover `id_ed25519.pub` or `id_ed25519-cert.pub`, which have no key.
/// 4. Makes `id_ed25519` with a lease private key's access rules in place before its bytes are
///    written, the comment `agentspace host <project>`, then checks the rules and that it loads
///    with an empty passphrase, and writes `id_ed25519.pub`. On any failure after that file exists,
///    every file this call made is deleted.
///
/// **It takes no lock; the caller MUST hold the host registry lock.**
pub fn ensure_operator_key(home: &Path, project: &str) -> Result<bool, CredFailure> {
    if !valid_project(project) {
        return Err(fail(BAD_PROJECT));
    }
    let dir = host_dir(home, project);
    fs::create_dir_all(&dir).map_err(|_| fail(KEY_NOT_MADE))?;
    if !dir.is_dir() {
        return Err(fail(KEY_NOT_MADE));
    }

    let key_path = dir.join(KEY_FILE);
    match found(&key_path).map_err(|()| fail(KEY_NOT_MADE))? {
        Found::File => return check_existing(project, &key_path).map(|()| false),
        // Something that is not a plain file where the key belongs is never replaced.
        Found::Other => return check_existing(project, &key_path).and(Err(fail(KEY_NOT_MADE))),
        Found::Missing => {}
    }

    // A leftover public key or certificate has no key, and is replaced.
    let mut stuck = Vec::new();
    for name in [PUB_FILE, CERT_FILE] {
        if !file_gone(&dir.join(name)) {
            stuck.push(relative(project, name));
        }
    }
    if !stuck.is_empty() {
        return Err(CredFailure {
            message: LEFTOVER_STUCK.to_string(),
            leftovers: stuck,
        });
    }

    let mut made: Vec<&str> = Vec::new();
    match make_key(project, &dir, &mut made) {
        Ok(()) => Ok(true),
        Err(message) => {
            // Newest first, as a failed lease issue removes what it made.
            let mut leftovers = Vec::new();
            for name in made.iter().rev() {
                if !file_gone(&dir.join(name)) {
                    leftovers.push(relative(project, name));
                }
            }
            let message = match (leftovers.is_empty(), message) {
                (true, message) => message,
                (false, KEY_UNPROTECTED) => KEY_UNPROTECTED_LEFT,
                (false, _) => KEY_NOT_MADE_LEFT,
            };
            Err(CredFailure {
                message: message.to_string(),
                leftovers,
            })
        }
    }
}

/// The steps of [`ensure_operator_key`] once the key is known to be missing. `made` is the files
/// this call has made, so that a failure removes exactly those.
fn make_key(project: &str, dir: &Path, made: &mut Vec<&'static str>) -> Result<(), &'static str> {
    let key = new_key(&format!("agentspace host {project}")).map_err(|_| KEY_NOT_MADE)?;

    // The private key's file is made with its rules in place, and no byte is written before that.
    let key_path = dir.join(KEY_FILE);
    let Some(mut file) = platform::create_private_file(&key_path) else {
        return Err(KEY_UNPROTECTED);
    };
    made.push(KEY_FILE);
    file.write_all(key.private_openssh.as_bytes())
        .and_then(|()| file.flush())
        .map_err(|_| KEY_NOT_MADE)?;
    drop(file);

    // Checked before the key is kept: its rules, and that it loads with an empty passphrase.
    if !platform::verify_private_key(&key_path) {
        return Err(KEY_UNPROTECTED);
    }
    let on_disk = fs::read(&key_path).map_err(|_| KEY_NOT_MADE)?;
    check_unencrypted(&on_disk).map_err(|_| KEY_NOT_MADE)?;

    let mut public = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(PUB_FILE))
        .map_err(|_| KEY_NOT_MADE)?;
    made.push(PUB_FILE);
    public
        .write_all(&with_line_ending(&key.public_line))
        .and_then(|()| public.flush())
        .map_err(|_| KEY_NOT_MADE)?;
    Ok(())
}

/// Signs a fresh one-hour certificate for the operator's key for the host `project`, and returns
/// when it ends, `now + 3600`, a Unix time in seconds.
///
/// 1. Checks the CA as a lease does, and fails, writing nothing, if it is not usable.
/// 2. Requires `id_ed25519` and `id_ed25519.pub` in the host's folder; a missing key is a failure
///    that names it. Nothing makes the key here: the caller calls [`ensure_operator_key`] first.
/// 3. Copies `id_ed25519.pub` to `sign-<hex>.pub`, and signs the copy: principal `ubuntu`, key ID
///    `operator-<project>`, valid from `now - 300` to `now + 3600`, the two extensions of a lease's
///    certificate.
/// 4. Moves `sign-<hex>-cert.pub` over `id_ed25519-cert.pub` in one rename, so ssh finds the old
///    certificate or the new one, never part of either.
/// 5. Deletes every `sign-<hex>` file, whether or not anything failed. A file that cannot be
///    deleted after a failure is in `leftovers`; after a success the certificate is in place, so
///    it is left and not reported.
///
/// `now` is Unix seconds. **It takes no lock.**
pub fn sign_operator_certificate(home: &Path, project: &str, now: i64) -> Result<i64, CredFailure> {
    let ca_private = load_ca(home).map_err(ca_failure)?;
    if !valid_project(project) {
        return Err(fail(BAD_PROJECT));
    }
    let valid_before = now
        .checked_add(i64::from(TTL_SECONDS))
        .ok_or_else(|| fail(SIGN_FAILED))?;
    let dir = host_dir(home, project);

    for name in [KEY_FILE, PUB_FILE] {
        let present = fs::metadata(dir.join(name)).is_ok_and(|m| m.is_file());
        if !present {
            return Err(fail(&format!(
                "The operator's key {} could not be found, so no certificate was signed. Nothing was changed.",
                relative(project, name)
            )));
        }
    }

    let hex = new_hex();
    let public_name = format!("sign-{hex}.pub");
    let cert_name = format!("sign-{hex}-cert.pub");
    let steps = sign_steps(
        &dir,
        &ca_private,
        project,
        now,
        (public_name.as_str(), cert_name.as_str()),
    );

    // Every `sign-<hex>` file goes, whatever happened. After a rename the certificate's name is
    // already gone, which `file_gone` counts as gone.
    let mut leftovers = Vec::new();
    for name in [&cert_name, &public_name] {
        if !file_gone(&dir.join(name)) {
            leftovers.push(relative(project, name));
        }
    }
    match steps {
        Ok(()) => Ok(valid_before),
        Err(()) => Err(CredFailure {
            message: SIGN_FAILED.to_string(),
            leftovers,
        }),
    }
}

/// Steps 3 and 4 of [`sign_operator_certificate`]. A failure leaves its temporary files for the
/// caller to delete.
fn sign_steps(
    dir: &Path,
    ca_private: &[u8],
    project: &str,
    now: i64,
    (public_name, cert_name): (&str, &str),
) -> Result<(), ()> {
    let public_bytes = fs::read(dir.join(PUB_FILE)).map_err(|_| ())?;
    let mut copy = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(public_name))
        .map_err(|_| ())?;
    copy.write_all(&public_bytes)
        .and_then(|()| copy.flush())
        .map_err(|_| ())?;
    drop(copy);

    let line = sign_certificate_commented(
        ca_private,
        &public_bytes,
        &format!("operator-{project}"),
        PRINCIPAL,
        now,
        TTL_SECONDS,
        &format!("agentspace host {project}"),
    )
    .map_err(|_| ())?;
    let mut cert = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(cert_name))
        .map_err(|_| ())?;
    cert.write_all(&with_line_ending(&line))
        .and_then(|()| cert.flush())
        .map_err(|_| ())?;
    drop(cert);

    // One rename, so ssh finds the old certificate or the new one.
    fs::rename(dir.join(cert_name), dir.join(CERT_FILE)).map_err(|_| ())
}

#[cfg(all(test, windows))]
mod tests;
