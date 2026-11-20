//! agentspace-ca: the operator's `ca` command and `sync`'s CA check (docs/operator.md, ca, and
//! sync step 1; docs/state.md, The CA; D7, D17, D21, D37, D42).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! # Public API
//!
//! - [`ca`] is the operator's `ca`: steps 1 to 6 of operator.md, in order. It creates the CA when
//!   neither of its files exists, and on every later run checks it and changes nothing.
//! - [`check_for_sync`] is `sync`'s step 1: `ca`'s steps 1, 2, 3 and 5, in that order, each with
//!   `ca`'s own message, and never step 4, so it never creates a CA (gaps FA-14, FB-12). It
//!   returns the public key's fingerprint, `SHA256:<base64>`, for `sync`'s step 4 line.
//!
//! # Rules
//!
//! - **Every message MUST be operator.md's, exactly**, with `<problems>` in its order and the
//!   placeholders in their forms: `<user>` is the value of `USERNAME`; `<owner>` and `<who>` are
//!   `<domain>\<name>`, or the SID when Windows cannot name it.
//! - **`ca` MUST NOT overwrite, repair or loosen a CA, and MUST NOT change the access rules of an
//!   existing one.** It sets rules only on the folder and the key it is creating, the folder before
//!   any key exists in it.
//! - **The key pair MUST be ed25519 with no passphrase, made in code**, and every access rule set
//!   and read through the Windows API, never by starting `ssh-keygen` or `icacls` (D42, FB-15).
//!   `ssh-keygen.exe` is only looked for, as step 1 says.
//! - **Paths come from the [`Context`]**: the home is `ctx.home` (`USERPROFILE`), and
//!   `ssh-keygen.exe` is under `ctx.system_root`. The library writes nothing to stdout or stderr
//!   but through [`Say`], and MUST NOT panic on bad input.
//! - **The library MUST NOT read any environment variable.** `USERNAME`, `COMPUTERNAME` and the
//!   three OneDrive values reach it in a [`CaEnv`], which the operator's program fills from its
//!   own environment beside the `Context` (operator.md, ca, Rulings for the rewrite's ca:
//!   architect, 2026-11-19, for E0.06-T02-S01). A test, tests/no_environment.rs, scans src/ for a
//!   read.
//! - **`unsafe` MUST NOT appear outside `win.rs`**, the one module that calls Windows. On any
//!   other platform (`other.rs`, not the contract's) every access rule fails closed, so `ca` and
//!   `check_for_sync` fail there; the crate builds and its portable half is tested on Linux.

#![deny(unsafe_code)]

mod access;
mod keys;
mod place;

#[cfg(windows)]
#[allow(unsafe_code)]
#[path = "win.rs"]
mod platform;

#[cfg(not(windows))]
#[path = "other.rs"]
mod platform;

#[cfg(test)]
mod tests;

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use agentspace_operator_api::{CallerFailure, Context, Failure, Say};

use access::Rules;
use keys::KeyFailure;

/// What `ca` asks of the platform: the user, accounts' names, elevation, and the access rules.
/// The real one is `platform`'s; the tests' is a fake, so that every step can be reached.
pub(crate) trait Host {
    /// The user SID of the process's token, `S-1-...`, or the system's error text.
    fn user_sid(&self) -> Result<String, String>;
    /// An account as Windows names it, `<domain>\<name>` (or `<name>` alone when it has no
    /// domain), or `None` when Windows cannot translate the SID.
    fn account_name(&self, sid: &str) -> Option<String>;
    /// The token user's account name, with no domain: `<user>` when `USERNAME` is unset.
    fn user_name(&self) -> Option<String>;
    /// Whether the process is a member of Administrators with an elevated token.
    fn elevated(&self) -> Result<bool, String>;
    /// A file's or a folder's owner and access control list.
    fn rules(&self, path: &Path) -> Result<Rules, String>;
    /// Locks a folder to `user_sid` alone, as operator.md's lock of the CA's folder does.
    fn lock_folder(&self, path: &Path, user_sid: &str) -> Result<(), String>;
    /// Locks a new key to `user_sid` alone: one explicit entry, full control, protected.
    fn lock_key(&self, path: &Path, user_sid: &str) -> Result<(), String>;
    /// Today's local date, `YYYY-MM-DD`.
    fn local_date(&self) -> String;
}

/// The five values of the operator's environment that `ca` needs and the pinned [`Context`] does
/// not carry (operator.md, ca, "Rulings for the rewrite's `ca`": architect, 2026-11-19, for
/// E0.06-T02-S01).
///
/// **agentspace-ca MUST NOT read any environment variable itself.** The operator's program fills
/// this struct from its own environment as it starts, beside the `Context`, and passes it to
/// [`ca`] and [`check_for_sync`]. A test builds it directly.
///
/// Each field is the variable's value as it was set. `None` and an empty value both mean unset.
///
/// Its `Debug` says only which fields are set, never their values: the user's and the computer's
/// names identify this machine (AGENTS.md, Terry's real files in a review).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct CaEnv {
    /// `USERNAME`: `<user>` in every message.
    pub user_name: Option<String>,
    /// `COMPUTERNAME`: the computer's name in the new public key's comment.
    pub computer_name: Option<String>,
    /// `OneDrive`: a synced folder the CA's folder MUST NOT be inside (step 2).
    pub one_drive: Option<OsString>,
    /// `OneDriveConsumer`: as `one_drive`.
    pub one_drive_consumer: Option<OsString>,
    /// `OneDriveCommercial`: as `one_drive`.
    pub one_drive_commercial: Option<OsString>,
}

impl CaEnv {
    /// The struct filled by `lookup`, which gives a variable's value by its name: `USERNAME`,
    /// `COMPUTERNAME`, `OneDrive`, `OneDriveConsumer` and `OneDriveCommercial`.
    ///
    /// A pure helper for the operator's program, which passes a closure over its own environment.
    /// It MUST NOT be given one inside this crate, which reads no environment.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<OsString>) -> CaEnv {
        let text = |name: &str| lookup(name).map(|value| value.to_string_lossy().into_owned());
        CaEnv {
            user_name: text("USERNAME"),
            computer_name: text("COMPUTERNAME"),
            one_drive: lookup("OneDrive"),
            one_drive_consumer: lookup("OneDriveConsumer"),
            one_drive_commercial: lookup("OneDriveCommercial"),
        }
    }

    /// `USERNAME`, when it is set and not empty.
    fn user(&self) -> Option<&str> {
        self.user_name.as_deref().filter(|name| !name.is_empty())
    }

    /// `COMPUTERNAME`, when it is set and not empty.
    fn computer(&self) -> Option<&str> {
        self.computer_name
            .as_deref()
            .filter(|name| !name.is_empty())
    }

    /// The three OneDrive values, in operator.md's order, each one that is set and not empty.
    fn one_drive_folders(&self) -> Vec<OsString> {
        [
            &self.one_drive,
            &self.one_drive_consumer,
            &self.one_drive_commercial,
        ]
        .into_iter()
        .flatten()
        .filter(|value| !value.is_empty())
        .cloned()
        .collect()
    }
}

impl std::fmt::Debug for CaEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let set = |is_set: bool| if is_set { "set" } else { "unset" };
        f.debug_struct("CaEnv")
            .field("user_name", &set(self.user().is_some()))
            .field("computer_name", &set(self.computer().is_some()))
            .field(
                "one_drive",
                &set(self.one_drive.as_ref().is_some_and(|v| !v.is_empty())),
            )
            .field(
                "one_drive_consumer",
                &set(self
                    .one_drive_consumer
                    .as_ref()
                    .is_some_and(|v| !v.is_empty())),
            )
            .field(
                "one_drive_commercial",
                &set(self
                    .one_drive_commercial
                    .as_ref()
                    .is_some_and(|v| !v.is_empty())),
            )
            .finish()
    }
}

/// Runs the operator's `ca` (operator.md, ca): steps 1 to 6, in order, printing through `say`.
///
/// It creates the CA in `%USERPROFILE%\.agentspace\ca` when neither of its files exists, and
/// otherwise checks it and changes nothing. A failure is the one `agentspace: ` line operator.md
/// gives for it. `env` carries the operator's `USERNAME`, `COMPUTERNAME` and OneDrive values.
pub fn ca(ctx: &Context, env: &CaEnv, say: &dyn Say) -> Result<(), Failure> {
    run(ctx, env, &platform_host(), Some(say)).map(|_| ())
}

/// `sync`'s step 1 (operator.md, sync): `ca`'s steps 1, 2, 3 and 5, in that order, each with
/// `ca`'s own message, and never step 4. It never creates or changes a CA, and prints nothing.
///
/// Returns the public key's fingerprint, `SHA256:<base64>`, as `sync` prints it.
pub fn check_for_sync(ctx: &Context, env: &CaEnv) -> Result<String, Failure> {
    run(ctx, env, &platform_host(), None)
}

#[cfg(windows)]
fn platform_host() -> platform::Windows {
    platform::Windows
}

#[cfg(not(windows))]
fn platform_host() -> platform::Unsupported {
    platform::Unsupported
}

/// The CA's three paths (state.md, The CA, Its files).
pub(crate) struct CaPaths {
    pub(crate) folder: PathBuf,
    pub(crate) key: PathBuf,
    pub(crate) public: PathBuf,
}

impl CaPaths {
    /// The paths under `home`, made full against the working folder when `home` is relative
    /// (state.md, The home folder).
    pub(crate) fn under(home: &Path) -> CaPaths {
        let home = std::path::absolute(home).unwrap_or_else(|_| home.to_path_buf());
        let folder = home.join(".agentspace").join("ca");
        CaPaths {
            key: folder.join("agentspace_ca"),
            public: folder.join("agentspace_ca.pub"),
            folder,
        }
    }
}

/// A failure of a person's command: its message, and the same line in caller mode.
fn fail(message: String) -> Failure {
    Failure {
        message,
        caller: CallerFailure::Same,
    }
}

/// Whether something is at `path` that `ca` takes for the file: a file, or an entry it cannot
/// look at (which steps 3 and 5 then report rather than build over). A missing entry, a folder,
/// or a path through a file is not one.
fn present(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(meta) => meta.is_file(),
        Err(e) => !matches!(
            e.kind(),
            io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
        ),
    }
}

/// An I/O error's text, trimmed.
fn io_text(e: &io::Error) -> String {
    e.to_string().trim().to_string()
}

/// `ca` (with `say`) or `sync`'s check (without it). Returns the fingerprint.
pub(crate) fn run(
    ctx: &Context,
    env: &CaEnv,
    host: &dyn Host,
    say: Option<&dyn Say>,
) -> Result<String, Failure> {
    // Step 1: ssh-keygen.exe, by full path from SystemRoot (A-24).
    let keygen = ctx
        .system_root
        .join("System32")
        .join("OpenSSH")
        .join("ssh-keygen.exe");
    // "Missing" is anything at that path that is not a file, a folder included; the file is never
    // run (operator.md, ca, Rulings for the rewrite's ca: architect, 2026-11-19, for
    // E0.06-T02-S01).
    if !keygen.is_file() {
        return Err(Failure {
            message: format!(
                "agentspace: {} is missing; install the Windows OpenSSH client.",
                keygen.display()
            ),
            caller: CallerFailure::CouldNotReach,
        });
    }

    let paths = CaPaths::under(&ctx.home);
    let folder = paths.folder.display().to_string();
    let key = paths.key.display().to_string();
    let public = paths.public.display().to_string();

    // Step 2: not inside a OneDrive folder, nor inside any git working tree.
    let inside = place::inside_one_drive(&paths.folder, &env.one_drive_folders())
        .or_else(|| place::inside_git(&paths.folder));
    if let Some(inside) = inside {
        return Err(fail(format!(
            "agentspace: {folder} is inside {}; the CA key MUST NOT live in a synced folder or in the repo.",
            inside.folder.display()
        )));
    }

    // Step 3: a half-made CA, never repaired.
    let (has_key, has_public) = (present(&paths.key), present(&paths.public));
    if has_key && !has_public {
        return Err(fail(format!(
            "agentspace: {key} exists but {public} does not. ca does not repair a half-made CA: restore the public key, or write it again from the private key with ssh-keygen -y, then run ca again."
        )));
    }
    if has_public && !has_key {
        return Err(fail(format!(
            "agentspace: {public} exists but {key} does not: the CA's private key is lost, and nothing more can be signed with it. ca does not replace it on its own. To make a new CA, delete {public} and run ca again, then run sync for every host and re-issue every lease's certificate."
        )));
    }

    // <user> is USERNAME; with USERNAME unset or empty, the token user's account name, with no
    // domain, and failing that "the current user" (operator.md, ca, Rulings for the rewrite's ca:
    // architect, 2026-11-19, for E0.06-T02-S01).
    let user = env
        .user()
        .map(str::to_string)
        .or_else(|| host.user_name())
        .unwrap_or_else(|| "the current user".to_string());
    let user_sid = host.user_sid();

    // Step 4, `ca` alone: neither file exists, so it creates the CA.
    let mut created = false;
    if let Some(say) = say
        && !has_key
        && !has_public
    {
        create(env, host, say, &paths, &user, &user_sid)?;
        created = true;
    }

    // Step 5: the checks of state.md, The CA, Its checks, in order.
    let fingerprint = check(host, &paths, &user, &user_sid)?;

    // Step 6: the result, four lines.
    if let Some(say) = say {
        if created {
            say.progress("Created the agentspace SSH CA.", None);
        } else {
            say.progress(
                "The agentspace SSH CA already exists; nothing changed.",
                None,
            );
        }
        say.progress(&format!("Private key  {key}"), None);
        say.progress(&format!("Public key   {public}"), None);
        say.progress(&format!("Fingerprint  {fingerprint}"), None);
    }
    Ok(fingerprint)
}

/// `<problems>` for `path`, or `None` when it is owner-only. Rules that cannot be read are a
/// problem of their own.
fn problems_of(
    host: &dyn Host,
    path: &Path,
    user: &str,
    user_sid: &Result<String, String>,
) -> Option<String> {
    // Rules that cannot be read (the user's SID or the file's list) fail closed: the problem is
    // "could not be checked: <system error text>", with that check's message (operator.md, ca,
    // Rulings for the rewrite's ca: architect, 2026-11-19, for E0.06-T02-S01).
    let sid = match user_sid {
        Ok(sid) => sid,
        Err(e) => return Some(format!("could not be checked: {e}")),
    };
    match host.rules(path) {
        Ok(rules) => {
            let text = access::problems(&rules, sid, user, &|s| host.account_name(s));
            (!text.is_empty()).then_some(text)
        }
        Err(e) => Some(format!("could not be checked: {e}")),
    }
}

/// Step 4: creates the CA. Every failure is operator.md's message for its sub-step.
fn create(
    env: &CaEnv,
    host: &dyn Host,
    say: &dyn Say,
    paths: &CaPaths,
    user: &str,
    user_sid: &Result<String, String>,
) -> Result<(), Failure> {
    let folder = paths.folder.display().to_string();
    let key = paths.key.display().to_string();
    let public = paths.public.display().to_string();

    // An elevation that cannot be read fails closed, with the elevated refusal (operator.md, ca,
    // Rulings for the rewrite's ca: architect, 2026-11-19, for E0.06-T02-S01).
    if host.elevated().unwrap_or(true) {
        return Err(fail(
            "agentspace: ca MUST NOT create the CA from an elevated shell; run it without \"Run as administrator\".".to_string(),
        ));
    }

    say.progress(
        &format!("Creating the agentspace SSH CA in {folder}..."),
        None,
    );
    // A CA folder that cannot be created fails with "agentspace: Could not create <ca folder>:
    // <system error text>" (operator.md, ca, Rulings for the rewrite's ca: architect, 2026-11-19,
    // for E0.06-T02-S01).
    if let Err(e) = fs::create_dir_all(&paths.folder) {
        return Err(fail(format!(
            "agentspace: Could not create {folder}: {}",
            io_text(&e)
        )));
    }

    // The folder is locked, and checked, before any key exists in it.
    let locked = match user_sid {
        Ok(sid) => host.lock_folder(&paths.folder, sid),
        Err(e) => Err(e.clone()),
    };
    if let Err(e) = locked {
        return Err(fail(format!(
            "agentspace: icacls could not lock {folder} to {user}: {e}"
        )));
    }
    if let Some(problems) = problems_of(host, &paths.folder, user, user_sid) {
        return Err(fail(format!(
            "agentspace: {folder} MUST be {user}'s alone before the CA is made in it, but it {problems}. Remove the other rules, or delete the directory, and run ca again."
        )));
    }

    // The key pair: ed25519, no passphrase, made in code.
    let comment = match env
        .computer()
        .filter(|name| !name.chars().any(|c| c.is_whitespace() || c.is_control()))
    {
        Some(computer) => format!("agentspace-ca {computer} {}", host.local_date()),
        None => format!("agentspace-ca {}", host.local_date()),
    };
    let made = keys::new_ca(&comment).and_then(|pair| {
        write_pair(paths, &pair).map_err(|e| io_text(&e))?;
        Ok(())
    });
    if let Err(e) = made {
        return Err(fail(format!(
            "agentspace: ssh-keygen could not create the CA: {e}"
        )));
    }

    // The private key, locked once it exists.
    let locked = match user_sid {
        Ok(sid) => host.lock_key(&paths.key, sid),
        Err(e) => Err(e.clone()),
    };
    if let Err(e) = locked {
        return Err(fail(format!(
            "agentspace: icacls could not lock the new {key} to {user}: {e}. Delete {key} and {public}, and run ca again."
        )));
    }
    if let Some(problems) = problems_of(host, &paths.key, user, user_sid) {
        return Err(fail(format!(
            "agentspace: The new {key} MUST be {user}'s alone, but it {problems}. Treat it as exposed: delete {key} and {public}, and run ca again."
        )));
    }
    Ok(())
}

/// Writes a new file, never over an existing one. The flag says whether this call made the file,
/// so that only a file it made is removed after a failure.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), (io::Error, bool)> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| (e, false))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| (e, true))
}

/// Writes the private key, then the public key. On a failure it removes the files this call
/// made, so that a failed creation never leaves a half-made CA.
fn write_pair(paths: &CaPaths, pair: &keys::NewCa) -> io::Result<()> {
    if let Err((e, made)) = write_new(&paths.key, pair.private.as_bytes()) {
        if made {
            let _ = fs::remove_file(&paths.key);
        }
        return Err(e);
    }
    if let Err((e, made)) = write_new(&paths.public, pair.public.as_bytes()) {
        if made {
            let _ = fs::remove_file(&paths.public);
        }
        let _ = fs::remove_file(&paths.key);
        return Err(e);
    }
    Ok(())
}

/// Step 5: state.md's checks 1 to 5, in order, each with `ca`'s message. Returns the fingerprint.
fn check(
    host: &dyn Host,
    paths: &CaPaths,
    user: &str,
    user_sid: &Result<String, String>,
) -> Result<String, Failure> {
    let folder = paths.folder.display().to_string();
    let key = paths.key.display().to_string();
    let public = paths.public.display().to_string();

    // Check 1: both files exist.
    if !present(&paths.key) {
        return Err(fail(format!("agentspace: {key} does not exist; run ca.")));
    }
    if !present(&paths.public) {
        return Err(fail(format!(
            "agentspace: {public} does not exist; run ca."
        )));
    }

    // Check 2: the folder, then the private key, are owner-only.
    for path in [&paths.folder, &paths.key] {
        if let Some(problems) = problems_of(host, path, user, user_sid) {
            return Err(fail(format!(
                "agentspace: {} MUST be {user}'s alone, but it {problems}. ca does not change access rules on an existing CA. If anyone else may have read the key, replace the CA (D17). Otherwise lock it by hand: icacls \"{folder}\" /inheritance:r /grant:r \"{user}:(OI)(CI)F\", then icacls \"{key}\" /reset, then icacls \"{key}\" /inheritance:d, then remove any other rule icacls still lists, and run ca again.",
                path.display()
            )));
        }
    }

    // Checks 3 to 5, on the files' bytes. A private key that cannot be read does not load
    // (check 3); a public key that cannot be read has no fingerprint (check 5).
    let read_key_failed = || {
        fail(format!(
            "agentspace: ssh-keygen could not read {key} without a passphrase; the lease CLI signs unattended (D17)."
        ))
    };
    let not_its_own = || {
        fail(format!(
            "agentspace: {public} is not the ed25519 public key of {key}. ca does not repair a mismatched pair."
        ))
    };
    let no_fingerprint = |why: &str| {
        fail(format!(
            "agentspace: ssh-keygen could not read {public}: {why}"
        ))
    };
    let private = fs::read(&paths.key).map_err(|_| read_key_failed())?;
    let public_bytes = match fs::read(&paths.public) {
        Ok(bytes) => bytes,
        // The checks keep their order when the public key cannot be read: the private key's own
        // failure (check 3, or check 4 for a key that is not ed25519) comes before check 5's.
        Err(e) => {
            return Err(match keys::check_pair(&private, b"") {
                Err(KeyFailure::Passphrase) => read_key_failed(),
                Err(KeyFailure::NotItsOwn) => not_its_own(),
                _ => no_fingerprint(&io_text(&e)),
            });
        }
    };
    keys::check_pair(&private, &public_bytes).map_err(|failure| match failure {
        KeyFailure::Passphrase => read_key_failed(),
        KeyFailure::NotItsOwn => not_its_own(),
        KeyFailure::Fingerprint(why) => no_fingerprint(&why),
    })
}

/// A UTC date, `YYYY-MM-DD`: the local date where the platform has none of its own.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn utc_date(now: std::time::SystemTime) -> String {
    let seconds = now
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    // Howard Hinnant's days-to-civil algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}
