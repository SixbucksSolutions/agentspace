//! The token push, its portable half (docs/operator.md, The token push; D42).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! Everything about the push that is not a Windows call: when it runs and when it is due, what
//! the laptop reads (the receipt, and the Bot credential down to its `SS` element's hex), the
//! content and the stamp, the ssh's arguments and remote command, what the laptop checks in the
//! box's answer, and every warning, through the pinned [`Say`]. What needs Windows, touches the
//! disk or starts a program is behind three small traits that v0.06-T02 implements: [`Files`],
//! [`Unprotect`] and [`SshRun`].
//!
//! # The public API
//!
//! - [`push_with`]: one push, start to end, never failing; returns its [`Outcome`].
//! - [`Seams`], holding the three traits and the zone: [`Files`] (with [`FileRead`], and
//!   [`StdFiles`], a portable `Files` that T02 MAY use or replace), [`Unprotect`] and [`SshRun`]
//!   (with [`SshResult`]).
//! - [`Token`] and [`Content`], the two types that hold the token; each redacts it in `Debug` and
//!   zeroes it when dropped.
//! - The steps, for a caller that needs one alone: [`receipt_path`], [`parse_receipt`] and
//!   [`Receipt`]; [`credential_path`], [`parse_credential_blob`] and [`read_token`];
//!   [`check_zone`], the zone's guard; [`content`]; [`stamp`], [`stamp_path`],
//!   [`parse_stamp_file`] and [`stamp_file_bytes`]; [`alias_is_valid`], [`remote_command`] and
//!   [`ssh_args`].
//! - The warnings' texts, as a person's command prints them: [`COULD_NOT_READ`], [`NO_ZONE`],
//!   [`TIMED_OUT`], [`NOT_CONFIRMED`], [`zone_not_taken`] and [`COULD_NOT_SEND`]; and the
//!   progress line [`SENDING`]. In caller mode, [`Say`] rewrites `the instance`.
//!
//! The zone is `agentspace-zone`'s: [`Seams::zone`] gives it already mapped and checked, and this
//! crate only guards it again before it goes on a command line.
//!
//! # Secrecy (operator.md, Secrecy)
//!
//! The token MUST travel only on the push's stdin. This crate passes it to nothing but
//! [`SshRun::run`]'s `stdin`, puts it in no argument, environment variable, file, message or
//! panic, and keeps it only in [`Token`] and [`Content`], which never print it. No warning quotes
//! the failure behind it: each is a fixed text. A [`SshRun`] MUST write `stdin` to the ssh's stdin
//! pipe and nowhere else, and an [`Unprotect`] MUST hand the plaintext to nothing but its return
//! value. The `Debug` of [`FileRead`] and [`SshResult`] gives byte counts, never the bytes, so
//! that no file's content and nothing the box printed reaches a log through them.

#![forbid(unsafe_code)]

mod boxside;
mod credential;
mod receipt;
mod stamp;
mod text;
mod zone;

pub use agentspace_operator_api::{Context, Say};
pub use boxside::{alias_is_valid, remote_command, ssh_args};
pub use credential::{parse_credential_blob, read_token};
pub use receipt::{Receipt, parse_receipt};
pub use stamp::{content, parse_stamp_file, stamp, stamp_file_bytes};
pub use zeroize::Zeroizing;
pub use zone::check_zone;

use boxside::Answer;
use std::ffi::OsString;
use std::fmt;
use std::path::{MAIN_SEPARATOR, Path, PathBuf};

/// The push's progress line. In caller mode it prints nothing.
pub const SENDING: &str = "Sending the localswim Bot token to the instance...";
/// The receipt or the credential could not be read or was refused, or the URL or the token is
/// missing, empty, or holds a CR, an LF or a NUL.
pub const COULD_NOT_READ: &str = "agentspace: Could not read the localswim Bot token; the board will not work on the box this session.";
/// A due push with the empty zone.
pub const NO_ZONE: &str =
    "agentspace: Could not work out the laptop's time zone; times on the box stay in its own zone.";
/// The ssh ran past the 20 seconds and its tree was stopped.
pub const TIMED_OUT: &str = "agentspace: Sending the localswim token took over 20 seconds and was stopped; the board will not work on the box this session.";
/// A non-zero exit, or no first hash line carrying the expected hash.
pub const NOT_CONFIRMED: &str = "agentspace: The instance did not confirm the localswim token; the board will not work on the box this session.";
/// Any other failure: an ssh that cannot be started, a stamp that cannot be written, or an alias
/// that breaks `-Alias`'s rule and still reached the push (architect, 2026-11-19, for
/// E0.06-T01-S04).
pub const COULD_NOT_SEND: &str = "agentspace: Could not send the localswim token; the board will not work on the box this session.";

/// The warning for a zone the box did not take.
pub fn zone_not_taken(zone: &str) -> String {
    format!(
        "agentspace: The instance did not take the time zone {zone}; times on the box stay in its own zone."
    )
}

/// The Bot token. Its `Debug` redacts it, it has no `Display`, and it is zeroed when dropped.
pub struct Token(pub(crate) Zeroizing<String>);

impl Token {
    /// The token's text, for [`content`] alone. It is not public: outside this crate a token can
    /// only be handed to [`content`], whose bytes go to the push's stdin and nowhere else.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

/// The content, the two lines that carry the URL and the token. Its `Debug` redacts it, and it is
/// zeroed when dropped.
pub struct Content(pub(crate) Zeroizing<Vec<u8>>);

impl Content {
    /// The content's bytes, for the push's stdin and nothing else.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for Content {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Content(<redacted>)")
    }
}

/// One whole-file read. Its `Debug` gives the bytes' count, never the bytes: a file read here is
/// the receipt or the credential, and no message or log may quote either (operator.md, Secrecy).
#[derive(Clone, PartialEq, Eq)]
pub enum FileRead {
    /// Nothing exists at the path, neither a file nor a folder.
    Missing,
    /// Something is there and cannot be read as a file: a folder, a file the user may not read,
    /// or one held without sharing read access.
    Unreadable,
    /// The file's bytes.
    Bytes(Vec<u8>),
}

impl fmt::Debug for FileRead {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FileRead::Missing => f.write_str("Missing"),
            FileRead::Unreadable => f.write_str("Unreadable"),
            FileRead::Bytes(bytes) => write!(f, "Bytes(<{} bytes>)", bytes.len()),
        }
    }
}

/// The laptop's files, as the push reads and writes them.
pub trait Files {
    /// Reads a whole file afresh. It MUST NOT keep what it read for a later push.
    fn read(&self, path: &Path) -> FileRead;
    /// Replaces the stamp file with `bytes` atomically (docs/state.md, The push stamp; Replacing
    /// a file), creating its folder if needed. `false` when it could not, which warns "Could not
    /// send".
    fn write_stamp(&self, path: &Path, bytes: &[u8]) -> bool;
}

/// The portable [`Files`], with std's own calls.
///
/// - `read` is `std::fs::read`, which on Windows opens the file sharing read, write and delete.
///   It gives `Missing` when nothing exists at the path (a folder on the way that is not a folder
///   included), and `Unreadable` for anything else that cannot be read as a file.
/// - `write_stamp` creates the folder if needed, writes the whole content to
///   `<file name>.<hex>.tmp` in the same folder, flushes it, and renames it over the file with
///   `std::fs::rename`, which on current Windows is the POSIX rename docs/state.md, Replacing a
///   file, asks for. The temporary file is removed when anything fails before the rename. It is
///   one attempt, with no retry.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdFiles;

impl Files for StdFiles {
    fn read(&self, path: &Path) -> FileRead {
        read_file(path)
    }

    fn write_stamp(&self, path: &Path, bytes: &[u8]) -> bool {
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            return false;
        };
        if !dir.as_os_str().is_empty() && std::fs::create_dir_all(dir).is_err() {
            return false;
        }
        let mut temp_name = name.to_os_string();
        temp_name.push(format!(".{}.tmp", unique_hex()));
        let temp = dir.join(temp_name);
        let written = (|| -> std::io::Result<()> {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
            drop(f);
            std::fs::rename(&temp, path)
        })();
        if written.is_err() {
            let _ = std::fs::remove_file(&temp);
            return false;
        }
        true
    }
}

/// Hex that no other temporary file of this process, and almost surely no other process's, has:
/// the process ID, the time and a counter. `create_new` refuses a name that exists anyway.
fn unique_hex() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let count = COUNT.fetch_add(1, Ordering::Relaxed);
    format!("{:x}{nanos:x}{count:x}", std::process::id())
}

fn read_file(path: &Path) -> FileRead {
    match std::fs::read(path) {
        Ok(bytes) => FileRead::Bytes(bytes),
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory => {
                // A dangling link is something at the path.
                if std::fs::symlink_metadata(path).is_ok() {
                    FileRead::Unreadable
                } else {
                    FileRead::Missing
                }
            }
            _ => FileRead::Unreadable,
        },
    }
}

/// The decryption of the credential's DPAPI blob (operator.md, The Bot credential):
/// `CryptUnprotectData` on Windows, which v0.06-T02 implements.
pub trait Unprotect {
    /// The plaintext, the token in UTF-16LE, or `None` when the call fails.
    fn unprotect(&self, blob: &[u8]) -> Option<Zeroizing<Vec<u8>>>;
}

/// How the push's ssh ended. Its `Debug` gives the stdout's byte count, never the bytes, so that
/// the output of a box that broke its own rule and printed the content reaches no log through it.
#[derive(Clone, PartialEq, Eq)]
pub enum SshResult {
    /// It could not be started at all.
    CouldNotStart,
    /// It ran past the 20 seconds, and it and every process descended from it were stopped.
    TimedOut,
    /// It exited on its own: its exit code, and its stdout (empty when the stdout did not end
    /// within 10 more seconds).
    Exited { code: i32, stdout: Vec<u8> },
}

impl fmt::Debug for SshResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SshResult::CouldNotStart => f.write_str("CouldNotStart"),
            SshResult::TimedOut => f.write_str("TimedOut"),
            SshResult::Exited { code, stdout } => write!(
                f,
                "Exited {{ code: {code}, stdout: <{} bytes> }}",
                stdout.len()
            ),
        }
    }
}

/// The push's one ssh (operator.md, What the laptop sends, steps 2 to 5), which v0.06-T02
/// implements.
pub trait SshRun {
    /// Starts ssh with `args`, exactly these and in this order, under the 20-second limit: a pipe
    /// for its stdin and one for its stdout, and its stderr the operator's. Writes `stdin` to it
    /// and closes it, inside the limit. `stdin` is the content, which holds the token: it MUST go
    /// nowhere else.
    fn run(&self, args: &[&str], stdin: &[u8]) -> SshResult;
}

/// What the push needs from outside this crate.
pub struct Seams<'a> {
    pub files: &'a dyn Files,
    pub unprotect: &'a dyn Unprotect,
    pub ssh: &'a dyn SshRun,
    /// The time zone (operator.md, The time zone), as `agentspace_zone::zone` gives it: the
    /// laptop's zone, or `AGENTSPACE_TIMEZONE`'s value in its place, already mapped and checked
    /// (steps 1 to 3); `None` for the empty zone. Called once for each push considered, after
    /// the credential is read. The push checks step 3 again as a guard ([`check_zone`]), and
    /// maps no legacy name itself (architect, 2026-11-19, for E0.06-T01-S04).
    pub zone: &'a dyn Fn() -> Option<String>,
}

/// How a push ended. `Nested`, `NoReceipt`, `NotDue` and `Pushed` print no warning of their own;
/// every other outcome prints the one it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A nested `up`, not forced: nothing read, nothing printed.
    Nested,
    /// No `LOCALAPPDATA`, or nothing at the receipt's path: skipped without a message.
    NoReceipt,
    /// [`COULD_NOT_READ`].
    CouldNotRead,
    /// The stored stamp matches: nothing sent, nothing printed.
    NotDue,
    /// [`COULD_NOT_SEND`]: an alias that breaks `-Alias`'s rule (nothing read, no [`SENDING`]
    /// line), an ssh that could not be started, or a stamp that could not be written.
    CouldNotSend,
    /// [`TIMED_OUT`].
    TimedOut,
    /// [`NOT_CONFIRMED`].
    NotConfirmed,
    /// The token was confirmed and the zone was not: [`zone_not_taken`].
    ZoneNotTaken,
    /// Confirmed, and the stamp written.
    Pushed,
}

/// The receipt's path, `<LOCALAPPDATA>\localswim-v2\service.json`, or `None` when
/// `LOCALAPPDATA` is unset, empty or only whitespace. A relative value stays relative, so the
/// read resolves it against the working folder.
pub fn receipt_path(ctx: &Context) -> Option<PathBuf> {
    let dir = ctx.local_app_data.as_ref()?;
    if dir.as_os_str().to_string_lossy().trim().is_empty() {
        return None;
    }
    Some(dir.join("localswim-v2").join("service.json"))
}

/// `bot.xml`'s path: `credential_directory`, then a separator unless it already ends in `\` or
/// `/`, then `bot.xml`, taken literally.
pub fn credential_path(credential_directory: &str) -> PathBuf {
    let mut p = OsString::from(credential_directory);
    if !credential_directory.ends_with(['\\', '/']) {
        p.push(MAIN_SEPARATOR.to_string());
    }
    p.push("bot.xml");
    PathBuf::from(p)
}

/// The stamp file's path, `%USERPROFILE%\.agentspace\localswim-pushed.json`, which
/// `AGENTSPACE_STATE_DIR` does not move.
pub fn stamp_path(ctx: &Context) -> PathBuf {
    ctx.home.join(".agentspace").join("localswim-pushed.json")
}

/// The receipt, the token, and so the content; `Err` is the outcome when they cannot be had.
fn read_content(
    ctx: &Context,
    files: &dyn Files,
    unprotect: &dyn Unprotect,
) -> Result<Content, Outcome> {
    let Some(path) = receipt_path(ctx) else {
        return Err(Outcome::NoReceipt);
    };
    let receipt = match files.read(&path) {
        FileRead::Missing => return Err(Outcome::NoReceipt),
        FileRead::Unreadable => return Err(Outcome::CouldNotRead),
        FileRead::Bytes(bytes) => parse_receipt(&bytes).ok_or(Outcome::CouldNotRead)?,
    };
    let token = match files.read(&credential_path(&receipt.credential_directory)) {
        FileRead::Bytes(bytes) => read_token(&bytes, unprotect).ok_or(Outcome::CouldNotRead)?,
        FileRead::Missing | FileRead::Unreadable => return Err(Outcome::CouldNotRead),
    };
    content(receipt.base.as_deref(), &token).ok_or(Outcome::CouldNotRead)
}

/// Whether a push is due: forced, or no stored stamp that matches its schema and equals `stamp`.
fn is_due(files: &dyn Files, path: &Path, stamp: &str, forced: bool) -> bool {
    if forced {
        return true;
    }
    match files.read(path) {
        FileRead::Bytes(bytes) => parse_stamp_file(&bytes).as_deref() != Some(stamp),
        FileRead::Missing | FileRead::Unreadable => true,
    }
}

/// One push (operator.md, The token push). It never fails: every outcome is a warning through
/// `say`, or nothing.
///
/// - `instance_id` is the instance's ID, which the stamp covers.
/// - `alias` is `up`'s `-Alias`, or `agentspace` for `sync`.
/// - `forced` is `sync`'s push: due whatever the stamp says, and not stopped by
///   `AGENTSPACE_NESTED`. Unforced, a nested `up` (`ctx.nested`) does not push.
pub fn push_with(
    ctx: &Context,
    say: &dyn Say,
    instance_id: &str,
    alias: &str,
    forced: bool,
    seams: &Seams,
) -> Outcome {
    let warn = |text: &str, outcome: Outcome| {
        say.warning(text);
        outcome
    };
    // The push checks `AGENTSPACE_NESTED`, unless forced, and the alias before it reads anything
    // (operator.md, What the laptop checks; architect, 2026-11-19, for E0.06-T01-S04).
    // `AGENTSPACE_NESTED` stops `up`'s push alone: `sync`'s forced push runs whatever it says.
    if ctx.nested && !forced {
        return Outcome::Nested;
    }
    // An alias that breaks `-Alias`'s rule is "any other failure" if one ever reaches the push:
    // no `Sending` line, and the one "Could not send" warning (the same ruling).
    if !alias_is_valid(alias) {
        return warn(COULD_NOT_SEND, Outcome::CouldNotSend);
    }

    // What the laptop reads, in its order: the receipt, the credential, then the zone.
    let content = match read_content(ctx, seams.files, seams.unprotect) {
        Ok(c) => c,
        Err(Outcome::NoReceipt) => return Outcome::NoReceipt,
        Err(o) => return warn(COULD_NOT_READ, o),
    };
    // The zone is already mapped and checked; step 3's check again is the guard before it goes on
    // the command line.
    let zone = check_zone((seams.zone)().as_deref());
    let computed = stamp(instance_id, &zone, &content);
    let path = stamp_path(ctx);
    if !is_due(seams.files, &path, &computed, forced) {
        return Outcome::NotDue;
    }

    // What the laptop sends.
    say.progress(SENDING, None);
    if zone.is_empty() {
        say.warning(NO_ZONE);
    }
    let command = remote_command(&zone);
    let args = ssh_args(alias, &command);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (code, stdout) = match seams.ssh.run(&args, content.as_bytes()) {
        SshResult::CouldNotStart => return warn(COULD_NOT_SEND, Outcome::CouldNotSend),
        SshResult::TimedOut => return warn(TIMED_OUT, Outcome::TimedOut),
        SshResult::Exited { code, stdout } => (code, stdout),
    };

    // What the laptop checks; the stamp is written only once everything sent is confirmed.
    match boxside::check_answer(code, &stdout, &stamp::box_hash(&content), &zone) {
        Answer::NotConfirmed => warn(NOT_CONFIRMED, Outcome::NotConfirmed),
        Answer::ZoneNotTaken => warn(&zone_not_taken(&zone), Outcome::ZoneNotTaken),
        Answer::Confirmed => {
            if seams.files.write_stamp(&path, &stamp_file_bytes(&computed)) {
                Outcome::Pushed
            } else {
                warn(COULD_NOT_SEND, Outcome::CouldNotSend)
            }
        }
    }
}

#[cfg(test)]
mod tests;
