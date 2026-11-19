//! agentspace-cli (docs/decisions.md, D38): the lease CLI's command line, help, errors and output.
//!
//! RFC 2119 keywords in this crate's comments; the capitals are load-bearing. The library never
//! writes to stdout or stderr, except `main_with`, which is the program's own entry point, and it
//! never panics on bad input.

pub mod commands;
mod help;
mod osx;
mod parse;
mod quote;
mod run;

use std::ffi::OsString;

pub use quote::quote_os;
pub use run::{Outcome, execute, main_with};

/// The five lease subcommands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sub {
    Create,
    Get,
    Renew,
    Release,
    List,
}

impl Sub {
    /// The subcommand's name, in lowercase.
    pub fn name(self) -> &'static str {
        match self {
            Sub::Create => "create",
            Sub::Get => "get",
            Sub::Renew => "renew",
            Sub::Release => "release",
            Sub::List => "list",
        }
    }
}

/// A command line that has no usage error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    pub sub: Sub,
    /// Each flag's lowercase name and its value as given, in line order.
    pub flags: Vec<(String, OsString)>,
    pub id: Option<OsString>,
}

/// The error codes, each with its exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Internal,
    Usage,
    InvalidValue,
    OutOfRange,
    NotFound,
    InvalidState,
}

impl ErrorCode {
    pub fn exit_code(self) -> u8 {
        match self {
            ErrorCode::Internal => 1,
            ErrorCode::Usage | ErrorCode::InvalidValue | ErrorCode::OutOfRange => 2,
            ErrorCode::NotFound => 3,
            ErrorCode::InvalidState => 4,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Internal => "internal",
            ErrorCode::Usage => "usage",
            ErrorCode::InvalidValue => "invalid_value",
            ErrorCode::OutOfRange => "out_of_range",
            ErrorCode::NotFound => "not_found",
            ErrorCode::InvalidState => "invalid_state",
        }
    }
}

/// One error document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseError {
    pub code: ErrorCode,
    pub message: String,
    pub field: Option<String>,
    pub lease_id: Option<String>,
    pub state: Option<String>,
}

impl LeaseError {
    pub fn internal(message: impl Into<String>) -> LeaseError {
        LeaseError {
            code: ErrorCode::Internal,
            message: message.into(),
            field: None,
            lease_id: None,
            state: None,
        }
    }
}
