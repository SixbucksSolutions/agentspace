//! The bytes and the folder of `cargo xtask dist` (docs/operator.md, "The programs" and "The
//! install folder").
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing.
//!
//! The output folder MUST hold exactly eight files: the seven of operator.md's table and v0.06's
//! forwarder `agentspace-ssh-proxy.cmd`. Each is named explicitly, nothing else is copied, and no
//! path is written into any file. This library never writes to stdout or stderr and never panics
//! on bad input; the binary prints what an error says.
//!
//! Clearing the folder MUST NOT reach anything outside it. The folder MUST be a real folder, not
//! a link or junction to one elsewhere, and only plain files in it are removed: a folder, a link
//! or a junction in it is refused before anything is removed. Each file is removed before its
//! replacement is written, so a hard link to a file elsewhere is never written through.

use std::fmt;
use std::fs;
use std::path::Path;

/// The lease CLI's file name.
pub const PROGRAM_FILE: &str = "agentspace.exe";
/// The wrapper for POSIX shells.
pub const WRAPPER_FILE: &str = "agentspace";
/// The forwarder's file name (FB-16).
pub const FORWARDER_FILE: &str = "agentspace-ssh-proxy.cmd";
/// The keeper task's launcher, which MUST be a Windows-subsystem program.
pub const KEEPER_TASK_FILE: &str = "agentspace-keeper-task.exe";

/// A program of the table: its Cargo package, the binary in it, and its file in the folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Program {
    /// The Cargo package.
    pub package: &'static str,
    /// The binary's name, which is also the file's name without `.exe`.
    pub binary: &'static str,
    /// The file's name in the output folder.
    pub file: &'static str,
}

/// The six programs, in the table's order.
pub const PROGRAMS: [Program; 6] = [
    Program {
        package: "agentspace-cli",
        binary: "agentspace",
        file: PROGRAM_FILE,
    },
    Program {
        package: "agentspace-operator",
        binary: "agentspace-operator",
        file: "agentspace-operator.exe",
    },
    Program {
        package: "agentspace-ssh-proxy",
        binary: "agentspace-ssh-proxy",
        file: "agentspace-ssh-proxy.exe",
    },
    Program {
        package: "agentspace-keeper",
        binary: "agentspace-keeper",
        file: "agentspace-keeper.exe",
    },
    Program {
        package: "agentspace-keeper-task",
        binary: "agentspace-keeper-task",
        file: KEEPER_TASK_FILE,
    },
    Program {
        package: "agentspace-keeper-install",
        binary: "agentspace-keeper-install",
        file: "agentspace-keeper-install.exe",
    },
];

/// Everything the output folder holds and nothing else: the table's seven files and the forwarder.
pub const DIST_FILES: [&str; 8] = [
    PROGRAM_FILE,
    WRAPPER_FILE,
    "agentspace-operator.exe",
    "agentspace-ssh-proxy.exe",
    "agentspace-keeper.exe",
    KEEPER_TASK_FILE,
    "agentspace-keeper-install.exe",
    FORWARDER_FILE,
];

/// What went wrong, in agentspace's own words and never with a path in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DistError {
    /// A built program is not where the build puts it.
    ProgramMissing(&'static str),
    /// The keeper task's launcher is not a Windows-subsystem program.
    KeeperTaskNotWindows,
    /// The output folder's path is a link, a junction or a file, not a folder.
    OutputNotAFolder,
    /// The output folder holds something other than a plain file: a folder, a link or a junction.
    OutputHoldsNonFile,
    /// A file or folder of the output folder could not be written or removed.
    OutputFolder(&'static str),
}

impl fmt::Display for DistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DistError::ProgramMissing(file) => {
                write!(
                    f,
                    "The build did not produce {file}, so nothing was written."
                )
            }
            DistError::KeeperTaskNotWindows => f.write_str(
                "agentspace-keeper-task.exe is not a Windows-subsystem program, so nothing was \
                 written.",
            ),
            DistError::OutputNotAFolder => f.write_str(
                "The output folder is a link, a junction or a file, so nothing was written.",
            ),
            DistError::OutputHoldsNonFile => f.write_str(
                "The output folder holds a folder, a link or a junction, so nothing was written. \
                 Remove it and run dist again.",
            ),
            DistError::OutputFolder(what) => f.write_str(what),
        }
    }
}

impl std::error::Error for DistError {}

/// The Windows GUI subsystem's number, `IMAGE_SUBSYSTEM_WINDOWS_GUI`: a program with no console.
pub const SUBSYSTEM_WINDOWS_GUI: u16 = 2;

/// The PE optional header's Subsystem field, or `None` when the bytes are not a PE file.
///
/// 2 is the Windows GUI and 3 is the console. The field sits 68 bytes into the optional header,
/// in PE32 and PE32+ alike.
pub fn pe_subsystem(bytes: &[u8]) -> Option<u16> {
    let offset = u32::from_le_bytes(bytes.get(0x3C..0x40)?.try_into().ok()?);
    let at = usize::try_from(offset).ok()?;
    if bytes.get(at..at.checked_add(4)?)? != b"PE\0\0" {
        return None;
    }
    let field = at.checked_add(24 + 68)?;
    let raw = bytes.get(field..field.checked_add(2)?)?;
    Some(u16::from_le_bytes(raw.try_into().ok()?))
}

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

/// The forwarder: the one fixed line `@"%~dp0agentspace-ssh-proxy.exe" %*`, ended by CRLF (FB-16).
///
/// `%~dp0` is the forwarder's own folder when it runs, so no path is written into it. The line
/// runs a program, which ends the forwarder within that line, as operator.md asks of a forwarder,
/// and it passes the arguments, input, output and exit code through.
pub fn forwarder_bytes() -> Vec<u8> {
    b"@\"%~dp0agentspace-ssh-proxy.exe\" %*\r\n".to_vec()
}

/// The build profile's folder name: `debug`, or `release` when asked for.
pub fn choose_profile(release: bool) -> &'static str {
    if release { "release" } else { "debug" }
}

/// Writes the eight files into `dist`, after removing every plain file there.
///
/// `built` is the build profile's folder. Each program is read by its explicit name, so nothing
/// else in it is copied. Nothing is touched when a program is missing, the keeper task's launcher
/// is not a Windows-subsystem program, `dist` is not a real folder, or `dist` holds anything but
/// plain files, so a refused run leaves any earlier output as it was.
pub fn write_dist(dist: &Path, built: &Path) -> Result<(), DistError> {
    for program in PROGRAMS {
        if !built.join(program.file).is_file() {
            return Err(DistError::ProgramMissing(program.file));
        }
    }
    let task = fs::read(built.join(KEEPER_TASK_FILE))
        .map_err(|_| DistError::ProgramMissing(KEEPER_TASK_FILE))?;
    if pe_subsystem(&task) != Some(SUBSYSTEM_WINDOWS_GUI) {
        return Err(DistError::KeeperTaskNotWindows);
    }

    let folder_err = DistError::OutputFolder("The output folder could not be prepared.");
    // The folder itself, without following a link: a link or junction would point the removal
    // below at a folder this program does not own.
    match fs::symlink_metadata(dist) {
        Ok(meta) if meta.file_type().is_dir() => {}
        Ok(_) => return Err(DistError::OutputNotAFolder),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(dist).map_err(|_| folder_err.clone())?;
            match fs::symlink_metadata(dist) {
                Ok(meta) if meta.file_type().is_dir() => {}
                _ => return Err(DistError::OutputNotAFolder),
            }
        }
        Err(_) => return Err(folder_err),
    }

    // Every entry is checked before any is removed. A `DirEntry`'s type does not follow a link,
    // and on Windows a junction is a link, never a folder.
    let mut files = Vec::new();
    for entry in fs::read_dir(dist).map_err(|_| folder_err.clone())? {
        let entry = entry.map_err(|_| folder_err.clone())?;
        if !entry.file_type().map_err(|_| folder_err.clone())?.is_file() {
            return Err(DistError::OutputHoldsNonFile);
        }
        files.push(entry.path());
    }
    // Every plain file goes, the eight included, so each is written new and a hard link to a
    // file elsewhere is only unlinked, never written through.
    for file in files {
        fs::remove_file(&file).map_err(|_| folder_err.clone())?;
    }

    let write_err = DistError::OutputFolder("A file of the output folder could not be written.");
    for program in PROGRAMS {
        fs::copy(built.join(program.file), dist.join(program.file))
            .map_err(|_| write_err.clone())?;
    }
    fs::write(dist.join(WRAPPER_FILE), wrapper_bytes()).map_err(|_| write_err.clone())?;
    fs::write(dist.join(FORWARDER_FILE), forwarder_bytes()).map_err(|_| write_err)?;
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
    },
}

/// The usage text.
pub const USAGE: &str = "\
Usage: cargo xtask dist [--release]

Builds the workspace's programs and writes target\\dist\\ with exactly eight files: the six
programs, the agentspace wrapper and the forwarder agentspace-ssh-proxy.cmd.

  --release   Builds with the release profile.
  -h, --help  Prints this text.
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
    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Request::Help),
            "--release" => release = true,
            other => return Err(format!("Unknown argument '{other}'.")),
        }
    }
    Ok(Request::Dist { release })
}
