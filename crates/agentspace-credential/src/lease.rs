//! A lease's credential on disk: the CA's check, the lease folder, issue, renew and delete
//! (docs/state.md, "A lease's credential", "Creation, in order", "Renewal" and "The CA").
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **The home folder is the caller's.** Every path here is under `<home>\.agentspace`, and the
//!   caller reads `home` from `USERPROFILE` (state.md, "The home folder").
//! - **A message MUST be in agentspace's own words:** no path, no user name, and no other program's
//!   or library's error text. A file or a folder that could not be removed is named only in its
//!   home-relative form, `~/.agentspace/leases/<id>/id_ed25519`, and only in `leftovers`.
//! - **The CA is only read.** Nothing here creates, repairs, loosens or replaces it (D17).
//! - **A function here MUST NOT write to stdout or stderr, and MUST NOT panic on bad input.**
//! - **The caller holds the lease state lock** (state.md, "The lease state lock") around `issue`,
//!   `renew` and the use of a [`RenewGuard`]. This crate takes no lock.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ssh_key::rand_core::{OsRng, RngCore};

use crate::platform::{self, CreateError};
use crate::{check_ca_keys, check_unencrypted, new_lease_key, sign_certificate};

/// The private key's file name.
const KEY_FILE: &str = "id_ed25519";
/// The public key's file name.
const PUB_FILE: &str = "id_ed25519.pub";
/// The certificate's file name, which ssh finds beside the key.
const CERT_FILE: &str = "id_ed25519-cert.pub";

/// Why a CA is not usable (state.md, "The CA", "Its checks").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaFailure {
    /// A file of the CA does not exist (check 1).
    Missing,
    /// Any other failed check (checks 2 to 5).
    NotUsable,
}

/// A failure in agentspace's own words, and what it could not clean up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredFailure {
    /// The message, without the `agentspace lease <sub> failed: ` prefix, which the CLI adds.
    pub message: String,
    /// Each file or folder that could not be removed, home-relative, as
    /// `~/.agentspace/leases/<id>/id_ed25519`.
    pub leftovers: Vec<String>,
}

/// What `issue` made: the certificate's expiry, a Unix time in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Issued {
    pub expires_at: i64,
}

fn fail(message: &str) -> CredFailure {
    CredFailure {
        message: message.to_string(),
        leftovers: Vec::new(),
    }
}

/// The message for a failed CA check, lease-api.md's wording without the
/// `agentspace lease <sub> failed: ` prefix, which the CLI adds.
pub fn ca_failure_message(f: CaFailure) -> &'static str {
    match f {
        CaFailure::Missing => {
            "agentspace's certificate authority has not been set up, so no certificate can be \
             issued. Nothing was changed; the operator sets it up."
        }
        CaFailure::NotUsable => {
            "agentspace's certificate authority is not usable. Nothing was changed; the operator's \
             ca command says why."
        }
    }
}

fn ca_failure(f: CaFailure) -> CredFailure {
    fail(ca_failure_message(f))
}

fn agentspace_dir(home: &Path) -> PathBuf {
    home.join(".agentspace")
}

fn lease_dir(home: &Path, id: &str) -> PathBuf {
    agentspace_dir(home).join("leases").join(id)
}

/// A lease ID is used as a folder's name, so it MUST be one path component of plain characters.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A lease folder, or a file in it, in its home-relative form.
fn relative(id: &str, name: Option<&str>) -> String {
    match name {
        Some(name) => format!("~/.agentspace/leases/{id}/{name}"),
        None => format!("~/.agentspace/leases/{id}"),
    }
}

fn now_unix() -> Option<i64> {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(seconds).ok()
}

/// All five of state.md's checks of the CA, in order, the first failure ending the check. On
/// success it returns the CA's private key, as it was read for the check.
fn load_ca(home: &Path) -> Result<Vec<u8>, CaFailure> {
    let dir = agentspace_dir(home).join("ca");
    let private_path = dir.join("agentspace_ca");
    let public_path = dir.join("agentspace_ca.pub");

    // Check 1: both files exist.
    match (private_path.try_exists(), public_path.try_exists()) {
        (Ok(true), Ok(true)) => {}
        (Ok(false), _) | (_, Ok(false)) => return Err(CaFailure::Missing),
        _ => return Err(CaFailure::NotUsable),
    }
    // Check 2: the folder, then the private key, are owner-only.
    if !platform::owner_only(&dir) || !platform::owner_only(&private_path) {
        return Err(CaFailure::NotUsable);
    }
    // Checks 3 to 5.
    let private = fs::read(&private_path).map_err(|_| CaFailure::NotUsable)?;
    let public = fs::read(&public_path).map_err(|_| CaFailure::NotUsable)?;
    check_ca_keys(&private, &public).map_err(|_| CaFailure::NotUsable)?;
    Ok(private)
}

/// Checks the CA by state.md's five checks, and no others: both files exist; the folder, then the
/// private key, are owner-only; the private key loads with an empty passphrase; the public key
/// matches it and is `ssh-ed25519`; and the public key parses.
///
/// It only reads, and never changes the CA (D17). `home` is the home folder, `USERPROFILE`.
pub fn check_ca(home: &Path) -> Result<(), CaFailure> {
    load_ca(home).map(|_| ())
}

/// What an `issue` has made so far, so that a failure can remove exactly that.
struct Made {
    id: String,
    dir: PathBuf,
    folder: bool,
    files: Vec<(PathBuf, String)>,
}

impl Made {
    /// Removes every file this call made, newest first, and then the folder if this call made it.
    /// Returns what could not be removed, home-relative.
    fn undo(self) -> Vec<String> {
        let mut leftovers = Vec::new();
        for (path, name) in self.files.iter().rev() {
            if !file_gone(path) {
                leftovers.push(relative(&self.id, Some(name)));
            }
        }
        // The folder is named only when nothing inside it explains why it stays.
        if self.folder && leftovers.is_empty() && !folder_gone(&self.dir) {
            leftovers.push(relative(&self.id, None));
        }
        leftovers
    }
}

/// Removes a file, and says whether it is gone: a file that was never there is gone.
fn file_gone(path: &Path) -> bool {
    match fs::remove_file(path) {
        Ok(()) => true,
        Err(e) => e.kind() == ErrorKind::NotFound,
    }
}

/// Removes an empty folder, and says whether it is gone.
fn folder_gone(path: &Path) -> bool {
    match fs::remove_dir(path) {
        Ok(()) => true,
        Err(e) => e.kind() == ErrorKind::NotFound,
    }
}

/// Makes a new file, never replacing one, and notes it in `made` once it exists.
fn write_new(made: &mut Made, name: &str, bytes: &[u8]) -> Result<(), ()> {
    let path = made.dir.join(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|_| ())?;
    made.files.push((path, name.to_string()));
    file.write_all(bytes).map_err(|_| ())?;
    file.flush().map_err(|_| ())
}

/// A line of a key file or a certificate file ends in one LF.
fn with_line_ending(line: &str) -> Vec<u8> {
    let mut bytes = line.as_bytes().to_vec();
    bytes.push(b'\n');
    bytes
}

const ISSUE_FAILED: &str =
    "The lease's credential could not be issued. Nothing was recorded; try again.";
const ISSUE_FAILED_LEFT: &str = "The lease's credential could not be issued, and some of the files it made could not be deleted. Nothing was recorded.";
const KEY_EXISTS: &str =
    "The lease already has a key, so a new credential cannot be issued. Nothing was changed.";
const KEY_UNPROTECTED: &str = "The lease's private key could not be kept from other users, so no certificate was issued. Nothing was recorded.";
const BAD_ID: &str = "The lease's ID cannot be used to name its folder. Nothing was changed.";

/// The steps of `issue` after the folder exists: the key, its rules, the public key, signing and
/// the certificate. A failure carries its message.
fn build(
    made: &mut Made,
    ca_private: &[u8],
    user: &str,
    ttl_seconds: u32,
) -> Result<i64, &'static str> {
    let id = made.id.clone();
    let key = new_lease_key(&id).map_err(|_| ISSUE_FAILED)?;

    // The private key's file is made with its rules in place before its bytes are written.
    let key_path = made.dir.join(KEY_FILE);
    let mut file = match platform::create_private_file(&key_path) {
        Ok(file) => file,
        Err(CreateError::NotCreated) => return Err(KEY_UNPROTECTED),
        Err(CreateError::CreatedButFailed) => {
            made.files.push((key_path, KEY_FILE.to_string()));
            return Err(KEY_UNPROTECTED);
        }
    };
    made.files.push((key_path.clone(), KEY_FILE.to_string()));
    file.write_all(key.private_openssh.as_bytes())
        .and_then(|()| file.flush())
        .map_err(|_| ISSUE_FAILED)?;
    drop(file);

    // Checked before signing: the key's rules, and that it loads with an empty passphrase.
    if !platform::verify_private_key(&key_path) {
        return Err(KEY_UNPROTECTED);
    }
    let on_disk = fs::read(&key_path).map_err(|_| ISSUE_FAILED)?;
    check_unencrypted(&on_disk).map_err(|_| ISSUE_FAILED)?;

    let public_bytes = with_line_ending(&key.public_line);
    write_new(made, PUB_FILE, &public_bytes).map_err(|_| ISSUE_FAILED)?;

    let issue_unix = now_unix().ok_or(ISSUE_FAILED)?;
    let line = sign_certificate(
        ca_private,
        &public_bytes,
        &id,
        user,
        issue_unix,
        ttl_seconds,
    )
    .map_err(|_| ISSUE_FAILED)?;
    write_new(made, CERT_FILE, &with_line_ending(&line)).map_err(|_| ISSUE_FAILED)?;
    issue_unix
        .checked_add(i64::from(ttl_seconds))
        .ok_or(ISSUE_FAILED)
}

/// Issues a lease's credential: steps 1 to 4 of state.md's "Creation, in order".
///
/// 1. Checks the CA, and fails if it is not usable.
/// 2. Refuses a lease folder that holds `id_ed25519` or `id_ed25519-cert.pub`, changing nothing,
///    and creates the folder when it is missing.
/// 3. Makes the key pair, with the private key's rules in place before its bytes are written, and
///    checks the rules and that it loads with an empty passphrase.
/// 4. Signs the certificate, and writes the public key and the certificate, each as one line
///    ending in LF.
///
/// On any failure every file this call made is removed, and the folder if this call made it; what
/// could not be removed is in `leftovers`. `user` is the certificate's one principal, and
/// `ttl_seconds` its lifetime. The caller holds the lease state lock.
pub fn issue(home: &Path, id: &str, user: &str, ttl_seconds: u32) -> Result<Issued, CredFailure> {
    let ca_private = load_ca(home).map_err(ca_failure)?;
    if !valid_id(id) {
        return Err(fail(BAD_ID));
    }
    let dir = lease_dir(home, id);

    // Step 2: refuse an existing key, then use or create the folder.
    for name in [KEY_FILE, CERT_FILE] {
        match fs::symlink_metadata(dir.join(name)) {
            Ok(_) => return Err(fail(KEY_EXISTS)),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(_) => return Err(fail(ISSUE_FAILED)),
        }
    }
    let folder = match fs::metadata(&dir) {
        Ok(meta) if meta.is_dir() => false,
        Ok(_) => return Err(fail(ISSUE_FAILED)),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            if let Some(parent) = dir.parent() {
                fs::create_dir_all(parent).map_err(|_| fail(ISSUE_FAILED))?;
            }
            match fs::create_dir(&dir) {
                Ok(()) => true,
                Err(e) if e.kind() == ErrorKind::AlreadyExists && dir.is_dir() => false,
                Err(_) => return Err(fail(ISSUE_FAILED)),
            }
        }
        Err(_) => return Err(fail(ISSUE_FAILED)),
    };

    let mut made = Made {
        id: id.to_string(),
        dir,
        folder,
        files: Vec::new(),
    };
    match build(&mut made, &ca_private, user, ttl_seconds) {
        Ok(expires_at) => Ok(Issued { expires_at }),
        Err(message) => {
            let leftovers = made.undo();
            let message = if leftovers.is_empty() || message != ISSUE_FAILED {
                message
            } else {
                ISSUE_FAILED_LEFT
            };
            Err(CredFailure {
                message: message.to_string(),
                leftovers,
            })
        }
    }
}

const RENEW_FAILED: &str =
    "The lease's certificate could not be renewed. Nothing was changed; try again.";
const RENEW_FAILED_LEFT: &str = "The lease's certificate could not be renewed, and some temporary files could not be deleted. Nothing was changed.";
const KEY_MISSING: &str =
    "The lease's key could not be found, so no certificate can be issued. Nothing was changed.";
const ROLLBACK_FAILED: &str = "The lease's previous certificate could not be put back. Some files of the failed renewal were left.";

/// What a successful step 5 of renewal left to settle.
struct RenewState {
    id: String,
    dir: PathBuf,
    /// The renewal's temporary public key, `renew-<hex>.pub`.
    public_name: String,
    /// The copy of the previous certificate, `renew-<hex>-previous-cert.pub`, when there was one.
    previous_name: Option<String>,
}

impl RenewState {
    /// Deletes the renewal's temporary files, and returns what could not be deleted.
    fn delete_temporaries(&self) -> Vec<String> {
        let mut leftovers = Vec::new();
        let names = std::iter::once(&self.public_name).chain(self.previous_name.iter());
        for name in names {
            if !file_gone(&self.dir.join(name)) {
                leftovers.push(relative(&self.id, Some(name)));
            }
        }
        leftovers
    }

    /// Steps 7 and 8 for a failed renewal: puts the previous certificate back byte for byte, or
    /// deletes the new one when there was none, then deletes the temporary files.
    fn roll_back(&self) -> Result<(), CredFailure> {
        let cert = self.dir.join(CERT_FILE);
        let restored = match &self.previous_name {
            Some(name) => fs::rename(self.dir.join(name), &cert).is_ok(),
            None => file_gone(&cert),
        };
        // A previous copy that could not be moved back is kept: it holds the old certificate.
        let leftovers = if restored {
            self.delete_temporaries()
        } else {
            let mut left = vec![relative(&self.id, Some(CERT_FILE))];
            if !file_gone(&self.dir.join(&self.public_name)) {
                left.push(relative(&self.id, Some(&self.public_name)));
            }
            if let Some(name) = &self.previous_name {
                left.push(relative(&self.id, Some(name)));
            }
            left
        };
        if restored && leftovers.is_empty() {
            Ok(())
        } else {
            Err(CredFailure {
                message: ROLLBACK_FAILED.to_string(),
                leftovers,
            })
        }
    }
}

/// A renewal whose new certificate is in place (steps 1 to 5 of state.md's "Renewal"), waiting for
/// the caller to write the record (step 6) and then settle it: [`RenewGuard::commit`] when the
/// record was written, [`RenewGuard::rollback`] when it was not.
///
/// A guard dropped without either rolls back, so that an early return cannot leave a new
/// certificate under an old record. A process killed first leaves the renewal as it was when it
/// died, and the next `renew` or `release` settles it (state.md, "Renewal").
pub struct RenewGuard {
    expires_at: i64,
    state: Option<RenewState>,
}

impl RenewGuard {
    /// The new certificate's expiry, a Unix time in seconds, to record as the lease's `expires_at`.
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }

    /// Step 8 for a renewal that succeeded: deletes the renewal's temporary files. A file that
    /// cannot be deleted is left, since the renewal itself is done; `release` removes it with the
    /// folder.
    pub fn commit(mut self) {
        if let Some(state) = self.state.take() {
            let _ = state.delete_temporaries();
        }
    }

    /// Steps 7 and 8 for a renewal that failed after step 5: puts the previous certificate back
    /// byte for byte, or deletes the new one when there was none, and deletes the temporary files.
    /// A failure names what it could not settle.
    pub fn rollback(mut self) -> Result<(), CredFailure> {
        match self.state.take() {
            Some(state) => state.roll_back(),
            None => Ok(()),
        }
    }
}

impl Drop for RenewGuard {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            let _ = state.roll_back();
        }
    }
}

impl std::fmt::Debug for RenewGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenewGuard")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Thirty-two lowercase hex digits, new for each renewal.
fn new_hex() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Renews a lease's certificate: steps 1 to 5 of state.md's "Renewal". It changes only the
/// certificate, and returns a guard for steps 6 to 8.
///
/// 1. Checks the CA, and fails, changing nothing, if it is not usable.
/// 2. Requires `id_ed25519` and `id_ed25519.pub` in the lease folder; a missing certificate is not
///    an error.
/// 3. Signs a new certificate under a temporary name: a copy of the public key as
///    `renew-<hex>.pub`, and the certificate as `renew-<hex>-cert.pub`.
/// 4. Keeps the previous certificate, if there is one, as `renew-<hex>-previous-cert.pub`.
/// 5. Moves the new certificate over `id_ed25519-cert.pub` in one rename.
///
/// On a failure in steps 3 to 5 the temporary files are removed, what could not be removed is in
/// `leftovers`, and the certificate is as it was. The caller holds the lease state lock.
pub fn renew(
    home: &Path,
    id: &str,
    user: &str,
    ttl_seconds: u32,
) -> Result<RenewGuard, CredFailure> {
    let ca_private = load_ca(home).map_err(ca_failure)?;
    if !valid_id(id) {
        return Err(fail(BAD_ID));
    }
    let dir = lease_dir(home, id);
    let is_file = |name: &str| fs::metadata(dir.join(name)).is_ok_and(|m| m.is_file());
    if !is_file(KEY_FILE) || !is_file(PUB_FILE) {
        return Err(fail(KEY_MISSING));
    }
    let has_previous = fs::symlink_metadata(dir.join(CERT_FILE)).is_ok();

    let hex = new_hex();
    let public_name = format!("renew-{hex}.pub");
    let new_cert_name = format!("renew-{hex}-cert.pub");
    let previous_name = format!("renew-{hex}-previous-cert.pub");
    let mut made: Vec<String> = Vec::new();

    let steps = (|| -> Result<i64, ()> {
        // Step 3: a copy of the public key under a temporary name, signed there.
        let public_bytes = fs::read(dir.join(PUB_FILE)).map_err(|_| ())?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(&public_name))
            .map_err(|_| ())?;
        made.push(public_name.clone());
        file.write_all(&public_bytes)
            .and_then(|()| file.flush())
            .map_err(|_| ())?;
        drop(file);

        let issue_unix = now_unix().ok_or(())?;
        let line = sign_certificate(
            &ca_private,
            &public_bytes,
            id,
            user,
            issue_unix,
            ttl_seconds,
        )
        .map_err(|_| ())?;
        let expires_at = issue_unix.checked_add(i64::from(ttl_seconds)).ok_or(())?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(&new_cert_name))
            .map_err(|_| ())?;
        made.push(new_cert_name.clone());
        file.write_all(&with_line_ending(&line))
            .and_then(|()| file.flush())
            .map_err(|_| ())?;
        drop(file);

        // Step 4: the previous certificate, byte for byte.
        if has_previous {
            let copy = dir.join(&previous_name);
            // Noted before the copy, since a copy that fails part way leaves a part file.
            made.push(previous_name.clone());
            fs::copy(dir.join(CERT_FILE), copy).map_err(|_| ())?;
        }
        // Step 5: one rename, so ssh finds the old certificate or the new one.
        fs::rename(dir.join(&new_cert_name), dir.join(CERT_FILE)).map_err(|_| ())?;
        Ok(expires_at)
    })();

    match steps {
        Ok(expires_at) => Ok(RenewGuard {
            expires_at,
            state: Some(RenewState {
                id: id.to_string(),
                dir,
                public_name,
                previous_name: has_previous.then_some(previous_name),
            }),
        }),
        Err(()) => {
            let mut leftovers = Vec::new();
            for name in made.iter().rev() {
                if !file_gone(&dir.join(name)) {
                    leftovers.push(relative(id, Some(name)));
                }
            }
            let message = if leftovers.is_empty() {
                RENEW_FAILED
            } else {
                RENEW_FAILED_LEFT
            };
            Err(CredFailure {
                message: message.to_string(),
                leftovers,
            })
        }
    }
}

/// Deletes everything under `path`, adding what could not be deleted to `leftovers`, home-relative
/// from `shown`. A folder is named only when nothing inside it explains why it stays.
fn remove_tree(path: &Path, shown: &str, leftovers: &mut Vec<String>) {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return,
        Err(_) => {
            leftovers.push(shown.to_string());
            return;
        }
    };
    let before = leftovers.len();
    for entry in entries {
        let Ok(entry) = entry else {
            leftovers.push(shown.to_string());
            return;
        };
        let child = entry.path();
        let child_shown = format!("{shown}/{}", entry.file_name().to_string_lossy());
        let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        if is_dir {
            remove_tree(&child, &child_shown, leftovers);
        } else if !file_gone(&child) {
            leftovers.push(child_shown);
        }
    }
    if leftovers.len() == before && !folder_gone(path) {
        leftovers.push(shown.to_string());
    }
}

/// Deletes a lease's folder with everything in it, a killed renewal's temporary files included. A
/// missing folder is not a failure. What could not be deleted is named in `leftovers`,
/// home-relative; a retry finishes the job (state.md, "Release, and what it leaves").
pub fn remove_lease_folder(home: &Path, id: &str) -> Result<(), CredFailure> {
    if !valid_id(id) {
        return Err(fail(BAD_ID));
    }
    let dir = lease_dir(home, id);
    match fs::symlink_metadata(&dir) {
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(_) => {
            return Err(CredFailure {
                message: DELETE_FAILED.to_string(),
                leftovers: vec![relative(id, None)],
            });
        }
    }
    let mut leftovers = Vec::new();
    remove_tree(&dir, &relative(id, None), &mut leftovers);
    if leftovers.is_empty() {
        Ok(())
    } else {
        Err(CredFailure {
            message: DELETE_FAILED.to_string(),
            leftovers,
        })
    }
}

const DELETE_FAILED: &str = "The lease's local key or certificate could not be deleted.";

#[cfg(all(test, windows))]
mod tests;
