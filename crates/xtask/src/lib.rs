//! The bytes and the folder of `cargo xtask dist` (docs/operator.md, "The install folder").
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing.
//!
//! The output folder MUST hold exactly three files: `agentspace.exe`, the `agentspace` wrapper
//! for POSIX shells, and v0.05's forwarder `agentspace-ssh-proxy.cmd`. This library never writes
//! to stdout or stderr and never panics on bad input; the binary prints what an error says.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// The file name of the lease CLI in the output folder.
pub const PROGRAM_FILE: &str = "agentspace.exe";
/// The file name of the wrapper for POSIX shells.
pub const WRAPPER_FILE: &str = "agentspace";
/// The file name of v0.05's forwarder.
pub const FORWARDER_FILE: &str = "agentspace-ssh-proxy.cmd";

/// Everything the output folder holds and nothing else.
pub const DIST_FILES: [&str; 3] = [PROGRAM_FILE, WRAPPER_FILE, FORWARDER_FILE];

/// The forwarder's target, below the checkout.
pub const PROXY_TARGET: [&str; 2] = ["scripts", "ssh-proxy.cmd"];

/// What went wrong, in agentspace's own words and never with a path in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DistError {
    /// The checkout has no `scripts\ssh-proxy.cmd`, so no forwarder can be written.
    ProxyTargetMissing,
    /// The checkout's path cannot be written into a batch file.
    ProxyPathUnusable,
    /// The path given for the checkout cannot be turned into a full path.
    CheckoutPathUnreadable,
    /// The built program is not where the build puts it.
    ProgramMissing,
    /// A file or folder of the output folder could not be written or removed.
    OutputFolder(&'static str),
}

impl fmt::Display for DistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DistError::ProxyTargetMissing => f.write_str(
                "The proxy checkout has no scripts\\ssh-proxy.cmd, so no forwarder was written. \
                 Name the main checkout with --proxy-checkout.",
            ),
            DistError::ProxyPathUnusable => f.write_str(
                "The proxy checkout's path cannot be written into a forwarder, so none was \
                 written.",
            ),
            DistError::CheckoutPathUnreadable => {
                f.write_str("The proxy checkout's path cannot be read.")
            }
            DistError::ProgramMissing => {
                f.write_str("The build did not produce agentspace.exe, so nothing was written.")
            }
            DistError::OutputFolder(what) => f.write_str(what),
        }
    }
}

impl std::error::Error for DistError {}

/// The `agentspace` wrapper: LF line ends only, `#!/bin/sh` first.
///
/// It turns MSYS path conversion off, finds `agentspace.exe` from its own file's path (`$0`, never
/// through `PATH`), passes the arguments on unchanged and ends with the program's exit code, since
/// `exec` replaces the shell.
pub fn wrapper_bytes() -> Vec<u8> {
    let text = "\
#!/bin/sh
# Runs agentspace.exe from this file's own folder, with MSYS path conversion off.
MSYS_NO_PATHCONV=1
export MSYS_NO_PATHCONV
MSYS2_ARG_CONV_EXCL='*'
export MSYS2_ARG_CONV_EXCL
here=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)
exec \"$here/agentspace.exe\" \"$@\"
";
    text.as_bytes().to_vec()
}

/// The forwarder: one line, `@"<target>" %*`, ended by CRLF, as a `.cmd` file is.
///
/// A batch file that runs another batch file without `call` hands control over and never reads
/// itself again, which is what operator.md asks of a forwarder. The line also passes the
/// arguments through and leaves standard input, output, error and the exit code alone. Each `%`
/// of the path is doubled, since a batch file reads `%%` as one `%`. A path cannot hold a
/// double quote on Windows, so quoting the whole path is enough for spaces and for `&`, `(`, `)`
/// and `^`. A path with a double quote, or a line break, is refused. So is a path with a character
/// outside ASCII, because cmd reads a batch file in the console's code page, which is the OEM page
/// for a task without a console, and so cannot follow it. So is a path in the `\\?\` form, which
/// cmd cannot run.
pub fn forwarder_bytes(target: &str) -> Result<Vec<u8>, DistError> {
    if target.is_empty()
        || !target.is_ascii()
        || target.contains(['"', '\r', '\n', '\0'])
        || target.starts_with(r"\\?\")
    {
        return Err(DistError::ProxyPathUnusable);
    }
    let escaped = target.replace('%', "%%");
    Ok(format!("@\"{escaped}\" %*\r\n").into_bytes())
}

/// The checkout the forwarder points into: the one given, made a full path, or else the
/// workspace's own root.
pub fn choose_checkout(given: Option<&Path>, workspace_root: &Path) -> Result<PathBuf, DistError> {
    match given {
        Some(folder) => std::path::absolute(folder).map_err(|_| DistError::CheckoutPathUnreadable),
        None => Ok(workspace_root.to_path_buf()),
    }
}

/// The build profile's folder name: `debug`, or `release` when asked for.
pub fn choose_profile(release: bool) -> &'static str {
    if release { "release" } else { "debug" }
}

/// The full path of `<checkout>\scripts\ssh-proxy.cmd`, when that file exists.
pub fn proxy_target(checkout: &Path) -> Result<PathBuf, DistError> {
    let mut target = checkout.to_path_buf();
    for part in PROXY_TARGET {
        target.push(part);
    }
    if target.is_file() {
        Ok(target)
    } else {
        Err(DistError::ProxyTargetMissing)
    }
}

/// Writes the three files into `dist`, after removing everything else there.
///
/// Nothing is touched when the forwarder cannot be made (a target that is missing, a path that
/// cannot be written), so a refused run leaves any earlier output as it was.
pub fn write_dist(dist: &Path, built_program: &Path, checkout: &Path) -> Result<(), DistError> {
    let target = proxy_target(checkout)?;
    let target_text = target.to_str().ok_or(DistError::ProxyPathUnusable)?;
    let forwarder = forwarder_bytes(target_text)?;
    if !built_program.is_file() {
        return Err(DistError::ProgramMissing);
    }

    let folder_err = DistError::OutputFolder("The output folder could not be prepared.");
    fs::create_dir_all(dist).map_err(|_| folder_err.clone())?;
    let entries = fs::read_dir(dist).map_err(|_| folder_err.clone())?;
    for entry in entries {
        let entry = entry.map_err(|_| folder_err.clone())?;
        let name = entry.file_name();
        if name.to_str().is_some_and(|n| DIST_FILES.contains(&n)) {
            continue;
        }
        let path = entry.path();
        let is_dir = entry.file_type().map_err(|_| folder_err.clone())?.is_dir();
        let removed = if is_dir {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        removed.map_err(|_| folder_err.clone())?;
    }

    let write_err = DistError::OutputFolder("A file of the output folder could not be written.");
    fs::copy(built_program, dist.join(PROGRAM_FILE)).map_err(|_| write_err.clone())?;
    fs::write(dist.join(WRAPPER_FILE), wrapper_bytes()).map_err(|_| write_err.clone())?;
    fs::write(dist.join(FORWARDER_FILE), forwarder).map_err(|_| write_err)?;
    Ok(())
}

/// What a command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Print the usage and exit 0.
    Help,
    /// Build and write the output folder.
    Dist {
        /// Build with the release profile.
        release: bool,
        /// The folder the forwarder points into, when given.
        proxy_checkout: Option<PathBuf>,
    },
}

/// The usage text.
pub const USAGE: &str = "\
Usage: cargo xtask dist [--release] [--proxy-checkout <folder>]

Builds the workspace's programs and writes target\\dist\\ with exactly three files:
agentspace.exe, the agentspace wrapper and the forwarder agentspace-ssh-proxy.cmd.

  --release                  Builds with the release profile.
  --proxy-checkout <folder>  The main checkout whose scripts\\ssh-proxy.cmd the forwarder runs.
                             Without it, the checkout the build runs from.
  -h, --help                 Prints this text.
";

/// Reads a command line, without the program's own name. A usage error is the message to print.
pub fn parse_args<I>(args: I) -> Result<Request, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let first = args
        .next()
        .ok_or_else(|| "A command is needed.".to_string())?;
    match first.as_str() {
        "-h" | "--help" => return Ok(Request::Help),
        "dist" => {}
        other => return Err(format!("Unknown command '{other}'.")),
    }
    let mut release = false;
    let mut proxy_checkout: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        let value = match arg.as_str() {
            "-h" | "--help" => return Ok(Request::Help),
            "--release" => {
                release = true;
                continue;
            }
            "--proxy-checkout" => args
                .next()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "--proxy-checkout needs a folder.".to_string())?,
            other => match other.strip_prefix("--proxy-checkout=") {
                Some("") => return Err("--proxy-checkout needs a folder.".to_string()),
                Some(value) => value.to_string(),
                None => return Err(format!("Unknown argument '{other}'.")),
            },
        };
        if proxy_checkout.replace(PathBuf::from(value)).is_some() {
            return Err("--proxy-checkout was given twice.".to_string());
        }
    }
    Ok(Request::Dist {
        release,
        proxy_checkout,
    })
}
