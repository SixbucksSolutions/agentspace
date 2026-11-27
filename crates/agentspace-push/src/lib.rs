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
//!   `host: Option<&str>`, `None` for the box and `Some(host name)` for another host, which keeps
//!   its own stamp ([`stamp_path_for`]). From v0.12 the host name is its folder's name,
//!   `<project>` on `linux/arm64` and `<project>.<os>-<arch>` on any other platform (D91), and a
//!   name that is not [`host_name_is_valid`] is refused.
//! - A host's platform (v0.12, D91, D101): [`parse_host_name`] and [`host_platform`] read it from
//!   the host's name, and the push takes the platform's rows from
//!   `agentspace_platforms::for_platform` alone. [`push_with_host_zone`] adds the laptop's Windows
//!   zone ID, which a host not on Linux is given, and [`push_on`] takes the platform as a
//!   [`HostPlatform`], so a test MAY pass a stand-in.
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
//!   [`parse_stamp_file`] and [`stamp_file_bytes`]; [`alias_is_valid`] and [`ssh_args`]. The
//!   remote half is the host's platform's row of the seam, `remote_command(push_script(zone))`,
//!   on every platform, both Linux ones included (D101's change at T03's start): this crate keeps
//!   no copy of its text.
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
pub use boxside::{alias_is_valid, ssh_args};
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

use agentspace_operator_api::HostPlatform;
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

/// A host's name that is not a host name, which reached a push for a host (see
/// [`host_name_is_valid`]), or a platform that is not the one the name names. Its text names no
/// path and no value.
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
    /// Also [`BAD_HOST`]: a host's name that is not a host name ([`host_name_is_valid`]), which is
    /// the same kind of skip (a name that breaks its rule, refused before anything is read or
    /// written), so it has no outcome of its own (architect to confirm, E0.07-T02-S04 review).
    BadAlias,
    /// [`COULD_NOT_SEND`]: an ssh that could not be started, a stamp that could not be written, or
    /// a platform's remote half that could not be made.
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
/// `%USERPROFILE%\.agentspace\localswim-pushed.json`. `Some(host name)` is that host's,
/// `%USERPROFILE%\.agentspace\hosts\<host name>\localswim-pushed.json`, the host name being its
/// folder's name: `<project>` on `linux/arm64`, and `<project>.<os>-<arch>` on every other platform
/// (D91; operator.md, A host's platform and its names). `AGENTSPACE_STATE_DIR` moves neither. A push
/// for a host MUST read and write its own stamp alone, never the box's nor another platform's.
///
/// **The caller MUST pass a valid host name** ([`host_name_is_valid`]): this function does no
/// check, and a name such as `..` or the empty string resolves to the box's stamp or into `hosts\`
/// itself. The push entry points that take a host ([`push_with_host`] and the ones built on it)
/// refuse any other name before they call it.
pub fn stamp_path_for(ctx: &Context, host: Option<&str>) -> PathBuf {
    let agentspace = ctx.home.join(".agentspace");
    match host {
        None => agentspace.join("localswim-pushed.json"),
        Some(name) => agentspace
            .join("hosts")
            .join(name)
            .join("localswim-pushed.json"),
    }
}

/// The platform every host had before v0.12, the box's, and the one a host name without a `.`
/// names (D91).
pub const LINUX_ARM64: &str = "linux/arm64";

/// Whether `project` is the host registry's normalized form, `^[a-z0-9]([a-z0-9-]{0,30}[a-z0-9])?$`
/// with no trailing newline: 1 to 32 bytes of lowercase ASCII letters, digits and hyphens, that
/// neither begin nor end with a hyphen.
fn project_is_valid(project: &str) -> bool {
    let bytes = project.as_bytes();
    let edge = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let (Some(&first), Some(&last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    bytes.len() <= 32 && edge(first) && edge(last) && bytes.iter().all(|&b| edge(b) || b == b'-')
}

/// The normalized platform a name part names: exactly `linux-amd64` or `windows-amd64`, the name
/// parts of D91's two non-default platforms, written `<os>-<arch>` (operator.md, A host's platform
/// and its names: `linux/arm64` never appears in a name; pin P3 of v0.12-T03). A name part is
/// compared exactly, so `linux-arm64`, an upper-case one and `windows-arm64` name none.
fn platform_of_name_part(part: &str) -> Option<&'static str> {
    match part {
        "linux-amd64" => Some("linux/amd64"),
        "windows-amd64" => Some("windows/amd64"),
        _ => None,
    }
}

/// A host name's project and normalized platform (operator.md, A host's platform and its names;
/// architect, 2026-11-26, for v0.12-T01, pins P1 and P12), or `None` when `name` is not one.
///
/// A host name is `<project>` for a `linux/arm64` host, and `<project>.<os>-<arch>` for a host of
/// any other supported platform. A project holds no `.`, so the last `.` always starts the
/// platform's name part. The project MUST be in the normalized form, and the name part MUST be
/// `linux-amd64` or `windows-amd64` exactly: the name parts of the supported platforms other than
/// `linux/arm64`.
pub fn parse_host_name(name: &str) -> Option<(&str, &'static str)> {
    match name.rsplit_once('.') {
        None => project_is_valid(name).then_some((name, LINUX_ARM64)),
        Some((project, part)) => {
            let platform = platform_of_name_part(part)?;
            project_is_valid(project).then_some((project, platform))
        }
    }
}

/// Whether `name` is a host name ([`parse_host_name`]): a normalized project, optionally followed
/// by `.` and `linux-amd64` or `windows-amd64`, and never `linux-arm64` as a suffix (pin P3 of
/// v0.12-T03). A host's name MUST pass this before it is a folder name.
pub fn host_name_is_valid(name: &str) -> bool {
    parse_host_name(name).is_some()
}

/// The normalized platform of the host a push names: `linux/arm64` for the box (`None`), and the
/// platform a valid host name names otherwise; `None` for a name that is not valid.
pub fn host_platform(host: Option<&str>) -> Option<&'static str> {
    match host {
        None => Some(LINUX_ARM64),
        Some(name) => parse_host_name(name).map(|(_, platform)| platform),
    }
}

/// Whether a host of `platform` is given the laptop's Windows zone ID (operator.md, The token push
/// to a Windows host; D96): a Windows host alone. Both Linux rows of `push_zone` ignore it, so it
/// is not read from the system for them.
fn takes_windows_zone(platform: &dyn HostPlatform) -> bool {
    platform.platform().starts_with("windows/")
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
/// `Some(host name)` for another host ([`parse_host_name`]), whose own stamp ([`stamp_path_for`])
/// is the only one the push reads and writes, its folder created if missing (architect,
/// 2026-11-20, for v0.07-T02). A `host` that is not [`host_name_is_valid`] is refused first: the
/// [`BAD_HOST`] warning, [`Outcome::BadAlias`], and nothing read or written.
///
/// The host's platform comes from its name ([`host_platform`]), and its rows from
/// `agentspace_platforms::for_platform` alone (D101). **A host not on Linux is given the laptop's
/// Windows zone ID, which this entry point does not have**, so its zone is the empty zone here;
/// [`push_on`] takes it, and the operator's own entry points ([`push_to_host`] and the ones built
/// on it) read it from the system or `AGENTSPACE_TIMEZONE`.
pub fn push_with_host(
    ctx: &Context,
    say: &dyn Say,
    instance_id: &str,
    alias: &str,
    forced: bool,
    seams: &Seams,
    host: Option<&str>,
) -> Outcome {
    push_with_host_zone(ctx, say, instance_id, alias, forced, seams, host, &|| None)
}

/// [`push_with_host`], with `windows_zone` the laptop's Windows zone ID, or
/// `AGENTSPACE_TIMEZONE`'s value in its place (operator.md, The token push to a Windows host),
/// asked only for a host whose platform takes it. The platform is picked by
/// `agentspace_platforms::for_platform` from the host's name.
#[allow(clippy::too_many_arguments)]
pub fn push_with_host_zone(
    ctx: &Context,
    say: &dyn Say,
    instance_id: &str,
    alias: &str,
    forced: bool,
    seams: &Seams,
    host: Option<&str>,
    windows_zone: &dyn Fn() -> Option<String>,
) -> Outcome {
    if ctx.nested && !forced {
        return Outcome::Nested;
    }
    let Some(platform) = host_platform(host).and_then(agentspace_platforms::for_platform) else {
        say.warning(BAD_HOST);
        return Outcome::BadAlias;
    };
    push_on(
        ctx,
        say,
        instance_id,
        alias,
        forced,
        seams,
        host,
        platform,
        windows_zone,
    )
}

/// One push to `host` on `platform`, whose rows of the seam give the zone and the remote half
/// (operator.md, The platform seam, the push's zone and remote half row; The token push to a
/// Windows host). The operator's entry points pass the platform `agentspace_platforms::for_platform`
/// gives for the host's name; a test MAY pass a stand-in. `platform` MUST be the host's own: one
/// whose `platform()` is not the one `host`'s name names is refused as a bad host name is, before
/// anything is read.
///
/// **The laptop's side is the same on every platform** (operator.md, The token push to a Windows
/// host): the reads, the stamp, the ssh, its limits, what the laptop checks and every warning.
/// What differs is the platform's rows alone, taken the same way on every platform:
/// - **The zone** is its `push_zone(windows_zone, iana)`, `iana` being the name `seams.zone`
///   gives, guarded by [`check_zone`]. Both Linux rows return `iana`, as before v0.12. `windows_zone`
///   is asked only for a Windows host, and is the empty string for any other. A row that fails
///   gives the empty zone, which is not sent, as on Linux. The stamp covers the zone as sent.
/// - **The remote command** is its `remote_command(push_script(zone))`; a row that fails warns
///   [`COULD_NOT_SEND`] after the `Sending` line, as any other failure to send does, and records
///   nothing.
#[allow(clippy::too_many_arguments)]
pub fn push_on(
    ctx: &Context,
    say: &dyn Say,
    instance_id: &str,
    alias: &str,
    forced: bool,
    seams: &Seams,
    host: Option<&str>,
    platform: &dyn HostPlatform,
    windows_zone: &dyn Fn() -> Option<String>,
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
    // A host whose name is not a host name is refused before anything is read or written: its name
    // would otherwise reach a folder name (`..` is the box's stamp, the empty name is `hosts\`
    // itself). So is a platform that is not the one the name names, whose rows would push to the
    // wrong kind of host under that host's stamp.
    if host_platform(host) != Some(platform.platform()) {
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
    // the command line. Every platform's zone is its own row's, from the IANA name and, on a
    // Windows host, the laptop's Windows zone ID (D96), read only for a host that is given it. A
    // row that fails, such as the Windows row's refusal of an ID (its NO_ZONE failure), is the
    // empty zone: NO_ZONE's warning, and the push goes on without a zone (architect, 2026-11-26,
    // for v0.12-T03, P9's readings).
    let iana = check_zone((seams.zone)().as_deref());
    let windows_zone = if takes_windows_zone(platform) {
        windows_zone().unwrap_or_default()
    } else {
        String::new()
    };
    let zone = platform.push_zone(&windows_zone, &iana).unwrap_or_default();
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
    // The remote half is the platform's row on every platform, both Linux ones included: the
    // seam's text, byte for byte what the push sent before v0.12 on Linux (pin P1 of v0.12-T03).
    let command = match platform
        .push_script(&zone)
        .and_then(|script| platform.remote_command(&script))
    {
        Ok(command) => command,
        Err(_) => return warn(COULD_NOT_SEND, Outcome::CouldNotSend),
    };
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
