//! Every refusal docs/state.md gives for the host registry, the project list and a host's run
//! lock, as a typed error. Each message is in agentspace's own words and names no path: a caller
//! prints it, and a path outside the home folder MUST NOT reach it (D1). The shape follows
//! agentspace-state's `StoreError`: a variant for the lock, one for a file that cannot be read, one
//! for a file that is not valid and one for a file that cannot be written, each saying why.

use std::fmt;

/// Why a file is not valid (docs/state.md, "What makes a file not valid, beyond its schema", and
/// the project list's rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalid {
    /// The bytes are not UTF-8.
    NotUtf8,
    /// The text is not one JSON value.
    NotJson,
    /// An object names one member twice.
    RepeatedMember,
    /// The document fails its schema, a `version` other than 0 and a `null` member included.
    Schema,
    /// Two records of the registry share a `project`.
    RepeatedProject,
    /// A record's stack is `agentspace-host-<p>` with `<p>` other than the record's own project.
    StackOfAnotherProject,
    /// The registry holds no record whose stack is `agentbox`, holds two, or holds one whose
    /// project is not `swimcoach`.
    BoxRecord,
    /// A `last_attempt_at` that matches its pattern but names no real instant.
    TimeNamesNoInstant,
    /// Two entries of the project list share a `name`.
    RepeatedName,
    /// An `approved` date that matches its pattern but names no real day.
    DateNamesNoDay,
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Invalid::NotUtf8 => "its bytes are not UTF-8",
            Invalid::NotJson => "it is not JSON",
            Invalid::RepeatedMember => "an object names one member twice",
            Invalid::Schema => "it does not match its schema",
            Invalid::RepeatedProject => "two records share a project",
            Invalid::StackOfAnotherProject => "a stack names a project other than its record's",
            Invalid::BoxRecord => "it does not hold exactly one box record, for swimcoach",
            Invalid::TimeNamesNoInstant => "a time names no real instant",
            Invalid::RepeatedName => "two entries share a name",
            Invalid::DateNamesNoDay => "a date names no real day",
        })
    }
}

/// Why a file or folder could not be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// A file is where agentspace's folder, a host's folder or the home folder belongs.
    FolderIsAFile,
    /// A folder could not be made.
    CannotMakeFolder,
    /// Something that is not a file, such as a folder, is where a file belongs. Nothing in
    /// agentspace repairs, rewrites or removes it, and a reader treats it as a file that is not
    /// valid, never as a missing file (docs/state.md).
    NotAFile,
    /// The file cannot be opened.
    CannotOpen,
    /// The file was opened and cannot be read.
    CannotRead,
    /// A lock file could not be opened, for a reason other than another process holding it, such as
    /// access denied or a folder at its path. It is never a free lock.
    CannotOpenLock,
    /// The temporary file could not be moved over the file.
    CannotReplace,
    /// A change that would not be valid. Nothing was written.
    ChangeNotValid(Invalid),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::FolderIsAFile => f.write_str("a file is where agentspace's folder belongs"),
            Failure::CannotMakeFolder => f.write_str("the folder could not be made"),
            Failure::NotAFile => {
                f.write_str("something that is not a file is where the file belongs")
            }
            Failure::CannotOpen => f.write_str("the file could not be opened"),
            Failure::CannotRead => f.write_str("the file could not be read"),
            Failure::CannotOpenLock => f.write_str("the lock file could not be opened"),
            Failure::CannotReplace => f.write_str("the file could not be replaced"),
            Failure::ChangeNotValid(why) => write!(f, "the change is not valid: {why}"),
        }
    }
}

/// Why a read, a lock or a write failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostsError {
    /// Another process held the host registry lock until the wait of 30 seconds ran out.
    Busy,
    /// The host registry lock could not be taken for a reason other than another process holding
    /// it. It is never reported as busy.
    CannotLock(Failure),
    /// The registry or the project list could not be read, or a folder is where it belongs. A
    /// reader treats it as a file that is not valid (docs/state.md).
    Unreadable(Failure),
    /// The registry or the project list is not valid.
    NotValid(Invalid),
    /// The registry could not be written, or the change would not be valid.
    Unwritable(Failure),
    /// A project that is not in the lease contract's normalized form, so no path is made from it.
    BadProject,
    /// A host's run lock could not be taken for a reason other than another process holding it. It
    /// is not a free lock: the caller starts or runs nothing (docs/state.md).
    CannotLockRun(Failure),
}

impl fmt::Display for HostsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostsError::Busy => f.write_str("The host registry is in use by another command."),
            HostsError::CannotLock(why) => {
                write!(f, "The host registry could not be locked: {why}.")
            }
            HostsError::Unreadable(why) => write!(f, "The file could not be read: {why}."),
            HostsError::NotValid(why) => write!(f, "The file is not valid: {why}."),
            HostsError::Unwritable(why) => {
                write!(f, "The host registry could not be written: {why}.")
            }
            HostsError::BadProject => {
                f.write_str("The project's name is not in its normalized form.")
            }
            HostsError::CannotLockRun(why) => {
                write!(f, "The provisioning run's lock could not be taken: {why}.")
            }
        }
    }
}

impl std::error::Error for HostsError {}
