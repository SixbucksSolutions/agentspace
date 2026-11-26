//! The token push: its portable half, and its Windows half (docs/operator.md, The token push; D42).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The portable half (v0.06-T01) is everything about the push that is not a Windows call: when it
//! runs and when it is due, what the laptop reads (the receipt, and the Bot credential, `bot.json`,
//! down to its `token_dpapi_hex` member's bytes), the content and the stamp, the ssh's arguments and remote command, what
//! the laptop checks in the box's answer, and every warning, through the pinned [`Say`]. What
//! needs Windows, touches the disk or starts a program is behind three small traits: [`Files`],
//! [`Unprotect`] and [`SshRun`].
//!
//! The Windows half (v0.06-T02) implements them: [`DpapiUnprotect`] (`CryptUnprotectData`),
//! [`SystemSsh`] (the push's one ssh, its pipes, its 20-second limit and the tree kill through the
//! Windows API) and [`SystemFiles`] (the stamp's atomic replace), and wires them to
//! [`push_with`] in [`push`], the entry point `up` and `sync` call.
//!
//! # The public API
//!
//! - [`push`]: one push as the operator runs it, never failing a command. [`push_using`] is the
//!   same over an injectable look-up of the environment, and returns its [`Outcome`].
//! - [`push_with`]: one push, start to end, never failing; returns its [`Outcome`].
//! - For a host other than the box (v0.07): [`push_to_host`], [`push_to_host_using`],
//!   [`push_using_with_host`] and [`push_with_host`] are the push's entry points over a
//!   `host: Option<&str>`, `None` for the box and `Some(project)` for another host, which keeps its
//!   own stamp ([`stamp_path_for`]). A project that is not [`host_name_is_valid`] is refused.
//! - [`Seams`], holding the three traits and the zone: [`Files`] (with [`FileRead`], and
//!   [`StdFiles`], a portable `Files` that T02 MAY use or replace), [`Unprotect`] and [`SshRun`]
//!   (with [`SshResult`]).
//! - The push's ssh's stderr (v0.08, #0167): [`SshStderr`], which a caller gives
//!   [`SystemSsh::with_stderr`] through its `configure`, inherited by default and relayed in the
//!   ProxyCommand's caller mode; [`StderrFilter`], the relay's line rule, which drops
//!   [`HOST_READY_LINE`]; and [`StderrSink`], where the relayed lines go.
//! - [`Token`], [`Bearer`] and [`Content`], the three types that hold the token; each redacts it in
//!   `Debug`, has no `Display` and zeroes it when dropped. [`Token::bearer`] gives the [`Bearer`],
//!   `Bearer <token>`, for the board read's `Authorization` header alone (docs/lease-api.md, The
//!   board read; architect, 2026-11-25, for v0.11-T02, pin P1).
//! - The steps, for a caller that needs one alone: [`receipt_path`], [`parse_receipt`] and
//!   [`Receipt`]; [`credential_path`], [`parse_credential_blob`] and [`read_token`];
//!   [`check_zone`], the zone's guard; [`content`]; [`stamp`], [`stamp_path`], [`stamp_path_for`],
//!   [`parse_stamp_file`] and [`stamp_file_bytes`]; [`alias_is_valid`], [`remote_command`] and
//!   [`ssh_args`].
//! - The warnings' texts, as a person's command prints them: [`COULD_NOT_READ`], [`NO_ZONE`],
//!   [`TIMED_OUT`], [`NOT_CONFIRMED`], [`zone_not_taken`], [`COULD_NOT_SEND`], [`BAD_ALIAS`] and [`BAD_HOST`];
//!   and the progress line [`SENDING`]. In caller mode, [`Say`] rewrites `the instance`.
//!
//! The zone is `agentspace-zone`'s: [`Seams::zone`] gives it already mapped and checked, and this
//! crate only guards it again before it goes on a command line.
//!
//! # Secrecy (operator.md, Secrecy)
//!
//! The token MUST travel only to two places: the push's stdin, and the board read's
//! `Authorization` header (docs/lease-api.md, The board read), and nowhere else. This crate passes
//! it to nothing but [`SshRun::run`]'s `stdin` and, through [`Token::bearer`], to the [`Bearer`]
//! that the board read sends as that header; it puts it in no argument, environment variable,
//! file, message or panic, and keeps it only in [`Token`], [`Bearer`] and [`Content`], which never
//! print it. No warning quotes the failure behind it: each is a fixed text. A [`SshRun`] MUST write `stdin` to the ssh's stdin
//! pipe and nowhere else, and an [`Unprotect`] MUST hand the plaintext to nothing but its return
//! value. The `Debug` of [`FileRead`] and [`SshResult`] gives byte counts, never the bytes, so
//! that no file's content and nothing the box printed reaches a log through them.

#![deny(unsafe_code)]

mod boxside;
mod credential;
// The Windows half's calls, and the only module with `unsafe` (the Windows one, `win.rs`). On any
// other system `other.rs` stands in, which is NOT THE CONTRACT'S: it exists so that the crate
// builds and its portable logic is tested on Linux.
#[cfg(windows)]
#[allow(unsafe_code)]
#[path = "win.rs"]
mod platform;
#[cfg(not(windows))]
#[path = "other.rs"]
mod platform;
mod receipt;
mod relay;
mod stamp;
mod system;
mod text;
mod zone;

pub use agentspace_operator_api::{Context, Say};
pub use boxside::{alias_is_valid, remote_command, ssh_args};
pub use credential::{parse_credential_blob, read_token};
pub use receipt::{Receipt, parse_receipt};
pub use relay::{HOST_READY_LINE, SshStderr, StderrFilter, StderrSink};
pub use stamp::{content, parse_stamp_file, stamp, stamp_file_bytes};
pub use system::{
    DpapiUnprotect, Lookup, SSH_LIMIT, STDOUT_WAIT, SystemFiles, SystemSsh, push, push_to_host,
    push_to_host_using, push_using, push_using_with, push_using_with_host,
};
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
/// Any other failure: an ssh that cannot be started, or a stamp that cannot be written.
pub const COULD_NOT_SEND: &str = "agentspace: Could not send the localswim token; the board will not work on the box this session.";
/// An alias that breaks `-Alias`'s rule and still reached the push, by any path but the command
/// line (architect, 2026-11-19, for E0.06-T01-S04).
pub const BAD_ALIAS: &str = "agentspace: The SSH host alias for the localswim token is not valid; the board will not work on the box this session.";

/// A host's name that is not the host registry's normalized form, which reached a push for a host
/// (see [`host_name_is_valid`]). Its text names no path and no value.
pub const BAD_HOST: &str =
    "agentspace: The host's name is not valid; the board will not work on that host this session.";

/// The warning for a zone the box did not take.
pub fn zone_not_taken(zone: &str) -> String {
    format!(
        "agentspace: The instance did not take the time zone {zone}; times on the box stay in its own zone."
    )
}

/// The Bot token. Its `Debug` redacts it, it has no `Display`, and it is zeroed when dropped.
pub struct Token(pub(crate) Zeroizing<String>);

impl Token {
    /// The token's text, for [`content`] and [`Token::bearer`] alone. It is not public: outside
    /// this crate a token can only go to the two places it MUST go and nowhere else, the push's
    /// stdin through [`content`], and the board read's `Authorization` header through
    /// [`Token::bearer`] (docs/lease-api.md, The board read).
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }

    /// The `Authorization` header's value for the board read, exactly `Bearer <token>`
    /// (docs/lease-api.md, The board read; architect, 2026-11-25, for v0.11-T02, pin P5).
    pub fn bearer(&self) -> Bearer {
        let token = self.expose();
        // Room for the whole value at once, so no reallocation leaves a copy of the token behind.
        let mut value = Zeroizing::new(String::with_capacity(BEARER_PREFIX.len() + token.len()));
        value.push_str(BEARER_PREFIX);
        value.push_str(token);
        Bearer(value)
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

/// The scheme and space before the token in [`Bearer`].
const BEARER_PREFIX: &str = "Bearer ";

/// The board read's `Authorization` header value, `Bearer <token>` (architect, 2026-11-25, for
/// v0.11-T02, pin P5). Its `Debug` redacts it, it has no `Display` and no `Clone`, and it is zeroed
/// when dropped.
pub struct Bearer(Zeroizing<String>);

impl Bearer {
    /// The header value's text, for an HTTP `Authorization` header alone, the one place besides
    /// the push's stdin that the token MUST go (docs/lease-api.md, The board read).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Bearer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Bearer(<redacted>)")
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

pub(crate) fn read_file(path: &Path) -> FileRead {
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
/// `CryptUnprotectData` on Windows, which [`DpapiUnprotect`] implements.
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

/// The push's one ssh (operator.md, What the laptop sends, steps 2 to 5), which [`SystemSsh`]
/// implements.
pub trait SshRun {
    /// Starts ssh with `args`, exactly these and in this order, under the 20-second limit: a pipe
    /// for its stdin and one for its stdout, and its stderr the operator's, or in the
    /// ProxyCommand's caller mode the relay its caller chose ([`SshStderr`]). Writes `stdin` to it
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
    /// [`BAD_ALIAS`]: an alias that breaks `-Alias`'s rule. Nothing read, no [`SENDING`] line.
    /// Also [`BAD_HOST`]: a host's name that is not the registry's normalized form, which is the
    /// same kind of skip (a name that breaks its rule, refused before anything is read or written),
    /// so it has no outcome of its own (architect to confirm, E0.07-T02-S04 review).
    BadAlias,
    /// [`COULD_NOT_SEND`]: an ssh that could not be started, or a stamp that could not be written.
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

/// `bot.json`'s path: `credential_directory`, then a separator unless it already ends in `\` or
/// `/`, then `bot.json`, taken literally (localswim's `docs/windows-operations.md`, "The credential
/// files": the file names are fixed).
pub fn credential_path(credential_directory: &str) -> PathBuf {
    let mut p = OsString::from(credential_directory);
    if !credential_directory.ends_with(['\\', '/']) {
        p.push(MAIN_SEPARATOR.to_string());
    }
    p.push("bot.json");
    PathBuf::from(p)
}

/// The stamp file's path, `%USERPROFILE%\.agentspace\localswim-pushed.json`, which
/// `AGENTSPACE_STATE_DIR` does not move.
pub fn stamp_path(ctx: &Context) -> PathBuf {
    stamp_path_for(ctx, None)
}

/// The stamp file's path for one host (docs/state.md, The push stamp; A host's push stamp; architect,
/// 2026-11-20, for v0.07-T02). `None` is the box, whose stamp is v0.06's,
/// `%USERPROFILE%\.agentspace\localswim-pushed.json`. `Some(project)` is that host's,
/// `%USERPROFILE%\.agentspace\hosts\<project>\localswim-pushed.json`. `AGENTSPACE_STATE_DIR` moves
/// neither. A push for a host MUST read and write its own stamp alone, never the box's.
///
/// **The caller MUST pass a normalized project** ([`host_name_is_valid`]): this function does no
/// check, and a project such as `..` or the empty string resolves to the box's stamp or into
/// `hosts\` itself. The push entry points that take a host ([`push_with_host`] and the ones built on
/// it) refuse any other name before they call it.
pub fn stamp_path_for(ctx: &Context, host: Option<&str>) -> PathBuf {
    let agentspace = ctx.home.join(".agentspace");
    match host {
        None => agentspace.join("localswim-pushed.json"),
        Some(project) => agentspace
            .join("hosts")
            .join(project)
            .join("localswim-pushed.json"),
    }
}

/// Whether `project` is the host registry's normalized form, `^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$`
/// with no trailing newline: 1 to 32 bytes of lowercase ASCII letters, digits and hyphens, that
/// neither begin nor end with a hyphen. A project name MUST pass this before it is a folder name.
pub fn host_name_is_valid(project: &str) -> bool {
    let bytes = project.as_bytes();
    let edge = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let (Some(&first), Some(&last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    bytes.len() <= 32 && edge(first) && edge(last) && bytes.iter().all(|&b| edge(b) || b == b'-')
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
    push_with_host(ctx, say, instance_id, alias, forced, seams, None)
}

/// [`push_with`] for one host: `host` is `None` for the box, whose stamp is v0.06's, or
/// `Some(project)` for another host, whose own stamp ([`stamp_path_for`]) is the only one the push
/// reads and writes, its folder created if missing. Nothing else in the push changes (architect,
/// 2026-11-20, for v0.07-T02). A `project` that is not [`host_name_is_valid`] is refused first:
/// the [`BAD_HOST`] warning, [`Outcome::BadAlias`], and nothing read or written.
pub fn push_with_host(
    ctx: &Context,
    say: &dyn Say,
    instance_id: &str,
    alias: &str,
    forced: bool,
    seams: &Seams,
    host: Option<&str>,
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
    // A host whose name is not the registry's normalized form is refused before anything is read
    // or written: its name would otherwise reach a folder name (`..` is the box's stamp, the empty
    // name is `hosts\` itself).
    if host.is_some_and(|project| !host_name_is_valid(project)) {
        return warn(BAD_HOST, Outcome::BadAlias);
    }
    // An alias that breaks `-Alias`'s rule, if one ever reaches the push, warns its own fixed
    // text, with no `Sending` line, and skips (the same ruling).
    if !alias_is_valid(alias) {
        return warn(BAD_ALIAS, Outcome::BadAlias);
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
    let path = stamp_path_for(ctx, host);
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

#[cfg(test)]
mod bearer_tests {
    use super::*;

    /// A planted token: 43 characters of unpadded base64url, drawn only from characters that
    /// neither `Token(<redacted>)` nor `Bearer(<redacted>)` holds, so a check that the two
    /// `Debug`s hold none of its characters can fail.
    const PLANTED: &str = "fghijlmpqsuvwxyzACDEFGHIJKLMNOPQRS0123456_-";

    fn planted() -> Token {
        Token(Zeroizing::new(PLANTED.to_owned()))
    }

    #[test]
    fn bearer_is_the_scheme_a_space_and_the_token() {
        let bearer = planted().bearer();
        // Compared without printing either value: a token is never printed, a planted one included.
        assert!(
            bearer.as_str() == format!("Bearer {PLANTED}"),
            "The bearer is not `Bearer <token>`"
        );
    }

    #[test]
    fn neither_debug_holds_a_character_of_the_token() {
        assert_eq!(PLANTED.len(), 43);
        assert!(
            PLANTED
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "The planted token is base64url"
        );
        // The check can fail: no character of the planted token is in either redaction.
        for redaction in ["Token(<redacted>)", "Bearer(<redacted>)"] {
            assert!(
                !PLANTED.chars().any(|c| redaction.contains(c)),
                "The planted token shares a character with {redaction}"
            );
        }
        let token = planted();
        let bearer = token.bearer();
        for shown in [format!("{token:?}"), format!("{bearer:?}")] {
            let leaked = PLANTED.chars().filter(|&c| shown.contains(c)).count();
            assert_eq!(
                leaked, 0,
                "A Debug holds {leaked} of the token's characters"
            );
        }
        assert!(format!("{token:?}") == "Token(<redacted>)", "Token's Debug");
        assert!(
            format!("{bearer:?}") == "Bearer(<redacted>)",
            "Bearer's Debug"
        );
    }
}
